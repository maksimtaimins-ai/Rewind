//! Live keystroke overlay: recent key presses (and clicks) drawn on screen as
//! they happen, in the export overlay's style. Its own transparent,
//! click-through, always-on-top window over the primary display that never
//! takes focus and is left out of capture.
//!
//! Off by default. While off there is no window, no forwarding thread, and
//! (unless clip recording wants it) no input hook. Events come from the
//! keystroke recorder's hook (`input::set_live_sink`), so the same filters
//! apply.

use std::sync::atomic::{AtomicBool, Ordering};

use parking_lot::Mutex;
use serde::Serialize;
use tauri::{AppHandle, Emitter, Manager, WebviewUrl, WebviewWindow, WebviewWindowBuilder};

use crate::input::{Event, Kind};
use crate::settings::Settings;

pub const LABEL: &str = "keys";

/// Whether events are being forwarded to the window.
static FORWARDING: AtomicBool = AtomicBool::new(false);

/// The window's monitor, physical pixels: (x, y, width, height). Clicks are
/// sent as fractions of it.
static SCREEN: Mutex<(i32, i32, u32, u32)> = Mutex::new((0, 0, 0, 0));

/// What the window gets per event: like a sidecar entry, with `x`/`y` as
/// 0-1 of the window's monitor.
#[derive(Debug, Clone, Serialize)]
struct LiveEvent {
    #[serde(rename = "type")]
    kind: Kind,
    key: String,
    mods: Vec<String>,
    x: Option<f32>,
    y: Option<f32>,
}

fn wanted(s: &Settings) -> bool {
    s.first_run_complete && s.keystrokes.shows_live()
}

/// Settings changed (or the app started): restyle the window, or create or
/// destroy it.
pub fn on_settings(app: &AppHandle, s: &Settings) {
    let want = wanted(s);
    forward(app, want);
    if want {
        let _ = app.emit_to(LABEL, "settings-changed", s);
    }
    // Called from the synchronous `set_settings` command.
    crate::on_main_thread_later(app, move |app| {
        if want {
            show(app);
        } else if let Some(w) = app.get_webview_window(LABEL) {
            // Off: no window kept around at all.
            let _ = w.destroy();
        }
    });
}

/// Start or stop the thread that passes hook events to the window.
fn forward(app: &AppHandle, on: bool) {
    if !on {
        if FORWARDING.swap(false, Ordering::Relaxed) {
            // Dropping the sender ends the thread.
            crate::input::set_live_sink(None);
        }
        return;
    }
    if FORWARDING.swap(true, Ordering::Relaxed) {
        return;
    }
    let (tx, rx) = crossbeam_channel::bounded::<Event>(256);
    let app = app.clone();
    let spawned = std::thread::Builder::new().name("rewind-live-keys".into()).spawn(move || {
        for ev in rx {
            if !matches!(ev.kind, Kind::KeyDown | Kind::KeyUp | Kind::MouseDown | Kind::MouseUp) {
                continue;
            }
            let (sx, sy, sw, sh) = *SCREEN.lock();
            let frac = |v: Option<i32>, o: i32, len: u32| v.filter(|_| len > 0).map(|v| (v - o) as f32 / len as f32);
            let out = LiveEvent { kind: ev.kind, x: frac(ev.x, sx, sw), y: frac(ev.y, sy, sh), key: ev.key, mods: ev.mods };
            let _ = app.emit_to(LABEL, "live-input", out);
        }
    });
    match spawned {
        Ok(_) => crate::input::set_live_sink(Some(tx)),
        Err(e) => {
            FORWARDING.store(false, Ordering::Relaxed);
            tracing::error!("live keystroke overlay: {e}");
        }
    }
}

fn show(app: &AppHandle) {
    let w = match app.get_webview_window(LABEL) {
        Some(w) => w,
        None => match build(app) {
            Ok(w) => w,
            Err(e) => {
                tracing::error!("live keystroke window: {e}");
                return;
            }
        },
    };
    let Ok(hwnd) = w.hwnd() else { return };
    // Re-read each time: the primary display can change.
    if let Some(m) = app.primary_monitor().ok().flatten() {
        let (p, size) = (m.position(), m.size());
        *SCREEN.lock() = (p.x, p.y, size.width, size.height);
        crate::platform::show_no_activate(hwnd, p.x, p.y, size.width as i32, size.height as i32);
    }
}

fn build(app: &AppHandle) -> tauri::Result<WebviewWindow> {
    let w = WebviewWindowBuilder::new(app, LABEL, WebviewUrl::App("keys".into()))
        .title("Rewind live keystrokes")
        .resizable(false)
        .maximizable(false)
        .minimizable(false)
        .decorations(false)
        .transparent(true)
        .shadow(false)
        .always_on_top(true)
        .skip_taskbar(true)
        .focused(false)
        .focusable(false)
        .visible(false)
        .build()?;
    // Before click-through: Windows refuses capture exclusion on a window
    // that is already layered.
    crate::platform::exclude_from_capture(w.hwnd()?);
    w.set_ignore_cursor_events(true)?;
    crate::platform::make_overlay(w.hwnd()?);
    Ok(w)
}
