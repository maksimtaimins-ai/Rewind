//! Versioned settings persisted as TOML in `%APPDATA%\Rewind\settings.toml`.
//!
//! Every change to the on-disk shape bumps [`SETTINGS_VERSION`] and adds a step
//! to [`migrate`], so old files keep loading.

use std::path::{Path, PathBuf};

use anyhow::{Context, Result};
use serde::{Deserialize, Serialize};

pub const SETTINGS_VERSION: u32 = 20;

/// Allowed range for [`BufferSettings::seconds`].
pub const MIN_BUFFER_SECS: u32 = 10;
pub const MAX_BUFFER_SECS: u32 = 300;
/// Slots besides the main buffer length (B and C).
pub const EXTRA_SLOTS: usize = 2;
/// Allowed range for [`PostRollSettings::seconds`].
pub const MIN_POST_ROLL_SECS: u32 = 1;
pub const MAX_POST_ROLL_SECS: u32 = 120;

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
    pub slow_mo: String,
    pub favorite_last: String,
    pub compact_hud: String,
}

/// Free-space limits for the clips drive, GB.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct DiskSettings {
    /// Warn on the dashboard below this.
    pub warn_gb: u32,
    /// Refuse new clip saves (not screenshots) below this.
    pub block_gb: u32,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum HighlightColor {
    Off,
    Blue,
    Green,
    Red,
    /// [`HighlightSettings::custom_color`].
    Custom,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum HighlightPosition {
    /// A glowing line around the whole screen edge.
    Border,
    TopLeft,
    TopRight,
    BottomRight,
}

/// On-screen sign that the buffer is recording, drawn over the primary display.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct HighlightSettings {
    pub color: HighlightColor,
    /// `#rrggbb`.
    pub custom_color: String,
    /// Pixels, 0-10. 0 hides it like `Off`.
    pub thickness: u32,
    /// Percent, 10-100.
    pub opacity: u32,
    pub position: HighlightPosition,
    /// Only while a game is detected (see `profiles::Detector`).
    pub only_in_game: bool,
}

impl HighlightSettings {
    pub fn enabled(&self) -> bool {
        self.color != HighlightColor::Off && self.thickness > 0
    }
}

/// Compact HUD: a small always-on-top pill instead of the main window.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct HudSettings {
    pub enabled: bool,
    /// Last position, physical pixels. Unset until the pill is first moved.
    pub x: Option<i32>,
    pub y: Option<i32>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct LibrarySettings {
    /// Clips saved within this many minutes of each other form one session.
    pub session_gap_mins: u32,
    /// Deleted clips are kept this many days before they're gone for good.
    pub purge_days: u32,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct SlowMoSettings {
    /// Seconds of the buffer to slow down: 2, 3 or 5.
    pub seconds: u32,
    /// Playback speed, 0.1-0.5 (0.25 turns 3 s into 12 s).
    pub speed: f32,
}

/// Opt-in footage recorded after the save hotkey. Off: a clip ends at the
/// keypress. On: it ends `seconds` later, and the save waits until then.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct PostRollSettings {
    pub enabled: bool,
    pub seconds: u32,
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

/// An extra replay length with its own save hotkey. All slots read the same
/// encoded buffer, which is sized to the longest enabled one.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct BufferSlot {
    pub enabled: bool,
    pub seconds: u32,
    pub hotkey: String,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct BufferSettings {
    /// Replay length in seconds. A saved clip is the last `seconds` ending at
    /// the keypress, never longer.
    pub seconds: u32,
    /// Slots B and C. Slot A is `seconds` with `hotkeys.save_clip`.
    pub extra_slots: Vec<BufferSlot>,
    /// Hard RAM ceiling for the encoded buffer, MiB.
    pub max_memory_mb: u32,
    /// Seconds the "extend last clip" hotkey adds before the clip's start.
    pub extend_secs: u32,
}

fn yes() -> bool {
    true
}

/// Settings that switch in while a game runs. Every `None` (or `false`)
/// keeps the global setting.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct GameProfile {
    /// Executable name, case-insensitive (`cs2.exe`).
    pub exe: String,
    pub name: String,
    /// Off: kept, but not applied.
    #[serde(default = "yes")]
    pub enabled: bool,
    pub fps: Option<u32>,
    pub height: Option<u32>,
    pub bitrate_kbps: Option<u32>,
    #[serde(default)]
    pub codec: Option<Codec>,
    /// Slot A's length.
    pub buffer_seconds: Option<u32>,
    #[serde(default)]
    pub slot_b_seconds: Option<u32>,
    #[serde(default)]
    pub slot_c_seconds: Option<u32>,
    /// Its own hotkeys instead of the global ones.
    #[serde(default)]
    pub hotkeys: Option<Hotkeys>,
    /// Save into `<clips folder>/<name>/`.
    #[serde(default)]
    pub subfolder: bool,
    /// A keystroke-overlay preset id (`input::presets`).
    #[serde(default)]
    pub keystroke_preset: Option<String>,
    #[serde(default)]
    pub highlight: Option<HighlightSettings>,
}

impl GameProfile {
    pub fn new(exe: &str, name: &str) -> Self {
        GameProfile {
            exe: exe.into(),
            name: name.into(),
            enabled: true,
            fps: None,
            height: None,
            bitrate_kbps: None,
            codec: None,
            buffer_seconds: None,
            slot_b_seconds: None,
            slot_c_seconds: None,
            hotkeys: None,
            subfolder: false,
            keystroke_preset: None,
            highlight: None,
        }
    }
}

/// Profiles Rewind ships with. Added once, then the user's to edit or delete.
pub fn builtin_profiles() -> Vec<GameProfile> {
    let shooter = |exe: &str, name: &str| GameProfile {
        fps: Some(120),
        bitrate_kbps: Some(30_000),
        buffer_seconds: Some(30),
        keystroke_preset: Some("gaming".into()),
        ..GameProfile::new(exe, name)
    };
    let battle_royale = |exe: &str, name: &str| GameProfile {
        fps: Some(60),
        bitrate_kbps: Some(25_000),
        buffer_seconds: Some(60),
        keystroke_preset: Some("gaming".into()),
        ..GameProfile::new(exe, name)
    };
    vec![
        shooter("cs2.exe", "Counter-Strike 2"),
        shooter("VALORANT-Win64-Shipping.exe", "VALORANT"),
        battle_royale("FortniteClient-Win64-Shipping.exe", "Fortnite"),
        battle_royale("r5apex.exe", "Apex Legends"),
        battle_royale("r5apex_dx12.exe", "Apex Legends"),
        GameProfile { fps: Some(60), buffer_seconds: Some(30), keystroke_preset: Some("wasd".into()), ..GameProfile::new("RocketLeague.exe", "Rocket League") },
        GameProfile { fps: Some(60), buffer_seconds: Some(60), keystroke_preset: Some("wasd_clicks".into()), ..GameProfile::new("Minecraft.Windows.exe", "Minecraft") },
        // Java Edition runs as javaw.exe, like every Java app, so it starts off.
        GameProfile { enabled: false, fps: Some(60), buffer_seconds: Some(60), keystroke_preset: Some("wasd_clicks".into()), ..GameProfile::new("javaw.exe", "Minecraft") },
    ]
}

/// A Discord channel webhook clips can be posted to. No bot, no token: the
/// URL itself is the credential.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct DiscordWebhook {
    pub name: String,
    pub url: String,
    /// Upload cap in MB: 10 (default) or 25 (boosted server).
    pub limit_mb: u32,
}

/// New .mp4 files in `input` are exported with `preset` into `output`.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct WatchFolder {
    pub enabled: bool,
    pub input: PathBuf,
    pub preset: crate::export::Preset,
    pub output: PathBuf,
    pub delete_original: bool,
}

/// A PC whose clips were accepted once; it isn't asked about again.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct TrustedPeer {
    pub id: String,
    pub name: String,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct SharingSettings {
    pub discord_webhooks: Vec<DiscordWebhook>,
    /// Listen for clips from other Rewinds and announce this PC over zeroconf.
    pub lan_enabled: bool,
    pub lan_port: u16,
    /// Name other PCs see; empty = this PC's name.
    pub device_name: String,
    /// Random id for this install, made on first launch.
    pub device_id: String,
    pub trusted_peers: Vec<TrustedPeer>,
}

/// Audio-based smart highlights. Off by default; while off nothing is analysed.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct SmartHighlightSettings {
    pub enabled: bool,
    /// Loudness (dBFS, -30 to -3) a spike or sustained stretch must reach.
    pub threshold_db: f32,
    /// Also analyse each clip right after it's saved. Off: only the button.
    pub analyze_on_save: bool,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum KeyFilterMode {
    All,
    /// Only `keys`.
    Allow,
    /// Everything but `keys`.
    Block,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum KeycapStyle {
    Flat,
    ThreeD,
    Minimal,
    Outlined,
    /// A mechanical keyboard key: darker skirt around a lighter top.
    Mechanical,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum OverlayFont {
    /// Closest installed match to the app's own typeface.
    Theme,
    Mono,
    Sans,
    Condensed,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum PressAnimation {
    None,
    Pop,
    Flash,
    Spring,
    Pulse,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ComboStyle {
    /// `Ctrl + Shift + P` in one row.
    Chain,
    /// Modifiers stacked above the key.
    Stack,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum OverlayAnchor {
    TopLeft,
    TopRight,
    BottomLeft,
    BottomRight,
    TopCenter,
    BottomCenter,
    /// `OverlayPlacement::x`/`y`.
    Custom,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ClickStyle {
    Dot,
    Ring,
    Flash,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ClickPosition {
    /// Where the click happened.
    Cursor,
    /// As a mouse-button keycap next to the keys.
    WithKeys,
}

/// How recorded input is drawn into exports. Sizes are pixels at 1080p and
/// scale with the export's height.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct OverlayStyle {
    pub keycap: KeycapStyle,
    /// Keycap height, 16-120.
    pub size: u32,
    pub font: OverlayFont,
    /// `#rrggbb`; empty = the theme accent at the time it was turned on.
    pub base_color: String,
    /// `#rrggbb`; empty = black or white, whichever reads better.
    pub text_color: String,
    pub border: bool,
    pub border_color: String,
    /// Corner radius, 0-40.
    pub radius: u32,
    pub shadow: bool,
    /// 0-100.
    pub shadow_intensity: u32,
    pub animation: PressAnimation,
    /// Seconds a released key takes to fade, 0.5-10.
    pub fade_secs: f32,
    /// Percent for allowlisted keys at rest, 0-100.
    pub idle_opacity: u32,
    /// Percent while pressed, 0-100.
    pub pressed_opacity: u32,
    /// Held keys filled, released ones outlined.
    pub hold_indicator: bool,
    pub combo: ComboStyle,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct OverlayPlacement {
    pub anchor: OverlayAnchor,
    /// Custom position of the overlay's top-left, 0-1 of the frame.
    pub x: f32,
    pub y: f32,
    /// Distance from the frame edge, pixels at 1080p, 0-200.
    pub margin: u32,
    pub click_position: ClickPosition,
    pub click_style: ClickStyle,
    /// Draw the pointer's recent path (needs mouse movement recorded).
    pub trail: bool,
    /// Trail length in milliseconds, 100-2000.
    pub trail_ms: u32,
}

/// Keystroke overlay. Off by default; while off no input hook is installed
/// and nothing is recorded.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct KeystrokeSettings {
    /// Record input with each clip for exports.
    pub enabled: bool,
    /// Draw recent input on screen as it happens, in its own window. Shares
    /// the style, position and filters below with exports.
    pub live: bool,
    pub keyboard: bool,
    pub clicks: bool,
    /// Pointer position, ~60 times a second. Makes much larger files.
    pub movement: bool,
    pub scroll: bool,
    pub filter: KeyFilterMode,
    /// Key names (`W`, `Space`, `Ctrl`) for the allow or block list.
    pub keys: Vec<String>,
    /// Last preset picked, or "custom".
    pub preset: String,
    /// Skip keys while a text box anywhere has the caret.
    pub pause_in_text_fields: bool,
    pub style: OverlayStyle,
    pub position: OverlayPlacement,
}

impl KeystrokeSettings {
    /// Whether input is recorded for clips.
    pub fn records(&self) -> bool {
        self.enabled && self.hooks()
    }

    /// Whether the input hook should be installed at all: recording, or the
    /// live overlay, needs it.
    pub fn hooks(&self) -> bool {
        (self.enabled || self.live) && (self.keyboard || self.clicks || self.movement || self.scroll)
    }

    /// Whether the live on-screen overlay has anything to show.
    pub fn shows_live(&self) -> bool {
        self.live && (self.keyboard || self.clicks)
    }
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Settings {
    pub version: u32,
    pub first_run_complete: bool,
    /// The Live tab's four-step tour has been finished or skipped.
    pub tour_complete: bool,
    pub pro_mode: bool,
    /// "system" (follow Windows light/dark) or a theme id from the UI's theme list.
    pub theme: String,
    pub clips_dir: PathBuf,
    pub launch_on_startup: bool,
    pub start_minimized: bool,
    pub play_sound_on_save: bool,
    pub video: VideoSettings,
    pub buffer: BufferSettings,
    pub post_roll: PostRollSettings,
    pub slow_mo: SlowMoSettings,
    pub library: LibrarySettings,
    pub audio: Vec<AudioTrack>,
    pub hotkeys: Hotkeys,
    /// Switch to a game's profile while it runs. Off: profiles are never applied.
    pub profiles_enabled: bool,
    pub profiles: Vec<GameProfile>,
    /// Never record while one of these executables has focus.
    pub privacy_apps: Vec<String>,
    /// Warn once a day's written clip volume exceeds this many GiB.
    pub ssd_warn_gb_per_day: u32,
    pub sharing: SharingSettings,
    pub watch_folders: Vec<WatchFolder>,
    pub hud: HudSettings,
    pub disk: DiskSettings,
    pub highlight: HighlightSettings,
    pub smart_highlights: SmartHighlightSettings,
    pub keystrokes: KeystrokeSettings,
}

impl Default for Settings {
    fn default() -> Self {
        Self {
            version: SETTINGS_VERSION,
            first_run_complete: false,
            tour_complete: false,
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
                extra_slots: vec![
                    BufferSlot { enabled: true, seconds: 120, hotkey: "Alt+F9".into() },
                    BufferSlot { enabled: false, seconds: 300, hotkey: String::new() },
                ],
                max_memory_mb: 1024,
                extend_secs: 15,
            },
            post_roll: PostRollSettings { enabled: false, seconds: 10 },
            slow_mo: SlowMoSettings { seconds: 3, speed: 0.25 },
            library: LibrarySettings { session_gap_mins: 30, purge_days: 30 },
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
                extend_last: "Alt+F6".into(),
                slow_mo: "Alt+F5".into(),
                favorite_last: "Alt+Shift+F".into(),
                compact_hud: "Alt+F11".into(),
            },
            profiles_enabled: false,
            profiles: builtin_profiles(),
            privacy_apps: vec!["KeePassXC.exe".into(), "1Password.exe".into(), "Bitwarden.exe".into()],
            ssd_warn_gb_per_day: 50,
            sharing: SharingSettings {
                discord_webhooks: Vec::new(),
                lan_enabled: false,
                lan_port: 7777,
                device_name: String::new(),
                device_id: String::new(),
                trusted_peers: Vec::new(),
            },
            watch_folders: Vec::new(),
            hud: HudSettings { enabled: false, x: None, y: None },
            disk: DiskSettings { warn_gb: 10, block_gb: 2 },
            // Off until asked for: no glowing border out of nowhere.
            highlight: HighlightSettings {
                color: HighlightColor::Off,
                custom_color: "#ffc53d".into(),
                thickness: 2,
                opacity: 60,
                position: HighlightPosition::Border,
                only_in_game: false,
            },
            smart_highlights: SmartHighlightSettings { enabled: false, threshold_db: -14.0, analyze_on_save: false },
            keystrokes: KeystrokeSettings {
                enabled: false,
                live: false,
                keyboard: true,
                clicks: false,
                movement: false,
                scroll: false,
                filter: KeyFilterMode::All,
                keys: Vec::new(),
                preset: "all_keys".into(),
                pause_in_text_fields: true,
                style: OverlayStyle {
                    keycap: KeycapStyle::ThreeD,
                    size: 44,
                    font: OverlayFont::Theme,
                    base_color: String::new(),
                    text_color: String::new(),
                    border: false,
                    border_color: "#ffffff".into(),
                    radius: 8,
                    shadow: true,
                    shadow_intensity: 40,
                    animation: PressAnimation::Pop,
                    fade_secs: 2.0,
                    idle_opacity: 30,
                    pressed_opacity: 100,
                    hold_indicator: true,
                    combo: ComboStyle::Chain,
                },
                position: OverlayPlacement {
                    anchor: OverlayAnchor::BottomLeft,
                    x: 0.05,
                    y: 0.8,
                    margin: 40,
                    click_position: ClickPosition::Cursor,
                    click_style: ClickStyle::Ring,
                    trail: false,
                    trail_ms: 400,
                },
            },
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
        self.buffer.extend_secs = self.buffer.extend_secs.clamp(5, 120);
        self.post_roll.seconds = self.post_roll.seconds.clamp(MIN_POST_ROLL_SECS, MAX_POST_ROLL_SECS);
        self.buffer.extra_slots.resize(EXTRA_SLOTS, BufferSlot { enabled: false, seconds: 60, hotkey: String::new() });
        self.library.session_gap_mins = self.library.session_gap_mins.clamp(1, 24 * 60);
        self.library.purge_days = self.library.purge_days.clamp(1, 365);
        if ![2, 3, 5].contains(&self.slow_mo.seconds) {
            self.slow_mo.seconds = 3;
        }
        self.slow_mo.speed = if self.slow_mo.speed.is_finite() { self.slow_mo.speed.clamp(0.1, 0.5) } else { 0.25 };
        for slot in &mut self.buffer.extra_slots {
            slot.seconds = slot.seconds.clamp(MIN_BUFFER_SECS, MAX_BUFFER_SECS);
        }
        self.disk.warn_gb = self.disk.warn_gb.clamp(1, 2000);
        self.disk.block_gb = self.disk.block_gb.min(self.disk.warn_gb);
        self.highlight.thickness = self.highlight.thickness.min(10);
        self.highlight.opacity = self.highlight.opacity.clamp(10, 100);
        if !is_hex_color(&self.highlight.custom_color) {
            self.highlight.custom_color = "#ffc53d".into();
        }
        let k = &mut self.keystrokes;
        k.style.size = k.style.size.clamp(16, 120);
        k.style.radius = k.style.radius.min(40);
        k.style.shadow_intensity = k.style.shadow_intensity.min(100);
        k.style.fade_secs = if k.style.fade_secs.is_finite() { k.style.fade_secs.clamp(0.5, 10.0) } else { 2.0 };
        k.style.idle_opacity = k.style.idle_opacity.min(100);
        k.style.pressed_opacity = k.style.pressed_opacity.min(100);
        for c in [&mut k.style.base_color, &mut k.style.text_color] {
            if !c.is_empty() && !is_hex_color(c) {
                c.clear();
            }
        }
        if !is_hex_color(&k.style.border_color) {
            k.style.border_color = "#ffffff".into();
        }
        let unit = |v: f32, d: f32| if v.is_finite() { v.clamp(0.0, 1.0) } else { d };
        k.position.x = unit(k.position.x, 0.05);
        k.position.y = unit(k.position.y, 0.8);
        k.position.margin = k.position.margin.min(200);
        k.position.trail_ms = k.position.trail_ms.clamp(100, 2000);
        k.keys.dedup();
        for p in &mut self.profiles {
            p.fps = p.fps.map(|v| v.clamp(10, 240));
            p.bitrate_kbps = p.bitrate_kbps.map(|v| v.clamp(500, 200_000));
            for s in [&mut p.buffer_seconds, &mut p.slot_b_seconds, &mut p.slot_c_seconds] {
                *s = s.map(|v| v.clamp(MIN_BUFFER_SECS, MAX_BUFFER_SECS));
            }
        }
        let t = self.smart_highlights.threshold_db;
        self.smart_highlights.threshold_db = if t.is_finite() { t.clamp(-30.0, -3.0) } else { -14.0 };
        if self.sharing.lan_port < 1024 {
            self.sharing.lan_port = 7777;
        }
        for w in &mut self.sharing.discord_webhooks {
            w.limit_mb = if w.limit_mb >= 25 { 25 } else { 10 };
        }
    }

    /// Seconds a normal save records after the keypress (0 when post-roll is off).
    pub fn post_roll_secs(&self) -> u32 {
        if self.post_roll.enabled { self.post_roll.seconds } else { 0 }
    }

    /// Longest enabled replay length, which is what the buffer must hold.
    pub fn longest_slot_secs(&self) -> u32 {
        self.buffer.extra_slots.iter().filter(|s| s.enabled).map(|s| s.seconds).fold(self.buffer.seconds, u32::max)
    }
}

/// `#rrggbb`.
pub fn is_hex_color(c: &str) -> bool {
    c.len() == 7 && c.starts_with('#') && c[1..].chars().all(|ch| ch.is_ascii_hexdigit())
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
    // v19 -> v20: `keystrokes.live` added (off); defaults fill it in.
    // v18 -> v19: profiles get a master switch (on for anyone who already had
    // profiles, so nothing changes for them) and the built-in game profiles.
    if version < 19 {
        let had = value.get("profiles").and_then(|v| v.as_array()).map(|a| !a.is_empty()).unwrap_or(false);
        value.insert("profiles_enabled".into(), toml::Value::Boolean(had));
        let mut list = value.get("profiles").and_then(|v| v.as_array()).cloned().unwrap_or_default();
        for p in builtin_profiles() {
            let present = list.iter().any(|x| x.get("exe").and_then(|e| e.as_str()).is_some_and(|e| e.eq_ignore_ascii_case(&p.exe)));
            if !present {
                list.push(toml::Value::try_from(p)?);
            }
        }
        value.insert("profiles".into(), toml::Value::Array(list));
    }
    // v17 -> v18: `keystrokes` added (off); defaults fill it in.
    // v16 -> v17: `smart_highlights` added (off); defaults fill it in.
    // v15 -> v16: `tour_complete` added (false: existing installs see the tour once).
    // v14 -> v15: `highlight` added (off); defaults fill it in.
    // v13 -> v14: `disk` free-space limits added; defaults fill them in.
    // v12 -> v13: `hud` and `hotkeys.compact_hud` (Alt+F11) added; defaults
    // fill them in. The v1 `hotkeys.toggle_pause` that also used Alt+F11 is
    // long gone, so the key is free.
    // v11 -> v12: `watch_folders` added (empty).
    // v10 -> v11: LAN send fields added under `sharing`; defaults fill them in.
    // v9 -> v10: `sharing` (Discord webhooks) added; defaults fill it in.
    // v8 -> v9: opt-in `post_roll` added (off); defaults fill it in. Unrelated
    // to the v1 `buffer.post_roll_secs`, which stays dropped.
    // v7 -> v8: `library.purge_days` added; defaults fill it in.
    // v6 -> v7: `hotkeys.favorite_last` added; defaults fill it in.
    // v5 -> v6: `library` added; defaults fill it in.
    // v4 -> v5: `slow_mo` and `hotkeys.slow_mo` added; defaults fill them in.
    // v3 -> v4: `buffer.extra_slots` added; missing fields take defaults.
    // v2 -> v3: "extend last clip" gets a default hotkey.
    if version < 3 {
        if let Some(toml::Value::Table(h)) = value.get_mut("hotkeys") {
            if h.get("extend_last").and_then(|v| v.as_str()).is_some_and(str::is_empty) {
                h.insert("extend_last".into(), "Alt+F6".into());
            }
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

/// Give this install its LAN id if it has none yet. Returns true if it changed.
pub fn ensure_device_id(s: &mut Settings) -> bool {
    if !s.sharing.device_id.is_empty() {
        return false;
    }
    use std::hash::BuildHasher;
    // RandomState is seeded per process from the OS's random source.
    let a = std::collections::hash_map::RandomState::new().hash_one(std::time::SystemTime::now());
    let b = std::collections::hash_map::RandomState::new().hash_one(std::process::id());
    s.sharing.device_id = format!("{a:016x}{b:016x}");
    true
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
    fn tour_shown_once_after_upgrade() {
        let s = parse("version = 15
first_run_complete = true
").unwrap();
        assert!(s.first_run_complete);
        assert!(!s.tour_complete);
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
        assert_eq!(s.version, SETTINGS_VERSION);
        let text = toml::to_string_pretty(&s).unwrap();
        assert!(!text.contains("post_roll_secs") && !text.contains("long_seconds") && !text.contains("save_long_clip"));
        assert!(!s.post_roll.enabled);
    }

    #[test]
    fn post_roll_defaults_off_and_clamps() {
        let s = parse("version = 8
").unwrap();
        assert_eq!(s.post_roll, PostRollSettings { enabled: false, seconds: 10 });
        assert_eq!(s.post_roll_secs(), 0);
        let s = parse("version = 9
[post_roll]
enabled = true
seconds = 500
").unwrap();
        assert_eq!(s.post_roll_secs(), MAX_POST_ROLL_SECS);
        let s = parse("version = 9
[post_roll]
enabled = true
seconds = 0
").unwrap();
        assert_eq!(s.post_roll_secs(), MIN_POST_ROLL_SECS);
    }

    #[test]
    fn v2_gets_extend_hotkey() {
        let s = parse("version = 2
[hotkeys]
extend_last = \"\"
").unwrap();
        assert_eq!(s.hotkeys.extend_last, "Alt+F6");
        let s = parse("version = 2
[hotkeys]
extend_last = \"Ctrl+E\"
").unwrap();
        assert_eq!(s.hotkeys.extend_last, "Ctrl+E");
    }

    #[test]
    fn slots_default_and_clamp() {
        let s = parse("version = 3\n").unwrap();
        assert_eq!(s.buffer.extra_slots.len(), EXTRA_SLOTS);
        assert_eq!(s.buffer.extra_slots[0].hotkey, "Alt+F9");
        assert_eq!(s.longest_slot_secs(), 120);
        let s = parse("version = 4\n[[buffer.extra_slots]]\nenabled = true\nseconds = 9999\nhotkey = \"\"\n").unwrap();
        assert_eq!(s.buffer.extra_slots.len(), EXTRA_SLOTS);
        assert_eq!(s.longest_slot_secs(), MAX_BUFFER_SECS);
    }

    #[test]
    fn slow_mo_sanitized() {
        let s = parse("version = 5\n[slow_mo]\nseconds = 4\nspeed = 2.0\n").unwrap();
        assert_eq!(s.slow_mo, SlowMoSettings { seconds: 3, speed: 0.5 });
        assert_eq!(s.hotkeys.slow_mo, "Alt+F5");
    }

    #[test]
    fn webhook_limit_snaps() {
        let s = parse("version = 10
[[sharing.discord_webhooks]]
name = \"a\"
url = \"u\"
limit_mb = 50
").unwrap();
        assert_eq!(s.sharing.discord_webhooks[0].limit_mb, 25);
        assert!(parse("version = 9
").unwrap().sharing.discord_webhooks.is_empty());
    }

    #[test]
    fn lan_defaults_and_id() {
        let mut s = parse("version = 10\n").unwrap();
        assert!(!s.sharing.lan_enabled);
        assert_eq!(s.sharing.lan_port, 7777);
        assert!(ensure_device_id(&mut s));
        assert_eq!(s.sharing.device_id.len(), 32);
        assert!(!ensure_device_id(&mut s));
    }

    #[test]
    fn hud_defaults() {
        let s = parse("version = 12\n").unwrap();
        assert!(!s.hud.enabled && s.hud.x.is_none());
        assert_eq!(s.hotkeys.compact_hud, "Alt+F11");
        let mut s = Settings::default();
        s.hud.x = Some(-1200);
        s.hud.y = Some(40);
        assert_eq!(parse(&toml::to_string_pretty(&s).unwrap()).unwrap().hud, s.hud);
    }

    #[test]
    fn disk_limits_ordered() {
        let s = parse("version = 13\n").unwrap();
        assert_eq!(s.disk, DiskSettings { warn_gb: 10, block_gb: 2 });
        let s = parse("version = 14\n[disk]\nwarn_gb = 5\nblock_gb = 50\n").unwrap();
        assert_eq!(s.disk, DiskSettings { warn_gb: 5, block_gb: 5 });
    }

    #[test]
    fn highlight_defaults_off_and_sanitized() {
        let s = parse("version = 14
").unwrap();
        assert_eq!(s.highlight.color, HighlightColor::Off);
        assert!(!s.highlight.enabled());
        assert_eq!((s.highlight.thickness, s.highlight.opacity), (2, 60));
        let s = parse("version = 15
[highlight]
color = \"custom\"
custom_color = \"red\"
thickness = 40
opacity = 0
position = \"top_right\"
").unwrap();
        assert_eq!(s.highlight.custom_color, "#ffc53d");
        assert_eq!((s.highlight.thickness, s.highlight.opacity), (10, 10));
        assert_eq!(s.highlight.position, HighlightPosition::TopRight);
        assert!(s.highlight.enabled());
        let s = parse("version = 15
[highlight]
color = \"blue\"
thickness = 0
").unwrap();
        assert!(!s.highlight.enabled());
    }

    #[test]
    fn smart_highlights_default_off_and_clamped() {
        let s = parse("version = 16\n").unwrap();
        assert!(!s.smart_highlights.enabled && !s.smart_highlights.analyze_on_save);
        let s = parse("version = 17\n[smart_highlights]\nenabled = true\nthreshold_db = 5.0\n").unwrap();
        assert_eq!(s.smart_highlights.threshold_db, -3.0);
    }

    #[test]
    fn keystrokes_default_off_and_sanitized() {
        let s = parse("version = 17\n").unwrap();
        assert!(!s.keystrokes.enabled && !s.keystrokes.records());
        assert!(!s.keystrokes.live && !s.keystrokes.hooks());
        assert!(s.keystrokes.pause_in_text_fields);
        let s = parse("version = 18\n[keystrokes]\nenabled = true\n[keystrokes.style]\nsize = 900\nbase_color = \"red\"\nfade_secs = 0.0\n").unwrap();
        assert!(s.keystrokes.records());
        assert_eq!((s.keystrokes.style.size, s.keystrokes.style.fade_secs), (120, 0.5));
        assert!(s.keystrokes.style.base_color.is_empty());
    }

    #[test]
    fn profiles_switch_and_builtins() {
        // Fresh install: built-ins present, switch off.
        let s = Settings::default();
        assert!(!s.profiles_enabled);
        assert!(s.profiles.iter().any(|p| p.exe == "cs2.exe" && p.enabled));
        // Upgrade without profiles: off, built-ins added.
        let s = parse("version = 18
").unwrap();
        assert!(!s.profiles_enabled && s.profiles.len() == builtin_profiles().len());
        // Upgrade with a profile: stays on, the user's own entry wins.
        let s = parse("version = 18
[[profiles]]
exe = \"CS2.exe\"
name = \"Mine\"
fps = 999
").unwrap();
        assert!(s.profiles_enabled);
        let cs = s.profiles.iter().filter(|p| p.exe.eq_ignore_ascii_case("cs2.exe")).collect::<Vec<_>>();
        assert_eq!((cs.len(), cs[0].name.as_str(), cs[0].fps, cs[0].enabled), (1, "Mine", Some(240), true));
    }

    #[test]
    fn newer_version_rejected() {
        assert!(parse("version = 999\n").is_err());
    }
}
