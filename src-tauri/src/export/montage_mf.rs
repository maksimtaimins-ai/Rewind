//! Montage encode pass: each segment's clip decoded with Media Foundation,
//! letterboxed onto one canvas, blended through crossfades, and written with
//! the Sink Writer alongside the mixed audio track.

use std::collections::BTreeMap;
use std::path::Path;

use anyhow::{bail, Context, Result};
use windows::core::HSTRING;
use windows::Win32::Media::MediaFoundation::*;

use super::mf::{add_aac_stream, add_h264_stream, bgra_sample, decode_audio, Reader};
use super::montage::{mix, plan, to_stereo_48k, total, weights_at, Request, Segment, RATE};
use super::scale_bgra;

const TICKS: i64 = 10_000_000;

fn even(v: f64) -> u32 {
    ((v / 2.0).round() as u32 * 2).max(2)
}

/// `(w, h)` scaled to fit `max_w`x`max_h`, never up.
fn fit(w: u32, h: u32, max_w: u32, max_h: u32) -> (u32, u32) {
    let k = (max_w as f64 / w as f64).min(max_h as f64 / h as f64).min(1.0);
    (even(w as f64 * k), even(h as f64 * k))
}

/// One clip's audio (its first track, the full mix) as 48 kHz stereo,
/// cut to `start..start + len` seconds.
fn clip_audio(path: &Path, start: f64, len: f64) -> Vec<i16> {
    let mut tracks: BTreeMap<u32, (u32, u32, Vec<i16>)> = BTreeMap::new();
    if let Err(e) = decode_audio(path, &mut |stream, rate, ch, pcm| tracks.entry(stream).or_insert((rate, ch, Vec::new())).2.extend_from_slice(pcm)) {
        tracing::info!("montage: no audio from {} ({e:#})", path.display());
    }
    let Some((_, (rate, ch, pcm))) = tracks.into_iter().next() else { return Vec::new() };
    let st = to_stereo_48k(&pcm, rate, ch);
    let a = ((start * RATE as f64) as usize * 2).min(st.len());
    let b = (a + (len * RATE as f64) as usize * 2).min(st.len());
    st[a..b].to_vec()
}

/// Streams one segment's frames: `at(t)` gives the frame showing `t`
/// seconds into the clip, letterboxed onto the canvas.
struct SegVideo {
    r: Reader,
    origin: Option<i64>,
    /// Next decoded sample not yet shown: (clip time, sample).
    ahead: Option<(i64, IMFSample)>,
    done: bool,
    canvas: Vec<u8>,
    /// Where the clip sits on the canvas.
    place: (u32, u32, u32, u32),
    size: (u32, u32),
}

impl SegVideo {
    fn open(path: &Path, size: (u32, u32)) -> Result<Self> {
        let probe = Reader::open(path, false, None)?;
        let (w, h) = fit(probe.width, probe.height, size.0, size.1);
        drop(probe);
        let r = Reader::open(path, false, Some((w, h)))?;
        let place = ((size.0 - w) / 2, (size.1 - h) / 2, w, h);
        Ok(SegVideo { r, origin: None, ahead: None, done: false, canvas: vec![0; size.0 as usize * size.1 as usize * 4], place, size })
    }

    fn pull(&mut self) -> Result<()> {
        while self.ahead.is_none() && !self.done {
            match self.r.next()? {
                Some((true, ts, Some(s))) => {
                    let t = ts - *self.origin.get_or_insert(ts);
                    self.ahead = Some((t, s));
                }
                Some(_) => {}
                None => self.done = true,
            }
        }
        Ok(())
    }

    /// Canvas showing the frame at `t` ticks into the clip. Frames passed
    /// over are skipped without converting them.
    fn at(&mut self, t: i64) -> Result<&[u8]> {
        let mut latest = None;
        loop {
            self.pull()?;
            match &self.ahead {
                Some((ts, _)) if *ts <= t => latest = self.ahead.take(),
                _ => break,
            }
        }
        if let Some((ts, s)) = latest {
            let f = self.r.frame(&s, ts)?;
            let (x, y, w, h) = self.place;
            let px = if (self.r.width, self.r.height) == (w, h) { f.px } else { scale_bgra(&f.px, self.r.width, self.r.height, (0, 0, self.r.width, self.r.height), w, h) };
            let stride = self.size.0 as usize * 4;
            for row in 0..h as usize {
                let d = (y as usize + row) * stride + x as usize * 4;
                self.canvas[d..d + w as usize * 4].copy_from_slice(&px[row * w as usize * 4..(row + 1) * w as usize * 4]);
            }
        }
        Ok(&self.canvas)
    }
}

/// Write the montage to `out`. `pieces` are (start, length) per request
/// clip; `progress` gets 0-1.
pub fn make(req: &Request, pieces: &[(f64, f64)], out: &Path, progress: &mut dyn FnMut(f64)) -> Result<()> {
    let segs: Vec<Segment> = plan(pieces, req.transition);
    if segs.is_empty() {
        bail!("pick at least one clip with footage");
    }
    let first = Reader::open(&req.paths[segs[0].clip], false, None).context("open the first clip")?;
    let size = fit(first.width, first.height, 1920, 1080);
    let fps = first.fps.clamp(24, 60);
    drop(first);
    let bps = ((size.0 as f64 * size.1 as f64 * fps as f64 * 0.1) as u32).clamp(4_000_000, 20_000_000);

    // Audio first: small, and it decides nothing about the video.
    let clips: Vec<Vec<i16>> = segs.iter().map(|s| if req.clip_audio { clip_audio(&req.paths[s.clip], s.src_start, s.len) } else { Vec::new() }).collect();
    let music = match &req.music {
        Some(m) => Some(clip_audio(m, 0.0, total(&segs)).into_iter().collect::<Vec<i16>>()).filter(|v| !v.is_empty()),
        None => None,
    };
    if req.music.is_some() && music.is_none() {
        bail!("couldn't read the music file (use an mp3 or wav)");
    }
    let pcm = mix(&segs, &clips, music.as_deref(), req.clip_audio);
    progress(0.05);

    let result = unsafe { encode(&req.paths, &segs, out, size, fps, bps, &pcm, progress) };
    if result.is_err() {
        let _ = std::fs::remove_file(out);
    }
    result
}

#[allow(clippy::too_many_arguments)]
unsafe fn encode(paths: &[std::path::PathBuf], segs: &[Segment], out: &Path, size: (u32, u32), fps: u32, bps: u32, pcm: &[i16], progress: &mut dyn FnMut(f64)) -> Result<()> {
    unsafe {
        let writer = MFCreateSinkWriterFromURL(&HSTRING::from(out.as_os_str()), None, None).context("create montage file")?;
        let vstream = add_h264_stream(&writer, size.0, size.1, fps, bps)?;
        let pcm_type = MFCreateMediaType()?;
        pcm_type.SetGUID(&MF_MT_MAJOR_TYPE, &MFMediaType_Audio)?;
        pcm_type.SetGUID(&MF_MT_SUBTYPE, &MFAudioFormat_PCM)?;
        pcm_type.SetUINT32(&MF_MT_AUDIO_BITS_PER_SAMPLE, 16)?;
        pcm_type.SetUINT32(&MF_MT_AUDIO_SAMPLES_PER_SECOND, RATE)?;
        pcm_type.SetUINT32(&MF_MT_AUDIO_NUM_CHANNELS, 2)?;
        pcm_type.SetUINT32(&MF_MT_AUDIO_BLOCK_ALIGNMENT, 4)?;
        pcm_type.SetUINT32(&MF_MT_AUDIO_AVG_BYTES_PER_SECOND, RATE * 4)?;
        let astream = add_aac_stream(&writer, &pcm_type, RATE, 2, 192_000)?;
        writer.BeginWriting()?;

        let frames = (total(segs) * fps as f64).floor() as u64;
        let interval = TICKS / fps as i64;
        let mut open: BTreeMap<usize, SegVideo> = BTreeMap::new();
        let mut blend = vec![0u8; size.0 as usize * size.1 as usize * 4];
        let mut audio_at = 0usize; // frames of pcm written
        for n in 0..frames {
            let t = n as f64 / fps as f64;
            let showing = weights_at(segs, t);
            open.retain(|i, _| showing.iter().any(|(j, _)| j == i));
            for &(i, _) in &showing {
                if !open.contains_key(&i) {
                    open.insert(i, SegVideo::open(&paths[segs[i].clip], size).with_context(|| format!("open {}", paths[segs[i].clip].display()))?);
                }
            }
            let local = |i: usize| ((t - segs[i].out_start + segs[i].src_start) * TICKS as f64) as i64;
            let frame: Vec<u8> = match showing.as_slice() {
                [(i, _)] => open.get_mut(i).unwrap().at(local(*i))?.to_vec(),
                [(a, wa), (b, _)] => {
                    let fa = open.get_mut(a).unwrap().at(local(*a))?.to_vec();
                    let fb = open.get_mut(b).unwrap().at(local(*b))?;
                    let ka = (*wa * 256.0) as u32;
                    for ((o, x), y) in blend.iter_mut().zip(&fa).zip(fb) {
                        *o = ((*x as u32 * ka + *y as u32 * (256 - ka)) >> 8) as u8;
                    }
                    blend.clone()
                }
                _ => vec![0; blend.len()],
            };
            let ts = n as i64 * interval;
            writer.WriteSample(vstream, &bgra_sample(&frame, ts, interval)?)?;

            // Audio up to the end of this frame, in ~20 ms pieces.
            let until = ((((n + 1) as f64 / fps as f64) * RATE as f64) as usize).min(pcm.len() / 2);
            while audio_at < until {
                let count = (until - audio_at).min(1024);
                let bytes: Vec<u8> = pcm[audio_at * 2..(audio_at + count) * 2].iter().flat_map(|s| s.to_le_bytes()).collect();
                let buf = MFCreateMemoryBuffer(bytes.len() as u32)?;
                let mut data = std::ptr::null_mut();
                buf.Lock(&mut data, None, None)?;
                std::ptr::copy_nonoverlapping(bytes.as_ptr(), data, bytes.len());
                buf.Unlock()?;
                buf.SetCurrentLength(bytes.len() as u32)?;
                let s = MFCreateSample()?;
                s.AddBuffer(&buf)?;
                s.SetSampleTime(audio_at as i64 * TICKS / RATE as i64)?;
                s.SetSampleDuration(count as i64 * TICKS / RATE as i64)?;
                writer.WriteSample(astream, &s)?;
                audio_at += count;
            }
            if n % 15 == 0 {
                progress(0.05 + 0.95 * n as f64 / frames.max(1) as f64);
            }
        }
        writer.Finalize().context("finish montage file")?;
        progress(1.0);
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::export::montage::{Transition, CROSSFADE_SECS};

    /// A `secs` long clip: a flat colour with a 440 Hz tone, 320x180 at 30 fps.
    unsafe fn synth(path: &Path, secs: f64, bgr: [u8; 3]) {
        unsafe {
            let w = MFCreateSinkWriterFromURL(&HSTRING::from(path.as_os_str()), None, None).unwrap();
            let v = add_h264_stream(&w, 320, 180, 30, 1_000_000).unwrap();
            let pcm = MFCreateMediaType().unwrap();
            pcm.SetGUID(&MF_MT_MAJOR_TYPE, &MFMediaType_Audio).unwrap();
            pcm.SetGUID(&MF_MT_SUBTYPE, &MFAudioFormat_PCM).unwrap();
            pcm.SetUINT32(&MF_MT_AUDIO_BITS_PER_SAMPLE, 16).unwrap();
            pcm.SetUINT32(&MF_MT_AUDIO_SAMPLES_PER_SECOND, RATE).unwrap();
            pcm.SetUINT32(&MF_MT_AUDIO_NUM_CHANNELS, 2).unwrap();
            pcm.SetUINT32(&MF_MT_AUDIO_BLOCK_ALIGNMENT, 4).unwrap();
            pcm.SetUINT32(&MF_MT_AUDIO_AVG_BYTES_PER_SECOND, RATE * 4).unwrap();
            let a = add_aac_stream(&w, &pcm, RATE, 2, 128_000).unwrap();
            w.BeginWriting().unwrap();
            let px: Vec<u8> = (0..320 * 180).flat_map(|_| [bgr[0], bgr[1], bgr[2], 255]).collect();
            let frames = (secs * 30.0) as i64;
            for n in 0..frames {
                w.WriteSample(v, &bgra_sample(&px, n * TICKS / 30, TICKS / 30).unwrap()).unwrap();
                let tone: Vec<u8> = (0..1600)
                    .flat_map(|i| {
                        let s = ((((n * 1600 + i) as f64) * 440.0 * std::f64::consts::TAU / RATE as f64).sin() * 8000.0) as i16;
                        [s.to_le_bytes(), s.to_le_bytes()].concat()
                    })
                    .collect();
                let buf = MFCreateMemoryBuffer(tone.len() as u32).unwrap();
                let mut d = std::ptr::null_mut();
                buf.Lock(&mut d, None, None).unwrap();
                std::ptr::copy_nonoverlapping(tone.as_ptr(), d, tone.len());
                buf.Unlock().unwrap();
                buf.SetCurrentLength(tone.len() as u32).unwrap();
                let s = MFCreateSample().unwrap();
                s.AddBuffer(&buf).unwrap();
                s.SetSampleTime(n * TICKS / 30).unwrap();
                s.SetSampleDuration(TICKS / 30).unwrap();
                w.WriteSample(a, &s).unwrap();
            }
            w.Finalize().unwrap();
        }
    }

    /// Two real clips through the whole decode/blend/encode path.
    #[test]
    fn makes_a_crossfaded_montage() {
        crate::platform::com_init();
        crate::encode::mf::mf_startup();
        let dir = std::env::temp_dir().join(format!("rewind-montage-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        let (a, b) = (dir.join("a.mp4"), dir.join("b.mp4"));
        unsafe {
            synth(&a, 2.0, [0, 0, 255]);
            synth(&b, 3.0, [255, 0, 0]);
        }
        let out = dir.join("m.mp4");
        let req = Request { paths: vec![a.clone(), b.clone()], transition: Transition::Crossfade, max_secs: None, music: Some(a.clone()), clip_audio: true };
        let mut last = 0.0;
        make(&req, &[(0.0, 2.0), (0.0, 3.0)], &out, &mut |f| last = f).unwrap();
        assert_eq!(last, 1.0);
        // 2 + 3 - 0.5 s, give or take a frame.
        let mut r = Reader::open(&out, false, None).unwrap();
        assert_eq!((r.width, r.height), (320, 180));
        let mut frames = Vec::new();
        while let Some((is_video, ts, s)) = r.next().unwrap() {
            if is_video {
                frames.push((ts, s.unwrap()));
            }
        }
        let expect = ((5.0 - CROSSFADE_SECS) * 30.0) as usize;
        assert!(frames.len().abs_diff(expect) <= 2, "{} frames", frames.len());
        // Red first, blue at the end (BGRA: red is byte 2).
        let px = |i: usize| r.frame(&frames[i].1, frames[i].0).unwrap().px[(90 * 320 + 160) * 4..][..3].to_vec();
        let (first, end) = (px(5), px(frames.len() - 5));
        assert!(first[2] > 200 && first[0] < 60, "{first:?}");
        assert!(end[0] > 200 && end[2] < 60, "{end:?}");
        // Mid-crossfade both colours show.
        let mid = px((1.75 * 30.0) as usize);
        assert!(mid[0] > 60 && mid[2] > 60, "{mid:?}");
        let mut samples = 0;
        decode_audio(&out, &mut |_, _, _, pcm| samples += pcm.len()).unwrap();
        assert!(samples > RATE as usize * 2 * 4, "{samples}");
        std::fs::remove_dir_all(&dir).ok();
    }
}
