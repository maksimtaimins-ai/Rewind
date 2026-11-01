//! Smart highlights: find loud spikes, excited voice and sustained loud
//! stretches in a clip's audio, entirely on this PC.
//!
//! Off by default. While off, nothing here runs: no thread, no decoding, no
//! file reads. Analysis only starts from the "Analyze" button or, when the
//! user also turns on "Analyze on save", right after a clip is written.

use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicUsize, Ordering};
use std::thread::JoinHandle;

use serde::{Deserialize, Serialize};

use crate::settings::Settings;

/// Analysis window: 50 ms.
const WINDOW_SECS: f64 = 0.05;
/// Floor for silent windows, dBFS.
const FLOOR_DB: f32 = -96.0;
/// At most this many moments are kept per clip, highest scores first.
const MAX_MOMENTS: usize = 20;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Reason {
    /// A sudden jump well above what came just before.
    Spike,
    /// A short burst of loud, speech-band sound over the background.
    Voice,
    /// Two seconds or more above the threshold.
    Sustained,
}

/// A flagged moment in a clip, seconds from its start.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Moment {
    pub at: f64,
    pub end: f64,
    /// 0-1, higher is more likely worth watching.
    pub score: f32,
    pub reason: Reason,
}

/// One 50 ms step: overall level and how much of it is in the speech band.
#[derive(Debug, Clone, Copy)]
struct Step {
    db: f32,
    voice: f32,
}

/// Streams mono samples into 50 ms steps. Holds only the per-step numbers
/// (6000 for five minutes), never the audio.
pub struct Analyzer {
    rate: u32,
    per_window: usize,
    n: usize,
    sum_sq: f64,
    voice_sq: f64,
    /// One-pole filters for a rough 300-3400 Hz band.
    hp_prev_in: f32,
    hp_prev_out: f32,
    lp_prev: f32,
    hp_a: f32,
    lp_a: f32,
    steps: Vec<Step>,
}

fn db(rms: f64) -> f32 {
    if rms <= 1e-5 { FLOOR_DB } else { (20.0 * rms.log10()) as f32 }
}

impl Analyzer {
    pub fn new(rate: u32) -> Self {
        let rate = rate.max(8000);
        let dt = 1.0 / rate as f32;
        let rc = |hz: f32| 1.0 / (2.0 * std::f32::consts::PI * hz);
        Self {
            rate,
            per_window: (rate as f64 * WINDOW_SECS) as usize,
            n: 0,
            sum_sq: 0.0,
            voice_sq: 0.0,
            hp_prev_in: 0.0,
            hp_prev_out: 0.0,
            lp_prev: 0.0,
            hp_a: rc(300.0) / (rc(300.0) + dt),
            lp_a: dt / (rc(3400.0) + dt),
            steps: Vec::new(),
        }
    }

    pub fn push(&mut self, mono: &[f32]) {
        for &x in mono {
            let hp = self.hp_a * (self.hp_prev_out + x - self.hp_prev_in);
            self.hp_prev_in = x;
            self.hp_prev_out = hp;
            self.lp_prev += self.lp_a * (hp - self.lp_prev);
            self.sum_sq += (x * x) as f64;
            self.voice_sq += (self.lp_prev * self.lp_prev) as f64;
            self.n += 1;
            if self.n == self.per_window {
                let total = self.sum_sq / self.n as f64;
                let voice = if self.sum_sq > 0.0 { (self.voice_sq / self.sum_sq) as f32 } else { 0.0 };
                self.steps.push(Step { db: db(total.sqrt()), voice });
                self.n = 0;
                self.sum_sq = 0.0;
                self.voice_sq = 0.0;
            }
        }
    }

    /// Interleaved 16-bit PCM, any channel count, mixed down to mono.
    pub fn push_i16(&mut self, pcm: &[i16], channels: usize) {
        let ch = channels.max(1);
        let mono: Vec<f32> = pcm.chunks_exact(ch).map(|f| f.iter().map(|&s| s as f32).sum::<f32>() / (ch as f32 * 32768.0)).collect();
        self.push(&mono);
    }

    pub fn sample_rate(&self) -> u32 {
        self.rate
    }

    pub fn finish(self, threshold_db: f32) -> Vec<Moment> {
        find_moments(&self.steps, threshold_db)
    }
}

fn percentile(v: &[f32], p: f32) -> f32 {
    if v.is_empty() {
        return FLOOR_DB;
    }
    let mut s = v.to_vec();
    s.sort_by(|a, b| a.total_cmp(b));
    s[((s.len() - 1) as f32 * p) as usize]
}

fn find_moments(steps: &[Step], threshold: f32) -> Vec<Moment> {
    let t = |i: usize| i as f64 * WINDOW_SECS;
    let levels: Vec<f32> = steps.iter().map(|s| s.db).collect();
    let background = percentile(&levels, 0.3);
    let mut out = Vec::new();

    // Spikes: over the threshold and 10 dB above the half second before.
    const LOOKBACK: usize = 10;
    let mut i = LOOKBACK;
    while i < steps.len() {
        let before = steps[i - LOOKBACK..i].iter().map(|s| s.db).sum::<f32>() / LOOKBACK as f32;
        let rise = steps[i].db - before;
        if steps[i].db >= threshold && rise >= 10.0 {
            let score = ((steps[i].db - threshold) / 12.0 + (rise - 10.0) / 20.0 + 0.4).clamp(0.0, 1.0);
            out.push(Moment { at: t(i), end: t(i) + 0.5, score, reason: Reason::Spike });
            // One spike per half second.
            i += LOOKBACK;
            continue;
        }
        i += 1;
    }

    // Sustained: 2 s or more within 6 dB of the threshold.
    let loud = threshold - 6.0;
    let mut run_start = None;
    for (i, s) in steps.iter().chain(std::iter::once(&Step { db: FLOOR_DB, voice: 0.0 })).enumerate() {
        match (s.db >= loud, run_start) {
            (true, None) => run_start = Some(i),
            (false, Some(start)) => {
                let len = t(i) - t(start);
                if len >= 2.0 {
                    let peak = steps[start..i].iter().map(|s| s.db).fold(FLOOR_DB, f32::max);
                    let score = ((len / 8.0) as f32 * 0.6 + ((peak - loud) / 12.0) * 0.4).clamp(0.2, 1.0);
                    out.push(Moment { at: t(start), end: t(i), score, reason: Reason::Sustained });
                }
                run_start = None;
            }
            _ => {}
        }
    }

    // Excited voice: mostly speech-band, 15 dB over the background, 0.3-2.5 s.
    let voiced = |s: &Step| s.voice >= 0.55 && s.db >= (background + 15.0).max(-40.0);
    let mut i = 0;
    while i < steps.len() {
        if !voiced(&steps[i]) {
            i += 1;
            continue;
        }
        let start = i;
        let mut end = i;
        let mut gap = 0;
        while i < steps.len() && gap <= 2 {
            if voiced(&steps[i]) {
                end = i;
                gap = 0;
            } else {
                gap += 1;
            }
            i += 1;
        }
        let len = t(end + 1) - t(start);
        if (0.3..=2.5).contains(&len) {
            let peak = steps[start..=end].iter().map(|s| s.db).fold(FLOOR_DB, f32::max);
            let score = (((peak - background) - 15.0) / 20.0 + 0.45).clamp(0.0, 1.0);
            out.push(Moment { at: t(start), end: t(end + 1), score, reason: Reason::Voice });
        }
    }

    merge(out)
}

/// Fold moments less than a second apart into the stronger one, keep the
/// best [`MAX_MOMENTS`], and order them by time.
fn merge(mut v: Vec<Moment>) -> Vec<Moment> {
    v.sort_by(|a, b| a.at.total_cmp(&b.at));
    let mut out: Vec<Moment> = Vec::new();
    for m in v {
        match out.last_mut() {
            Some(last) if m.at <= last.end + 1.0 => {
                if m.score > last.score {
                    last.score = m.score;
                    last.reason = m.reason;
                }
                last.end = last.end.max(m.end);
            }
            _ => out.push(m),
        }
    }
    out.sort_by(|a, b| b.score.total_cmp(&a.score));
    out.truncate(MAX_MOMENTS);
    out.sort_by(|a, b| a.at.total_cmp(&b.at));
    for m in &mut out {
        m.score = (m.score * 100.0).round() / 100.0;
    }
    out
}

/// Analysis jobs running right now. Stays 0 for anyone who never turns the
/// feature on; the Performance page shows it.
static ACTIVE: AtomicUsize = AtomicUsize::new(0);
/// One clip at a time, whoever asked.
static ONE_AT_A_TIME: parking_lot::Mutex<()> = parking_lot::Mutex::new(());

pub fn active_jobs() -> usize {
    ACTIVE.load(Ordering::Relaxed)
}

struct Running;

impl Running {
    fn start() -> Self {
        ACTIVE.fetch_add(1, Ordering::Relaxed);
        Running
    }
}

impl Drop for Running {
    fn drop(&mut self) {
        ACTIVE.fetch_sub(1, Ordering::Relaxed);
    }
}

/// Decode every audio track of a clip and find its moments. Each track is
/// analysed on its own, so a mic track's shouting isn't drowned by the game.
#[cfg(windows)]
pub fn analyze_file(path: &Path, threshold_db: f32) -> anyhow::Result<Vec<Moment>> {
    let _running = Running::start();
    let _one = ONE_AT_A_TIME.lock();
    let mut tracks: Vec<(u32, Analyzer)> = Vec::new();
    crate::export::mf::decode_audio(path, &mut |stream, rate, channels, pcm| {
        let a = match tracks.iter().position(|(s, _)| *s == stream) {
            Some(i) => &mut tracks[i].1,
            None => {
                tracks.push((stream, Analyzer::new(rate)));
                &mut tracks.last_mut().unwrap().1
            }
        };
        a.push_i16(pcm, channels as usize);
    })?;
    Ok(merge(tracks.into_iter().flat_map(|(_, a)| a.finish(threshold_db)).collect()))
}

/// Analyse and store the result in `library.json`.
#[cfg(windows)]
pub fn analyze_and_store(path: &Path, threshold_db: f32) -> anyhow::Result<Vec<Moment>> {
    let moments = analyze_file(path, threshold_db)?;
    crate::library::set_highlights(path, Some(moments.clone()))?;
    Ok(moments)
}

/// The analysis switch, checked before anything else.
pub fn enabled(s: &Settings) -> bool {
    s.smart_highlights.enabled
}

/// Called after every save. Returns the analysis thread when one was
/// started, which only happens with both the feature and "Analyze on save" on.
pub fn on_clip_saved(s: &Settings, path: &Path, done: impl FnOnce() + Send + 'static) -> Option<JoinHandle<()>> {
    if !(enabled(s) && s.smart_highlights.analyze_on_save) {
        return None;
    }
    let path: PathBuf = path.to_path_buf();
    let threshold = s.smart_highlights.threshold_db;
    std::thread::Builder::new()
        .name("rewind-analysis".into())
        .spawn(move || {
            #[cfg(windows)]
            {
                crate::platform::com_init();
                match analyze_and_store(&path, threshold) {
                    Ok(_) => done(),
                    Err(e) => tracing::warn!("analyze {}: {e:#}", path.display()),
                }
            }
            #[cfg(not(windows))]
            let _ = (path, threshold, done);
        })
        .ok()
}

#[cfg(test)]
mod tests {
    use super::*;

    const RATE: u32 = 48_000;

    fn noise(secs: f64, amp: f32, seed: &mut u32) -> Vec<f32> {
        (0..(secs * RATE as f64) as usize)
            .map(|_| {
                *seed = seed.wrapping_mul(1_664_525).wrapping_add(1_013_904_223);
                (*seed as f32 / u32::MAX as f32 * 2.0 - 1.0) * amp
            })
            .collect()
    }

    fn tone(secs: f64, hz: f32, amp: f32) -> Vec<f32> {
        (0..(secs * RATE as f64) as usize).map(|i| (i as f32 * hz * std::f32::consts::TAU / RATE as f32).sin() * amp).collect()
    }

    fn run(parts: &[Vec<f32>], threshold: f32) -> Vec<Moment> {
        let mut a = Analyzer::new(RATE);
        for p in parts {
            a.push(p);
        }
        a.finish(threshold)
    }

    #[test]
    fn quiet_clip_has_nothing() {
        let mut seed = 1;
        assert!(run(&[noise(10.0, 0.01, &mut seed)], -14.0).is_empty());
    }

    #[test]
    fn finds_a_spike() {
        let mut seed = 7;
        let m = run(&[noise(4.0, 0.01, &mut seed), noise(0.3, 0.9, &mut seed), noise(4.0, 0.01, &mut seed)], -14.0);
        assert_eq!(m.len(), 1, "{m:?}");
        assert_eq!(m[0].reason, Reason::Spike);
        assert!((m[0].at - 4.0).abs() < 0.1, "{m:?}");
    }

    #[test]
    fn finds_sustained_loudness() {
        let mut seed = 3;
        let m = run(&[noise(2.0, 0.01, &mut seed), noise(5.0, 0.5, &mut seed), noise(2.0, 0.01, &mut seed)], -14.0);
        // The onset is also a spike; merged, the moment covers the whole stretch.
        assert_eq!(m.len(), 1, "{m:?}");
        assert!(m[0].end - m[0].at >= 4.5, "{m:?}");
        // Loud from the first second: no onset, so it's purely sustained.
        let m = run(&[noise(5.0, 0.5, &mut seed), noise(2.0, 0.01, &mut seed)], -14.0);
        assert!(m.iter().any(|m| m.reason == Reason::Sustained), "{m:?}");
    }

    #[test]
    fn finds_excited_voice() {
        let mut seed = 5;
        // A shout-like 1 kHz burst, too quiet for the spike threshold.
        let m = run(&[noise(3.0, 0.003, &mut seed), tone(0.8, 1000.0, 0.15), noise(3.0, 0.003, &mut seed)], -6.0);
        assert_eq!(m.len(), 1, "{m:?}");
        assert_eq!(m[0].reason, Reason::Voice);
        // Broadband noise of the same level is not a voice.
        let m = run(&[noise(3.0, 0.003, &mut seed), noise(0.8, 0.26, &mut seed), noise(3.0, 0.003, &mut seed)], -6.0);
        assert!(m.iter().all(|m| m.reason != Reason::Voice), "{m:?}");
    }

    #[test]
    fn merges_close_moments() {
        let a = Moment { at: 1.0, end: 1.5, score: 0.4, reason: Reason::Spike };
        let b = Moment { at: 2.0, end: 3.0, score: 0.9, reason: Reason::Voice };
        let c = Moment { at: 9.0, end: 9.5, score: 0.5, reason: Reason::Spike };
        let m = merge(vec![c.clone(), b, a]);
        assert_eq!(m.len(), 2);
        assert_eq!((m[0].reason, m[0].end, m[0].score), (Reason::Voice, 3.0, 0.9));
        assert_eq!(m[1], c);
    }

    /// Zero impact when disabled: with default settings a save starts no
    /// analysis thread and no job is counted.
    #[test]
    fn off_by_default_spawns_nothing() {
        let s = Settings::default();
        assert!(!enabled(&s));
        assert!(on_clip_saved(&s, Path::new("C:/x.mp4"), || {}).is_none());
        // On, but manual only (the default): still nothing on save.
        let mut s = Settings::default();
        s.smart_highlights.enabled = true;
        assert!(on_clip_saved(&s, Path::new("C:/x.mp4"), || {}).is_none());
        assert_eq!(active_jobs(), 0);
    }
}
