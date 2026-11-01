pub mod analysis;
pub mod audio;
pub mod bench;
pub mod buffer;
pub mod capture;
mod commands;
pub mod encode;
pub mod engine;
pub mod hud;
pub mod export;
pub mod gpu;
pub mod highlight;
pub mod hotkey;
pub mod input;
pub mod library;
pub mod mux;
pub mod pipeline;
pub mod platform;
pub mod profiles;
pub mod recycled;
pub mod settings;
pub mod share;
pub mod watch;

use engine::{Engine, Events, Level, Status};
use library::ClipInfo;
use tauri::menu::{Menu, MenuItem, PredefinedMenuItem};
use tauri::tray::{MouseButton, MouseButtonState, TrayIconBuilder, TrayIconEvent};
use tauri::{AppHandle, Emitter, Manager, WindowEvent};

struct UiEvents(AppHandle);

impl Events for UiEvents {
    fn status(&self, status: &Status) {
        let _ = self.0.emit("status", status);
        highlight::on_status(&self.0, &self.0.state::<Engine>().effective_settings(), status);
        if let Some(tray) = self.0.tray_by_id("main") {
            let text = match status.state {
                engine::State::Recording => {
                    format!("Rewind · Buffer {:.0}s / {}s", status.buffered_secs.min(status.buffer_secs as f64), status.buffer_secs)
                }
                engine::State::Paused => "Rewind · Paused while a private app is focused".to_string(),
                engine::State::Idle => "Rewind · Starting capture".to_string(),
                engine::State::Error => "Rewind · Error".to_string(),
            };
            let _ = tray.set_tooltip(Some(text));
        }
    }

    fn clip_saved(&self, clip: &ClipInfo) {
        let _ = self.0.emit("clip-saved", clip);
        if self.0.state::<Engine>().settings().play_sound_on_save {
            platform::play_save_sound();
        }
    }

    fn library_changed(&self) {
        let _ = self.0.emit("library-changed", ());
    }

    fn toggle_hud(&self) {
        let app = self.0.clone();
        let on = !app.state::<Engine>().settings().hud.enabled;
        // Windows are created and destroyed on the main thread.
        let _ = self.0.run_on_main_thread(move || {
            if let Err(e) = hud::set_enabled(&app, on) {
                tracing::warn!("toggle HUD: {e:#}");
            }
        });
    }

    fn profile_changed(&self, effective: &settings::Settings) {
        let _ = self.0.emit("profile-changed", ());
        // A profile may restyle or turn off the recording highlight.
        highlight::on_settings(&self.0, effective, &self.0.state::<Engine>().status());
    }

    fn notice(&self, level: Level, message: &str) {
        tracing::info!("notice {level:?}: {message}");
        let _ = self.0.emit("notice", serde_json::json!({ "level": level, "message": message }));
    }
}

pub(crate) fn show_main(app: &AppHandle) {
    if let Some(w) = app.get_webview_window("main") {
        let _ = w.show();
        let _ = w.unminimize();
        let _ = w.set_focus();
    }
}

/// Run `f` on the main thread once the current main-thread work is done.
/// For window creation from synchronous commands (`set_settings`): building a
/// webview window inside one never completes on Windows, and
/// `run_on_main_thread` runs inline when called from the main thread, so this
/// posts from another thread to get it queued.
pub(crate) fn on_main_thread_later(app: &AppHandle, f: impl FnOnce(&AppHandle) + Send + 'static) {
    let app = app.clone();
    std::thread::spawn(move || {
        let _ = app.clone().run_on_main_thread(move || f(&app));
    });
}

/// Bring up the full window, leaving the compact HUD if it's on.
fn open_full(app: &AppHandle) {
    if app.state::<Engine>().settings().hud.enabled {
        let _ = hud::set_enabled(app, false);
    } else {
        show_main(app);
    }
}

fn build_tray(app: &AppHandle) -> tauri::Result<()> {
    let open = MenuItem::with_id(app, "open", "Open Rewind", true, None::<&str>)?;
    let save = MenuItem::with_id(app, "save", "Save clip", true, None::<&str>)?;
    let folder = MenuItem::with_id(app, "folder", "Open clips folder", true, None::<&str>)?;
    let quit_item = MenuItem::with_id(app, "quit", "Quit Rewind", true, None::<&str>)?;
    let sep = PredefinedMenuItem::separator(app)?;
    let menu = Menu::with_items(app, &[&open, &save, &folder, &sep, &quit_item])?;
    TrayIconBuilder::with_id("main")
        .icon(app.default_window_icon().cloned().expect("app icon"))
        .tooltip("Rewind")
        .menu(&menu)
        .show_menu_on_left_click(false)
        .on_menu_event(|app, event| {
            let engine = app.state::<Engine>();
            match event.id.as_ref() {
                "open" => open_full(app),
                "save" => engine.handle(hotkey::Action::SaveClip),
                "folder" => {
                    let dir = engine.clips_dir_path();
                    let _ = std::fs::create_dir_all(&dir);
                    let _ = tauri_plugin_opener::open_path(dir, None::<&str>);
                }
                "quit" => quit(app),
                _ => {}
            }
        })
        .on_tray_icon_event(|tray, event| {
            if let TrayIconEvent::Click { button: MouseButton::Left, button_state: MouseButtonState::Up, .. } = event {
                open_full(tray.app_handle());
            }
        })
        .build(app)?;
    Ok(())
}

/// Take the tray icon down now. Tauri only drops it after the exit callbacks
/// (and so after `Engine::shutdown`), and only if no other handle to it is
/// alive; if shutdown is slow or the process is killed meanwhile, Windows
/// keeps a dead icon in the tray until someone hovers it.
pub(crate) fn remove_tray(app: &AppHandle) {
    if let Some(tray) = app.remove_tray_by_id("main") {
        // Hiding sends NIM_DELETE right away, whoever still holds a handle.
        let _ = tray.set_visible(false);
    }
}

/// Quit Rewind: tray icon first, then stop the pipeline, then the event loop.
pub(crate) fn quit(app: &AppHandle) {
    remove_tray(app);
    app.state::<Engine>().shutdown();
    app.exit(0);
}

/// Log to `%APPDATA%\Rewind\rewind.log` (truncated each launch); panics included.
fn init_logging() {
    let dir = settings::app_data_dir();
    let _ = std::fs::create_dir_all(&dir);
    let filter = tracing_subscriber::EnvFilter::try_from_default_env().unwrap_or_else(|_| "info".into());
    match std::fs::File::create(dir.join("rewind.log")) {
        Ok(file) => tracing_subscriber::fmt().with_ansi(false).with_env_filter(filter).with_writer(std::sync::Mutex::new(file)).init(),
        Err(_) => tracing_subscriber::fmt().with_env_filter(filter).init(),
    }
    std::panic::set_hook(Box::new(|info| tracing::error!("panic: {info}")));
}

pub fn run() {
    let start_hidden = std::env::args().any(|a| a == "--tray");
    // `rewind --quit` asks a running Rewind to exit cleanly (the installer uses it).
    let quit_only = std::env::args().any(|a| a == "--quit");

    tauri::Builder::default()
        .plugin(tauri_plugin_single_instance::init(|app, args, _cwd| {
            if args.iter().any(|a| a == "--quit") {
                quit(app);
            } else {
                open_full(app);
            }
        }))
        .plugin(tauri_plugin_opener::init())
        .plugin(tauri_plugin_dialog::init())
        .setup(move |app| {
            let handle = app.handle().clone();
            if quit_only {
                // Nothing was running to quit.
                handle.exit(0);
                return Ok(());
            }
            // Only now: a second launch exits in the single-instance plugin
            // above and must not truncate the running instance's log.
            init_logging();
            // Starts the always-on capture -> encode -> replay buffer pipeline on
            // background threads. It runs until the app quits; saving only reads it.
            let engine = Engine::start(Box::new(UiEvents(handle.clone())));
            let settings = engine.settings();
            let emit_handle = handle.clone();
            let lan = share::lan::Lan::new(
                engine.clone(),
                std::sync::Arc::new(move |event: &str, payload: serde_json::Value| {
                    // Someone asking to send a clip needs the window, even from the tray.
                    if event == "lan-offer" {
                        show_main(&emit_handle);
                    }
                    let _ = emit_handle.emit(event, payload);
                }),
            );
            let watch_handle = handle.clone();
            watch::start(engine.clone(), std::sync::Arc::new(move |event: &str, payload: serde_json::Value| {
                let _ = watch_handle.emit(event, payload);
            }));
            app.manage(engine);
            app.manage(lan);
            build_tray(&handle)?;
            let hidden = settings.first_run_complete && (start_hidden || settings.start_minimized);
            if settings.hud.enabled && settings.first_run_complete {
                // The pill is the minimized form; it shows either way.
                hud::apply(&handle, &settings);
            } else if !hidden {
                show_main(&handle);
            }
            Ok(())
        })
        .on_menu_event(|app, event| hud::on_menu(app, event.id.as_ref()))
        .on_window_event(|window, event| match event {
            // Closing the window keeps Rewind in the tray; the buffer keeps running.
            // Closing the pill (Alt+F4) goes back to the full window.
            WindowEvent::CloseRequested { api, .. } => {
                api.prevent_close();
                if window.label() == hud::LABEL {
                    let _ = hud::set_enabled(window.app_handle(), false);
                } else {
                    let _ = window.hide();
                }
            }
            WindowEvent::Moved(pos) if window.label() == hud::LABEL => hud::moved(window.app_handle(), *pos),
            _ => {}
        })
        .invoke_handler(tauri::generate_handler![
            commands::get_settings,
            commands::set_settings,
            commands::default_settings,
            commands::get_status,
            commands::list_monitors,
            commands::list_encoders,
            commands::list_microphones,
            commands::run_benchmark,
            commands::list_clips,
            commands::rename_clip,
            commands::delete_clips,
            commands::list_recycled,
            commands::restore_clips,
            commands::library_meta,
            commands::rename_session,
            commands::set_favorite,
            commands::set_tags,
            commands::set_tag_color,
            commands::clips_dir,
            commands::save_clip_now,
            commands::run_action,
            commands::save_marker,
            commands::take_screenshot,
            commands::trim_clip,
            commands::merge_clips,
            commands::export_clip,
            commands::set_thumbnail,
            commands::clip_keyframes,
            commands::autostart_enabled,
            commands::export_settings,
            commands::import_settings,
            commands::running_apps,
            commands::post_to_discord,
            commands::lan_peers,
            commands::lan_send,
            commands::lan_answer,
            commands::watch_history,
            commands::set_compact_hud,
            commands::run_live_benchmark,
            commands::hud_menu,
            commands::analyze_clips,
            commands::keystroke_presets,
        ])
        .build(tauri::generate_context!())
        .expect("error while building Rewind")
        .run(|app, event| {
            if let tauri::RunEvent::ExitRequested { .. } | tauri::RunEvent::Exit = event {
                remove_tray(app);
                // Flush state so nothing in flight is lost on shutdown.
                if let Some(engine) = app.try_state::<Engine>() {
                    engine.shutdown();
                }
            }
        });
}
