//! Export presets: re-encode a clip for a destination (Discord size caps,
//! YouTube Shorts, X, GIF). Planning, scaling and GIF timing are portable; the
//! decode/encode itself is Media Foundation and WIC (`export::mf`).

use std::path::{Path, PathBuf};

use anyhow::{bail, ensure, Result};
use serde::{Deserialize, Serialize};

#[cfg(windows)]
pub mod mf;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Preset {
    Discord10,
    Discord25,
    Shorts,
    Twitter,
    Gif,
}

impl Preset {
    pub fn suffix(self) -> &'static str {
        match self {
            Preset::Discord10 => "discord10",
            Preset::Discord25 => "discord25",
            Preset::Shorts => "shorts",
            Preset::Twitter => "twitter",
            Preset::Gif => "gif",
        }
    }

    pub fn extension(self) -> &'static str {
        if self == Preset::Gif { "gif" } else { "mp4" }
    }
}

/// Source pixels to use: x, y, width, height.
pub type Rect = (u32, u32, u32, u32);

/// Everything the encoder needs, decided up front from the preset and source.
#[derive(Debug, Clone, PartialEq)]
pub struct Plan {
    pub width: u32,
    pub height: u32,
    pub crop: Rect,
    /// Seconds of the clip to export, from the start.
    pub secs: f64,
    pub fps: u32,
    pub video_bps: u32,
    /// 0 = no audio.
    pub audio_bps: u32,
    /// Output must be at most this many bytes (Discord).
    pub size_limit: Option<u64>,
}

const MB: u64 = 1024 * 1024;

fn even(v: f64) -> u32 {
    ((v / 2.0).round() as u32 * 2).max(2)
}

/// Scale `(w, h)` down to fit `max_h` lines (and `max_w`), never up.
fn fit(w: u32, h: u32, max_w: u32, max_h: u32) -> (u32, u32) {
    let k = (max_w as f64 / w as f64).min(max_h as f64 / h as f64).min(1.0);
    (even(w as f64 * k), even(h as f64 * k))
}

pub fn plan(preset: Preset, src_w: u32, src_h: u32, src_fps: u32, duration: f64) -> Result<Plan> {
    ensure!(src_w > 0 && src_h > 0 && duration > 0.0, "clip has no video");
    let full = (0, 0, src_w, src_h);
    let fps = src_fps.clamp(1, 60);
    Ok(match preset {
        Preset::Discord10 | Preset::Discord25 => {
            let limit = if preset == Preset::Discord10 { 10 * MB } else { 25 * MB };
            let audio_bps = if preset == Preset::Discord10 { 96_000 } else { 128_000 };
            // Keep ~6% for the container and encoder overshoot.
            let budget = limit as f64 * 8.0 * 0.94;
            // Below ~500 kb/s of video the picture falls apart: cut the clip short instead.
            let secs = duration.min(budget / (500_000.0 + audio_bps as f64));
            let video_bps = (budget / secs - audio_bps as f64) as u32;
            let max_h = match video_bps {
                b if b >= 4_500_000 && preset == Preset::Discord25 => 1080,
                b if b >= 1_800_000 => 720,
                b if b >= 900_000 => 540,
                _ => 360,
            };
            let (width, height) = fit(src_w, src_h, 1920, max_h);
            let fps = if video_bps < 1_500_000 { fps.min(30) } else { fps };
            Plan { width, height, crop: full, secs, fps, video_bps, audio_bps, size_limit: Some(limit) }
        }
        Preset::Shorts => {
            // Centre 9:16 cut of the frame, at Shorts' native 1080x1920.
            let crop = if src_w * 16 > src_h * 9 {
                let w = (src_h as f64 * 9.0 / 16.0) as u32 & !1;
                ((src_w - w) / 2, 0, w, src_h)
            } else {
                let h = (src_w as f64 * 16.0 / 9.0) as u32 & !1;
                (0, (src_h - h) / 2, src_w, h)
            };
            Plan { width: 1080, height: 1920, crop, secs: duration.min(60.0), fps, video_bps: 12_000_000, audio_bps: 192_000, size_limit: None }
        }
        Preset::Twitter => {
            let (width, height) = fit(src_w, src_h, 1920, 1080);
            // 2:20 at 8 Mb/s is ~140 MB, far under X's 512 MB cap.
            Plan { width, height, crop: full, secs: duration.min(140.0), fps, video_bps: 8_000_000, audio_bps: 128_000, size_limit: Some(512 * MB) }
        }
        Preset::Gif => {
            let (width, height) = fit(src_w, src_h, 854, 480);
            Plan { width, height, crop: full, secs: duration.min(15.0), fps: 15, video_bps: 0, audio_bps: 0, size_limit: None }
        }
    })
}

/// `<dir>/<stem>-<suffix>.<ext>` next to the source, made unique.
pub fn output_path(src: &Path, preset: Preset) -> PathBuf {
    output_path_in(src, preset, src.parent().unwrap_or(Path::new(".")))
}

/// `<dir>/<stem>-<suffix>.<ext>`, made unique.
pub fn output_path_in(src: &Path, preset: Preset, dir: &Path) -> PathBuf {
    let stem = src.file_stem().unwrap_or_default().to_string_lossy();
    crate::library::unique_path(dir, &format!("{stem}-{}", preset.suffix()), preset.extension())
}

/// Separable resampling weights for one axis: for each output pixel, the first
/// source index and normalised weights. A tent filter as wide as the scale
/// factor: bilinear when enlarging, area-like averaging when shrinking.
fn axis_weights(src_len: u32, off: u32, span: u32, dst_len: u32) -> Vec<(usize, Vec<f32>)> {
    let scale = span as f64 / dst_len as f64;
    let support = scale.max(1.0);
    (0..dst_len)
        .map(|i| {
            let center = off as f64 + (i as f64 + 0.5) * scale - 0.5;
            // Stay inside the crop so nothing outside it bleeds in.
            let first = off as i64;
            let last = (off + span).min(src_len) as i64 - 1;
            let lo = ((center - support).floor() as i64).clamp(first, last) as usize;
            let hi = ((center + support).ceil() as i64).clamp(lo as i64, last) as usize;
            let mut w: Vec<f32> = (lo..=hi).map(|j| (1.0 - (j as f64 - center).abs() / support).max(0.0) as f32).collect();
            let sum: f32 = w.iter().sum();
            if sum > 0.0 {
                w.iter_mut().for_each(|x| *x /= sum);
            } else {
                w = vec![0.0; hi - lo + 1];
                let nearest = (center.round().max(0.0) as usize).clamp(lo, hi);
                w[nearest - lo] = 1.0;
            }
            (lo, w)
        })
        .collect()
}

/// Where to have the decoder scale the whole frame so that cutting `plan.crop`
/// out of it needs no further resampling: `(decode size, crop in that size)`.
pub fn prescale(p: &Plan, src_w: u32, src_h: u32) -> ((u32, u32), Rect) {
    let (cx, cy, cw, ch) = p.crop;
    let kx = p.width as f64 / cw as f64;
    let ky = p.height as f64 / ch as f64;
    let dw = even(src_w as f64 * kx).max(p.width);
    let dh = even(src_h as f64 * ky).max(p.height);
    let x = ((cx as f64 * kx).round() as u32).min(dw - p.width);
    let y = ((cy as f64 * ky).round() as u32).min(dh - p.height);
    ((dw, dh), (x, y, p.width, p.height))
}

/// Run `f(first_row, rows)` over `out` split into row bands, one per core.
fn par_rows(out: &mut [u8], row_bytes: usize, f: impl Fn(usize, &mut [u8]) + Sync) {
    let rows = out.len() / row_bytes.max(1);
    let threads = std::thread::available_parallelism().map(|n| n.get()).unwrap_or(4).min(rows.max(1));
    let per = rows.div_ceil(threads).max(1);
    std::thread::scope(|s| {
        for (i, band) in out.chunks_mut(per * row_bytes).enumerate() {
            let f = &f;
            s.spawn(move || f(i * per, band));
        }
    });
}

/// Crop then resize packed BGRA (`src_w * 4` bytes per row) to `dst_w x dst_h`.
pub fn scale_bgra(src: &[u8], src_w: u32, src_h: u32, crop: Rect, dst_w: u32, dst_h: u32) -> Vec<u8> {
    let (cx, cy, cw, ch) = crop;
    let stride = dst_w as usize * 4;
    let src_stride = src_w as usize * 4;
    let mut dst = vec![0u8; stride * dst_h as usize];
    if (cw, ch) == (dst_w, dst_h) {
        // Pure crop.
        for (y, row) in dst.chunks_exact_mut(stride).enumerate() {
            let s = (cy as usize + y) * src_stride + cx as usize * 4;
            row.copy_from_slice(&src[s..s + stride]);
        }
        return dst;
    }
    // 14-bit fixed-point weights.
    let fixed = |w: Vec<(usize, Vec<f32>)>| -> Vec<(usize, Vec<i32>)> {
        w.into_iter().map(|(lo, w)| (lo, w.into_iter().map(|x| (x * 16384.0).round() as i32).collect())).collect()
    };
    let xw = fixed(axis_weights(src_w, cx, cw, dst_w));
    let yw = fixed(axis_weights(src_h, cy, ch, dst_h));
    let row_lo = yw.iter().map(|(lo, _)| *lo).min().unwrap_or(0);
    let row_hi = yw.iter().map(|(lo, w)| lo + w.len()).max().unwrap_or(0).min(src_h as usize);
    // Horizontal pass over only the rows the vertical pass reads, kept at
    // 8.6 fixed point (u16) for the vertical pass.
    let mut mid = vec![0u8; (row_hi - row_lo) * stride * 2];
    par_rows(&mut mid, stride * 2, |first, band| {
        for (r, out) in band.chunks_exact_mut(stride * 2).enumerate() {
            let y = row_lo + first + r;
            let line = &src[y * src_stride..(y + 1) * src_stride];
            for (x, (lo, w)) in xw.iter().enumerate() {
                let mut acc = [0i32; 4];
                for (k, &wk) in w.iter().enumerate() {
                    let p = &line[(lo + k) * 4..(lo + k) * 4 + 4];
                    for c in 0..4 {
                        acc[c] += p[c] as i32 * wk;
                    }
                }
                for c in 0..4 {
                    let v = (acc[c] >> 8).clamp(0, 255 << 6) as u16;
                    out[(x * 4 + c) * 2..(x * 4 + c) * 2 + 2].copy_from_slice(&v.to_ne_bytes());
                }
            }
        }
    });
    let mid16 = |row: usize, i: usize| u16::from_ne_bytes([mid[(row * stride + i) * 2], mid[(row * stride + i) * 2 + 1]]) as i64;
    par_rows(&mut dst, stride, |first, band| {
        for (r, out) in band.chunks_exact_mut(stride).enumerate() {
            let (lo, w) = &yw[first + r];
            for (i, o) in out.iter_mut().enumerate() {
                let mut acc = 0i64;
                for (k, &wk) in w.iter().enumerate() {
                    acc += mid16(lo + k - row_lo, i) * wk as i64;
                }
                *o = ((acc + (1 << 19)) >> 20).clamp(0, 255) as u8;
            }
        }
    });
    dst
}

/// Rewrite a GIF to play at `fps` and loop forever (WIC writes frames without
/// timing). Delays are whole hundredths, so they alternate (7, 7, 6 at 15 fps)
/// to keep the real speed.
pub fn gif_set_timing(gif: &[u8], fps: u32) -> Result<Vec<u8>> {
    let mut frame = 0u32;
    ensure!(gif.len() >= 13 && (gif.starts_with(b"GIF89a") || gif.starts_with(b"GIF87a")), "not a GIF");
    let mut out = Vec::with_capacity(gif.len() + 64);
    out.extend_from_slice(b"GIF89a");
    out.extend_from_slice(&gif[6..13]);
    let mut p = 13;
    let flags = gif[10];
    if flags & 0x80 != 0 {
        let n = 3 << ((flags & 7) + 1);
        ensure!(gif.len() >= p + n, "truncated GIF");
        out.extend_from_slice(&gif[p..p + n]);
        p += n;
    }
    // NETSCAPE2.0 application extension: loop count 0 = forever.
    out.extend_from_slice(&[0x21, 0xFF, 0x0B]);
    out.extend_from_slice(b"NETSCAPE2.0");
    out.extend_from_slice(&[0x03, 0x01, 0x00, 0x00, 0x00]);

    fn sub_blocks(gif: &[u8], mut p: usize) -> Result<usize> {
        loop {
            let Some(&n) = gif.get(p) else { bail!("truncated GIF") };
            p += 1 + n as usize;
            if n == 0 {
                return Ok(p);
            }
        }
    }

    loop {
        match gif.get(p) {
            Some(0x21) => {
                let label = *gif.get(p + 1).ok_or_else(|| anyhow::anyhow!("truncated GIF"))?;
                let end = sub_blocks(gif, p + 2)?;
                // Drop existing graphic control and NETSCAPE blocks; ours replace them.
                let netscape = label == 0xFF && gif.get(p + 3..p + 14) == Some(b"NETSCAPE2.0");
                if label != 0xF9 && !netscape {
                    out.extend_from_slice(&gif[p..end]);
                }
                p = end;
            }
            Some(0x2C) => {
                ensure!(gif.len() >= p + 10, "truncated GIF");
                let at = |n: u32| (n as f64 * 100.0 / fps.max(1) as f64).round() as u16;
                let [lo, hi] = (at(frame + 1) - at(frame)).max(2).to_le_bytes();
                frame += 1;
                out.extend_from_slice(&[0x21, 0xF9, 0x04, 0x00, lo, hi, 0x00, 0x00]);
                let lflags = gif[p + 9];
                let mut q = p + 10;
                if lflags & 0x80 != 0 {
                    q += 3 << ((lflags & 7) + 1);
                }
                q += 1; // LZW minimum code size
                let end = sub_blocks(gif, q)?;
                out.extend_from_slice(&gif[p..end]);
                p = end;
            }
            Some(0x3B) => {
                out.push(0x3B);
                return Ok(out);
            }
            Some(b) => bail!("unexpected GIF block 0x{b:02x}"),
            None => bail!("truncated GIF"),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn discord_fits_budget() {
        let p = plan(Preset::Discord10, 1920, 1080, 60, 30.0).unwrap();
        let bytes = (p.video_bps + p.audio_bps) as f64 / 8.0 * p.secs;
        assert!(bytes <= (10 * MB) as f64 * 0.95, "{bytes}");
        assert!(p.height <= 720 && p.width % 2 == 0 && p.height % 2 == 0);
        // A long clip is cut rather than starved of bitrate.
        let long = plan(Preset::Discord10, 1920, 1080, 60, 300.0).unwrap();
        assert!(long.secs < 300.0 && long.video_bps >= 500_000);
    }

    #[test]
    fn shorts_crops_centre_9_16() {
        let p = plan(Preset::Shorts, 1920, 1080, 60, 90.0).unwrap();
        assert_eq!((p.width, p.height), (1080, 1920));
        assert_eq!(p.crop, (657, 0, 606, 1080));
        assert_eq!(p.secs, 60.0);
    }

    #[test]
    fn gif_and_twitter_caps() {
        let g = plan(Preset::Gif, 2560, 1440, 144, 40.0).unwrap();
        assert_eq!((g.width, g.height, g.fps, g.secs), (854, 480, 15, 15.0));
        let t = plan(Preset::Twitter, 1280, 720, 60, 200.0).unwrap();
        assert_eq!((t.width, t.height, t.secs), (1280, 720, 140.0));
    }

    #[test]
    fn scale_averages_and_crops() {
        // 4x2 image: left half black, right half white. Crop the right half.
        let mut src = vec![0u8; 4 * 2 * 4];
        for y in 0..2 {
            for x in 2..4 {
                src[(y * 4 + x) * 4..(y * 4 + x) * 4 + 4].copy_from_slice(&[255; 4]);
            }
        }
        assert_eq!(scale_bgra(&src, 4, 2, (2, 0, 2, 2), 1, 1), vec![255; 4]);
        let half = scale_bgra(&src, 4, 2, (0, 0, 4, 2), 2, 1);
        assert!(half[0] < 64 && half[4] > 191, "{half:?}");
        assert_eq!(scale_bgra(&src, 4, 2, (0, 0, 4, 2), 8, 4).len(), 8 * 4 * 4);
    }

    #[test]
    fn prescale_maps_crop() {
        let p = plan(Preset::Shorts, 1920, 1080, 60, 30.0).unwrap();
        let ((dw, dh), crop) = prescale(&p, 1920, 1080);
        assert_eq!(dh, 1920);
        assert!(dw >= 3400 && crop.0 + crop.2 <= dw && (crop.2, crop.3) == (1080, 1920));
        let t = plan(Preset::Twitter, 3440, 1440, 60, 30.0).unwrap();
        assert_eq!(prescale(&t, 3440, 1440), ((t.width, t.height), (0, 0, t.width, t.height)));
    }

    #[test]
    fn gif_timing_patched() {
        // Minimal GIF: header, LSD with 2-colour table, a GCE, one 1x1 image.
        let mut g = b"GIF89a".to_vec();
        g.extend_from_slice(&[1, 0, 1, 0, 0x80, 0, 0]);
        g.extend_from_slice(&[0, 0, 0, 255, 255, 255]);
        g.extend_from_slice(&[0x21, 0xF9, 0x04, 0, 0, 0, 0, 0]);
        g.extend_from_slice(&[0x2C, 0, 0, 0, 0, 1, 0, 1, 0, 0, 2, 2, 0x4C, 0x01, 0]);
        g.push(0x3B);
        let out = gif_set_timing(&g, 15).unwrap();
        let s = out.windows(11).position(|w| w == b"NETSCAPE2.0");
        assert!(s.is_some());
        let gce: Vec<usize> = out.windows(3).enumerate().filter(|(_, w)| *w == [0x21, 0xF9, 0x04]).map(|(i, _)| i).collect();
        assert_eq!(gce.len(), 1);
        assert_eq!(&out[gce[0] + 4..gce[0] + 6], &[7, 0]);
        assert_eq!(*out.last().unwrap(), 0x3B);
    }

    /// Real encode/decode round trip. Needs a clip: set REWIND_EXPORT_CLIP and
    /// run `cargo test -- --ignored export_real_clip`.
    #[cfg(windows)]
    #[test]
    #[ignore]
    fn export_real_clip() {
        let Some(clip) = std::env::var_os("REWIND_EXPORT_CLIP") else { return };
        crate::platform::com_init();
        crate::encode::mf::mf_startup();
        let (w, h, px) = mf::frame_at(Path::new(&clip), 3.0, 640).unwrap();
        assert_eq!((w as usize * h as usize * 4, w <= 640), (px.len(), true));
        let thumb = Path::new(&clip).with_extension("frame.jpg");
        crate::platform::write_image(&thumb, w, h, &px, crate::platform::ImageFormat::Jpeg).unwrap();
        println!("frame_at: {w}x{h} -> {}", thumb.display());
        for preset in [Preset::Discord10, Preset::Discord25, Preset::Shorts, Preset::Twitter, Preset::Gif].into_iter().filter(|_| std::env::var_os("REWIND_EXPORT_FRAME_ONLY").is_none()) {
            let t = std::time::Instant::now();
            let out = mf::export(Path::new(&clip), preset).unwrap_or_else(|e| panic!("{preset:?}: {e:#}"));
            let len = std::fs::metadata(&out).unwrap().len();
            println!("{preset:?}: {} ({len} bytes, {:.1}s)", out.display(), t.elapsed().as_secs_f64());
            assert!(len > 1000);
            if preset != Preset::Gif {
                let tracks = crate::mux::reader::read_file(&out);
                println!("  readback: {:?}", tracks.as_ref().map(|t| (t.len(), crate::mux::duration_secs(t))).map_err(|e| e.to_string()));
            }
        }
    }
}
