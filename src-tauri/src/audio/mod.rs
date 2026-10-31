//! Multi-track audio. Each enabled track runs its own capture thread:
//! WASAPI -> gap fill / drift correction -> AAC -> replay buffer track.

pub mod aac;
pub mod wasapi;

use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;
use std::thread::JoinHandle;
use std::time::{Duration, Instant};

use anyhow::Result;
use bytes::Bytes;
use parking_lot::Mutex;

use crate::buffer::{AudioConfig, Packet, ReplayBuffer, TICKS_PER_SEC};
use crate::mux::codec::aac_asc;
use crate::pipeline::Flags;
use crate::platform::{find_pid, qpc_now};
use crate::settings::{AudioSource, AudioTrack};
use wasapi::{Capture, Source, CHANNELS, SAMPLE_RATE};

pub use wasapi::{microphones, DeviceInfo};

const BITRATE: u32 = 160_000;
/// Gaps/overlaps smaller than this are absorbed as clock jitter.
const TOLERANCE_FRAMES: i64 = SAMPLE_RATE as i64 * 30 / 1000;
/// Silence is only synthesised up to this far behind "now", so late packets
/// still land in their slot.
const SILENCE_LAG: i64 = TICKS_PER_SEC / 10;

pub struct AudioTrackHandle {
    stop: Arc<AtomicBool>,
    thread: Option<JoinHandle<()>>,
}

impl AudioTrackHandle {
    pub fn stop(&mut self) {
        self.stop.store(true, Ordering::Relaxed);
        if let Some(t) = self.thread.take() {
            let _ = t.join();
        }
    }
}

impl Drop for AudioTrackHandle {
    fn drop(&mut self) {
        self.stop();
    }
}

/// Keeps a continuous sample timeline anchored to QPC and feeds the encoder.
struct Timeline {
    base: i64,
    /// Frames fed to the encoder so far.
    fed: i64,
    volume: f32,
    enc: aac::AacEncoder,
    pcm: Vec<i16>,
    frames: Vec<aac::AacFrame>,
}

impl Timeline {
    fn frame_at(&self, qpc: i64) -> i64 {
        (qpc - self.base) * SAMPLE_RATE as i64 / TICKS_PER_SEC
    }

    fn pts_of(&self, frame: i64) -> i64 {
        self.base + frame * TICKS_PER_SEC / SAMPLE_RATE as i64
    }

    fn feed(&mut self, samples: Option<&[f32]>, frames: usize) -> Result<()> {
        if frames == 0 {
            return Ok(());
        }
        self.pcm.clear();
        match samples {
            Some(s) => self.pcm.extend(s.iter().map(|&x| ((x * self.volume).clamp(-1.0, 1.0) * 32767.0) as i16)),
            None => self.pcm.resize(frames * CHANNELS as usize, 0),
        }
        let pts = self.pts_of(self.fed);
        self.enc.encode(&self.pcm, pts, &mut self.frames)?;
        self.fed += frames as i64;
        Ok(())
    }

    /// Place a captured chunk on the timeline, padding gaps with silence and
    /// trimming overlaps.
    fn push(&mut self, qpc: i64, samples: Option<&[f32]>, frames: usize) -> Result<()> {
        let at = self.frame_at(qpc);
        let gap = at - self.fed;
        if gap > TOLERANCE_FRAMES {
            self.feed(None, gap as usize)?;
        } else if gap < -TOLERANCE_FRAMES {
            let skip = ((-gap) as usize).min(frames);
            let rest = samples.map(|s| &s[skip * CHANNELS as usize..]);
            return self.feed(rest, frames - skip);
        }
        self.feed(samples, frames)
    }

    fn fill_silence_until(&mut self, now: i64) -> Result<()> {
        let target = self.frame_at(now - SILENCE_LAG);
        if target > self.fed + TOLERANCE_FRAMES {
            self.feed(None, (target - self.fed) as usize)?;
        }
        Ok(())
    }
}

fn resolve(source: &AudioSource) -> Option<Source> {
    Some(match source {
        AudioSource::System => Source::System,
        AudioSource::Mic { device_id } => Source::Mic(device_id.clone()),
        AudioSource::App { exe } => Source::Process { pid: find_pid(exe)?, exclude: false },
        AudioSource::SystemExcluding { exe } => match find_pid(exe) {
            Some(pid) => Source::Process { pid, exclude: true },
            None => Source::System, // nothing to exclude right now
        },
    })
}

pub fn start_track(track: &AudioTrack, buffer: Arc<Mutex<ReplayBuffer>>, flags: Arc<Flags>) -> Result<AudioTrackHandle> {
    let index = buffer.lock().add_audio_track(AudioConfig {
        name: track.name.clone(),
        sample_rate: SAMPLE_RATE,
        channels: CHANNELS,
        bitrate: aac::supported_bitrate(BITRATE),
        asc: aac_asc(SAMPLE_RATE, CHANNELS),
    });
    let stop = Arc::new(AtomicBool::new(false));
    let st = stop.clone();
    let track = track.clone();
    let thread = std::thread::Builder::new().name(format!("rewind-audio-{}", track.name)).spawn(move || {
        crate::platform::com_init();
        if let Err(e) = run_track(&track, index, &buffer, &flags, &st) {
            tracing::error!("audio track {} stopped: {e:#}", track.name);
        }
    })?;
    Ok(AudioTrackHandle { stop, thread: Some(thread) })
}

fn run_track(
    track: &AudioTrack,
    index: usize,
    buffer: &Mutex<ReplayBuffer>,
    flags: &Flags,
    stop: &AtomicBool,
) -> Result<()> {
    let mut tl = Timeline {
        base: qpc_now(),
        fed: 0,
        volume: track.volume,
        enc: aac::AacEncoder::new(BITRATE)?,
        pcm: Vec::new(),
        frames: Vec::new(),
    };
    let app_source = matches!(track.source, AudioSource::App { .. } | AudioSource::SystemExcluding { .. });
    let mut capture: Option<Capture> = None;
    let mut last_open = Instant::now() - Duration::from_secs(60);
    let mut opened_pid: Option<u32> = None;

    while !stop.load(Ordering::Relaxed) {
        // (Re)open the device: app tracks follow the app across restarts.
        if last_open.elapsed() > Duration::from_secs(if capture.is_some() { 5 } else { 2 }) {
            last_open = Instant::now();
            let src = resolve(&track.source);
            let pid = match &src {
                Some(Source::Process { pid, .. }) => Some(*pid),
                _ => None,
            };
            if capture.is_none() || (app_source && pid != opened_pid) {
                capture = src.and_then(|s| Capture::open(&s).map_err(|e| tracing::warn!("{}: {e:#}", track.name)).ok());
                opened_pid = pid;
            }
        }

        let mut err = None;
        if let Some(cap) = capture.as_mut() {
            let paused = flags.paused.load(Ordering::Relaxed);
            let r = cap.read(Duration::from_millis(20), &mut |chunk| {
                let samples = if paused { None } else { chunk.samples };
                if let Err(e) = tl.push(chunk.qpc, samples, chunk.frames) {
                    err = Some(e);
                }
            });
            if let Err(e) = r {
                tracing::warn!("{}: capture lost ({e:#}), reopening", track.name);
                capture = None;
            }
        } else {
            std::thread::sleep(Duration::from_millis(20));
        }
        if let Some(e) = err {
            return Err(e);
        }
        // Loopback delivers nothing while nothing plays: keep the track continuous.
        tl.fill_silence_until(qpc_now())?;

        if !tl.frames.is_empty() {
            let mut b = buffer.lock();
            for f in tl.frames.drain(..) {
                b.push_audio(index, Packet { pts: f.pts, duration: f.duration, keyframe: true, data: Bytes::from(f.data) });
            }
        }
    }
    Ok(())
}
