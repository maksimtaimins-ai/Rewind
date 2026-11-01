//! Montage: several clips joined into one MP4, with hard cuts or short
//! crossfades and optional music. Runs only when the user asks; nothing here
//! exists in the background. Planning and audio mixing are portable; the
//! decode/encode pass is `export::mf::montage`.

use std::path::PathBuf;
use std::sync::atomic::{AtomicUsize, Ordering};

use serde::Deserialize;

/// Output audio: 48 kHz stereo 16-bit.
pub const RATE: u32 = 48_000;
pub const CROSSFADE_SECS: f64 = 0.5;
/// A montage never runs longer than this.
pub const MAX_TOTAL_SECS: f64 = 600.0;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Transition {
    Cut,
    Crossfade,
}

#[derive(Debug, Clone, Deserialize)]
pub struct Request {
    /// In playing order.
    pub paths: Vec<PathBuf>,
    pub transition: Transition,
    /// Longest piece taken from each clip, seconds. `None`: whole clips.
    pub max_secs: Option<f64>,
    /// mp3 or wav played under the montage.
    pub music: Option<PathBuf>,
    /// Keep the clips' own sound (lowered under music).
    pub clip_audio: bool,
}

/// The part of a source clip used, and where it lands in the montage.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Segment {
    /// Index into the request's clips.
    pub clip: usize,
    pub src_start: f64,
    pub len: f64,
    pub out_start: f64,
    /// Overlap with the previous segment, seconds (0 for the first, or cuts).
    pub fade_in: f64,
}

/// Which part of a clip to use when it's longer than `max`: around its best
/// smart-highlight moment if it has one, else the end, since a replay clip
/// ends at the moment the hotkey was pressed for.
pub fn window(duration: f64, max: Option<f64>, best_moment: Option<f64>) -> (f64, f64) {
    let len = max.map_or(duration, |m| m.min(duration)).max(0.0);
    let start = match best_moment {
        Some(at) => (at - len * 0.6).clamp(0.0, duration - len),
        None => duration - len,
    };
    (start.max(0.0), len)
}

/// Lay pieces `(src_start, len)` end to end, overlapping by the crossfade.
/// The last piece is shortened if the total would pass [`MAX_TOTAL_SECS`].
pub fn plan(pieces: &[(f64, f64)], transition: Transition) -> Vec<Segment> {
    let mut out: Vec<Segment> = Vec::new();
    let mut t = 0.0;
    for (clip, &(src_start, len)) in pieces.iter().enumerate() {
        if len <= 0.05 || t >= MAX_TOTAL_SECS {
            continue;
        }
        let fade_in = match (transition, out.last()) {
            (Transition::Crossfade, Some(prev)) => CROSSFADE_SECS.min(prev.len / 2.0).min(len / 2.0),
            _ => 0.0,
        };
        let out_start = (t - fade_in).max(0.0);
        let len = len.min(MAX_TOTAL_SECS - out_start);
        out.push(Segment { clip, src_start, len, out_start, fade_in });
        t = out_start + len;
    }
    out
}

pub fn total(segs: &[Segment]) -> f64 {
    segs.last().map_or(0.0, |s| s.out_start + s.len)
}

/// The segments showing at `t` with their weights (a crossfade has two).
pub fn weights_at(segs: &[Segment], t: f64) -> Vec<(usize, f32)> {
    let mut v: Vec<(usize, f32)> = segs.iter().enumerate().filter(|(_, s)| t >= s.out_start && t < s.out_start + s.len).map(|(i, _)| (i, 1.0)).collect();
    if v.len() == 2 {
        let s = &segs[v[1].0];
        let k = (((t - s.out_start) / s.fade_in.max(1e-6)) as f32).clamp(0.0, 1.0);
        v[0].1 = 1.0 - k;
        v[1].1 = k;
    }
    v
}

/// Any interleaved 16-bit PCM to 48 kHz stereo, linear interpolation.
pub fn to_stereo_48k(pcm: &[i16], rate: u32, channels: u32) -> Vec<i16> {
    let ch = channels.max(1) as usize;
    let frames = pcm.len() / ch;
    let frame = |i: usize| -> (f32, f32) {
        let f = &pcm[i * ch..i * ch + ch];
        if ch == 1 { (f[0] as f32, f[0] as f32) } else { (f[0] as f32, f[1] as f32) }
    };
    if frames == 0 {
        return Vec::new();
    }
    if rate == RATE && ch == 2 {
        return pcm[..frames * 2].to_vec();
    }
    let out_frames = (frames as u64 * RATE as u64 / rate.max(1) as u64) as usize;
    let step = rate as f64 / RATE as f64;
    let mut out = Vec::with_capacity(out_frames * 2);
    for i in 0..out_frames {
        let pos = i as f64 * step;
        let a = (pos as usize).min(frames - 1);
        let b = (a + 1).min(frames - 1);
        let k = (pos - a as f64) as f32;
        let (la, ra) = frame(a);
        let (lb, rb) = frame(b);
        out.push((la + (lb - la) * k) as i16);
        out.push((ra + (rb - ra) * k) as i16);
    }
    out
}

/// Mix each segment's audio (48 kHz stereo, already cut to the segment) and
/// the music into one track as long as the montage. Crossfades ramp both
/// sides; music fades out over the last 1.5 s.
pub fn mix(segs: &[Segment], clips: &[Vec<i16>], music: Option<&[i16]>, clip_audio: bool) -> Vec<i16> {
    let total_frames = (total(segs) * RATE as f64).round() as usize;
    let mut acc = vec![0f32; total_frames * 2];
    let clip_gain = if music.is_some() { 0.7 } else { 1.0 };
    if clip_audio {
        for (i, (s, pcm)) in segs.iter().zip(clips).enumerate() {
            let start = (s.out_start * RATE as f64).round() as usize;
            let frames = ((s.len * RATE as f64) as usize).min(pcm.len() / 2);
            let fade_in = (s.fade_in * RATE as f64) as usize;
            let fade_out = segs.get(i + 1).map_or(0, |n| (n.fade_in * RATE as f64) as usize);
            for f in 0..frames {
                let mut g = clip_gain;
                if f < fade_in {
                    g *= f as f32 / fade_in as f32;
                }
                if fade_out > 0 && f + fade_out >= frames {
                    g *= (frames - f) as f32 / fade_out as f32;
                }
                let o = (start + f) * 2;
                if o + 1 < acc.len() {
                    acc[o] += pcm[f * 2] as f32 * g;
                    acc[o + 1] += pcm[f * 2 + 1] as f32 * g;
                }
            }
        }
    }
    if let Some(m) = music {
        let gain = if clip_audio { 0.55 } else { 1.0 };
        let tail = (1.5 * RATE as f64) as usize;
        let frames = (m.len() / 2).min(total_frames);
        for f in 0..frames {
            let left = total_frames - f;
            let g = if left < tail { gain * left as f32 / tail as f32 } else { gain };
            acc[f * 2] += m[f * 2] as f32 * g;
            acc[f * 2 + 1] += m[f * 2 + 1] as f32 * g;
        }
    }
    acc.into_iter().map(|v| v.clamp(-32768.0, 32767.0) as i16).collect()
}

/// Montages being made right now (one at most). 0 unless the user started one.
static ACTIVE: AtomicUsize = AtomicUsize::new(0);

pub fn active_jobs() -> usize {
    ACTIVE.load(Ordering::Relaxed)
}

/// Held while a montage is made; refuses a second one.
pub struct Job(());

impl Job {
    pub fn start() -> Option<Job> {
        ACTIVE.compare_exchange(0, 1, Ordering::SeqCst, Ordering::SeqCst).ok().map(|_| Job(()))
    }
}

impl Drop for Job {
    fn drop(&mut self) {
        ACTIVE.store(0, Ordering::SeqCst);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn windows_pick_the_moment() {
        assert_eq!(window(60.0, None, None), (0.0, 60.0));
        // No highlight: the end, where the hotkey was pressed.
        assert_eq!(window(60.0, Some(10.0), None), (50.0, 10.0));
        // Around the best moment, a little more before it than after.
        assert_eq!(window(60.0, Some(10.0), Some(20.0)), (14.0, 10.0));
        assert_eq!(window(60.0, Some(10.0), Some(1.0)), (0.0, 10.0));
        assert_eq!(window(8.0, Some(10.0), None), (0.0, 8.0));
    }

    #[test]
    fn plan_overlaps_crossfades() {
        let cut = plan(&[(0.0, 10.0), (5.0, 4.0)], Transition::Cut);
        assert_eq!(cut[1].out_start, 10.0);
        assert_eq!(total(&cut), 14.0);
        let xf = plan(&[(0.0, 10.0), (5.0, 4.0), (0.0, 0.6)], Transition::Crossfade);
        assert_eq!((xf[1].out_start, xf[1].fade_in), (9.5, 0.5));
        // A tiny clip gets a shorter fade.
        assert_eq!(xf[2].fade_in, 0.3);
        assert!((total(&xf) - (13.5 - 0.3 + 0.6)).abs() < 1e-9);
        // Halfway through a crossfade both clips weigh the same.
        let w = weights_at(&xf, 9.75);
        assert_eq!((w.len(), w[0].1), (2, 0.5));
        assert_eq!(weights_at(&xf, 2.0), vec![(0, 1.0)]);
        // Capped length.
        assert!(total(&plan(&[(0.0, 400.0), (0.0, 400.0)], Transition::Cut)) <= MAX_TOTAL_SECS);
    }

    #[test]
    fn resamples_and_mixes() {
        let mono = vec![1000i16; 44_100];
        let st = to_stereo_48k(&mono, 44_100, 1);
        assert_eq!(st.len(), 96_000);
        assert!(st.iter().all(|&s| s == 1000));
        let segs = plan(&[(0.0, 1.0), (0.0, 1.0)], Transition::Crossfade);
        let clips = vec![vec![1000i16; 96_000]; 2];
        let out = mix(&segs, &clips, None, true);
        assert_eq!(out.len(), (1.5 * RATE as f64) as usize * 2);
        // Mid-crossfade the two fades sum back to full level.
        let mid = (0.75 * RATE as f64) as usize * 2;
        assert!((out[mid] as i32 - 1000).abs() <= 2, "{}", out[mid]);
        // Music alone, clip sound off: music at full gain, fading at the end.
        let music = vec![2000i16; 4 * 96_000];
        let out = mix(&segs, &clips, Some(&music), false);
        assert_eq!(out[0], 2000);
        assert!(out[out.len() - 2].abs() < 10);
    }

    #[test]
    fn one_at_a_time_and_idle_by_default() {
        assert_eq!(active_jobs(), 0);
        let j = Job::start().unwrap();
        assert!(Job::start().is_none());
        drop(j);
        assert_eq!(active_jobs(), 0);
    }
}
