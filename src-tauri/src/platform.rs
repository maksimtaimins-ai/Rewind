//! Small Windows helpers: COM init, QPC clock, processes, autostart, recycle
//! bin, image encoding.

use std::path::Path;

use anyhow::{bail, Result};
use windows::core::{GUID, HSTRING, PCWSTR};
use windows::Win32::Foundation::{CloseHandle, GENERIC_WRITE, HWND, MAX_PATH};
use windows::Win32::Graphics::Imaging::*;
use windows::Win32::System::Com::{CoCreateInstance, CoInitializeEx, CLSCTX_INPROC_SERVER, COINIT_MULTITHREADED};
use windows::Win32::System::Diagnostics::ToolHelp::{
    CreateToolhelp32Snapshot, Process32FirstW, Process32NextW, PROCESSENTRY32W, TH32CS_SNAPPROCESS,
};
use windows::Win32::System::Performance::{QueryPerformanceCounter, QueryPerformanceFrequency};
use windows::Win32::System::Registry::{RegDeleteKeyValueW, RegGetValueW, RegSetKeyValueW, HKEY_CURRENT_USER, REG_SZ, RRF_RT_REG_SZ};
use windows::Win32::System::Threading::{
    OpenProcess, QueryFullProcessImageNameW, PROCESS_NAME_WIN32, PROCESS_QUERY_LIMITED_INFORMATION,
};
use windows::Win32::UI::Shell::{SHFileOperationW, FOF_ALLOWUNDO, FOF_NOCONFIRMATION, FOF_NOERRORUI, FOF_SILENT, FO_DELETE, SHFILEOPSTRUCTW};
use windows::Win32::UI::WindowsAndMessaging::{GetForegroundWindow, GetWindowRect, GetWindowThreadProcessId};

/// Initialise COM (MTA) on the current thread. Safe to call repeatedly.
pub fn com_init() {
    unsafe {
        let _ = CoInitializeEx(None, COINIT_MULTITHREADED);
    }
}

/// Current QPC time in 100 ns units: the clock WGC frame times and WASAPI
/// packet positions use.
/// Bytes free for this user on the drive holding `path` (or its nearest
/// existing parent).
pub fn disk_free(path: &Path) -> Option<u64> {
    let dir = path.ancestors().find(|p| p.exists())?;
    let mut avail = 0u64;
    unsafe { windows::Win32::Storage::FileSystem::GetDiskFreeSpaceExW(&HSTRING::from(dir.as_os_str()), Some(&mut avail), None, None).ok()? };
    Some(avail)
}

/// Size and free space of the drive holding `path`: (total, free for this user).
pub fn disk_space(path: &Path) -> Option<(u64, u64)> {
    let dir = path.ancestors().find(|p| p.exists())?;
    let (mut avail, mut total) = (0u64, 0u64);
    unsafe { windows::Win32::Storage::FileSystem::GetDiskFreeSpaceExW(&HSTRING::from(dir.as_os_str()), Some(&mut avail), Some(&mut total), None).ok()? };
    Some((total, avail))
}

pub fn qpc_now() -> i64 {
    unsafe {
        let mut c = 0i64;
        let mut f = 0i64;
        let _ = QueryPerformanceCounter(&mut c);
        let _ = QueryPerformanceFrequency(&mut f);
        (c as i128 * 10_000_000 / f.max(1) as i128) as i64
    }
}

#[derive(Debug, Clone)]
pub struct ProcessEntry {
    pub pid: u32,
    pub parent: u32,
    pub exe: String,
}

pub fn processes() -> Vec<ProcessEntry> {
    let mut out = Vec::new();
    unsafe {
        let Ok(snap) = CreateToolhelp32Snapshot(TH32CS_SNAPPROCESS, 0) else { return out };
        let mut e = PROCESSENTRY32W { dwSize: std::mem::size_of::<PROCESSENTRY32W>() as u32, ..Default::default() };
        if Process32FirstW(snap, &mut e).is_ok() {
            loop {
                let len = e.szExeFile.iter().position(|&c| c == 0).unwrap_or(e.szExeFile.len());
                out.push(ProcessEntry {
                    pid: e.th32ProcessID,
                    parent: e.th32ParentProcessID,
                    exe: String::from_utf16_lossy(&e.szExeFile[..len]),
                });
                if Process32NextW(snap, &mut e).is_err() {
                    break;
                }
            }
        }
        let _ = CloseHandle(snap);
    }
    out
}

pub fn find_pid(exe: &str) -> Option<u32> {
    processes().into_iter().find(|p| p.exe.eq_ignore_ascii_case(exe)).map(|p| p.pid)
}

fn process_path(pid: u32) -> Option<String> {
    unsafe {
        let h = OpenProcess(PROCESS_QUERY_LIMITED_INFORMATION, false, pid).ok()?;
        let mut buf = [0u16; MAX_PATH as usize * 2];
        let mut len = buf.len() as u32;
        let r = QueryFullProcessImageNameW(h, PROCESS_NAME_WIN32, windows::core::PWSTR(buf.as_mut_ptr()), &mut len);
        let _ = CloseHandle(h);
        r.ok()?;
        Some(String::from_utf16_lossy(&buf[..len as usize]))
    }
}

#[derive(Debug, Clone)]
pub struct Foreground {
    pub pid: u32,
    pub exe: String,
    pub path: String,
    /// Window covers its whole monitor (fullscreen or borderless).
    pub fullscreen: bool,
}

pub fn foreground() -> Option<Foreground> {
    use windows::Win32::Graphics::Gdi::{GetMonitorInfoW, MonitorFromWindow, MONITORINFO, MONITOR_DEFAULTTONEAREST};
    unsafe {
        let hwnd: HWND = GetForegroundWindow();
        if hwnd.0.is_null() {
            return None;
        }
        let mut pid = 0u32;
        GetWindowThreadProcessId(hwnd, Some(&mut pid));
        let path = process_path(pid)?;
        let exe = Path::new(&path).file_name()?.to_string_lossy().to_string();
        let mut r = Default::default();
        let _ = GetWindowRect(hwnd, &mut r);
        let mon = MonitorFromWindow(hwnd, MONITOR_DEFAULTTONEAREST);
        let mut mi = MONITORINFO { cbSize: std::mem::size_of::<MONITORINFO>() as u32, ..Default::default() };
        let fullscreen = GetMonitorInfoW(mon, &mut mi).as_bool() && {
            let m = mi.rcMonitor;
            r.left <= m.left && r.top <= m.top && r.right >= m.right && r.bottom >= m.bottom
        };
        Some(Foreground { pid, exe, path, fullscreen })
    }
}

const RUN_KEY: PCWSTR = windows::core::w!("Software\\Microsoft\\Windows\\CurrentVersion\\Run");
const RUN_VALUE: PCWSTR = windows::core::w!("Rewind");

pub fn autostart_enabled() -> bool {
    unsafe { RegGetValueW(HKEY_CURRENT_USER, RUN_KEY, RUN_VALUE, RRF_RT_REG_SZ, None, None, None).is_ok() }
}

pub fn set_autostart(enabled: bool) -> Result<()> {
    unsafe {
        if enabled {
            let exe = std::env::current_exe()?;
            let cmd = format!("\"{}\" --tray", exe.display());
            let wide: Vec<u16> = cmd.encode_utf16().chain(Some(0)).collect();
            let r = RegSetKeyValueW(
                HKEY_CURRENT_USER,
                RUN_KEY,
                RUN_VALUE,
                REG_SZ.0,
                Some(wide.as_ptr() as _),
                (wide.len() * 2) as u32,
            );
            if r.is_err() {
                bail!("could not write startup entry ({r:?})");
            }
        } else {
            let _ = RegDeleteKeyValueW(HKEY_CURRENT_USER, RUN_KEY, RUN_VALUE);
        }
    }
    Ok(())
}

/// Move files to the Recycle Bin (never a permanent delete).
pub fn recycle(paths: &[&Path]) -> Result<()> {
    let mut from: Vec<u16> = Vec::new();
    for p in paths {
        from.extend(p.as_os_str().to_string_lossy().encode_utf16());
        from.push(0);
    }
    from.push(0);
    let mut op = SHFILEOPSTRUCTW {
        wFunc: FO_DELETE,
        pFrom: PCWSTR(from.as_ptr()),
        fFlags: (FOF_ALLOWUNDO | FOF_NOCONFIRMATION | FOF_SILENT | FOF_NOERRORUI).0 as u16,
        ..Default::default()
    };
    let r = unsafe { SHFileOperationW(&mut op) };
    if r != 0 {
        bail!("recycle failed ({r})");
    }
    Ok(())
}

pub enum ImageFormat {
    Jpeg,
    Png,
}

/// Box-filter downscale of BGRA pixels so the longest side is <= `max`.
pub fn downscale_bgra(w: u32, h: u32, px: &[u8], max: u32) -> (u32, u32, Vec<u8>) {
    let scale = (w.max(h) as f64 / max as f64).ceil().max(1.0) as u32;
    if scale == 1 {
        return (w, h, px.to_vec());
    }
    let (nw, nh) = (w / scale, h / scale);
    let mut out = vec![0u8; (nw * nh * 4) as usize];
    for y in 0..nh {
        for x in 0..nw {
            let mut acc = [0u32; 4];
            for dy in 0..scale {
                let row = ((y * scale + dy) * w + x * scale) as usize * 4;
                for dx in 0..scale as usize {
                    for c in 0..4 {
                        acc[c] += px[row + dx * 4 + c] as u32;
                    }
                }
            }
            let n = scale * scale;
            let o = ((y * nw + x) * 4) as usize;
            for c in 0..4 {
                out[o + c] = (acc[c] / n) as u8;
            }
        }
    }
    (nw, nh, out)
}

/// Encode BGRA pixels to a JPEG or PNG file with WIC.
pub fn write_image(path: &Path, w: u32, h: u32, bgra: &[u8], format: ImageFormat) -> Result<()> {
    if let Some(dir) = path.parent() {
        std::fs::create_dir_all(dir)?;
    }
    unsafe {
        let factory: IWICImagingFactory = CoCreateInstance(&CLSID_WICImagingFactory, None, CLSCTX_INPROC_SERVER)?;
        let stream = factory.CreateStream()?;
        stream.InitializeFromFilename(&HSTRING::from(path.as_os_str()), GENERIC_WRITE.0)?;
        let (container, mut pixel, data, stride): (GUID, GUID, Vec<u8>, u32) = match format {
            ImageFormat::Jpeg => {
                let mut bgr = Vec::with_capacity((w * h * 3) as usize);
                for p in bgra.as_chunks::<4>().0 {
                    bgr.extend_from_slice(&p[..3]);
                }
                (GUID_ContainerFormatJpeg, GUID_WICPixelFormat24bppBGR, bgr, w * 3)
            }
            ImageFormat::Png => {
                let mut v = bgra.to_vec();
                for p in v.as_chunks_mut::<4>().0 {
                    p[3] = 255;
                }
                (GUID_ContainerFormatPng, GUID_WICPixelFormat32bppBGRA, v, w * 4)
            }
        };
        let encoder = factory.CreateEncoder(&container, std::ptr::null())?;
        encoder.Initialize(&stream, WICBitmapEncoderNoCache)?;
        let mut frame = None;
        let mut bag = None;
        encoder.CreateNewFrame(&mut frame, &mut bag)?;
        let frame = frame.ok_or_else(|| anyhow::anyhow!("no WIC frame"))?;
        frame.Initialize(bag.as_ref())?;
        frame.SetSize(w, h)?;
        let wanted = pixel;
        frame.SetPixelFormat(&mut pixel)?;
        if pixel != wanted {
            bail!("WIC encoder does not accept the pixel format");
        }
        frame.WritePixels(h, stride, &data)?;
        frame.Commit()?;
        encoder.Commit()?;
    }
    Ok(())
}

/// Short two-tone confirmation blip, synthesised once and played async.
pub fn play_save_sound() {
    use std::sync::OnceLock;
    use windows::Win32::Media::Audio::{PlaySoundW, SND_ASYNC, SND_MEMORY, SND_NODEFAULT};
    static WAV: OnceLock<Vec<u8>> = OnceLock::new();
    let wav = WAV.get_or_init(|| {
        let rate = 44_100u32;
        let mut pcm: Vec<i16> = Vec::new();
        for (freq, ms) in [(880.0f32, 60u32), (1320.0, 90)] {
            let n = rate * ms / 1000;
            for i in 0..n {
                let t = i as f32 / rate as f32;
                let env = (1.0 - i as f32 / n as f32).powi(2);
                pcm.push(((t * freq * std::f32::consts::TAU).sin() * env * 9000.0) as i16);
            }
        }
        let data_len = (pcm.len() * 2) as u32;
        let mut w = Vec::with_capacity(44 + data_len as usize);
        w.extend_from_slice(b"RIFF");
        w.extend_from_slice(&(36 + data_len).to_le_bytes());
        w.extend_from_slice(b"WAVEfmt ");
        w.extend_from_slice(&16u32.to_le_bytes());
        w.extend_from_slice(&1u16.to_le_bytes());
        w.extend_from_slice(&1u16.to_le_bytes());
        w.extend_from_slice(&rate.to_le_bytes());
        w.extend_from_slice(&(rate * 2).to_le_bytes());
        w.extend_from_slice(&2u16.to_le_bytes());
        w.extend_from_slice(&16u16.to_le_bytes());
        w.extend_from_slice(b"data");
        w.extend_from_slice(&data_len.to_le_bytes());
        for s in pcm {
            w.extend_from_slice(&s.to_le_bytes());
        }
        w
    });
    unsafe {
        let _ = PlaySoundW(PCWSTR(wav.as_ptr() as _), None, SND_MEMORY | SND_ASYNC | SND_NODEFAULT);
    }
}

/// Keep `hwnd` out of screen capture, so an overlay doesn't end up in
/// Rewind's own clips. Must happen before the window gets `WS_EX_LAYERED`;
/// Windows fails it with ERROR_NOT_ENOUGH_MEMORY after.
pub fn exclude_from_capture(hwnd: HWND) {
    use windows::Win32::UI::WindowsAndMessaging::{SetWindowDisplayAffinity, WDA_EXCLUDEFROMCAPTURE};
    if let Err(e) = unsafe { SetWindowDisplayAffinity(hwnd, WDA_EXCLUDEFROMCAPTURE) } {
        tracing::warn!("exclude overlay from capture: {e}");
    }
}

/// Make `hwnd` a passive overlay: never activated by clicks or showing, and
/// no taskbar or Alt+Tab entry.
pub fn make_overlay(hwnd: HWND) {
    use windows::Win32::UI::WindowsAndMessaging::{GetWindowLongPtrW, SetWindowLongPtrW, GWL_EXSTYLE, WS_EX_APPWINDOW, WS_EX_NOACTIVATE, WS_EX_TOOLWINDOW};
    unsafe {
        let ex = GetWindowLongPtrW(hwnd, GWL_EXSTYLE) & !(WS_EX_APPWINDOW.0 as isize);
        SetWindowLongPtrW(hwnd, GWL_EXSTYLE, ex | (WS_EX_NOACTIVATE.0 | WS_EX_TOOLWINDOW.0) as isize);
    }
}

/// Show `hwnd` topmost over exactly the physical rect `(x, y, w, h)` without
/// taking focus from whatever has it (a game, usually). False, with the
/// window left hidden, if Windows won't keep it there.
///
/// Moving a window onto a monitor with a different DPI makes Windows send
/// WM_DPICHANGED, and the window rescales itself to keep its logical size:
/// a full-screen overlay then spills onto the neighbouring monitors. So the
/// window is moved first, sized once it's on the target monitor, and checked.
pub fn place_overlay(hwnd: HWND, (x, y, w, h): (i32, i32, u32, u32)) -> bool {
    use windows::Win32::Foundation::RECT;
    use windows::Win32::UI::WindowsAndMessaging::{SetWindowPos, HWND_TOPMOST, SWP_NOACTIVATE, SWP_NOMOVE, SWP_NOSIZE, SWP_SHOWWINDOW};
    let (w, h) = (w as i32, h as i32);
    let want = RECT { left: x, top: y, right: x + w, bottom: y + h };
    let mut got = RECT::default();
    unsafe {
        let _ = SetWindowPos(hwnd, Some(HWND_TOPMOST), x, y, 0, 0, SWP_NOACTIVATE | SWP_NOSIZE);
        // A second pass only matters if the first one's DPI change resized it.
        for _ in 0..2 {
            let _ = SetWindowPos(hwnd, Some(HWND_TOPMOST), x, y, w, h, SWP_NOACTIVATE);
            if GetWindowRect(hwnd, &mut got).is_ok() && got == want {
                let _ = SetWindowPos(hwnd, Some(HWND_TOPMOST), 0, 0, 0, 0, SWP_NOACTIVATE | SWP_NOMOVE | SWP_NOSIZE | SWP_SHOWWINDOW);
                return true;
            }
        }
    }
    tracing::warn!(
        "overlay: wanted {w}x{h} at ({x}, {y}), Windows kept {}x{} at ({}, {}); leaving it hidden",
        got.right - got.left,
        got.bottom - got.top,
        got.left,
        got.top
    );
    hide_window(hwnd);
    false
}

pub fn hide_window(hwnd: HWND) {
    use windows::Win32::UI::WindowsAndMessaging::{ShowWindow, SW_HIDE};
    unsafe {
        let _ = ShowWindow(hwnd, SW_HIDE);
    }
}
