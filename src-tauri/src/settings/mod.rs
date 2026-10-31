//! Versioned settings persisted as TOML in `%APPDATA%\Rewind\settings.toml`.
//!
//! Every change to the on-disk shape bumps [`SETTINGS_VERSION`] and adds a step
//! to [`migrate`], so old files keep loading.

use std::path::{Path, PathBuf};

use anyhow::{Context, Result};
use serde::{Deserialize, Serialize};

pub const SETTINGS_VERSION: u32 = 2;

/// Allowed range for [`BufferSettings::seconds`].
pub const MIN_BUFFER_SECS: u32 = 10;
pub const MAX_BUFFER_SECS: u32 = 300;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Codec {
    H264,
    Hevc,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum EncoderChoice {
    /// Pick the best detected hardware encoder (NVENC > AMF > QSV).
    Auto,
    Nvenc,
    Amf,
    Qsv,
    /// CPU encode. Never chosen automatically while a hardware encoder exists.
    Software,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum RateControl {
    Cbr,
    Vbr,
    /// Constant quality; `quality` (1-100) is used instead of bitrate.
    Quality,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum HdrMode {
    /// Tonemap HDR desktops to SDR (BT.709) before encoding.
    TonemapToSdr,
    /// Capture the SDR-mapped desktop as Windows presents it.
    Passthrough,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case", tag = "kind")]
pub enum AudioSource {
    /// Everything the default output device plays.
    System,
    /// Everything except one app's process tree (e.g. game audio without Discord).
    SystemExcluding { exe: String },
    /// Only one app's process tree (e.g. Discord, Spotify).
    App { exe: String },
    /// A capture device. `None` = default microphone.
    Mic { device_id: Option<String> },
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct AudioTrack {
    pub name: String,
    pub enabled: bool,
    pub source: AudioSource,
    /// Linear gain, 0.0-2.0.
    pub volume: f32,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Hotkeys {
    pub save_clip: String,
    pub marker: String,
    pub screenshot: String,
    pub extend_last: String,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct VideoSettings {
    /// Index into the monitor list (0 = primary).
    pub monitor: u32,
    /// Output height in pixels; 0 = native. Width follows the aspect ratio.
    pub height: u32,
    pub fps: u32,
    pub codec: Codec,
    pub encoder: EncoderChoice,
    pub rate_control: RateControl,
    /// kbit/s for CBR/VBR.
    pub bitrate_kbps: u32,
    /// 1-100 for `RateControl::Quality`.
    pub quality: u32,
    /// Keyframe interval in seconds. Clips can only start on keyframes.
    pub keyframe_secs: f32,
    pub capture_cursor: bool,
    pub hdr: HdrMode,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct BufferSettings {
    /// Replay length in seconds. A saved clip is the last `seconds` ending at
    /// the keypress, never longer.
    pub seconds: u32,
    /// Hard RAM ceiling for the encoded buffer, MiB.
    pub max_memory_mb: u32,
    /// Seconds added by the "extend last clip" hotkey.
    pub extend_secs: u32,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct GameProfile {
    /// Executable name, case-insensitive (`cs2.exe`).
    pub exe: String,
    pub name: String,
    /// Fields left `None` fall back to the global settings.
    pub fps: Option<u32>,
    pub height: Option<u32>,
    pub bitrate_kbps: Option<u32>,
    pub buffer_seconds: Option<u32>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Settings {
    pub version: u32,
    pub first_run_complete: bool,
    pub pro_mode: bool,
    /// "system" | "dark" | "light"
    pub theme: String,
    pub clips_dir: PathBuf,
    pub launch_on_startup: bool,
    pub start_minimized: bool,
    pub play_sound_on_save: bool,
    pub video: VideoSettings,
    pub buffer: BufferSettings,
    pub audio: Vec<AudioTrack>,
    pub hotkeys: Hotkeys,
    pub profiles: Vec<GameProfile>,
    /// Never record while one of these executables has focus.
    pub privacy_apps: Vec<String>,
    /// Warn once a day's written clip volume exceeds this many GiB.
    pub ssd_warn_gb_per_day: u32,
}

impl Default for Settings {
    fn default() -> Self {
        Self {
            version: SETTINGS_VERSION,
            first_run_complete: false,
            pro_mode: false,
            theme: "system".into(),
            clips_dir: default_clips_dir(),
            launch_on_startup: false,
            start_minimized: false,
            play_sound_on_save: true,
            video: VideoSettings {
                monitor: 0,
                height: 1080,
                fps: 60,
                codec: Codec::H264,
                encoder: EncoderChoice::Auto,
                rate_control: RateControl::Vbr,
                bitrate_kbps: 20_000,
                quality: 70,
                keyframe_secs: 1.0,
                capture_cursor: true,
                hdr: HdrMode::TonemapToSdr,
            },
            buffer: BufferSettings {
                seconds: 30,
                max_memory_mb: 1024,
                extend_secs: 15,
            },
            audio: vec![
                AudioTrack { name: "Desktop".into(), enabled: true, source: AudioSource::System, volume: 1.0 },
                AudioTrack {
                    name: "Microphone".into(),
                    enabled: false,
                    source: AudioSource::Mic { device_id: None },
                    volume: 1.0,
                },
                AudioTrack {
                    name: "Discord".into(),
                    enabled: false,
                    source: AudioSource::App { exe: "Discord.exe".into() },
                    volume: 1.0,
                },
            ],
            hotkeys: Hotkeys {
                save_clip: "Alt+F8".into(),
                marker: "Alt+F7".into(),
                screenshot: "Alt+F12".into(),
                extend_last: String::new(),
            },
            profiles: Vec::new(),
            privacy_apps: vec!["KeePassXC.exe".into(), "1Password.exe".into(), "Bitwarden.exe".into()],
            ssd_warn_gb_per_day: 50,
        }
    }
}

pub fn app_data_dir() -> PathBuf {
    std::env::var_os("APPDATA")
        .map(PathBuf::from)
        .unwrap_or_else(|| PathBuf::from("."))
        .join("Rewind")
}

pub fn default_clips_dir() -> PathBuf {
    app_data_dir().join("Clips")
}

pub fn settings_path() -> PathBuf {
    app_data_dir().join("settings.toml")
}

/// Load settings, migrating older versions. A missing or unreadable file yields
/// defaults; an unreadable file is kept aside as `settings.toml.bad`.
pub fn load() -> Settings {
    let path = settings_path();
    let Ok(text) = std::fs::read_to_string(&path) else {
        return Settings::default();
    };
    match parse(&text) {
        Ok(s) => s,
        Err(e) => {
            tracing::warn!("settings unreadable ({e:#}), using defaults");
            let _ = std::fs::rename(&path, path.with_extension("toml.bad"));
            Settings::default()
        }
    }
}

pub fn parse(text: &str) -> Result<Settings> {
    let mut value: toml::Table = toml::from_str(text).context("parse settings toml")?;
    migrate(&mut value)?;
    // Unknown/missing fields: merge onto defaults so new fields get default values.
    let mut merged = toml::Table::try_from(Settings::default())?;
    merge(&mut merged, value);
    let mut s: Settings = merged.try_into()?;
    s.sanitize();
    Ok(s)
}

impl Settings {
    /// Clamp values the engine relies on into their supported ranges.
    pub fn sanitize(&mut self) {
        self.buffer.seconds = self.buffer.seconds.clamp(MIN_BUFFER_SECS, MAX_BUFFER_SECS);
    }
}

/// Upgrade an older settings table in place to [`SETTINGS_VERSION`].
fn migrate(value: &mut toml::Table) -> Result<()> {
    let version = value.get("version").and_then(|v| v.as_integer()).unwrap_or(0) as u32;
    anyhow::ensure!(version <= SETTINGS_VERSION, "settings from a newer Rewind (v{version})");
    // v0 -> v1: files written before versioning existed. Shape is identical.
    // v1 -> v2: clip length is the buffer length; post-roll and the second
    // replay length are gone.
    if version < 2 {
        if let Some(toml::Value::Table(b)) = value.get_mut("buffer") {
            b.remove("post_roll_secs");
            b.remove("long_seconds");
        }
        if let Some(toml::Value::Table(h)) = value.get_mut("hotkeys") {
            h.remove("save_long_clip");
        }
    }
    value.insert("version".into(), toml::Value::Integer(SETTINGS_VERSION as i64));
    Ok(())
}

fn merge(base: &mut toml::Table, overlay: toml::Table) {
    for (k, v) in overlay {
        match (base.get_mut(&k), v) {
            (Some(toml::Value::Table(b)), toml::Value::Table(o)) => merge(b, o),
            (_, v) => {
                base.insert(k, v);
            }
        }
    }
}

pub fn save(settings: &Settings) -> Result<()> {
    write_atomic(&settings_path(), toml::to_string_pretty(settings)?.as_bytes())
}

/// Write via temp file + rename so a crash never leaves a half-written file.
pub fn write_atomic(path: &Path, bytes: &[u8]) -> Result<()> {
    if let Some(dir) = path.parent() {
        std::fs::create_dir_all(dir)?;
    }
    let tmp = path.with_extension("tmp");
    std::fs::write(&tmp, bytes)?;
    std::fs::rename(&tmp, path)?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn roundtrip() {
        let s = Settings::default();
        let text = toml::to_string_pretty(&s).unwrap();
        assert_eq!(parse(&text).unwrap(), s);
    }

    #[test]
    fn missing_fields_get_defaults() {
        let s = parse("version = 1\n[buffer]\nseconds = 120\n").unwrap();
        assert_eq!(s.buffer.seconds, 120);
        assert_eq!(s.video.fps, 60);
    }

    #[test]
    fn unversioned_file_migrates() {
        let s = parse("pro_mode = true\n").unwrap();
        assert!(s.pro_mode);
        assert_eq!(s.version, SETTINGS_VERSION);
    }

    #[test]
    fn buffer_length_clamped() {
        assert_eq!(parse("version = 1
[buffer]
seconds = 2
").unwrap().buffer.seconds, MIN_BUFFER_SECS);
        assert_eq!(parse("version = 1
[buffer]
seconds = 9000
").unwrap().buffer.seconds, MAX_BUFFER_SECS);
    }

    #[test]
    fn legacy_record_mode_ignored() {
        let s = parse("version = 1
record_mode = \"manual\"
[hotkeys]
toggle_pause = \"Alt+F11\"
").unwrap();
        assert_eq!(s.hotkeys.save_clip, "Alt+F8");
    }

    #[test]
    fn v1_post_roll_dropped() {
        let s = parse("version = 1
[buffer]
seconds = 45
post_roll_secs = 10
long_seconds = 600
[hotkeys]
save_long_clip = \"Alt+F9\"
").unwrap();
        assert_eq!(s.buffer.seconds, 45);
        assert_eq!(s.version, 2);
        let text = toml::to_string_pretty(&s).unwrap();
        assert!(!text.contains("post_roll") && !text.contains("long_seconds") && !text.contains("save_long_clip"));
    }

    #[test]
    fn newer_version_rejected() {
        assert!(parse("version = 999\n").is_err());
    }
}
