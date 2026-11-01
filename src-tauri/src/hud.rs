//! Compact HUD: a small always-on-top pill in place of the main window. It is
//! its own transparent window sized to the pill, so everything around the
//! pill stays clickable.

use std::sync::OnceLock;
use std::time::{Duration, Instant};

use parking_lot::Mutex;
use tauri::menu::{Menu, MenuItem, PredefinedMenuItem};
use tauri::{AppHandle, Emitter, Manager, PhysicalPosition, WebviewUrl, WebviewWindow, WebviewWindowBuilder};

use crate::engine::Engine;
use crate::settings::Settings;

pub const LABEL: &str = "hud";
const WIDTH: f64 = 256.0;
const HEIGHT: f64 = 56.0;

/// Show the pill (and hide the main window) or the other way round, to match
/// `settings.hud.enabled`.
pub fn apply(app: &AppHandle, s: &Settings) {
    let on = s.hud.enabled && s.first_run_complete;
    match (on, app.get_webview_window(LABEL)) {
        (true, Some(w)) => {
            let _ = w.show();
        }
        (true, None) => {
            if let Err(e) = build(app, s) {
                tracing::error!("HUD window: {e}");
                return;
            }
        }
        (false, Some(w)) => {
            let _ = w.destroy();
            crate::show_main(app);
            return;
        }
        (false, None) => return,
    }
    if let Some(main) = app.get_webview_window("main") {
        let _ = main.hide();
    }
}

fn build(app: &AppHandle, s: &Settings) -> tauri::Result<WebviewWindow> {
    let w = WebviewWindowBuilder::new(app, LABEL, WebviewUrl::App("hud".into()))
        .title("Rewind")
        .inner_size(WIDTH, HEIGHT)
        .resizable(false)
        .maximizable(false)
        .minimizable(false)
        .decorations(false)
        .transparent(true)
        .shadow(false)
        .always_on_top(true)
        .skip_taskbar(true)
        .focused(false)
        .visible(false)
        .build()?;
    let saved = s.hud.x.zip(s.hud.y).map(|(x, y)| PhysicalPosition::new(x, y));
    // A saved spot on a monitor that's gone would leave the pill unreachable.
    let on_screen = |p: &PhysicalPosition<i32>| {
        app.available_monitors().unwrap_or_default().iter().any(|m| {
            let (mp, ms) = (m.position(), m.size());
            p.x >= mp.x && p.y >= mp.y && p.x < mp.x + ms.width as i32 - 40 && p.y < mp.y + ms.height as i32 - 20
        })
    };
    let pos = saved.filter(on_screen).or_else(|| {
        // Top right of the main screen.
        let m = app.primary_monitor().ok().flatten()?;
        let k = m.scale_factor();
        Some(PhysicalPosition::new(m.position().x + m.size().width as i32 - ((WIDTH + 24.0) * k) as i32, m.position().y + (24.0 * k) as i32))
    });
    if let Some(p) = pos {
        w.set_position(p)?;
    }
    w.show()?;
    Ok(w)
}

/// Turn the HUD on or off and save that.
pub fn set_enabled(app: &AppHandle, on: bool) -> anyhow::Result<()> {
    let engine = app.state::<Engine>();
    let mut s = engine.settings();
    if s.hud.enabled != on {
        s.hud.enabled = on;
        engine.set_settings(s.clone())?;
        let _ = app.emit("settings-changed", &s);
    }
    apply(app, &s);
    Ok(())
}

static MOVED: Mutex<Option<(PhysicalPosition<i32>, Instant)>> = Mutex::new(None);

/// Remember where the pill was dragged, once it stops moving.
pub fn moved(app: &AppHandle, pos: PhysicalPosition<i32>) {
    *MOVED.lock() = Some((pos, Instant::now()));
    static SAVER: OnceLock<()> = OnceLock::new();
    SAVER.get_or_init(|| {
        let app = app.clone();
        std::thread::spawn(move || loop {
            std::thread::sleep(Duration::from_millis(300));
            let settled = {
                let mut m = MOVED.lock();
                match *m {
                    Some((p, at)) if at.elapsed() >= Duration::from_millis(500) => {
                        *m = None;
                        Some(p)
                    }
                    _ => None,
                }
            };
            if let Some(p) = settled {
                let engine = app.state::<Engine>();
                let mut s = engine.settings();
                if (s.hud.x, s.hud.y) != (Some(p.x), Some(p.y)) {
                    s.hud.x = Some(p.x);
                    s.hud.y = Some(p.y);
                    if let Err(e) = engine.set_settings(s) {
                        tracing::warn!("save HUD position: {e:#}");
                    }
                }
            }
        });
    });
}

/// Right-click menu on the pill.
pub fn popup_menu(w: &WebviewWindow) -> tauri::Result<()> {
    let app = w.app_handle();
    let menu = Menu::with_items(
        app,
        &[
            &MenuItem::with_id(app, "hud-save", "Save clip", true, None::<&str>)?,
            &MenuItem::with_id(app, "hud-shot", "Screenshot", true, None::<&str>)?,
            &PredefinedMenuItem::separator(app)?,
            &MenuItem::with_id(app, "hud-open", "Full window", true, None::<&str>)?,
            &MenuItem::with_id(app, "hud-settings", "Settings…", true, None::<&str>)?,
            &PredefinedMenuItem::separator(app)?,
            &MenuItem::with_id(app, "hud-quit", "Quit Rewind", true, None::<&str>)?,
        ],
    )?;
    w.popup_menu(&menu)
}

pub fn on_menu(app: &AppHandle, id: &str) {
    let engine = app.state::<Engine>();
    match id {
        "hud-save" => engine.handle(crate::hotkey::Action::SaveClip),
        "hud-shot" => engine.handle(crate::hotkey::Action::Screenshot),
        "hud-open" => {
            let _ = set_enabled(app, false);
        }
        "hud-settings" => {
            let _ = set_enabled(app, false);
            let _ = app.emit_to("main", "navigate", "/settings#appearance");
        }
        "hud-quit" => crate::quit(app),
        _ => {}
    }
}
