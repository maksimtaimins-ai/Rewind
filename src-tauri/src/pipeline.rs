//! Wires capture -> GPU convert -> encoder -> replay buffer (+ audio tracks).

use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;

use anyhow::{Context, Result};
use crossbeam_channel::Sender;
use parking_lot::Mutex;
use windows::Win32::Graphics::Direct3D11::{ID3D11Texture2D, D3D11_TEXTURE2D_DESC};

use crate::audio::{self, AudioTrackHandle};
use crate::buffer::{ReplayBuffer, TICKS_PER_SEC};
use crate::capture::wgc::{self, WgcSession};
use crate::capture::{CaptureSession, MonitorInfo};
use crate::encode::{self, EncoderEvent, EncoderHandle, EncoderInfo, EncoderParams, EncoderStats};
use crate::gpu::{output_size, Converter, Gpu};
use crate::settings::AudioTrack;

/// Flags the engine flips while the pipeline runs.
#[derive(Default)]
pub struct Flags {
    /// A privacy-listed app has focus.
    pub privacy: AtomicBool,
}

impl Flags {
    pub fn blocked(&self) -> bool {
        self.privacy.load(Ordering::Relaxed)
    }
}

pub type FrameGrab = (u32, u32, Vec<u8>);

pub struct PipelineConfig {
    pub monitor: MonitorInfo,
    pub cursor: bool,
    pub target_height: u32,
    pub encoder: EncoderInfo,
    pub params: EncoderParams,
    pub audio: Vec<AudioTrack>,
}

pub struct Pipeline {
    capture: Option<WgcSession>,
    encoder: Arc<Mutex<Option<EncoderHandle>>>,
    audio: Vec<AudioTrackHandle>,
    pub stats: Arc<EncoderStats>,
    pub output: (u32, u32),
    pub encoder_name: String,
    grab: Arc<Mutex<Vec<Sender<FrameGrab>>>>,
}

impl Pipeline {
    pub fn start(cfg: PipelineConfig, buffer: Arc<Mutex<ReplayBuffer>>, flags: Arc<Flags>) -> Result<Self> {
        let gpu = Gpu::new(cfg.encoder.luid).context("create D3D11 device")?;
        let native = (cfg.monitor.width, cfg.monitor.height);
        let out = output_size(native, cfg.target_height);
        let mut params = cfg.params;
        params.width = out.0;
        params.height = out.1;

        let buf = buffer.clone();
        let sink = Box::new(move |ev: EncoderEvent| {
            let mut b = buf.lock();
            match ev {
                EncoderEvent::Config(c) => b.set_video_config(c),
                EncoderEvent::Packet(p) => b.push_video(p),
            }
        });
        let enc = encode::spawn(cfg.encoder.clone(), gpu.clone(), params, sink)
            .with_context(|| format!("start {}", cfg.encoder.name))?;
        let stats = enc.stats.clone();
        let encoder = Arc::new(Mutex::new(Some(enc)));

        let grab: Arc<Mutex<Vec<Sender<FrameGrab>>>> = Arc::default();
        let interval = TICKS_PER_SEC / params.fps.max(1) as i64;
        let mut next_due = 0i64;
        let mut converter: Option<Converter> = None;
        let enc_ref = encoder.clone();
        let grab_ref = grab.clone();
        let gpu_cb = gpu.clone();
        let fps = params.fps;
        let flags_cb = flags.clone();
        let on_frame: wgc::FrameCallback = Box::new(move |tex: &ID3D11Texture2D, pts: i64| {
            // Screenshot/thumbnail requests get the full-res frame even when paced out.
            let waiting: Vec<_> = std::mem::take(&mut *grab_ref.lock());
            if !waiting.is_empty() {
                if let Ok(px) = gpu_cb.read_bgra(tex) {
                    for w in waiting {
                        let _ = w.send(px.clone());
                    }
                }
            }
            if flags_cb.blocked() {
                return;
            }
            // FPS cap from frame timestamps (works with VRR: no fixed polling).
            let tolerance = interval / 4;
            if pts + tolerance < next_due {
                return;
            }
            next_due = if pts - next_due > interval { pts + interval } else { next_due + interval };

            let mut desc = D3D11_TEXTURE2D_DESC::default();
            unsafe { tex.GetDesc(&mut desc) };
            let in_size = (desc.Width, desc.Height);
            if converter.as_ref().map(|c| c.input_size()) != Some(in_size) {
                match Converter::new(&gpu_cb, in_size, out, fps) {
                    Ok(c) => converter = Some(c),
                    Err(e) => {
                        tracing::error!("converter: {e:#}");
                        return;
                    }
                }
            }
            match converter.as_mut().unwrap().convert(tex) {
                Ok(nv12) => {
                    if let Some(e) = enc_ref.lock().as_ref() {
                        e.submit(nv12, pts);
                    }
                }
                Err(e) => tracing::warn!("convert: {e:#}"),
            }
        });

        let (capture, _) = wgc::start(&gpu, cfg.monitor.handle, cfg.cursor, on_frame).context("start screen capture")?;

        let mut audio = Vec::new();
        for track in cfg.audio.iter().filter(|t| t.enabled) {
            match audio::start_track(track, buffer.clone()) {
                Ok(h) => audio.push(h),
                Err(e) => tracing::warn!("audio track {}: {e:#}", track.name),
            }
        }

        Ok(Self {
            capture: Some(capture),
            encoder,
            audio,
            stats,
            output: out,
            encoder_name: cfg.encoder.name.clone(),
            grab,
        })
    }

    /// Grab the next captured frame as BGRA (for screenshots and thumbnails).
    pub fn request_frame(&self) -> crossbeam_channel::Receiver<FrameGrab> {
        let (tx, rx) = crossbeam_channel::bounded(1);
        self.grab.lock().push(tx);
        rx
    }

    pub fn stop(&mut self) {
        if let Some(mut c) = self.capture.take() {
            c.stop();
        }
        if let Some(mut e) = self.encoder.lock().take() {
            e.stop();
        }
        for mut a in self.audio.drain(..) {
            a.stop();
        }
    }
}

impl Drop for Pipeline {
    fn drop(&mut self) {
        self.stop();
    }
}
