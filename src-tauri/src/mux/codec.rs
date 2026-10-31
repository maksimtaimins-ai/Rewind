//! Bitstream helpers: Annex B parsing and the codec configuration records
//! (avcC / hvcC / esds) that go into MP4 sample entries.

use super::boxes::BoxWriter;
use crate::buffer::{AudioConfig, VideoCodecConfig, VideoConfig};

/// Split an Annex B stream (00 00 01 / 00 00 00 01 start codes) into NAL units.
pub fn split_annexb(data: &[u8]) -> Vec<&[u8]> {
    let mut nals = Vec::new();
    let mut i = 0;
    let mut start: Option<usize> = None;
    while i + 2 < data.len() {
        if data[i] == 0 && data[i + 1] == 0 && data[i + 2] == 1 {
            if let Some(s) = start {
                let mut end = i;
                // A 4-byte start code's leading zero belongs to the start code.
                while end > s && data[end - 1] == 0 {
                    end -= 1;
                }
                nals.push(&data[s..end]);
            }
            i += 3;
            start = Some(i);
        } else {
            i += 1;
        }
    }
    if let Some(s) = start {
        if s < data.len() {
            nals.push(&data[s..]);
        }
    } else if !data.is_empty() {
        nals.push(data); // no start codes: treat as one NAL
    }
    nals
}

/// Remove emulation-prevention bytes (00 00 03 -> 00 00).
pub fn unescape_rbsp(nal: &[u8]) -> Vec<u8> {
    let mut out = Vec::with_capacity(nal.len());
    let mut zeros = 0;
    for &b in nal {
        if zeros >= 2 && b == 3 {
            zeros = 0;
            continue;
        }
        zeros = if b == 0 { zeros + 1 } else { 0 };
        out.push(b);
    }
    out
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum NalCodec {
    H264,
    Hevc,
}

pub fn nal_type(codec: NalCodec, nal: &[u8]) -> u8 {
    match codec {
        NalCodec::H264 => nal.first().map(|b| b & 0x1f).unwrap_or(0),
        NalCodec::Hevc => nal.first().map(|b| (b >> 1) & 0x3f).unwrap_or(0),
    }
}

/// Encoder output after splitting: sample payload (length-prefixed, without
/// parameter sets / AUDs) plus any parameter sets found in it.
#[derive(Default, Debug)]
pub struct AccessUnit {
    pub sample: Vec<u8>,
    pub vps: Option<Vec<u8>>,
    pub sps: Option<Vec<u8>>,
    pub pps: Option<Vec<u8>>,
    pub has_idr: bool,
}

pub fn annexb_to_sample(codec: NalCodec, data: &[u8]) -> AccessUnit {
    let mut au = AccessUnit::default();
    for nal in split_annexb(data) {
        if nal.is_empty() {
            continue;
        }
        let t = nal_type(codec, nal);
        match (codec, t) {
            (NalCodec::H264, 7) | (NalCodec::Hevc, 33) => au.sps = Some(nal.to_vec()),
            (NalCodec::H264, 8) | (NalCodec::Hevc, 34) => au.pps = Some(nal.to_vec()),
            (NalCodec::Hevc, 32) => au.vps = Some(nal.to_vec()),
            (NalCodec::H264, 9) | (NalCodec::Hevc, 35) => {} // access unit delimiter
            _ => {
                if matches!((codec, t), (NalCodec::H264, 5) | (NalCodec::Hevc, 16..=21)) {
                    au.has_idr = true;
                }
                au.sample.extend_from_slice(&(nal.len() as u32).to_be_bytes());
                au.sample.extend_from_slice(nal);
            }
        }
    }
    au
}

fn avcc(w: &mut BoxWriter, sps: &[u8], pps: &[u8]) {
    w.begin(b"avcC");
    let profile = sps.get(1).copied().unwrap_or(100);
    w.u8(1);
    w.u8(profile);
    w.u8(sps.get(2).copied().unwrap_or(0));
    w.u8(sps.get(3).copied().unwrap_or(40));
    w.u8(0xff); // lengthSizeMinusOne = 3
    w.u8(0xe1); // 1 SPS
    w.u16(sps.len() as u16);
    w.bytes(sps);
    w.u8(1);
    w.u16(pps.len() as u16);
    w.bytes(pps);
    if matches!(profile, 100 | 110 | 122 | 244) {
        w.u8(0xfc | 1); // chroma_format 4:2:0
        w.u8(0xf8); // bit_depth_luma_minus8 = 0
        w.u8(0xf8); // bit_depth_chroma_minus8 = 0
        w.u8(0); // no SPS extensions
    }
    w.end();
}

fn hvcc(w: &mut BoxWriter, vps: &[u8], sps: &[u8], pps: &[u8]) {
    let rbsp = unescape_rbsp(sps);
    // profile_tier_level starts after the 2-byte NAL header and one byte of
    // vps_id / max_sub_layers / temporal_id_nesting.
    let ptl = |i: usize| rbsp.get(3 + i).copied().unwrap_or(0);
    w.begin(b"hvcC");
    w.u8(1);
    w.u8(ptl(0)); // profile_space, tier, profile_idc
    for i in 1..5 {
        w.u8(ptl(i)); // compatibility flags
    }
    for i in 5..11 {
        w.u8(ptl(i)); // constraint flags
    }
    w.u8(ptl(11)); // level_idc
    w.u16(0xf000); // min_spatial_segmentation_idc = 0
    w.u8(0xfc); // parallelismType = 0
    w.u8(0xfd); // chroma 4:2:0
    w.u8(0xf8); // 8-bit luma
    w.u8(0xf8); // 8-bit chroma
    w.u16(0); // avgFrameRate
    w.u8((1 << 3) | (1 << 2) | 3); // 1 temporal layer, nested, 4-byte lengths
    w.u8(3);
    for (t, nal) in [(32u8, vps), (33, sps), (34, pps)] {
        w.u8(0x80 | t);
        w.u16(1);
        w.u16(nal.len() as u16);
        w.bytes(nal);
    }
    w.end();
}

fn colr_bt709(w: &mut BoxWriter) {
    w.begin(b"colr");
    w.bytes(b"nclx");
    w.u16(1); // BT.709 primaries
    w.u16(1); // BT.709 transfer
    w.u16(1); // BT.709 matrix
    w.u8(0); // limited range
    w.end();
}

/// Serialized `avc1` / `hvc1` sample entry.
pub fn video_sample_entry(cfg: &VideoConfig) -> Vec<u8> {
    let mut w = BoxWriter::new();
    let fourcc = match cfg.codec {
        VideoCodecConfig::H264 { .. } => b"avc1",
        VideoCodecConfig::Hevc { .. } => b"hvc1",
    };
    w.begin(fourcc);
    w.zeros(6);
    w.u16(1); // data_reference_index
    w.zeros(16);
    w.u16(cfg.width as u16);
    w.u16(cfg.height as u16);
    w.u32(0x0048_0000);
    w.u32(0x0048_0000);
    w.u32(0);
    w.u16(1); // frame_count
    let mut name = [0u8; 32];
    let label = b"Rewind";
    name[0] = label.len() as u8;
    name[1..1 + label.len()].copy_from_slice(label);
    w.bytes(&name);
    w.u16(0x0018);
    w.u16(0xffff);
    match &cfg.codec {
        VideoCodecConfig::H264 { sps, pps } => avcc(&mut w, sps, pps),
        VideoCodecConfig::Hevc { vps, sps, pps } => hvcc(&mut w, vps, sps, pps),
    }
    colr_bt709(&mut w);
    w.end();
    w.buf
}

/// AudioSpecificConfig for AAC-LC.
pub fn aac_asc(sample_rate: u32, channels: u16) -> Vec<u8> {
    const RATES: [u32; 13] = [96000, 88200, 64000, 48000, 44100, 32000, 24000, 22050, 16000, 12000, 11025, 8000, 7350];
    let idx = RATES.iter().position(|&r| r == sample_rate).unwrap_or(3) as u16;
    let v: u16 = (2 << 11) | (idx << 7) | ((channels & 0xf) << 3);
    v.to_be_bytes().to_vec()
}

fn descriptor(w: &mut BoxWriter, tag: u8, body: &[u8]) {
    w.u8(tag);
    w.u8(body.len() as u8);
    w.bytes(body);
}

/// Serialized `mp4a` sample entry with esds.
pub fn audio_sample_entry(cfg: &AudioConfig) -> Vec<u8> {
    let mut w = BoxWriter::new();
    w.begin(b"mp4a");
    w.zeros(6);
    w.u16(1);
    w.zeros(8);
    w.u16(cfg.channels);
    w.u16(16);
    w.u32(0);
    w.u32(cfg.sample_rate << 16);

    let mut dsi = BoxWriter::new();
    descriptor(&mut dsi, 0x05, &cfg.asc);
    let mut dcd = BoxWriter::new();
    dcd.u8(0x40); // MPEG-4 audio
    dcd.u8(0x15); // audio stream
    dcd.u24(0);
    dcd.u32(cfg.bitrate);
    dcd.u32(cfg.bitrate);
    dcd.bytes(&dsi.buf);
    let mut es = BoxWriter::new();
    es.u16(0); // ES_ID
    es.u8(0);
    descriptor(&mut es, 0x04, &dcd.buf);
    descriptor(&mut es, 0x06, &[0x02]);

    w.begin_full(b"esds", 0, 0);
    descriptor(&mut w, 0x03, &es.buf);
    w.end();
    w.end();
    w.buf
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn splits_mixed_start_codes() {
        let d = [0, 0, 0, 1, 0x67, 1, 2, 0, 0, 1, 0x68, 3, 0, 0, 0, 1, 0x65, 9];
        let n = split_annexb(&d);
        assert_eq!(n, vec![&[0x67, 1, 2][..], &[0x68, 3], &[0x65, 9]]);
    }

    #[test]
    fn extracts_parameter_sets() {
        let d = [0, 0, 0, 1, 0x09, 0xf0, 0, 0, 0, 1, 0x67, 1, 0, 0, 0, 1, 0x68, 2, 0, 0, 0, 1, 0x65, 9, 9];
        let au = annexb_to_sample(NalCodec::H264, &d);
        assert_eq!(au.sps.unwrap(), vec![0x67, 1]);
        assert_eq!(au.pps.unwrap(), vec![0x68, 2]);
        assert!(au.has_idr);
        assert_eq!(au.sample, vec![0, 0, 0, 3, 0x65, 9, 9]);
    }

    #[test]
    fn unescapes() {
        assert_eq!(unescape_rbsp(&[1, 0, 0, 3, 1, 0, 0, 3]), vec![1, 0, 0, 1, 0, 0]);
    }

    #[test]
    fn asc_48k_stereo() {
        assert_eq!(aac_asc(48000, 2), vec![0x11, 0x90]);
    }
}
