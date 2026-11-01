pub mod audio;
pub mod buffer;
pub mod capture;
mod commands;
pub mod encode;
pub mod engine;
pub mod export;
pub mod gpu;
pub mod hotkey;
pub mod library;
pub mod mux;
pub mod pipeline;
pub mod platform;
pub mod profiles;
pub mod recycled;
pub mod settings;

use engine::{Engine, Events, Level, Status};
use library::ClipInfo;
use tauri::menu::{Menu, MenuItem, PredefinedMenuItem};
use tauri::tray::{MouseButton, MouseButtonState, TrayIconBuilder, TrayIconEvent};
use tauri::{AppHandle, Emitter, Manager, WindowEvent};

struct UiEvents(AppHandle);

impl Events for UiEvents {
    fn status(&self, status: &Status) {
        let _ = self.0.emit("status", status);
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

    fn notice(&self, level: Level, message: &str) {
        tracing::info!("notice {level:?}: {message}");
        let _ = self.0.emit("notice", serde_json::json!({ "level": level, "message": message }));
    }
}

fn show_main(app: &AppHandle) {
    if let Some(w) = app.get_webview_window("main") {
        let _ = w.show();
        let _ = w.unminimize();
        let _ = w.set_focus();
    }
}

fn build_tray(app: &AppHandle) -> tauri::Result<()> {
    let open = MenuItem::with_id(app, "open", "Open Rewind", true, None::<&str>)?;
    let save = MenuItem::with_id(app, "save", "Save clip", true, None::<&str>)?;
    let folder = MenuItem::with_id(app, "folder", "Open clips folder", true, None::<&str>)?;
    let quit = MenuItem::with_id(app, "quit", "Quit Rewind", true, None::<&str>)?;
    let sep = PredefinedMenuItem::separator(app)?;
    let menu = Menu::with_items(app, &[&open, &save, &folder, &sep, &quit])?;
    TrayIconBuilder::with_id("main")
        .icon(app.default_window_icon().cloned().expect("app icon"))
        .tooltip("Rewind")
        .menu(&menu)
        .show_menu_on_left_click(false)
        .on_menu_event(|app, event| {
            let engine = app.state::<Engine>();
            match event.id.as_ref() {
                "open" => show_main(app),
                "save" => engine.handle(hotkey::Action::SaveClip),
                "folder" => {
                    let dir = engine.clips_dir_path();
                    let _ = std::fs::create_dir_all(&dir);
                    let _ = tauri_plugin_opener::open_path(dir, None::<&str>);
                }
                "quit" => {
                    engine.shutdown();
                    app.exit(0);
                }
                _ => {}
            }
        })
        .on_tray_icon_event(|tray, event| {
            if let TrayIconEvent::Click { button: MouseButton::Left, button_state: MouseButtonState::Up, .. } = event {
                show_main(tray.app_handle());
            }
        })
        .build(app)?;
    Ok(())
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
    init_logging();
    let start_hidden = std::env::args().any(|a| a == "--tray");

    tauri::Builder::default()
        .plugin(tauri_plugin_single_instance::init(|app, _args, _cwd| show_main(app)))
        .plugin(tauri_plugin_opener::init())
        .plugin(tauri_plugin_dialog::init())
        .setup(move |app| {
            let handle = app.handle().clone();
            // Starts the always-on capture -> encode -> replay buffer pipeline on
            // background threads. It runs until the app quits; saving only reads it.
            let engine = Engine::start(Box::new(UiEvents(handle.clone())));
            let settings = engine.settings();
            app.manage(engine);
            build_tray(&handle)?;
            let hidden = settings.first_run_complete && (start_hidden || settings.start_minimized);
            if !hidden {
                show_main(&handle);
            }
            Ok(())
        })
        .on_window_event(|window, event| {
            // Closing the window keeps Rewind in the tray; the buffer keeps running.
            if let WindowEvent::CloseRequested { api, .. } = event {
                api.prevent_close();
                let _ = window.hide();
            }
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
            commands::save_marker,
            commands::take_screenshot,
            commands::trim_clip,
            commands::merge_clips,
            commands::export_clip,
            commands::clip_keyframes,
            commands::autostart_enabled,
            commands::export_settings,
            commands::import_settings,
            commands::running_apps,
        ])
        .build(tauri::generate_context!())
        .expect("error while building Rewind")
        .run(|app, event| {
            if let tauri::RunEvent::ExitRequested { .. } | tauri::RunEvent::Exit = event {
                // Flush state so nothing in flight is lost on shutdown.
                app.state::<Engine>().shutdown();
            }
        });
}
