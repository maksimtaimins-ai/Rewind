//! Screen capture. Windows uses Windows.Graphics.Capture (handles fullscreen
//! exclusive, HDR and VRR; no hooks or injection, so anti-cheat safe).

#[cfg(windows)]
pub mod wgc;

use serde::Serialize;

#[derive(Debug, Clone, Serialize)]
pub struct MonitorInfo {
    pub index: u32,
    pub name: String,
    pub width: u32,
    pub height: u32,
    pub primary: bool,
    #[serde(skip)]
    pub handle: isize,
}

/// A running capture; dropping it stops capture.
pub trait CaptureSession: Send {
    fn stop(&mut self);
}

#[cfg(windows)]
pub fn monitors() -> Vec<MonitorInfo> {
    use windows::core::BOOL;
    use windows::Win32::Foundation::{LPARAM, RECT};
    use windows::Win32::Graphics::Gdi::{EnumDisplayMonitors, GetMonitorInfoW, HDC, HMONITOR, MONITORINFO, MONITORINFOEXW};

    unsafe extern "system" fn cb(m: HMONITOR, _: HDC, _: *mut RECT, data: LPARAM) -> BOOL {
        let list = unsafe { &mut *(data.0 as *mut Vec<MonitorInfo>) };
        let mut info = MONITORINFOEXW::default();
        info.monitorInfo.cbSize = std::mem::size_of::<MONITORINFOEXW>() as u32;
        if unsafe { GetMonitorInfoW(m, &mut info as *mut _ as *mut MONITORINFO) }.as_bool() {
            let r = info.monitorInfo.rcMonitor;
            let name = String::from_utf16_lossy(&info.szDevice).trim_end_matches('\0').replace("\\\\.\\", "");
            list.push(MonitorInfo {
                index: 0,
                name,
                width: (r.right - r.left) as u32,
                height: (r.bottom - r.top) as u32,
                primary: info.monitorInfo.dwFlags & 1 != 0,
                handle: m.0 as isize,
            });
        }
        true.into()
    }

    let mut list: Vec<MonitorInfo> = Vec::new();
    unsafe {
        let _ = EnumDisplayMonitors(None, None, Some(cb), LPARAM(&mut list as *mut _ as isize));
    }
    list.sort_by_key(|m| !m.primary);
    for (i, m) in list.iter_mut().enumerate() {
        m.index = i as u32;
    }
    list
}

#[cfg(not(windows))]
pub fn monitors() -> Vec<MonitorInfo> {
    Vec::new()
}
