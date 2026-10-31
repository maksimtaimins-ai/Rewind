//! Video encoding: encoder discovery, auto-selection, benchmark, and the
//! encoder thread that turns GPU textures into buffer packets.

#[cfg(windows)]
pub mod mf;

use serde::Serialize;

use crate::settings::{Codec, EncoderChoice, RateControl};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, PartialOrd, Ord)]
#[serde(rename_all = "snake_case")]
pub enum EncoderKind {
    // Declaration order is preference order.
    Nvenc,
    Amf,
    Qsv,
    Other,
    Software,
}

#[derive(Debug, Clone, Serialize)]
pub struct EncoderInfo {
    pub kind: EncoderKind,
    pub name: String,
    pub codec: Codec,
    /// Adapter the encoder runs on; the D3D device must be created there.
    #[serde(skip)]
    pub luid: Option<u64>,
    #[serde(skip)]
    pub index: u32,
}

#[derive(Debug, Clone, Copy)]
pub struct EncoderParams {
    pub codec: Codec,
    pub width: u32,
    pub height: u32,
    pub fps: u32,
    pub rate_control: RateControl,
    pub bitrate_kbps: u32,
    pub quality: u32,
    pub keyframe_secs: f32,
}

/// Pick an encoder for the user's choice. `Auto` never picks software while
/// any hardware encoder exists.
pub fn pick(list: &[EncoderInfo], choice: EncoderChoice, codec: Codec) -> Option<EncoderInfo> {
    let want = match choice {
        EncoderChoice::Auto => None,
        EncoderChoice::Nvenc => Some(EncoderKind::Nvenc),
        EncoderChoice::Amf => Some(EncoderKind::Amf),
        EncoderChoice::Qsv => Some(EncoderKind::Qsv),
        EncoderChoice::Software => Some(EncoderKind::Software),
    };
    let mut candidates: Vec<&EncoderInfo> = list.iter().filter(|e| e.codec == codec).collect();
    candidates.sort_by_key(|e| e.kind);
    match want {
        Some(k) => candidates.into_iter().find(|e| e.kind == k).cloned(),
        None => candidates.into_iter().next().cloned(),
    }
}

#[cfg(windows)]
pub use win::*;

#[cfg(windows)]
mod win {
    use std::sync::atomic::{AtomicU64, Ordering};
    use std::sync::Arc;
    use std::thread::JoinHandle;
    use std::time::{Duration, Instant};

    use anyhow::{anyhow, Result};
    use bytes::Bytes;
    use crossbeam_channel::{bounded, Receiver, Sender, TrySendError};
    use windows::Win32::Graphics::Direct3D11::*;
    use windows::Win32::Graphics::Dxgi::Common::DXGI_FORMAT_B8G8R8A8_UNORM;

    use super::mf::{self, EncodedFrame, MfEncoder};
    use super::*;
    use crate::buffer::{Packet, VideoCodecConfig, VideoConfig, TICKS_PER_SEC};
    use crate::gpu::{Converter, Gpu, SendPtr};
    use crate::mux::codec::{annexb_to_sample, split_annexb, nal_type, NalCodec};

    pub fn enumerate_all() -> Vec<EncoderInfo> {
        let mut v = mf::enumerate(Codec::H264);
        v.extend(mf::enumerate(Codec::Hevc));
        v
    }

    pub enum EncoderEvent {
        Config(VideoConfig),
        Packet(Packet),
    }

    enum Msg {
        Frame(SendPtr<ID3D11Texture2D>, i64),
        Stop,
    }

    #[derive(Default)]
    pub struct EncoderStats {
        pub frames_in: AtomicU64,
        pub frames_out: AtomicU64,
        pub bytes_out: AtomicU64,
        pub dropped: AtomicU64,
    }

    pub struct EncoderHandle {
        tx: Sender<Msg>,
        thread: Option<JoinHandle<()>>,
        pub stats: Arc<EncoderStats>,
    }

    impl EncoderHandle {
        /// Queue a frame. Never blocks: if the encoder is behind, the frame is
        /// dropped so capture keeps its cadence.
        pub fn submit(&self, tex: ID3D11Texture2D, pts: i64) {
            match self.tx.try_send(Msg::Frame(SendPtr(tex), pts)) {
                Ok(()) => {
                    self.stats.frames_in.fetch_add(1, Ordering::Relaxed);
                }
                Err(TrySendError::Full(_)) | Err(TrySendError::Disconnected(_)) => {
                    self.stats.dropped.fetch_add(1, Ordering::Relaxed);
                }
            }
        }

        pub fn stop(&mut self) {
            let _ = self.tx.send(Msg::Stop);
            if let Some(t) = self.thread.take() {
                let _ = t.join();
            }
        }
    }

    impl Drop for EncoderHandle {
        fn drop(&mut self) {
            self.stop();
        }
    }

    fn nal_codec(c: Codec) -> NalCodec {
        match c {
            Codec::H264 => NalCodec::H264,
            Codec::Hevc => NalCodec::Hevc,
        }
    }

    /// Turns raw encoder output into buffer events, tracking parameter sets.
    struct Packetizer {
        codec: Codec,
        params: EncoderParams,
        vps: Option<Vec<u8>>,
        sps: Option<Vec<u8>>,
        pps: Option<Vec<u8>>,
        current: Option<VideoConfig>,
    }

    impl Packetizer {
        fn absorb_header(&mut self, header: &[u8]) {
            let nc = nal_codec(self.codec);
            for nal in split_annexb(header) {
                match (nc, nal_type(nc, nal)) {
                    (NalCodec::H264, 7) | (NalCodec::Hevc, 33) => self.sps = Some(nal.to_vec()),
                    (NalCodec::H264, 8) | (NalCodec::Hevc, 34) => self.pps = Some(nal.to_vec()),
                    (NalCodec::Hevc, 32) => self.vps = Some(nal.to_vec()),
                    _ => {}
                }
            }
        }

        fn process(&mut self, f: EncodedFrame, header: Option<&[u8]>, emit: &mut dyn FnMut(EncoderEvent)) {
            let au = annexb_to_sample(nal_codec(self.codec), &f.data);
            if let Some(v) = au.vps {
                self.vps = Some(v);
            }
            if let Some(s) = au.sps {
                self.sps = Some(s);
            }
            if let Some(p) = au.pps {
                self.pps = Some(p);
            }
            if self.sps.is_none() {
                if let Some(h) = header {
                    self.absorb_header(h);
                }
            }
            let codec = match (self.codec, &self.vps, &self.sps, &self.pps) {
                (Codec::H264, _, Some(sps), Some(pps)) => VideoCodecConfig::H264 { sps: sps.clone(), pps: pps.clone() },
                (Codec::Hevc, Some(vps), Some(sps), Some(pps)) => {
                    VideoCodecConfig::Hevc { vps: vps.clone(), sps: sps.clone(), pps: pps.clone() }
                }
                _ => return, // no parameter sets yet: can't use this frame
            };
            let cfg = VideoConfig { width: self.params.width, height: self.params.height, fps: self.params.fps, codec };
            if self.current.as_ref() != Some(&cfg) {
                self.current = Some(cfg.clone());
                emit(EncoderEvent::Config(cfg));
            }
            if au.sample.is_empty() {
                return;
            }
            emit(EncoderEvent::Packet(Packet {
                pts: f.pts,
                duration: f.duration.max(1),
                keyframe: f.keyframe || au.has_idr,
                data: Bytes::from(au.sample),
            }));
        }
    }

    /// Start an encoder thread. Blocks until the encoder is initialised so
    /// configuration errors are reported to the caller.
    pub fn spawn(
        info: EncoderInfo,
        gpu: Gpu,
        params: EncoderParams,
        mut sink: Box<dyn FnMut(EncoderEvent) + Send>,
    ) -> Result<EncoderHandle> {
        let (tx, rx) = bounded::<Msg>(4);
        let (ready_tx, ready_rx) = bounded::<Result<()>>(1);
        let stats = Arc::new(EncoderStats::default());
        let st = stats.clone();
        let thread = std::thread::Builder::new()
            .name("rewind-encoder".into())
            .spawn(move || {
                crate::platform::com_init();
                let mut enc = match MfEncoder::new(&info, &gpu, params) {
                    Ok(e) => {
                        let _ = ready_tx.send(Ok(()));
                        e
                    }
                    Err(e) => {
                        let _ = ready_tx.send(Err(e));
                        return;
                    }
                };
                if let Err(e) = run(&mut enc, &rx, params, &st, &mut *sink) {
                    tracing::error!("encoder stopped: {e:#}");
                }
            })?;
        ready_rx.recv().map_err(|_| anyhow!("encoder thread died"))??;
        Ok(EncoderHandle { tx, thread: Some(thread), stats })
    }

    fn run(
        enc: &mut MfEncoder,
        rx: &Receiver<Msg>,
        params: EncoderParams,
        stats: &EncoderStats,
        sink: &mut dyn FnMut(EncoderEvent),
    ) -> Result<()> {
        let mut pk = Packetizer { codec: params.codec, params, vps: None, sps: None, pps: None, current: None };
        let kf_ticks = (params.keyframe_secs.max(0.25) as f64 * TICKS_PER_SEC as f64) as i64;
        let mut last_kf: Option<i64> = None;
        let mut out = Vec::new();
        let mut emit = |out: &mut Vec<EncodedFrame>, enc: &MfEncoder, pk: &mut Packetizer| {
            for f in out.drain(..) {
                stats.frames_out.fetch_add(1, Ordering::Relaxed);
                stats.bytes_out.fetch_add(f.data.len() as u64, Ordering::Relaxed);
                pk.process(f, enc.sequence_header.as_deref(), sink);
            }
        };
        loop {
            while !enc.needs_input() {
                enc.wait_event(&mut out)?;
                emit(&mut out, enc, &mut pk);
            }
            match rx.recv() {
                Ok(Msg::Frame(tex, pts)) => {
                    // Time-based keyframes: clips can start anywhere within
                    // `keyframe_secs`, even when frames arrive irregularly.
                    if last_kf.is_none_or(|k| pts - k >= kf_ticks) {
                        enc.force_keyframe();
                        last_kf = Some(pts);
                    }
                    enc.encode(&tex.0, pts, &mut out)?;
                    emit(&mut out, enc, &mut pk);
                }
                Ok(Msg::Stop) | Err(_) => break,
            }
        }
        Ok(())
    }

    #[derive(Debug, Clone, Serialize)]
    pub struct BenchResult {
        pub kind: EncoderKind,
        pub name: String,
        pub codec: Codec,
        pub fps: f64,
        pub ok: bool,
        pub error: Option<String>,
    }

    /// Encode synthetic frames at `width`x`height` as fast as possible for
    /// `secs` per encoder and report throughput.
    pub fn benchmark(list: &[EncoderInfo], width: u32, height: u32, secs: f64) -> Vec<BenchResult> {
        list.iter()
            .map(|info| match bench_one(info, width, height, secs) {
                Ok(fps) => BenchResult { kind: info.kind, name: info.name.clone(), codec: info.codec, fps, ok: true, error: None },
                Err(e) => BenchResult {
                    kind: info.kind,
                    name: info.name.clone(),
                    codec: info.codec,
                    fps: 0.0,
                    ok: false,
                    error: Some(format!("{e:#}")),
                },
            })
            .collect()
    }

    fn bench_one(info: &EncoderInfo, width: u32, height: u32, secs: f64) -> Result<f64> {
        let gpu = Gpu::new(info.luid)?;
        let params = EncoderParams {
            codec: info.codec,
            width,
            height,
            fps: 60,
            rate_control: RateControl::Cbr,
            bitrate_kbps: 20_000,
            quality: 70,
            keyframe_secs: 1.0,
        };
        let mut enc = MfEncoder::new(info, &gpu, params)?;
        // A few noisy source frames so the encoder has real work to do.
        let mut sources = Vec::new();
        let mut seed = 0x1234_5678u32;
        for _ in 0..4 {
            let tex = gpu.texture(width, height, DXGI_FORMAT_B8G8R8A8_UNORM, D3D11_BIND_RENDER_TARGET)?;
            let mut px = vec![0u8; (width * height * 4) as usize];
            for p in px.iter_mut() {
                seed ^= seed << 13;
                seed ^= seed >> 17;
                seed ^= seed << 5;
                *p = (seed >> 24) as u8;
            }
            unsafe { gpu.context.UpdateSubresource(&tex, 0, None, px.as_ptr() as _, width * 4, 0) };
            sources.push(tex);
        }
        let mut conv = Converter::new(&gpu, (width, height), (width, height), 60)?;
        let start = Instant::now();
        let mut out = Vec::new();
        let mut produced = 0u64;
        let mut i = 0i64;
        while start.elapsed() < Duration::from_secs_f64(secs) {
            while !enc.needs_input() {
                enc.wait_event(&mut out)?;
                produced += out.drain(..).count() as u64;
            }
            let nv12 = conv.convert(&sources[(i % 4) as usize])?;
            if i % 60 == 0 {
                enc.force_keyframe();
            }
            enc.encode(&nv12, i * TICKS_PER_SEC / 60, &mut out)?;
            produced += out.drain(..).count() as u64;
            i += 1;
        }
        let elapsed = start.elapsed().as_secs_f64();
        if produced == 0 {
            return Err(anyhow!("encoder produced no output"));
        }
        Ok(produced as f64 / elapsed)
    }
}
