//! Screen capture. Windows uses Windows.Graphics.Capture (handles fullscreen
//! exclusive, HDR and VRR; no hooks or injection, so anti-cheat safe).

#[cfg(windows)]
pub mod wgc;

use parking_lot::Mutex;
use serde::Serialize;

use crate::settings::Settings;

#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct MonitorInfo {
    pub index: u32,
    pub name: String,
    pub width: u32,
    pub height: u32,
    /// Top-left on the virtual desktop.
    pub x: i32,
    pub y: i32,
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
                x: r.left,
                y: r.top,
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

/// Current refresh rate of the monitor named `name` (`DISPLAY1`), in Hz.
#[cfg(windows)]
pub fn refresh_hz(name: &str) -> Option<u32> {
    use windows::core::PCWSTR;
    use windows::Win32::Graphics::Gdi::{EnumDisplaySettingsW, DEVMODEW, ENUM_CURRENT_SETTINGS};
    let wide: Vec<u16> = format!(r"\\.\{name}").encode_utf16().chain(Some(0)).collect();
    let mut mode = DEVMODEW { dmSize: std::mem::size_of::<DEVMODEW>() as u16, ..Default::default() };
    unsafe { EnumDisplaySettingsW(PCWSTR(wide.as_ptr()), ENUM_CURRENT_SETTINGS, &mut mode) }
        .as_bool()
        .then_some(mode.dmDisplayFrequency)
        // 0 and 1 mean "hardware default".
        .filter(|&hz| hz > 1)
}

#[cfg(not(windows))]
pub fn refresh_hz(_: &str) -> Option<u32> {
    None
}

/// Largest side a real display mode has; anything bigger is a bogus rect.
const MAX_SIDE: u32 = 16384;

impl MonitorInfo {
    /// Physical rect on the virtual desktop: x, y, width, height.
    pub fn rect(&self) -> (i32, i32, u32, u32) {
        (self.x, self.y, self.width, self.height)
    }

    /// Whether the rect looks like a working display. Negative x/y are normal
    /// (a screen left of or above the main one); a zero or huge size, or a
    /// main screen off the origin, means the display is mid-change.
    pub fn sane(&self) -> bool {
        (1..=MAX_SIDE).contains(&self.width) && (1..=MAX_SIDE).contains(&self.height) && (!self.primary || (self.x, self.y) == (0, 0))
    }
}

/// The monitor list from the last enumeration. Enumerating is cheap but
/// touches the display stack, so it is done once and redone only when a
/// cached entry stops matching Windows (unplugged, moved, mode changed) or
/// the monitor count changes.
static CACHE: Mutex<Vec<MonitorInfo>> = Mutex::new(Vec::new());

/// The cached monitor list, re-enumerated if it went stale.
pub fn cached() -> Vec<MonitorInfo> {
    let mut c = CACHE.lock();
    if c.is_empty() || !still_current(&c) {
        let fresh = monitors();
        if !c.is_empty() {
            tracing::info!("displays changed: {} -> {} monitor(s)", c.len(), fresh.len());
        }
        *c = fresh;
    }
    c.clone()
}

#[cfg(windows)]
fn still_current(list: &[MonitorInfo]) -> bool {
    use windows::Win32::Graphics::Gdi::{GetMonitorInfoW, HMONITOR, MONITORINFO};
    use windows::Win32::UI::WindowsAndMessaging::{GetSystemMetrics, SM_CMONITORS};
    if unsafe { GetSystemMetrics(SM_CMONITORS) } as usize != list.len() {
        return false;
    }
    list.iter().all(|m| {
        let mut info = MONITORINFO { cbSize: std::mem::size_of::<MONITORINFO>() as u32, ..Default::default() };
        unsafe { GetMonitorInfoW(HMONITOR(m.handle as _), &mut info) }.as_bool() && {
            let r = info.rcMonitor;
            (r.left, r.top, r.right - r.left, r.bottom - r.top) == (m.x, m.y, m.width as i32, m.height as i32)
                && (info.dwFlags & 1 != 0) == m.primary
        }
    })
}

#[cfg(not(windows))]
fn still_current(_: &[MonitorInfo]) -> bool {
    true
}

/// `index` from `list`, or the main screen if that index is gone.
fn pick(list: &[MonitorInfo], index: u32) -> Option<&MonitorInfo> {
    list.iter().find(|m| m.index == index).or_else(|| list.first())
}

/// The monitor to record, from `settings.video.monitor`.
pub fn capture_monitor(s: &Settings) -> Option<MonitorInfo> {
    pick(&cached(), s.video.monitor).cloned()
}

/// The monitor on-screen overlays (live keystrokes,
/// the HUD's default spot) go on: `settings.overlay_monitor` if set, else
/// the recorded one. None when that display looks broken: better no overlay
/// than a full-screen window on a display mid-change. Cheap enough for the
/// twice-a-second status tick; callers log.
pub fn overlay_monitor(s: &Settings) -> Option<MonitorInfo> {
    pick(&cached(), s.overlay_monitor.unwrap_or(s.video.monitor)).filter(|m| m.sane()).cloned()
}

/// `overlay_monitor`'s rect, which is what overlay windows compare and use.
pub fn overlay_rect(s: &Settings) -> Option<(i32, i32, u32, u32)> {
    overlay_monitor(s).map(|m| m.rect())
}

/// One log line per monitor and the one chosen, for triaging multi-monitor
/// reports from `rewind.log`.
pub fn log_monitors(list: &[MonitorInfo], chosen: &MonitorInfo) {
    tracing::info!("monitors: {} found, capturing #{} {} (handle {:#x})", list.len(), chosen.index, chosen.name, chosen.handle);
    for m in list {
        tracing::info!(
            "  #{} {}: {}x{} at ({}, {}){}{}, handle {:#x}",
            m.index,
            m.name,
            m.width,
            m.height,
            m.x,
            m.y,
            if m.primary { " primary" } else { "" },
            if m.sane() { "" } else { " (looks mid-change)" },
            m.handle
        );
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn mon(index: u32, x: i32, y: i32, width: u32, height: u32, primary: bool) -> MonitorInfo {
        MonitorInfo { index, name: format!("DISPLAY{}", index + 1), width, height, x, y, primary, handle: index as isize + 1 }
    }

    #[test]
    fn sane_rects() {
        assert!(mon(0, 0, 0, 2560, 1440, true).sane());
        // Left of the main screen, mixed DPI: negative origin is fine.
        assert!(mon(1, -1920, -300, 1920, 1080, false).sane());
        assert!(!mon(0, 0, 0, 0, 1080, true).sane());
        assert!(!mon(1, 0, 0, 40000, 1080, false).sane());
        // The main screen is always at the origin.
        assert!(!mon(0, 1920, 0, 1920, 1080, true).sane());
    }

    #[test]
    fn pick_falls_back_to_main() {
        let list = [mon(0, 0, 0, 1920, 1080, true), mon(1, 1920, 0, 1920, 1080, false), mon(2, -1920, 0, 1920, 1080, false)];
        assert_eq!(pick(&list, 2).unwrap().index, 2);
        assert_eq!(pick(&list, 7).unwrap().index, 0);
        assert!(pick(&[], 0).is_none());
    }
}
