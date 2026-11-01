//! Recording highlight: a border or corner dot over the primary display while
//! the buffer is recording. It is its own transparent, click-through,
//! always-on-top window that never takes focus and is left out of capture.

use std::sync::atomic::{AtomicBool, Ordering};

use tauri::{AppHandle, Emitter, Manager, WebviewUrl, WebviewWindow, WebviewWindowBuilder};

use crate::engine::{State, Status};
use crate::settings::Settings;

pub const LABEL: &str = "highlight";

/// Whether the window was last asked to be visible, so the twice-a-second
/// status tick only bothers the main thread when that changes.
static SHOWN: AtomicBool = AtomicBool::new(false);

fn wanted(s: &Settings, st: &Status) -> bool {
    let h = &s.highlight;
    s.first_run_complete && h.enabled() && st.state == State::Recording && (!h.only_in_game || st.game.is_some())
}

/// Called with every status update: show or hide to match.
pub fn on_status(app: &AppHandle, s: &Settings, st: &Status) {
    let want = wanted(s, st);
    if SHOWN.swap(want, Ordering::Relaxed) != want {
        let app = app.clone();
        // Windows are created and shown on the main thread.
        let _ = app.clone().run_on_main_thread(move || apply(&app, want));
    }
}

/// Settings changed: restyle the window and show, hide or close it.
pub fn on_settings(app: &AppHandle, s: &Settings, st: &Status) {
    let enabled = s.highlight.enabled();
    let want = wanted(s, st);
    SHOWN.store(want, Ordering::Relaxed);
    if enabled {
        let _ = app.emit_to(LABEL, "settings-changed", s);
    }
    // This runs inside the synchronous `set_settings` command, and building a
    // webview window there never completes on Windows. `run_on_main_thread`
    // runs inline when called from the main thread, so post it from another
    // thread to get it queued until the command has returned.
    let handle = app.clone();
    std::thread::spawn(move || {
        let _ = handle.clone().run_on_main_thread(move || {
            if enabled {
                apply(&handle, want);
            } else if let Some(w) = handle.get_webview_window(LABEL) {
                // Off: no window kept around at all.
                let _ = w.destroy();
            }
        });
    });
}

fn apply(app: &AppHandle, show: bool) {
    let w = match (show, app.get_webview_window(LABEL)) {
        (_, Some(w)) => w,
        (false, None) => return,
        (true, None) => match build(app) {
            Ok(w) => w,
            Err(e) => {
                tracing::error!("highlight window: {e}");
                return;
            }
        },
    };
    let Ok(hwnd) = w.hwnd() else { return };
    if !show {
        crate::platform::hide_window(hwnd);
        return;
    }
    // Re-read each time: the primary display can change while hidden.
    if let Some(m) = app.primary_monitor().ok().flatten() {
        let (p, size) = (m.position(), m.size());
        crate::platform::show_no_activate(hwnd, p.x, p.y, size.width as i32, size.height as i32);
    }
}

fn build(app: &AppHandle) -> tauri::Result<WebviewWindow> {
    let w = WebviewWindowBuilder::new(app, LABEL, WebviewUrl::App("highlight".into()))
        .title("Rewind recording highlight")
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
