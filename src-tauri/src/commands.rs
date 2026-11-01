//! Tauri commands exposed to the UI.

use std::path::PathBuf;

use tauri::State;

use crate::audio::{self, DeviceInfo};
use crate::capture::{self, MonitorInfo};
use crate::encode::{BenchResult, EncoderInfo};
use crate::engine::{Engine, Status};
use crate::library::{self, ClipInfo};
use crate::mux;
use crate::platform;
use crate::recycled;
use crate::settings::{self, Settings};

type R<T> = Result<T, String>;

fn err(e: impl std::fmt::Display) -> String {
    e.to_string()
}

#[tauri::command]
pub fn get_settings(engine: State<Engine>) -> Settings {
    engine.settings()
}

#[tauri::command]
pub fn set_settings(engine: State<Engine>, mut settings: Settings) -> R<()> {
    settings.sanitize();
    engine.set_settings(settings).map_err(err)
}

#[tauri::command]
pub fn default_settings() -> Settings {
    Settings::default()
}

#[tauri::command]
pub fn get_status(engine: State<Engine>) -> Status {
    engine.status()
}

#[tauri::command]
pub fn list_monitors() -> Vec<MonitorInfo> {
    capture::monitors()
}

#[tauri::command]
pub fn list_encoders(engine: State<Engine>) -> Vec<EncoderInfo> {
    engine.encoders()
}

#[tauri::command]
pub fn list_microphones() -> Vec<DeviceInfo> {
    platform::com_init();
    audio::microphones()
}

#[tauri::command]
pub async fn run_benchmark(engine: State<'_, Engine>, secs: f64) -> R<Vec<BenchResult>> {
    let engine = engine.inner().clone();
    tauri::async_runtime::spawn_blocking(move || {
        platform::com_init();
        engine.benchmark(secs.clamp(1.0, 30.0))
    })
    .await
    .map_err(err)
}

#[tauri::command]
pub fn list_clips(engine: State<Engine>) -> Vec<ClipInfo> {
    engine.clips()
}

#[tauri::command]
pub fn rename_clip(path: PathBuf, name: String) -> R<ClipInfo> {
    let p = library::rename(&path, &name).map_err(err)?;
    library::info(&p).map_err(err)
}

#[tauri::command]
pub fn set_favorite(path: PathBuf, favorite: bool) -> R<ClipInfo> {
    library::set_favorite(&path, favorite).map_err(err)?;
    library::info(&path).map_err(err)
}

#[tauri::command]
pub fn set_tags(path: PathBuf, tags: Vec<String>) -> R<ClipInfo> {
    library::set_tags(&path, &tags).map_err(err)?;
    library::info(&path).map_err(err)
}

#[tauri::command]
pub fn set_tag_color(tag: String, color: String) -> R<()> {
    library::update_meta(|m| {
        m.tag_colors.insert(tag, color);
    })
    .map_err(err)
}

/// Tag colours and session names from `library.json`.
#[tauri::command]
pub fn library_meta() -> serde_json::Value {
    let m = library::load_meta();
    serde_json::json!({ "tag_colors": m.tag_colors, "sessions": m.sessions })
}

/// Name a session; an empty name goes back to the default.
#[tauri::command]
pub fn rename_session(key: String, name: String) -> R<()> {
    let name = name.trim().to_string();
    library::update_meta(|m| {
        if name.is_empty() {
            m.sessions.remove(&key);
        } else {
            m.sessions.insert(key, name);
        }
    })
    .map_err(err)
}

/// Soft delete: clips go to Rewind's own bin and can be restored.
#[tauri::command]
pub fn delete_clips(paths: Vec<PathBuf>) -> R<usize> {
    recycled::delete(&paths).map_err(err)
}

#[tauri::command]
pub fn list_recycled() -> Vec<recycled::RecycledClip> {
    recycled::list()
}

#[tauri::command]
pub fn restore_clips(ids: Vec<String>) -> R<usize> {
    recycled::restore(&ids).map_err(err)
}

#[tauri::command]
pub fn clips_dir(engine: State<Engine>) -> PathBuf {
    let d = engine.clips_dir_path();
    let _ = std::fs::create_dir_all(&d);
    d
}

#[tauri::command]
pub fn save_clip_now(engine: State<Engine>, secs: Option<u32>) -> R<()> {
    engine.save_clip(secs).map_err(err)
}

#[tauri::command]
pub fn save_marker(engine: State<Engine>, id: u64, secs: u32) -> R<()> {
    engine.save_marker(id, secs).map_err(err)
}

#[tauri::command]
pub fn take_screenshot(engine: State<Engine>) -> R<()> {
    engine.screenshot().map_err(err)
}

#[tauri::command]
pub async fn trim_clip(engine: State<'_, Engine>, path: PathBuf, from: f64, to: f64) -> R<ClipInfo> {
    let engine = engine.inner().clone();
    tauri::async_runtime::spawn_blocking(move || engine.trim(&path, from, to)).await.map_err(err)?.map_err(err)
}

#[tauri::command]
pub async fn merge_clips(engine: State<'_, Engine>, paths: Vec<PathBuf>) -> R<ClipInfo> {
    let engine = engine.inner().clone();
    tauri::async_runtime::spawn_blocking(move || engine.merge(&paths)).await.map_err(err)?.map_err(err)
}

/// Re-encode a clip with an export preset; returns the new file's path.
#[tauri::command]
pub async fn export_clip(path: PathBuf, preset: crate::export::Preset) -> R<PathBuf> {
    tauri::async_runtime::spawn_blocking(move || {
        platform::com_init();
        crate::export::mf::export(&path, preset)
    })
    .await
    .map_err(err)?
    .map_err(|e| format!("{e:#}"))
}

/// Keyframe times (seconds) and duration, for the trim scrubber.
#[tauri::command]
pub async fn clip_keyframes(path: PathBuf) -> R<(Vec<f64>, f64)> {
    tauri::async_runtime::spawn_blocking(move || {
        let tracks = mux::reader::read_file(&path)?;
        Ok::<_, anyhow::Error>((mux::keyframe_times(&tracks), mux::duration_secs(&tracks)))
    })
    .await
    .map_err(err)?
    .map_err(err)
}

#[tauri::command]
pub fn autostart_enabled() -> bool {
    platform::autostart_enabled()
}

#[tauri::command]
pub fn export_settings(engine: State<Engine>, path: PathBuf) -> R<()> {
    let text = toml::to_string_pretty(&engine.settings()).map_err(err)?;
    std::fs::write(path, text).map_err(err)
}

/// Import a shared settings file. Machine-specific paths are kept.
#[tauri::command]
pub fn import_settings(engine: State<Engine>, path: PathBuf) -> R<Settings> {
    let text = std::fs::read_to_string(path).map_err(err)?;
    let mut s = settings::parse(&text).map_err(err)?;
    let current = engine.settings();
    s.clips_dir = current.clips_dir;
    s.first_run_complete = current.first_run_complete;
    engine.set_settings(s.clone()).map_err(err)?;
    Ok(s)
}

#[tauri::command]
pub fn running_apps() -> Vec<String> {
    let mut v: Vec<String> = platform::processes().into_iter().map(|p| p.exe).collect();
    v.sort_by_key(|s| s.to_ascii_lowercase());
    v.dedup_by(|a, b| a.eq_ignore_ascii_case(b));
    v
}
