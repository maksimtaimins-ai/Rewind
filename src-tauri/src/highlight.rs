//! Recording highlight: a border or corner dot over the recorded display while
//! the buffer is recording. It is its own transparent, click-through,
//! always-on-top window that never takes focus and is left out of capture.

use std::sync::atomic::{AtomicBool, Ordering};

use parking_lot::Mutex;
use tauri::{AppHandle, Emitter, Manager, WebviewUrl, WebviewWindow, WebviewWindowBuilder};

use crate::engine::{State, Status};
use crate::settings::Settings;

pub const LABEL: &str = "highlight";

/// Whether the window was last asked to be visible, so the twice-a-second
/// status tick only bothers the main thread when that changes.
static SHOWN: AtomicBool = AtomicBool::new(false);

/// The monitor rect the window was last placed over, so a display change
/// while it's up moves it.
static PLACED: Mutex<Option<Rect>> = Mutex::new(None);

type Rect = (i32, i32, u32, u32);

fn wanted(s: &Settings, st: &Status) -> bool {
    let h = &s.highlight;
    s.first_run_complete && h.enabled() && st.state == State::Recording && (!h.only_in_game || st.game.is_some())
}

/// Called with every status update: show or hide to match.
pub fn on_status(app: &AppHandle, s: &Settings, st: &Status) {
    let want = wanted(s, st);
    let target = crate::capture::overlay_rect(s);
    let moved = want && SHOWN.load(Ordering::Relaxed) && *PLACED.lock() != target;
    if SHOWN.swap(want, Ordering::Relaxed) != want || moved {
        let app = app.clone();
        // Windows are created and shown on the main thread.
        let _ = app.clone().run_on_main_thread(move || apply(&app, want, target));
    }
}

/// Settings changed: restyle the window and show, hide or close it.
pub fn on_settings(app: &AppHandle, s: &Settings, st: &Status) {
    let enabled = s.highlight.enabled();
    let want = wanted(s, st);
    let target = crate::capture::overlay_rect(s);
    SHOWN.store(want, Ordering::Relaxed);
    if enabled {
        let _ = app.emit_to(LABEL, "settings-changed", s);
    }
    // Called from the synchronous `set_settings` command.
    crate::on_main_thread_later(app, move |app| {
        if enabled {
            apply(app, want, target);
        } else if let Some(w) = app.get_webview_window(LABEL) {
            // Off: no window kept around at all.
            *PLACED.lock() = None;
            let _ = w.destroy();
        }
    });
}

/// `target`: the monitor rect to cover, None if no display is usable.
fn apply(app: &AppHandle, show: bool, target: Option<Rect>) {
    // A broken display gets no window built at all.
    let w = match (show && target.is_some(), app.get_webview_window(LABEL)) {
        (_, Some(w)) => w,
        (false, None) => {
            if show {
                tracing::warn!("highlight: no usable monitor, not showing");
            }
            return;
        }
        (true, None) => match build(app) {
            Ok(w) => w,
            Err(e) => {
                tracing::error!("highlight window: {e}");
                return;
            }
        },
    };
    let Ok(hwnd) = w.hwnd() else { return };
    let mut placed = PLACED.lock();
    match target.filter(|_| show) {
        // Remembered even if placing fails, so the status tick doesn't retry
        // twice a second; the next display change tries again.
        Some(r) => {
            if *placed != Some(r) {
                tracing::info!("highlight: covering {}x{} at ({}, {})", r.2, r.3, r.0, r.1);
            }
            crate::platform::place_overlay(hwnd, r);
            *placed = Some(r);
        }
        None => {
            if show {
                tracing::warn!("highlight: no usable monitor, hiding");
            }
            crate::platform::hide_window(hwnd);
            *placed = None;
        }
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
