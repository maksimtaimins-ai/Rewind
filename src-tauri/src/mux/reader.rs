//! Reads back the fragmented MP4 files Rewind writes, so clips can be trimmed
//! and merged without re-encoding. Supports the subset of ISO-BMFF our writer
//! produces (plus the common tfhd/trun defaults).

use std::io::{Read, Seek, SeekFrom};
use std::path::Path;

use anyhow::{bail, Context, Result};
use bytes::Bytes;

use super::fmp4::{Handler, MuxSample, MuxTrack};

struct Cursor<'a> {
    b: &'a [u8],
    p: usize,
}

impl<'a> Cursor<'a> {
    fn new(b: &'a [u8]) -> Self {
        Self { b, p: 0 }
    }
    fn take(&mut self, n: usize) -> Result<&'a [u8]> {
        if self.p + n > self.b.len() {
            bail!("truncated box");
        }
        let s = &self.b[self.p..self.p + n];
        self.p += n;
        Ok(s)
    }
    fn u8(&mut self) -> Result<u8> {
        Ok(self.take(1)?[0])
    }
    fn u32(&mut self) -> Result<u32> {
        Ok(u32::from_be_bytes(self.take(4)?.try_into()?))
    }
    fn u64(&mut self) -> Result<u64> {
        Ok(u64::from_be_bytes(self.take(8)?.try_into()?))
    }
    fn skip(&mut self, n: usize) -> Result<()> {
        self.take(n).map(|_| ())
    }
}

/// Iterate child boxes of a container payload: (fourcc, payload).
fn children(b: &[u8]) -> Vec<([u8; 4], &[u8])> {
    let mut out = Vec::new();
    let mut p = 0;
    while p + 8 <= b.len() {
        let mut size = u32::from_be_bytes(b[p..p + 4].try_into().unwrap()) as usize;
        let fourcc: [u8; 4] = b[p + 4..p + 8].try_into().unwrap();
        let mut hdr = 8;
        if size == 1 && p + 16 <= b.len() {
            size = u64::from_be_bytes(b[p + 8..p + 16].try_into().unwrap()) as usize;
            hdr = 16;
        } else if size == 0 {
            size = b.len() - p;
        }
        if size < hdr || p + size > b.len() {
            break;
        }
        out.push((fourcc, &b[p + hdr..p + size]));
        p += size;
    }
    out
}

fn child<'a>(b: &'a [u8], name: &[u8; 4]) -> Option<&'a [u8]> {
    children(b).into_iter().find(|(f, _)| f == name).map(|(_, p)| p)
}

#[derive(Default, Clone, Copy)]
struct Trex {
    duration: u32,
    size: u32,
    flags: u32,
}

/// A track's samples as (file offset, length) before the bytes are loaded.
struct PendingSample {
    dts: u64,
    duration: u32,
    cts_offset: i32,
    keyframe: bool,
    offset: u64,
    size: u32,
}

pub fn read_file(path: &Path) -> Result<Vec<MuxTrack>> {
    let mut f = std::fs::File::open(path).with_context(|| format!("open {}", path.display()))?;
    let len = f.metadata()?.len();
    let mut tracks: Vec<MuxTrack> = Vec::new();
    let mut ids: Vec<u32> = Vec::new();
    let mut trex: Vec<Trex> = Vec::new();
    let mut pending: Vec<Vec<PendingSample>> = Vec::new();
    let mut next_dts: Vec<u64> = Vec::new();

    let mut pos = 0u64;
    while pos + 8 <= len {
        f.seek(SeekFrom::Start(pos))?;
        let mut hdr = [0u8; 16];
        f.read_exact(&mut hdr[..8])?;
        let mut size = u32::from_be_bytes(hdr[..4].try_into()?) as u64;
        let fourcc: [u8; 4] = hdr[4..8].try_into()?;
        let mut hdr_len = 8;
        if size == 1 {
            f.read_exact(&mut hdr[8..16])?;
            size = u64::from_be_bytes(hdr[8..16].try_into()?);
            hdr_len = 16;
        } else if size == 0 {
            size = len - pos;
        }
        if size < hdr_len || pos + size > len {
            break;
        }
        match &fourcc {
            b"moov" | b"moof" => {
                let mut body = vec![0u8; (size - hdr_len) as usize];
                f.read_exact(&mut body)?;
                if &fourcc == b"moov" {
                    parse_moov(&body, &mut tracks, &mut ids, &mut trex)?;
                    pending = (0..tracks.len()).map(|_| Vec::new()).collect();
                    next_dts = vec![0; tracks.len()];
                } else {
                    parse_moof(&body, pos, &ids, &trex, &mut pending, &mut next_dts)?;
                }
            }
            _ => {}
        }
        pos += size;
    }
    if tracks.is_empty() {
        bail!("not a Rewind MP4 (no moov)");
    }

    for (t, list) in pending.into_iter().enumerate() {
        let mut samples = Vec::with_capacity(list.len());
        for s in list {
            f.seek(SeekFrom::Start(s.offset))?;
            let mut data = vec![0u8; s.size as usize];
            f.read_exact(&mut data)?;
            samples.push(MuxSample {
                dts: s.dts,
                duration: s.duration,
                cts_offset: s.cts_offset,
                keyframe: s.keyframe,
                data: Bytes::from(data),
            });
        }
        tracks[t].samples = samples;
    }
    // Video first, as the writer expects.
    tracks.sort_by_key(|t| t.handler != Handler::Video);
    Ok(tracks)
}

fn parse_moov(body: &[u8], tracks: &mut Vec<MuxTrack>, ids: &mut Vec<u32>, trex: &mut Vec<Trex>) -> Result<()> {
    for (f, trak) in children(body) {
        if &f != b"trak" {
            continue;
        }
        let tkhd = child(trak, b"tkhd").context("tkhd")?;
        let mut c = Cursor::new(tkhd);
        let version = c.u8()?;
        c.skip(3)?;
        c.skip(if version == 1 { 16 } else { 8 })?;
        let id = c.u32()?;
        let (w, h) = if tkhd.len() >= 8 {
            let t = &tkhd[tkhd.len() - 8..];
            (u32::from_be_bytes(t[..4].try_into()?) >> 16, u32::from_be_bytes(t[4..].try_into()?) >> 16)
        } else {
            (0, 0)
        };
        let mdia = child(trak, b"mdia").context("mdia")?;
        let mdhd = child(mdia, b"mdhd").context("mdhd")?;
        let mut c = Cursor::new(mdhd);
        let version = c.u8()?;
        c.skip(3)?;
        c.skip(if version == 1 { 16 } else { 8 })?;
        let timescale = c.u32()?;
        let hdlr = child(mdia, b"hdlr").context("hdlr")?;
        let handler = match &hdlr[8..12] {
            b"vide" => Handler::Video,
            b"soun" => Handler::Audio,
            _ => continue,
        };
        let name = hdlr.get(24..).map(|n| String::from_utf8_lossy(n).trim_end_matches('\0').to_string()).unwrap_or_default();
        let stsd = child(mdia, b"minf").and_then(|m| child(m, b"stbl")).and_then(|s| child(s, b"stsd")).context("stsd")?;
        // stsd: version/flags + count, then the raw entry box we copy verbatim.
        let entry = stsd.get(8..).context("stsd entry")?.to_vec();
        ids.push(id);
        tracks.push(MuxTrack { handler, name, timescale, sample_entry: entry, width: w, height: h, samples: Vec::new() });
        trex.push(Trex::default());
    }
    if let Some(mvex) = child(body, b"mvex") {
        for (f, b) in children(mvex) {
            if &f == b"trex" {
                let mut c = Cursor::new(b);
                c.skip(4)?;
                let id = c.u32()?;
                c.skip(4)?;
                let t = Trex { duration: c.u32()?, size: c.u32()?, flags: c.u32()? };
                if let Some(i) = ids.iter().position(|&x| x == id) {
                    trex[i] = t;
                }
            }
        }
    }
    Ok(())
}

fn parse_moof(
    body: &[u8],
    moof_pos: u64,
    ids: &[u32],
    trex: &[Trex],
    pending: &mut [Vec<PendingSample>],
    next_dts: &mut [u64],
) -> Result<()> {
    for (f, traf) in children(body) {
        if &f != b"traf" {
            continue;
        }
        let tfhd = child(traf, b"tfhd").context("tfhd")?;
        let mut c = Cursor::new(tfhd);
        let flags = u32::from_be_bytes(c.take(4)?.try_into()?) & 0xff_ffff;
        let id = c.u32()?;
        let Some(t) = ids.iter().position(|&x| x == id) else { continue };
        let mut base = moof_pos;
        if flags & 0x1 != 0 {
            base = c.u64()?;
        }
        if flags & 0x2 != 0 {
            c.skip(4)?;
        }
        let def_dur = if flags & 0x8 != 0 { c.u32()? } else { trex[t].duration };
        let def_size = if flags & 0x10 != 0 { c.u32()? } else { trex[t].size };
        let def_flags = if flags & 0x20 != 0 { c.u32()? } else { trex[t].flags };

        let mut dts = next_dts[t];
        if let Some(tfdt) = child(traf, b"tfdt") {
            let mut c = Cursor::new(tfdt);
            let v = c.u8()?;
            c.skip(3)?;
            dts = if v == 1 { c.u64()? } else { c.u32()? as u64 };
        }
        let mut data_pos = base;
        for (f, trun) in children(traf) {
            if &f != b"trun" {
                continue;
            }
            let mut c = Cursor::new(trun);
            let tf = u32::from_be_bytes(c.take(4)?.try_into()?);
            let tf = tf & 0xff_ffff;
            let count = c.u32()?;
            if tf & 0x1 != 0 {
                data_pos = base.wrapping_add_signed(c.u32()? as i32 as i64);
            }
            let first_flags = if tf & 0x4 != 0 { Some(c.u32()?) } else { None };
            for i in 0..count {
                let dur = if tf & 0x100 != 0 { c.u32()? } else { def_dur };
                let size = if tf & 0x200 != 0 { c.u32()? } else { def_size };
                let mut sflags = if tf & 0x400 != 0 { c.u32()? } else { def_flags };
                if i == 0 {
                    if let Some(ff) = first_flags {
                        sflags = ff;
                    }
                }
                // v0 offsets are unsigned, v1 signed; ours never exceed i32 either way.
                let cts = if tf & 0x800 != 0 { c.u32()? as i32 } else { 0 };
                pending[t].push(PendingSample {
                    dts,
                    duration: dur,
                    cts_offset: cts,
                    keyframe: sflags & 0x0001_0000 == 0,
                    offset: data_pos,
                    size,
                });
                dts += dur as u64;
                data_pos += size as u64;
            }
        }
        next_dts[t] = dts;
    }
    Ok(())
}
