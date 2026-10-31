//! Rolling replay buffer of already-encoded packets. Raw frames never land here.
//!
//! Video is kept as whole GOPs: the oldest packet is always a keyframe, so any
//! snapshot can be muxed without re-encoding. Audio tracks are trimmed to the
//! video window.

use std::collections::VecDeque;

use bytes::Bytes;

/// All timestamps are in 100 ns units on the QPC clock (same clock WGC and
/// WASAPI report), so video and audio line up without extra sync work.
pub const TICKS_PER_SEC: i64 = 10_000_000;

#[derive(Debug, Clone)]
pub struct Packet {
    pub pts: i64,
    pub duration: i64,
    pub keyframe: bool,
    /// Video: length-prefixed (4-byte) NAL units. Audio: one raw AAC frame.
    pub data: Bytes,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum VideoCodecConfig {
    H264 { sps: Vec<u8>, pps: Vec<u8> },
    Hevc { vps: Vec<u8>, sps: Vec<u8>, pps: Vec<u8> },
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct VideoConfig {
    pub width: u32,
    pub height: u32,
    pub fps: u32,
    pub codec: VideoCodecConfig,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct AudioConfig {
    pub name: String,
    pub sample_rate: u32,
    pub channels: u16,
    pub bitrate: u32,
    /// AudioSpecificConfig for the esds box.
    pub asc: Vec<u8>,
}

#[derive(Default)]
struct Track {
    packets: VecDeque<Packet>,
    bytes: usize,
}

impl Track {
    fn push(&mut self, p: Packet) {
        self.bytes += p.data.len();
        self.packets.push_back(p);
    }

    fn pop(&mut self) -> Option<Packet> {
        let p = self.packets.pop_front()?;
        self.bytes -= p.data.len();
        Some(p)
    }

    fn last_end(&self) -> Option<i64> {
        self.packets.back().map(|p| p.pts + p.duration)
    }
}

/// A self-contained copy of part of the buffer, ready to mux.
#[derive(Debug, Clone)]
pub struct Snapshot {
    pub video_config: VideoConfig,
    pub video: Vec<Packet>,
    pub audio: Vec<(AudioConfig, Vec<Packet>)>,
}

impl Snapshot {
    pub fn start(&self) -> i64 {
        self.video.first().map(|p| p.pts).unwrap_or(0)
    }
    pub fn end(&self) -> i64 {
        self.video.last().map(|p| p.pts + p.duration).unwrap_or(0)
    }
}

pub struct ReplayBuffer {
    max_ticks: i64,
    max_bytes: usize,
    video_config: Option<VideoConfig>,
    video: Track,
    audio: Vec<(AudioConfig, Track)>,
}

impl ReplayBuffer {
    pub fn new(max_secs: u32, max_bytes: usize) -> Self {
        Self {
            max_ticks: max_secs as i64 * TICKS_PER_SEC,
            max_bytes,
            video_config: None,
            video: Track::default(),
            audio: Vec::new(),
        }
    }

    pub fn set_video_config(&mut self, cfg: VideoConfig) {
        if self.video_config.as_ref() != Some(&cfg) {
            // New parameter sets: old packets can't share a sample entry.
            self.video = Track::default();
            self.video_config = Some(cfg);
        }
    }

    pub fn video_config(&self) -> Option<&VideoConfig> {
        self.video_config.as_ref()
    }

    /// Register an audio track and return its index.
    pub fn add_audio_track(&mut self, cfg: AudioConfig) -> usize {
        self.audio.push((cfg, Track::default()));
        self.audio.len() - 1
    }

    pub fn push_video(&mut self, p: Packet) {
        if self.video.packets.is_empty() && !p.keyframe {
            return; // a buffer must start on a keyframe
        }
        self.video.push(p);
        self.trim();
    }

    pub fn push_audio(&mut self, track: usize, p: Packet) {
        let max = self.max_ticks + 2 * TICKS_PER_SEC;
        if let Some((_, t)) = self.audio.get_mut(track) {
            t.push(p);
            // Bound audio on its own too, for when video is paused.
            let end = t.last_end().unwrap_or(0);
            while t.packets.front().is_some_and(|f| end - f.pts > max) {
                t.pop();
            }
        }
    }

    pub fn clear(&mut self) {
        self.video = Track::default();
        for (_, t) in &mut self.audio {
            *t = Track::default();
        }
    }

    pub fn total_bytes(&self) -> usize {
        self.video.bytes + self.audio.iter().map(|(_, t)| t.bytes).sum::<usize>()
    }

    /// Seconds of video currently held.
    pub fn buffered_ticks(&self) -> i64 {
        match (self.video.packets.front(), self.video.last_end()) {
            (Some(f), Some(e)) => e - f.pts,
            _ => 0,
        }
    }

    pub fn latest_pts(&self) -> Option<i64> {
        self.video.last_end()
    }

    /// First keyframe at or after `t`.
    pub fn keyframe_at_or_after(&self, t: i64) -> Option<i64> {
        self.video.packets.iter().find(|p| p.keyframe && p.pts >= t).map(|p| p.pts)
    }

    /// Timestamp of the oldest video frame held.
    pub fn earliest_pts(&self) -> Option<i64> {
        self.video.packets.front().map(|p| p.pts)
    }

    /// Drop whole GOPs from the front while the remainder still covers the window
    /// (or while over the memory cap).
    fn trim(&mut self) {
        let Some(end) = self.video.last_end() else { return };
        loop {
            let over_time = self.video.packets.front().is_some_and(|f| end - f.pts > self.max_ticks);
            let over_mem = self.total_bytes() > self.max_bytes;
            if !over_time && !over_mem {
                break;
            }
            // Next keyframe after the front one.
            let Some(next_kf) = self.video.packets.iter().skip(1).position(|p| p.keyframe).map(|i| i + 1) else {
                break; // only one GOP left: keep it
            };
            if over_time && !over_mem {
                // Only drop the GOP if the window is still covered without it.
                let next_pts = self.video.packets[next_kf].pts;
                if end - next_pts < self.max_ticks {
                    break;
                }
            }
            for _ in 0..next_kf {
                self.video.pop();
            }
        }
        let start = self.video.packets.front().map(|p| p.pts).unwrap_or(end);
        for (_, t) in &mut self.audio {
            while t.packets.front().is_some_and(|p| p.pts + p.duration < start) {
                t.pop();
            }
        }
    }

    /// Copy out `[from, to)`. `from` snaps back to the nearest keyframe at or
    /// before it so the clip decodes; returns `None` when there's no video yet.
    pub fn snapshot(&self, from: i64, to: i64) -> Option<Snapshot> {
        let pk = &self.video.packets;
        let start_idx = pk
            .iter()
            .rposition(|p| p.keyframe && p.pts <= from)
            .or_else(|| pk.iter().position(|p| p.keyframe))?;
        self.snapshot_from(start_idx, to)
    }

    /// Like [`snapshot`](Self::snapshot), but `from` snaps forward to the next
    /// keyframe so the clip never runs longer than `to - from`.
    pub fn snapshot_within(&self, from: i64, to: i64) -> Option<Snapshot> {
        let pk = &self.video.packets;
        let start_idx = pk
            .iter()
            .position(|p| p.keyframe && p.pts >= from && p.pts < to)
            .or_else(|| pk.iter().rposition(|p| p.keyframe && p.pts <= from))?;
        self.snapshot_from(start_idx, to)
    }

    fn snapshot_from(&self, start_idx: usize, to: i64) -> Option<Snapshot> {
        let cfg = self.video_config.clone()?;
        let pk = &self.video.packets;
        let video: Vec<Packet> = pk.iter().skip(start_idx).take_while(|p| p.pts < to).cloned().collect();
        if video.is_empty() {
            return None;
        }
        let v_start = video[0].pts;
        let v_end = video.last().map(|p| p.pts + p.duration).unwrap_or(v_start);
        let audio = self
            .audio
            .iter()
            .map(|(c, t)| {
                let pkts = t
                    .packets
                    .iter()
                    .filter(|p| p.pts + p.duration > v_start && p.pts < v_end)
                    .cloned()
                    .collect();
                (c.clone(), pkts)
            })
            .collect();
        Some(Snapshot { video_config: cfg, video, audio })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn cfg() -> VideoConfig {
        VideoConfig { width: 64, height: 64, fps: 10, codec: VideoCodecConfig::H264 { sps: vec![], pps: vec![] } }
    }

    fn fill(buf: &mut ReplayBuffer, secs: i64) {
        // 10 fps, keyframe every second
        for i in 0..secs * 10 {
            buf.push_video(Packet {
                pts: i * TICKS_PER_SEC / 10,
                duration: TICKS_PER_SEC / 10,
                keyframe: i % 10 == 0,
                data: Bytes::from_static(&[0; 100]),
            });
        }
    }

    #[test]
    fn keeps_window_and_starts_on_keyframe() {
        let mut b = ReplayBuffer::new(5, usize::MAX);
        b.set_video_config(cfg());
        fill(&mut b, 20);
        let held = b.buffered_ticks();
        assert!(held >= 5 * TICKS_PER_SEC && held < 6 * TICKS_PER_SEC, "{held}");
        assert!(b.video.packets.front().unwrap().keyframe);
    }

    #[test]
    fn memory_cap_drops_gops() {
        let mut b = ReplayBuffer::new(1000, 100 * 25);
        b.set_video_config(cfg());
        fill(&mut b, 20);
        assert!(b.total_bytes() <= 100 * 25 || b.buffered_ticks() <= TICKS_PER_SEC);
    }

    #[test]
    fn snapshot_snaps_to_keyframe() {
        let mut b = ReplayBuffer::new(30, usize::MAX);
        b.set_video_config(cfg());
        fill(&mut b, 10);
        let s = b.snapshot(35 * TICKS_PER_SEC / 10, 8 * TICKS_PER_SEC).unwrap();
        assert_eq!(s.start(), 3 * TICKS_PER_SEC);
        assert!(s.video[0].keyframe);
        assert_eq!(s.end(), 8 * TICKS_PER_SEC);
    }

    #[test]
    fn snapshot_within_never_exceeds_length() {
        // Buffer holds more than the clip length (slack); the clip must not.
        let mut b = ReplayBuffer::new(60, usize::MAX);
        b.set_video_config(cfg());
        fill(&mut b, 40);
        let to = 40 * TICKS_PER_SEC;
        let s = b.snapshot_within(to - 105 * TICKS_PER_SEC / 10, to).unwrap();
        assert!(s.video[0].keyframe);
        assert!(s.end() - s.start() <= 105 * TICKS_PER_SEC / 10, "{}", s.end() - s.start());
        assert_eq!(s.start(), 30 * TICKS_PER_SEC);
    }
}
