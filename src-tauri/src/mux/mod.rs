//! MP4 muxing: snapshot -> fragmented MP4, plus keyframe-aligned trim/merge on
//! existing clips (no re-encode).

pub mod boxes;
pub mod codec;
pub mod fmp4;
pub mod reader;

use std::path::Path;

use anyhow::{ensure, Result};

use crate::buffer::{Snapshot, TICKS_PER_SEC};
use fmp4::{Handler, MuxSample, MuxTrack};

pub const VIDEO_TIMESCALE: u32 = 90_000;

fn to_ts(ticks: i64, timescale: u32) -> u64 {
    (ticks.max(0) as i128 * timescale as i128 / TICKS_PER_SEC as i128) as u64
}

/// Convert a buffer snapshot into mux tracks. Times are rebased so the first
/// video frame is at 0; durations come from timestamp deltas so frame pacing
/// (VRR, dropped frames, static screens) is preserved exactly.
pub fn snapshot_to_tracks(s: &Snapshot) -> Vec<MuxTrack> {
    let start = s.start();
    let mut tracks = Vec::with_capacity(1 + s.audio.len());

    let vc = &s.video_config;
    let dts: Vec<u64> = s.video.iter().map(|p| to_ts(p.pts - start, VIDEO_TIMESCALE)).collect();
    let samples = s
        .video
        .iter()
        .enumerate()
        .map(|(i, p)| {
            let next = dts.get(i + 1).copied().unwrap_or_else(|| to_ts(p.pts + p.duration - start, VIDEO_TIMESCALE));
            MuxSample {
                dts: dts[i],
                duration: next.saturating_sub(dts[i]).max(1) as u32,
                cts_offset: 0,
                keyframe: p.keyframe,
                data: p.data.clone(),
            }
        })
        .collect();
    tracks.push(MuxTrack {
        handler: Handler::Video,
        name: "Video".into(),
        timescale: VIDEO_TIMESCALE,
        sample_entry: codec::video_sample_entry(vc),
        width: vc.width,
        height: vc.height,
        samples,
    });

    for (cfg, packets) in &s.audio {
        let ts = cfg.sample_rate;
        let mut samples: Vec<MuxSample> = Vec::with_capacity(packets.len());
        for p in packets.iter().filter(|p| p.pts >= start) {
            let dts = to_ts(p.pts - start, ts);
            let duration = to_ts(p.duration, ts) as u32;
            // AAC frames are contiguous; snap tiny clock jitter onto the previous end.
            let dts = match samples.last() {
                Some(prev) => {
                    let prev_end = prev.dts + prev.duration as u64;
                    if dts.abs_diff(prev_end) < duration as u64 / 2 { prev_end } else { dts.max(prev_end) }
                }
                None => dts,
            };
            samples.push(MuxSample { dts, duration, cts_offset: 0, keyframe: true, data: p.data.clone() });
        }
        tracks.push(MuxTrack {
            handler: Handler::Audio,
            name: cfg.name.clone(),
            timescale: ts,
            sample_entry: codec::audio_sample_entry(cfg),
            width: 0,
            height: 0,
            samples,
        });
    }
    tracks
}

pub fn write_snapshot(path: &Path, s: &Snapshot) -> Result<u64> {
    fmp4::write_file(path, &snapshot_to_tracks(s))
}

/// Clip duration in seconds.
pub fn duration_secs(tracks: &[MuxTrack]) -> f64 {
    tracks
        .first()
        .and_then(|v| v.samples.last().map(|s| (s.dts + s.duration as u64) as f64 / v.timescale as f64))
        .unwrap_or(0.0)
}

/// Video keyframe times in seconds (for snapping trim markers in the UI).
pub fn keyframe_times(tracks: &[MuxTrack]) -> Vec<f64> {
    let Some(v) = tracks.first() else { return Vec::new() };
    v.samples.iter().filter(|s| s.keyframe).map(|s| s.dts as f64 / v.timescale as f64).collect()
}

/// Keyframe-aligned cut: starts at the last keyframe at or before `from`,
/// ends at `to`. Returns the actual start time used.
pub fn trim(tracks: &[MuxTrack], from: f64, to: f64) -> Result<(Vec<MuxTrack>, f64)> {
    ensure!(to > from, "out point must be after in point");
    let v = &tracks[0];
    let vts = v.timescale as f64;
    let start = v
        .samples
        .iter()
        .filter(|s| s.keyframe && s.dts as f64 / vts <= from + 1e-6)
        .map(|s| s.dts as f64 / vts)
        .last()
        .unwrap_or(0.0);

    let out = tracks
        .iter()
        .map(|t| {
            let ts = t.timescale as f64;
            let base = (start * ts).round() as u64;
            let samples = t
                .samples
                .iter()
                .filter(|s| {
                    let sec = s.dts as f64 / ts;
                    sec + 1e-6 >= start && sec < to
                })
                .map(|s| MuxSample { dts: s.dts - base.min(s.dts), ..s.clone() })
                .collect();
            MuxTrack { samples, ..t.clone() }
        })
        .collect::<Vec<_>>();
    ensure!(!out[0].samples.is_empty(), "selection contains no video");
    Ok((out, start))
}

/// Stitch clips end to end. All inputs must share codec parameters (true for
/// clips recorded with the same settings).
pub fn merge(clips: &[Vec<MuxTrack>]) -> Result<Vec<MuxTrack>> {
    ensure!(clips.len() >= 2, "need at least two clips");
    let first = &clips[0];
    for c in &clips[1..] {
        ensure!(c.len() == first.len(), "clips have different track layouts");
        for (a, b) in c.iter().zip(first) {
            ensure!(a.sample_entry == b.sample_entry, "clips were recorded with different encoder settings");
        }
    }
    let mut out: Vec<MuxTrack> = first.iter().map(|t| MuxTrack { samples: Vec::new(), ..t.clone() }).collect();
    let mut offset_secs = 0.0;
    for clip in clips {
        let len = duration_secs(clip);
        for (t, track) in clip.iter().enumerate() {
            let base = (offset_secs * track.timescale as f64).round() as u64;
            out[t].samples.extend(track.samples.iter().map(|s| MuxSample { dts: s.dts + base, ..s.clone() }));
        }
        offset_secs += len;
    }
    Ok(out)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::buffer::*;
    use bytes::Bytes;

    fn snapshot() -> Snapshot {
        let video = (0..30)
            .map(|i| Packet {
                pts: 1_000_000 + i * TICKS_PER_SEC / 10,
                duration: TICKS_PER_SEC / 10,
                keyframe: i % 10 == 0,
                data: Bytes::from(vec![0, 0, 0, 2, 0x65, i as u8]),
            })
            .collect();
        let audio = (0..140)
            .map(|i| Packet {
                pts: 1_000_000 + i * 1024 * TICKS_PER_SEC / 48000,
                duration: 1024 * TICKS_PER_SEC / 48000,
                keyframe: true,
                data: Bytes::from(vec![0x21, i as u8]),
            })
            .collect();
        Snapshot {
            video_config: VideoConfig {
                width: 1920,
                height: 1080,
                fps: 10,
                codec: VideoCodecConfig::H264 { sps: vec![0x67, 100, 0, 40], pps: vec![0x68, 1] },
            },
            video,
            audio: vec![(
                AudioConfig { name: "Desktop".into(), sample_rate: 48000, channels: 2, bitrate: 160_000, asc: codec::aac_asc(48000, 2) },
                audio,
            )],
        }
    }

    #[test]
    fn write_then_read_roundtrips() {
        let dir = std::env::temp_dir().join(format!("rewind-test-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        let path = dir.join("clip.mp4");
        let snap = snapshot();
        write_snapshot(&path, &snap).unwrap();
        let tracks = reader::read_file(&path).unwrap();
        assert_eq!(tracks.len(), 2);
        assert_eq!(tracks[0].samples.len(), 30);
        assert_eq!(tracks[1].name, "Desktop");
        assert_eq!(tracks[0].samples[5].data.as_ref(), &[0, 0, 0, 2, 0x65, 5]);
        assert!((duration_secs(&tracks) - 3.0).abs() < 0.01);
        assert_eq!(keyframe_times(&tracks), vec![0.0, 1.0, 2.0]);

        let (cut, start) = trim(&tracks, 1.5, 2.5).unwrap();
        assert_eq!(start, 1.0);
        assert_eq!(cut[0].samples.len(), 15);
        assert_eq!(cut[0].samples[0].dts, 0);
        assert!(cut[0].samples[0].keyframe);

        let merged = merge(&[tracks.clone(), tracks]).unwrap();
        assert_eq!(merged[0].samples.len(), 60);
        assert!((duration_secs(&merged) - 6.0).abs() < 0.01);
        std::fs::remove_dir_all(&dir).ok();
    }
}
