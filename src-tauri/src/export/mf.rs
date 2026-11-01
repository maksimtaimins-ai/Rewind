//! Export via Media Foundation: Source Reader decodes to RGB32 + PCM, frames
//! are cropped/scaled on the CPU, and the Sink Writer encodes H.264 + AAC.
//! GIFs go through WIC's GIF encoder with an error-diffused palette per frame.

use std::path::Path;

use anyhow::{anyhow, bail, Context, Result};
use windows::core::{Interface, HSTRING};
use windows::Win32::Foundation::GENERIC_WRITE;
use windows::Win32::Graphics::Imaging::*;
use windows::Win32::Media::MediaFoundation::*;
use windows::Win32::System::Com::{CoCreateInstance, CLSCTX_INPROC_SERVER};

use super::overlay::{draw_or_warn, Renderer};
use super::{gif_set_timing, plan, prescale, scale_bgra, Plan, Preset, Rect};
use crate::settings::KeystrokeSettings;
use crate::gpu::Gpu;

const VIDEO: u32 = MF_SOURCE_READER_FIRST_VIDEO_STREAM.0 as u32;
const AUDIO: u32 = MF_SOURCE_READER_FIRST_AUDIO_STREAM.0 as u32;
const TICKS: i64 = 10_000_000;

/// A decoded frame: packed top-down BGRA.
struct Frame {
    ts: i64,
    px: Vec<u8>,
}

struct Reader {
    reader: IMFSourceReader,
    width: u32,
    height: u32,
    fps: u32,
    stride: i32,
    audio: Option<IMFMediaType>,
    /// Selected streams that have reached their end.
    ended: usize,
    /// Kept alive while the reader decodes on the GPU.
    _gpu: Option<(Gpu, IMFDXGIDeviceManager)>,
}

impl Reader {
    /// Open for decoding. With `scale_to`, first try decoding and scaling on
    /// the GPU straight to that size; check `width`/`height` for what you got.
    fn open(path: &Path, with_audio: bool, scale_to: Option<(u32, u32)>) -> Result<Self> {
        if let Some(size) = scale_to {
            match Self::open_with(path, with_audio, Some(size)) {
                Ok(r) => return Ok(r),
                Err(e) => tracing::warn!("export: GPU decode unavailable, using CPU ({e:#})"),
            }
        }
        Self::open_with(path, with_audio, None)
    }

    fn open_with(path: &Path, with_audio: bool, gpu_size: Option<(u32, u32)>) -> Result<Self> {
        unsafe {
            let mut attrs = None;
            MFCreateAttributes(&mut attrs, 4)?;
            let attrs = attrs.ok_or_else(|| anyhow!("MFCreateAttributes"))?;
            let mut gpu = None;
            if gpu_size.is_some() {
                let g = Gpu::new(None)?;
                let mut token = 0u32;
                let mut mgr = None;
                MFCreateDXGIDeviceManager(&mut token, &mut mgr)?;
                let mgr = mgr.ok_or_else(|| anyhow!("no DXGI device manager"))?;
                mgr.ResetDevice(&g.device, token)?;
                attrs.SetUnknown(&MF_SOURCE_READER_D3D_MANAGER, &mgr)?;
                attrs.SetUINT32(&MF_SOURCE_READER_ENABLE_ADVANCED_VIDEO_PROCESSING, 1)?;
                attrs.SetUINT32(&MF_READWRITE_ENABLE_HARDWARE_TRANSFORMS, 1)?;
                gpu = Some((g, mgr));
            } else {
                attrs.SetUINT32(&MF_SOURCE_READER_ENABLE_VIDEO_PROCESSING, 1)?;
            }
            let reader = MFCreateSourceReaderFromURL(&HSTRING::from(path.as_os_str()), &attrs).context("open clip for export")?;
            reader.SetStreamSelection(MF_SOURCE_READER_ALL_STREAMS.0 as u32, false)?;
            reader.SetStreamSelection(VIDEO, true)?;

            let t = MFCreateMediaType()?;
            t.SetGUID(&MF_MT_MAJOR_TYPE, &MFMediaType_Video)?;
            t.SetGUID(&MF_MT_SUBTYPE, &MFVideoFormat_RGB32)?;
            if let Some((w, h)) = gpu_size {
                t.SetUINT64(&MF_MT_FRAME_SIZE, ((w as u64) << 32) | h as u64)?;
            }
            reader
                .SetCurrentMediaType(VIDEO, None, &t)
                .context("no decoder for this clip's video (HEVC clips need the HEVC Video Extensions)")?;
            let cur = reader.GetCurrentMediaType(VIDEO)?;
            let size = cur.GetUINT64(&MF_MT_FRAME_SIZE)?;
            let (width, height) = ((size >> 32) as u32, size as u32);
            let rate = cur.GetUINT64(&MF_MT_FRAME_RATE).unwrap_or((60 << 32) | 1);
            let fps = ((rate >> 32) as f64 / (rate as u32).max(1) as f64).round().clamp(1.0, 240.0) as u32;
            let stride = cur.GetUINT32(&MF_MT_DEFAULT_STRIDE).map(|s| s as i32).unwrap_or(width as i32 * 4);
            if let Some(size) = gpu_size {
                if (width, height) != size {
                    bail!("decoder gave {width}x{height}, wanted {}x{}", size.0, size.1);
                }
            }

            let mut audio = None;
            if with_audio && reader.SetStreamSelection(AUDIO, true).is_ok() {
                let a = MFCreateMediaType()?;
                a.SetGUID(&MF_MT_MAJOR_TYPE, &MFMediaType_Audio)?;
                a.SetGUID(&MF_MT_SUBTYPE, &MFAudioFormat_PCM)?;
                a.SetUINT32(&MF_MT_AUDIO_BITS_PER_SAMPLE, 16)?;
                if reader.SetCurrentMediaType(AUDIO, None, &a).is_ok() {
                    audio = reader.GetCurrentMediaType(AUDIO).ok();
                }
                if audio.is_none() {
                    let _ = reader.SetStreamSelection(AUDIO, false);
                }
            }
            Ok(Self { reader, width, height, fps, stride, audio, ended: 0, _gpu: gpu })
        }
    }

    fn frame(&self, sample: &IMFSample, ts: i64) -> Result<Frame> {
        unsafe {
            let buf = sample.GetBufferByIndex(0)?;
            let row = self.width as usize * 4;
            let mut px = vec![0u8; row * self.height as usize];
            let mut copy = |scan0: *const u8, pitch: isize| {
                for y in 0..self.height as usize {
                    let src = std::slice::from_raw_parts(scan0.offset(y as isize * pitch), row);
                    px[y * row..(y + 1) * row].copy_from_slice(src);
                }
            };
            if let Ok(b2) = buf.cast::<IMF2DBuffer>() {
                let mut scan0 = std::ptr::null_mut();
                let mut pitch = 0i32;
                b2.Lock2D(&mut scan0, &mut pitch)?;
                copy(scan0, pitch as isize);
                b2.Unlock2D()?;
            } else {
                let mut data = std::ptr::null_mut();
                let mut len = 0u32;
                buf.Lock(&mut data, None, Some(&mut len))?;
                // A negative default stride means bottom-up rows.
                let (start, pitch) = if self.stride < 0 {
                    (data.add((self.height as usize - 1) * row), -(row as isize))
                } else {
                    (data, self.stride as isize)
                };
                let ok = len as usize >= row * self.height as usize;
                if ok {
                    copy(start, pitch);
                }
                buf.Unlock()?;
                if !ok {
                    bail!("decoded frame is smaller than expected");
                }
            }
            // RGB32's fourth byte is padding, not alpha.
            for p in px.chunks_exact_mut(4) {
                p[3] = 255;
            }
            Ok(Frame { ts, px })
        }
    }

    /// Next sample from any selected stream: `(is_video, timestamp, sample)`,
    /// or `None` at the end of everything.
    fn next(&mut self) -> Result<Option<(bool, i64, Option<IMFSample>)>> {
        unsafe {
            loop {
                let mut index = 0u32;
                let mut flags = 0u32;
                let mut ts = 0i64;
                let mut sample = None;
                self.reader.ReadSample(
                    MF_SOURCE_READER_ANY_STREAM.0 as u32,
                    0,
                    Some(&mut index),
                    Some(&mut flags),
                    Some(&mut ts),
                    Some(&mut sample),
                )?;
                if flags & MF_SOURCE_READERF_ENDOFSTREAM.0 as u32 != 0 {
                    // One stream ended; carry on until all selected ones have.
                    let _ = self.reader.SetStreamSelection(index, false);
                    self.ended += 1;
                    if self.ended >= 1 + self.audio.is_some() as usize {
                        return Ok(None);
                    }
                    continue;
                }
                if sample.is_none() {
                    continue;
                }
                let is_video = self.reader.GetCurrentMediaType(index)?.GetGUID(&MF_MT_MAJOR_TYPE)? == MFMediaType_Video;
                return Ok(Some((is_video, ts, sample)));
            }
        }
    }
}

/// Decode the frame shown at `secs`, scaled to at most `max_w` wide, as BGRA.
pub fn frame_at(src: &Path, secs: f64, max_w: u32) -> Result<(u32, u32, Vec<u8>)> {
    let probe = Reader::open(src, false, None)?;
    let (sw, sh) = (probe.width, probe.height);
    drop(probe);
    let k = (max_w as f64 / sw as f64).min(1.0);
    let (w, h) = (((sw as f64 * k) as u32) & !1, ((sh as f64 * k) as u32) & !1);
    let mut r = Reader::open(src, false, Some((w, h)))?;
    let target = (secs.max(0.0) * TICKS as f64) as i64;
    let mut origin = None;
    let mut last = None;
    while let Some((_, ts, sample)) = r.next()? {
        let t = ts - *origin.get_or_insert(ts);
        let Some(sample) = sample else { continue };
        // The frame on screen at `secs` is the last one starting at or before it.
        if t > target && last.is_some() {
            break;
        }
        last = Some(r.frame(&sample, t)?);
    }
    let f = last.ok_or_else(|| anyhow!("no frames decoded"))?;
    if (r.width, r.height) == (w, h) {
        Ok((w, h, f.px))
    } else {
        Ok((w, h, scale_bgra(&f.px, r.width, r.height, (0, 0, r.width, r.height), w, h)))
    }
}

/// Decode every audio track of `src` to 16-bit PCM, calling
/// `f(stream, sample rate, channels, samples)` per decoded chunk. Video is
/// never decoded.
pub fn decode_audio(src: &Path, f: &mut dyn FnMut(u32, u32, u32, &[i16])) -> Result<()> {
    unsafe {
        let reader = MFCreateSourceReaderFromURL(&HSTRING::from(src.as_os_str()), None).context("open clip for analysis")?;
        reader.SetStreamSelection(MF_SOURCE_READER_ALL_STREAMS.0 as u32, false)?;
        // (rate, channels) per selected audio stream.
        let mut formats: Vec<(u32, u32, u32)> = Vec::new();
        let mut i = 0u32;
        while let Ok(native) = reader.GetNativeMediaType(i, 0) {
            if native.GetGUID(&MF_MT_MAJOR_TYPE)? == MFMediaType_Audio {
                let a = MFCreateMediaType()?;
                a.SetGUID(&MF_MT_MAJOR_TYPE, &MFMediaType_Audio)?;
                a.SetGUID(&MF_MT_SUBTYPE, &MFAudioFormat_PCM)?;
                a.SetUINT32(&MF_MT_AUDIO_BITS_PER_SAMPLE, 16)?;
                if reader.SetCurrentMediaType(i, None, &a).is_ok() {
                    let cur = reader.GetCurrentMediaType(i)?;
                    let rate = cur.GetUINT32(&MF_MT_AUDIO_SAMPLES_PER_SECOND)?;
                    let channels = cur.GetUINT32(&MF_MT_AUDIO_NUM_CHANNELS)?;
                    reader.SetStreamSelection(i, true)?;
                    formats.push((i, rate, channels));
                }
            }
            i += 1;
        }
        if formats.is_empty() {
            bail!("this clip has no audio to analyze");
        }
        let mut ended = 0;
        while ended < formats.len() {
            let mut index = 0u32;
            let mut flags = 0u32;
            let mut sample = None;
            reader.ReadSample(MF_SOURCE_READER_ANY_STREAM.0 as u32, 0, Some(&mut index), Some(&mut flags), None, Some(&mut sample))?;
            if flags & MF_SOURCE_READERF_ENDOFSTREAM.0 as u32 != 0 {
                let _ = reader.SetStreamSelection(index, false);
                ended += 1;
                continue;
            }
            let (Some(sample), Some(&(_, rate, channels))) = (sample, formats.iter().find(|x| x.0 == index)) else { continue };
            let buf = sample.ConvertToContiguousBuffer()?;
            let mut data = std::ptr::null_mut();
            let mut len = 0u32;
            buf.Lock(&mut data, None, Some(&mut len))?;
            let pcm = std::slice::from_raw_parts(data as *const i16, len as usize / 2);
            f(index, rate, channels, pcm);
            buf.Unlock()?;
        }
        Ok(())
    }
}

/// Re-encode `src` for `preset`, writing next to it. Returns the new file.
/// With `overlay`, recorded keystrokes (if the clip has any) are drawn in.
pub fn export(src: &Path, preset: Preset, overlay: Option<&KeystrokeSettings>) -> Result<std::path::PathBuf> {
    let out = super::output_path(src, preset);
    export_to(src, preset, &out, overlay)?;
    Ok(out)
}

/// Re-encode `src` for `preset` into `out`. Nothing is left behind on failure.
pub fn export_to(src: &Path, preset: Preset, out: &Path, overlay: Option<&KeystrokeSettings>) -> Result<()> {
    let duration = crate::library::probe_duration(src).context("read clip length")?;
    let result = if preset == Preset::Gif { export_gif(src, out, duration, overlay) } else { export_video(src, out, preset, duration, overlay) };
    if result.is_err() {
        let _ = std::fs::remove_file(out);
    }
    result
}

fn export_video(src: &Path, out: &Path, preset: Preset, duration: f64, overlay: Option<&KeystrokeSettings>) -> Result<()> {
    let probe = Reader::open(src, false, None)?;
    let mut p = plan(preset, probe.width, probe.height, probe.fps, duration)?;
    let src_size = (probe.width, probe.height);
    drop(probe);
    // Encoders overshoot a little: if the file comes out too big, try again lower.
    for _ in 0..3 {
        encode_mp4(src, out, &p, src_size, overlay)?;
        let size = std::fs::metadata(out)?.len();
        match p.size_limit {
            Some(limit) if size > limit => {
                let k = limit as f64 / size as f64 * 0.92;
                p.video_bps = ((p.video_bps as f64 * k) as u32).max(200_000);
            }
            _ => return Ok(()),
        }
    }
    bail!("couldn't get the export under {} MB", p.size_limit.unwrap_or(0) / (1024 * 1024))
}

/// Open `src` decoding as close to the plan's output as the GPU allows, and
/// say which part of each decoded frame to use.
fn open_for(src: &Path, p: &Plan, src_size: (u32, u32), with_audio: bool) -> Result<(Reader, Rect)> {
    let (size, crop) = prescale(p, src_size.0, src_size.1);
    let r = Reader::open(src, with_audio, Some(size))?;
    let crop = if (r.width, r.height) == size { crop } else { p.crop };
    Ok((r, crop))
}

fn encode_mp4(src: &Path, out: &Path, p: &Plan, src_size: (u32, u32), overlay: Option<&KeystrokeSettings>) -> Result<()> {
    let (mut r, crop) = open_for(src, p, src_size, p.audio_bps > 0)?;
    let mut keys = overlay.map(|c| Renderer::open(src, c)).transpose()?.flatten();
    unsafe {
        let writer = MFCreateSinkWriterFromURL(&HSTRING::from(out.as_os_str()), None, None).context("create export file")?;
        let size = ((p.width as u64) << 32) | p.height as u64;
        let rate = ((p.fps as u64) << 32) | 1;

        let vout = MFCreateMediaType()?;
        vout.SetGUID(&MF_MT_MAJOR_TYPE, &MFMediaType_Video)?;
        vout.SetGUID(&MF_MT_SUBTYPE, &MFVideoFormat_H264)?;
        vout.SetUINT32(&MF_MT_AVG_BITRATE, p.video_bps)?;
        vout.SetUINT64(&MF_MT_FRAME_SIZE, size)?;
        vout.SetUINT64(&MF_MT_FRAME_RATE, rate)?;
        vout.SetUINT64(&MF_MT_PIXEL_ASPECT_RATIO, (1u64 << 32) | 1)?;
        vout.SetUINT32(&MF_MT_INTERLACE_MODE, MFVideoInterlace_Progressive.0 as u32)?;
        vout.SetUINT32(&MF_MT_MPEG2_PROFILE, eAVEncH264VProfile_High.0 as u32)?;
        let vstream = writer.AddStream(&vout)?;

        let vin = MFCreateMediaType()?;
        vin.SetGUID(&MF_MT_MAJOR_TYPE, &MFMediaType_Video)?;
        vin.SetGUID(&MF_MT_SUBTYPE, &MFVideoFormat_RGB32)?;
        vin.SetUINT64(&MF_MT_FRAME_SIZE, size)?;
        vin.SetUINT64(&MF_MT_FRAME_RATE, rate)?;
        vin.SetUINT64(&MF_MT_PIXEL_ASPECT_RATIO, (1u64 << 32) | 1)?;
        vin.SetUINT32(&MF_MT_INTERLACE_MODE, MFVideoInterlace_Progressive.0 as u32)?;
        // Positive stride: rows are top-down.
        vin.SetUINT32(&MF_MT_DEFAULT_STRIDE, p.width * 4)?;
        writer.SetInputMediaType(vstream, &vin, None).context("H.264 encoder rejected the frame format")?;

        let mut astream = None;
        if let Some(pcm) = &r.audio {
            let rate = pcm.GetUINT32(&MF_MT_AUDIO_SAMPLES_PER_SECOND)?;
            let channels = pcm.GetUINT32(&MF_MT_AUDIO_NUM_CHANNELS)?;
            // The AAC encoder takes 44.1/48 kHz, mono or stereo.
            if matches!(rate, 44_100 | 48_000) && matches!(channels, 1 | 2) {
                let aout = MFCreateMediaType()?;
                aout.SetGUID(&MF_MT_MAJOR_TYPE, &MFMediaType_Audio)?;
                aout.SetGUID(&MF_MT_SUBTYPE, &MFAudioFormat_AAC)?;
                aout.SetUINT32(&MF_MT_AUDIO_BITS_PER_SAMPLE, 16)?;
                aout.SetUINT32(&MF_MT_AUDIO_SAMPLES_PER_SECOND, rate)?;
                aout.SetUINT32(&MF_MT_AUDIO_NUM_CHANNELS, channels)?;
                // Supported: 12000, 16000, 20000 or 24000 bytes per second.
                let bytes = (p.audio_bps / 8).clamp(12_000, 24_000) / 4000 * 4000;
                aout.SetUINT32(&MF_MT_AUDIO_AVG_BYTES_PER_SECOND, bytes)?;
                let s = writer.AddStream(&aout)?;
                writer.SetInputMediaType(s, pcm, None).context("AAC encoder rejected the audio format")?;
                astream = Some(s);
            } else {
                tracing::warn!("export: skipping audio ({rate} Hz, {channels} ch)");
            }
        }

        writer.BeginWriting()?;
        let end = (p.secs * TICKS as f64) as i64;
        let interval = TICKS / p.fps.max(1) as i64;
        let mut next_due = 0i64;
        let mut origin: Option<i64> = None;
        while let Some((is_video, ts, sample)) = r.next()? {
            let sample = sample.unwrap();
            let base = *origin.get_or_insert(ts);
            let t = ts - base;
            if t >= end {
                if is_video {
                    break;
                }
                continue;
            }
            if is_video {
                // Drop frames above the target rate.
                if t + interval / 4 < next_due {
                    continue;
                }
                next_due = if t - next_due > interval { t + interval } else { next_due + interval };
                let f = r.frame(&sample, t)?;
                let mut px = scale_bgra(&f.px, r.width, r.height, crop, p.width, p.height);
                draw_or_warn(&mut keys, &mut px, (p.width, p.height), f.ts as f64 / TICKS as f64, (r.width, r.height), crop);
                writer.WriteSample(vstream, &bgra_sample(&px, f.ts, interval)?)?;
            } else if let Some(a) = astream {
                if t >= 0 {
                    sample.SetSampleTime(t)?;
                    writer.WriteSample(a, &sample)?;
                }
            }
        }
        writer.Finalize().context("finish export file")?;
    }
    Ok(())
}

unsafe fn bgra_sample(px: &[u8], ts: i64, duration: i64) -> Result<IMFSample> {
    unsafe {
        let buf = MFCreateMemoryBuffer(px.len() as u32)?;
        let mut data = std::ptr::null_mut();
        buf.Lock(&mut data, None, None)?;
        std::ptr::copy_nonoverlapping(px.as_ptr(), data, px.len());
        buf.Unlock()?;
        buf.SetCurrentLength(px.len() as u32)?;
        let s = MFCreateSample()?;
        s.AddBuffer(&buf)?;
        s.SetSampleTime(ts)?;
        s.SetSampleDuration(duration)?;
        Ok(s)
    }
}

fn export_gif(src: &Path, out: &Path, duration: f64, overlay: Option<&KeystrokeSettings>) -> Result<()> {
    let probe = Reader::open(src, false, None)?;
    let p = plan(Preset::Gif, probe.width, probe.height, probe.fps, duration)?;
    let src_size = (probe.width, probe.height);
    drop(probe);
    let (mut r, crop) = open_for(src, &p, src_size, false)?;
    let mut keys = overlay.map(|c| Renderer::open(src, c)).transpose()?.flatten();
    unsafe {
        let factory: IWICImagingFactory = CoCreateInstance(&CLSID_WICImagingFactory, None, CLSCTX_INPROC_SERVER)?;
        let stream = factory.CreateStream()?;
        stream.InitializeFromFilename(&HSTRING::from(out.as_os_str()), GENERIC_WRITE.0)?;
        let encoder = factory.CreateEncoder(&GUID_ContainerFormatGif, std::ptr::null())?;
        encoder.Initialize(&stream, WICBitmapEncoderNoCache)?;

        let end = (p.secs * TICKS as f64) as i64;
        let interval = TICKS / p.fps as i64;
        let mut next_due = 0i64;
        let mut origin = None;
        let mut frames = 0;
        while let Some((_, ts, sample)) = r.next()? {
            let base = *origin.get_or_insert(ts);
            let t = ts - base;
            if t >= end {
                break;
            }
            if t + interval / 4 < next_due {
                continue;
            }
            next_due = if t - next_due > interval { t + interval } else { next_due + interval };
            let f = r.frame(&sample.unwrap(), t)?;
            let mut px = scale_bgra(&f.px, r.width, r.height, crop, p.width, p.height);
            draw_or_warn(&mut keys, &mut px, (p.width, p.height), t as f64 / TICKS as f64, (r.width, r.height), crop);

            let bitmap = factory.CreateBitmapFromMemory(p.width, p.height, &GUID_WICPixelFormat32bppBGRA, p.width * 4, &px)?;
            let palette = factory.CreatePalette()?;
            palette.InitializeFromBitmap(&bitmap, 256, false)?;
            let indexed = factory.CreateFormatConverter()?;
            indexed.Initialize(&bitmap, &GUID_WICPixelFormat8bppIndexed, WICBitmapDitherTypeErrorDiffusion, &palette, 0.0, WICBitmapPaletteTypeCustom)?;

            let mut frame = None;
            let mut bag = None;
            encoder.CreateNewFrame(&mut frame, &mut bag)?;
            let frame = frame.ok_or_else(|| anyhow!("no WIC frame"))?;
            frame.Initialize(bag.as_ref())?;
            frame.SetSize(p.width, p.height)?;
            let mut fmt = GUID_WICPixelFormat8bppIndexed;
            frame.SetPixelFormat(&mut fmt)?;
            frame.SetPalette(&palette)?;
            frame.WriteSource(&indexed, std::ptr::null())?;
            frame.Commit()?;
            frames += 1;
        }
        if frames == 0 {
            bail!("no frames decoded");
        }
        encoder.Commit()?;
    }
    let bytes = std::fs::read(out)?;
    std::fs::write(out, gif_set_timing(&bytes, p.fps)?)?;
    Ok(())
}
