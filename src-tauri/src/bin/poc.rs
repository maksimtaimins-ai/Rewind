//! Pipeline proof of concept: `cargo run --release --bin poc -- <out_dir>`.
//! Benchmarks encoders, records the primary monitor + desktop audio for a few
//! seconds, then writes the last 6 s to an MP4 and a thumbnail.

use std::sync::Arc;
use std::time::Duration;

use parking_lot::Mutex;
use rewind_lib::buffer::{ReplayBuffer, TICKS_PER_SEC};
use rewind_lib::encode::{self, EncoderParams};
use rewind_lib::pipeline::{Flags, Pipeline, PipelineConfig};
use rewind_lib::settings::{Codec, EncoderChoice, RateControl, Settings};
use rewind_lib::{capture, mux, platform};

unsafe extern "system" fn on_fault(info: *mut windows::Win32::System::Diagnostics::Debug::EXCEPTION_POINTERS) -> i32 {
    use windows::Win32::System::LibraryLoader::*;
    unsafe {
        let rec = &*(*info).ExceptionRecord;
        if rec.ExceptionCode.0 as u32 == 0xC0000005 {
            let addr = rec.ExceptionAddress;
            let mut module = windows::Win32::Foundation::HMODULE::default();
            let _ = GetModuleHandleExW(
                GET_MODULE_HANDLE_EX_FLAG_FROM_ADDRESS | GET_MODULE_HANDLE_EX_FLAG_UNCHANGED_REFCOUNT,
                windows::core::PCWSTR(addr as _),
                &mut module,
            );
            let mut name = [0u16; 260];
            let len = GetModuleFileNameW(Some(module), &mut name);
            let mut frames = [std::ptr::null_mut(); 32];
            let n = windows::Win32::System::Diagnostics::Debug::RtlCaptureStackBackTrace(0, &mut frames, None);
            for f in &frames[..n as usize] {
                let mut m = windows::Win32::Foundation::HMODULE::default();
                let _ = GetModuleHandleExW(
                    GET_MODULE_HANDLE_EX_FLAG_FROM_ADDRESS | GET_MODULE_HANDLE_EX_FLAG_UNCHANGED_REFCOUNT,
                    windows::core::PCWSTR(*f as _),
                    &mut m,
                );
                let mut nm = [0u16; 260];
                let k = GetModuleFileNameW(Some(m), &mut nm);
                eprintln!("  frame {:?} {}+{:#x}", f, String::from_utf16_lossy(&nm[..k as usize]), *f as usize - m.0 as usize);
            }
            eprintln!(
                "ACCESS VIOLATION at {:?} in {} (+{:#x}) {} {:#x}
{}",
                addr,
                String::from_utf16_lossy(&name[..len as usize]),
                addr as usize - module.0 as usize,
                if rec.ExceptionInformation[0] == 0 { "reading" } else { "writing" },
                rec.ExceptionInformation[1],
                std::backtrace::Backtrace::force_capture()
            );
        }
    }
    0 // EXCEPTION_CONTINUE_SEARCH
}

fn main() -> anyhow::Result<()> {
    unsafe { windows::Win32::System::Diagnostics::Debug::AddVectoredExceptionHandler(1, Some(on_fault)) };
    tracing_subscriber::fmt().with_env_filter("info").init();
    let out_dir = std::path::PathBuf::from(std::env::args().nth(1).unwrap_or_else(|| ".".into()));
    std::fs::create_dir_all(&out_dir)?;
    platform::com_init();
    encode::mf::mf_startup();

    let encoders = encode::enumerate_all();
    for e in &encoders {
        println!("encoder: {:?} {:?} {} luid={:?}", e.kind, e.codec, e.name, e.luid);
    }
    for r in encode::benchmark(&encoders, 1920, 1080, 1.0) {
        println!("bench: {:<45} {:?} {:>7.1} fps {}", r.name, r.codec, r.fps, r.error.unwrap_or_default());
    }

    let codec = if std::env::args().any(|a| a == "--hevc") { Codec::Hevc } else { Codec::H264 };
    let info = encode::pick(&encoders, EncoderChoice::Auto, codec).expect("no encoder");
    let monitor = capture::monitors().into_iter().next().expect("no monitor");
    println!("recording {}x{} {} with {}", monitor.width, monitor.height, monitor.name, info.name);

    let settings = Settings::default();
    let buffer = Arc::new(Mutex::new(ReplayBuffer::new(30, 512 << 20)));
    let mut p = Pipeline::start(
        PipelineConfig {
            monitor,
            cursor: true,
            target_height: 1080,
            encoder: info,
            params: EncoderParams {
                codec,
                width: 0,
                height: 0,
                fps: 60,
                rate_control: RateControl::Vbr,
                bitrate_kbps: 20_000,
                quality: 70,
                keyframe_secs: 1.0,
            },
            audio: settings.audio.clone(),
        },
        buffer.clone(),
        Arc::new(Flags::default()),
    )?;
    for _ in 0..8 {
        std::thread::sleep(Duration::from_secs(1));
        let s = &p.stats;
        use std::sync::atomic::Ordering::Relaxed;
        let b = buffer.lock();
        println!(
            "in={} out={} dropped={} bytes={} buffered={:.2}s mem={}KiB",
            s.frames_in.load(Relaxed),
            s.frames_out.load(Relaxed),
            s.dropped.load(Relaxed),
            s.bytes_out.load(Relaxed),
            b.buffered_ticks() as f64 / TICKS_PER_SEC as f64,
            b.total_bytes() / 1024
        );
    }
    let thumb = p.request_frame();
    let end = buffer.lock().latest_pts().unwrap();
    let snap = buffer.lock().snapshot(end - 6 * TICKS_PER_SEC, end).expect("snapshot");
    let path = out_dir.join("poc-clip.mp4");
    let bytes = mux::write_snapshot(&path, &snap)?;
    println!("wrote {} ({} KiB, {} video packets, audio {:?})", path.display(), bytes / 1024, snap.video.len(),
        snap.audio.iter().map(|(c, p)| (c.name.clone(), p.len())).collect::<Vec<_>>());
    if let Ok((w, h, px)) = thumb.recv_timeout(Duration::from_secs(2)) {
        let (tw, th, small) = platform::downscale_bgra(w, h, &px, 480);
        platform::write_image(&out_dir.join("poc-thumb.jpg"), tw, th, &small, platform::ImageFormat::Jpeg)?;
        println!("thumbnail {tw}x{th}");
    } else {
        println!("no frame for thumbnail (static screen?)");
    }
    p.stop();
    let tracks = mux::reader::read_file(&path)?;
    verify_with_windows(&path)?;
    println!("readback: {} tracks, {:.2}s, keyframes {:?}", tracks.len(), mux::duration_secs(&tracks), mux::keyframe_times(&tracks));
    Ok(())
}

/// Decode the clip with Windows' own MP4 demuxer + decoders to prove it is valid.
fn verify_with_windows(path: &std::path::Path) -> anyhow::Result<()> {
    use windows::Win32::Media::MediaFoundation::*;
    let url = windows::core::HSTRING::from(path.as_os_str());
    unsafe {
        for stream in 0..8u32 {
            let reader = MFCreateSourceReaderFromURL(&url, None)?;
            let Ok(t) = reader.GetCurrentMediaType(stream) else { break };
            let major = t.GetGUID(&MF_MT_MAJOR_TYPE)?;
            let kind = if major == MFMediaType_Video { "video" } else { "audio" };
            reader.SetStreamSelection(0xFFFFFFFE, false)?;
            reader.SetStreamSelection(stream, true)?;
            let (mut n, mut last, mut err) = (0, 0i64, None);
            loop {
                let mut flags = 0u32;
                let mut ts = 0i64;
                let mut sample = None;
                if let Err(e) = reader.ReadSample(stream, 0, None, Some(&mut flags), Some(&mut ts), Some(&mut sample)) {
                    err = Some(e);
                    break;
                }
                if flags & MF_SOURCE_READERF_ENDOFSTREAM.0 as u32 != 0 {
                    break;
                }
                if sample.is_some() {
                    n += 1;
                    last = ts;
                }
            }
            println!("windows decode: stream {stream} {kind}: {n} samples, last ts {:.3}s {:?}", last as f64 / 1e7, err);
        }
    }
    Ok(())
}
