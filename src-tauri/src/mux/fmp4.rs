//! Fragmented MP4 writer: `ftyp` + `moov` (empty sample tables + `mvex`), one
//! `moof`/`mdat` pair per video GOP, and an `mfra` index at the end so players
//! can seek.

use std::io::{BufWriter, Write};
use std::path::Path;

use anyhow::{ensure, Context, Result};
use bytes::Bytes;

use super::boxes::BoxWriter;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Handler {
    Video,
    Audio,
}

#[derive(Debug, Clone)]
pub struct MuxSample {
    /// Decode time in the track timescale, relative to the clip start.
    pub dts: u64,
    pub duration: u32,
    pub cts_offset: i32,
    pub keyframe: bool,
    pub data: Bytes,
}

#[derive(Debug, Clone)]
pub struct MuxTrack {
    pub handler: Handler,
    /// Shown by editors (Premiere/DaVinci/OBS) as the track name.
    pub name: String,
    pub timescale: u32,
    /// Serialized sample entry box (`avc1`, `hvc1`, `mp4a`).
    pub sample_entry: Vec<u8>,
    pub width: u32,
    pub height: u32,
    pub samples: Vec<MuxSample>,
}

impl MuxTrack {
    fn duration(&self) -> u64 {
        self.samples.last().map(|s| s.dts + s.duration as u64).unwrap_or(0)
    }
}

const MOVIE_TIMESCALE: u32 = 1000;
const FLAGS_SYNC: u32 = 0x0200_0000;
const FLAGS_NON_SYNC: u32 = 0x0101_0000;

/// Write `tracks` to `path` (via `<path>.partial`, renamed on success).
/// Track 0 must be the video track; fragments are cut at its keyframes.
pub fn write_file(path: &Path, tracks: &[MuxTrack]) -> Result<u64> {
    let partial = path.with_extension("mp4.partial");
    let file = std::fs::File::create(&partial).with_context(|| format!("create {}", partial.display()))?;
    let mut out = BufWriter::with_capacity(1 << 20, file);
    let written = write(&mut out, tracks)?;
    out.flush()?;
    out.into_inner().map_err(|e| e.into_error())?.sync_all()?;
    std::fs::rename(&partial, path)?;
    Ok(written)
}

pub fn write<W: Write>(out: &mut W, tracks: &[MuxTrack]) -> Result<u64> {
    ensure!(!tracks.is_empty() && tracks[0].handler == Handler::Video, "first track must be video");
    ensure!(!tracks[0].samples.is_empty(), "no video samples");

    let mut pos: u64 = 0;
    let init = init_segment(tracks);
    out.write_all(&init)?;
    pos += init.len() as u64;

    // Fragment boundaries in seconds, from video keyframes.
    let v = &tracks[0];
    let vts = v.timescale as f64;
    let mut cuts: Vec<f64> = v.samples.iter().filter(|s| s.keyframe).map(|s| s.dts as f64 / vts).collect();
    if cuts.first().copied() != Some(v.samples[0].dts as f64 / vts) {
        cuts.insert(0, v.samples[0].dts as f64 / vts);
    }

    let mut cursors = vec![0usize; tracks.len()];
    let mut index: Vec<(u64, u64)> = Vec::new(); // (video dts, moof offset)
    for (seq, i) in (0..cuts.len()).enumerate() {
        let end = cuts.get(i + 1).copied();
        let mut ranges = Vec::with_capacity(tracks.len());
        for (t, track) in tracks.iter().enumerate() {
            let ts = track.timescale as f64;
            let begin = cursors[t];
            let mut j = begin;
            while j < track.samples.len() && end.is_none_or(|e| (track.samples[j].dts as f64 / ts) < e) {
                j += 1;
            }
            cursors[t] = j;
            ranges.push(begin..j);
        }
        if ranges.iter().all(|r| r.is_empty()) {
            continue;
        }
        if !ranges[0].is_empty() {
            index.push((v.samples[ranges[0].start].dts, pos));
        }
        let (moof, payload_len) = moof(seq as u32 + 1, tracks, &ranges);
        out.write_all(&moof)?;
        let mdat_size = 8 + payload_len;
        out.write_all(&(mdat_size as u32).to_be_bytes())?;
        out.write_all(b"mdat")?;
        for (t, r) in ranges.iter().enumerate() {
            for s in &tracks[t].samples[r.clone()] {
                out.write_all(&s.data)?;
            }
        }
        pos += moof.len() as u64 + mdat_size;
    }

    let mfra = mfra(&index);
    out.write_all(&mfra)?;
    pos += mfra.len() as u64;
    Ok(pos)
}

fn init_segment(tracks: &[MuxTrack]) -> Vec<u8> {
    let mut w = BoxWriter::new();
    w.begin(b"ftyp");
    w.bytes(b"isom");
    w.u32(0x200);
    for b in [b"isom", b"iso6", b"mp41", b"avc1"] {
        w.bytes(b);
    }
    w.end();

    let movie_dur = tracks
        .iter()
        .map(|t| t.duration() * MOVIE_TIMESCALE as u64 / t.timescale as u64)
        .max()
        .unwrap_or(0);

    w.begin(b"moov");
    w.begin_full(b"mvhd", 0, 0);
    w.u32(0);
    w.u32(0);
    w.u32(MOVIE_TIMESCALE);
    w.u32(0); // fragmented: real duration lives in mehd
    w.u32(0x0001_0000);
    w.u16(0x0100);
    w.zeros(10);
    w.matrix();
    w.zeros(24);
    w.u32(tracks.len() as u32 + 1);
    w.end();

    let mut first_audio = true;
    for (i, t) in tracks.iter().enumerate() {
        let id = i as u32 + 1;
        w.begin(b"trak");
        // Only the first audio track is enabled for playback in players; editors
        // still import all of them.
        let enabled = t.handler == Handler::Video || first_audio;
        w.begin_full(b"tkhd", 0, if enabled { 3 } else { 2 });
        w.u32(0);
        w.u32(0);
        w.u32(id);
        w.u32(0);
        w.u32(0);
        w.zeros(8);
        w.u16(0); // layer
        w.u16(if t.handler == Handler::Audio { 1 } else { 0 }); // alternate group
        w.u16(if t.handler == Handler::Audio { 0x0100 } else { 0 });
        w.u16(0);
        w.matrix();
        w.u32(t.width << 16);
        w.u32(t.height << 16);
        w.end();
        if t.handler == Handler::Audio {
            first_audio = false;
        }

        w.begin(b"mdia");
        w.begin_full(b"mdhd", 0, 0);
        w.u32(0);
        w.u32(0);
        w.u32(t.timescale);
        w.u32(0);
        w.u16(0x55c4); // "und"
        w.u16(0);
        w.end();
        w.begin_full(b"hdlr", 0, 0);
        w.u32(0);
        w.bytes(if t.handler == Handler::Video { b"vide" } else { b"soun" });
        w.zeros(12);
        w.bytes(t.name.as_bytes());
        w.u8(0);
        w.end();

        w.begin(b"minf");
        match t.handler {
            Handler::Video => {
                w.begin_full(b"vmhd", 0, 1);
                w.zeros(8);
                w.end();
            }
            Handler::Audio => {
                w.begin_full(b"smhd", 0, 0);
                w.u32(0);
                w.end();
            }
        }
        w.begin(b"dinf");
        w.begin_full(b"dref", 0, 0);
        w.u32(1);
        w.begin_full(b"url ", 0, 1);
        w.end();
        w.end();
        w.end();
        w.begin(b"stbl");
        w.begin_full(b"stsd", 0, 0);
        w.u32(1);
        w.bytes(&t.sample_entry);
        w.end();
        for empty in [b"stts", b"stsc", b"stco"] {
            w.begin_full(empty, 0, 0);
            w.u32(0);
            w.end();
        }
        w.begin_full(b"stsz", 0, 0);
        w.u32(0);
        w.u32(0);
        w.end();
        w.end(); // stbl
        w.end(); // minf
        w.end(); // mdia
        w.end(); // trak
    }

    w.begin(b"mvex");
    w.begin_full(b"mehd", 1, 0);
    w.u64(movie_dur);
    w.end();
    for i in 0..tracks.len() {
        w.begin_full(b"trex", 0, 0);
        w.u32(i as u32 + 1);
        w.u32(1);
        w.u32(0);
        w.u32(0);
        w.u32(0);
        w.end();
    }
    w.end();
    w.end(); // moov
    w.buf
}

/// Build one `moof`. Returns the box and the size of the mdat payload that
/// must follow it (track 0's samples, then track 1's, ...).
fn moof(seq: u32, tracks: &[MuxTrack], ranges: &[std::ops::Range<usize>]) -> (Vec<u8>, u64) {
    let mut w = BoxWriter::new();
    w.begin(b"moof");
    w.begin_full(b"mfhd", 0, 0);
    w.u32(seq);
    w.end();

    let mut patches = Vec::new(); // (position of data_offset field, payload offset)
    let mut payload: u64 = 0;
    for (t, r) in ranges.iter().enumerate() {
        if r.is_empty() {
            continue;
        }
        let track = &tracks[t];
        let samples = &track.samples[r.clone()];
        let has_cts = samples.iter().any(|s| s.cts_offset != 0);
        w.begin(b"traf");
        w.begin_full(b"tfhd", 0, 0x02_0000); // default-base-is-moof
        w.u32(t as u32 + 1);
        w.end();
        w.begin_full(b"tfdt", 1, 0);
        w.u64(samples[0].dts);
        w.end();
        let flags = 0x1 | 0x100 | 0x200 | 0x400 | if has_cts { 0x800 } else { 0 };
        w.begin_full(b"trun", if has_cts { 1 } else { 0 }, flags);
        w.u32(samples.len() as u32);
        patches.push((w.pos(), payload));
        w.i32(0);
        for s in samples {
            w.u32(s.duration);
            w.u32(s.data.len() as u32);
            w.u32(if s.keyframe { FLAGS_SYNC } else { FLAGS_NON_SYNC });
            if has_cts {
                w.i32(s.cts_offset);
            }
            payload += s.data.len() as u64;
        }
        w.end();
        w.end();
    }
    w.end();
    let moof_len = w.buf.len() as u64;
    for (at, off) in patches {
        w.patch_u32(at, (moof_len + 8 + off) as u32);
    }
    (w.buf, payload)
}

fn mfra(index: &[(u64, u64)]) -> Vec<u8> {
    let mut w = BoxWriter::new();
    w.begin(b"mfra");
    w.begin_full(b"tfra", 1, 0);
    w.u32(1); // video track
    w.u32(0); // 1-byte traf/trun/sample numbers
    w.u32(index.len() as u32);
    for &(time, offset) in index {
        w.u64(time);
        w.u64(offset);
        w.u8(1);
        w.u8(1);
        w.u8(1);
    }
    w.end();
    w.begin_full(b"mfro", 0, 0);
    let size_at = w.pos();
    w.u32(0);
    w.end();
    w.end();
    let total = w.buf.len() as u32;
    w.patch_u32(size_at, total);
    w.buf
}
