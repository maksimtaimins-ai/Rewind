//! Keystroke overlay: input recorded alongside the buffer and saved as
//! `<clip>-input.json`, drawn into exports by `export::overlay`.
//!
//! Off by default. While off there is no hook, no thread and no memory: the
//! Windows low-level keyboard/mouse hooks are installed only when the setting
//! flips on ([`Recorder::apply`]) and removed when it flips off. The live
//! on-screen overlay (`crate::live_keys`) shares the same hook: it is
//! installed while either one is on, and events also go to [`set_live_sink`].

pub mod overlay;

#[cfg(windows)]
mod hook;

use std::collections::VecDeque;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicBool, AtomicI64, Ordering};
use std::sync::Arc;

use anyhow::Result;
use parking_lot::{Mutex, RwLock};
use serde::{Deserialize, Serialize};

use crate::buffer::TICKS_PER_SEC;
use crate::settings::{KeyFilterMode, KeystrokeSettings};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Kind {
    KeyDown,
    KeyUp,
    MouseDown,
    MouseUp,
    Move,
    Scroll,
}

/// One recorded input. In a sidecar `t` is seconds from the clip's start;
/// `x`/`y` are pixels on the recorded monitor.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Event {
    pub t: f64,
    #[serde(rename = "type")]
    pub kind: Kind,
    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub key: String,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub mods: Vec<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub x: Option<i32>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub y: Option<i32>,
    /// Wheel notches, positive away from the user.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub delta: Option<i32>,
}

impl Event {
    pub fn key(t: f64, kind: Kind, key: &str, mods: Vec<String>) -> Self {
        Event { t, kind, key: key.into(), mods, x: None, y: None, delta: None }
    }
}

pub const MODIFIERS: [&str; 4] = ["Ctrl", "Shift", "Alt", "Win"];

pub fn is_modifier(key: &str) -> bool {
    MODIFIERS.contains(&key)
}

/// Display name of a Windows virtual-key code. Left and right modifiers
/// share one name, so filters and combos treat them alike.
pub fn key_name(vk: u32) -> String {
    let s = match vk {
        0x41..=0x5A | 0x30..=0x39 => return char::from_u32(vk).unwrap().to_string(),
        0x70..=0x87 => return format!("F{}", vk - 0x6F),
        0x60..=0x69 => return format!("Num{}", vk - 0x60),
        0x08 => "Backspace",
        0x09 => "Tab",
        0x0D => "Enter",
        0x10 | 0xA0 | 0xA1 => "Shift",
        0x11 | 0xA2 | 0xA3 => "Ctrl",
        0x12 | 0xA4 | 0xA5 => "Alt",
        0x5B | 0x5C => "Win",
        0x13 => "Pause",
        0x14 => "Caps",
        0x1B => "Esc",
        0x20 => "Space",
        0x21 => "PgUp",
        0x22 => "PgDn",
        0x23 => "End",
        0x24 => "Home",
        0x25 => "Left",
        0x26 => "Up",
        0x27 => "Right",
        0x28 => "Down",
        0x2C => "PrtSc",
        0x2D => "Insert",
        0x2E => "Delete",
        0x5D => "Menu",
        0x6A => "Num*",
        0x6B => "Num+",
        0x6D => "Num-",
        0x6E => "Num.",
        0x6F => "Num/",
        0xBA => ";",
        0xBB => "=",
        0xBC => ",",
        0xBD => "-",
        0xBE => ".",
        0xBF => "/",
        0xC0 => "`",
        0xDB => "[",
        0xDC => "\\",
        0xDD => "]",
        0xDE => "'",
        _ => return format!("Key {vk:02X}"),
    };
    s.to_string()
}

/// Mouse buttons by index: left, right, middle, back, forward.
pub const MOUSE_BUTTONS: [&str; 5] = ["LMB", "RMB", "MMB", "Mouse4", "Mouse5"];

/// Whether a keyboard key gets recorded under the key filter.
pub fn key_allowed(cfg: &KeystrokeSettings, key: &str) -> bool {
    let listed = cfg.keys.iter().any(|k| k.eq_ignore_ascii_case(key));
    match cfg.filter {
        KeyFilterMode::All => true,
        KeyFilterMode::Allow => listed,
        KeyFilterMode::Block => !listed,
    }
}

/// A one-click setup for the recording filters.
#[derive(Debug, Clone, Serialize)]
pub struct Preset {
    pub id: &'static str,
    pub label: &'static str,
    pub filter: KeyFilterMode,
    pub keys: Vec<String>,
    pub keyboard: bool,
    pub clicks: bool,
    pub scroll: bool,
}

fn keys(list: &[&str]) -> Vec<String> {
    list.iter().map(|k| k.to_string()).collect()
}

pub fn presets() -> Vec<Preset> {
    let wasd = ["W", "A", "S", "D"];
    let letters: Vec<String> = ('A'..='Z').map(String::from).chain(["Space".to_string()]).collect();
    vec![
        Preset { id: "wasd", label: "WASD only", filter: KeyFilterMode::Allow, keys: keys(&wasd), keyboard: true, clicks: false, scroll: false },
        Preset { id: "wasd_clicks", label: "WASD + mouse clicks", filter: KeyFilterMode::Allow, keys: keys(&wasd), keyboard: true, clicks: true, scroll: false },
        Preset {
            id: "gaming",
            label: "Gaming",
            filter: KeyFilterMode::Allow,
            keys: keys(&["W", "A", "S", "D", "Space", "Shift", "Ctrl", "Q", "E", "R", "F", "G", "C", "Tab", "1", "2", "3", "4", "5"]),
            keyboard: true,
            clicks: true,
            scroll: true,
        },
        Preset { id: "arrows", label: "Arrow keys", filter: KeyFilterMode::Allow, keys: keys(&["Up", "Down", "Left", "Right"]), keyboard: true, clicks: false, scroll: false },
        Preset { id: "typing", label: "Typing", filter: KeyFilterMode::Allow, keys: letters, keyboard: true, clicks: false, scroll: false },
        Preset { id: "all_keys", label: "All keys", filter: KeyFilterMode::All, keys: Vec::new(), keyboard: true, clicks: false, scroll: false },
        Preset { id: "all", label: "All inputs", filter: KeyFilterMode::All, keys: Vec::new(), keyboard: true, clicks: true, scroll: true },
    ]
}

/// Set the recording filters from a preset id. Unknown ids change nothing.
pub fn apply_preset(cfg: &mut KeystrokeSettings, id: &str) -> bool {
    let Some(p) = presets().into_iter().find(|p| p.id == id) else { return false };
    cfg.filter = p.filter;
    cfg.keys = p.keys;
    cfg.keyboard = p.keyboard;
    cfg.clicks = p.clicks;
    cfg.scroll = p.scroll;
    cfg.preset = p.id.into();
    true
}

/// Recent input with QPC times, trimmed to what the replay buffer holds.
#[derive(Default)]
pub struct Log {
    events: VecDeque<(i64, Event)>,
}

impl Log {
    pub fn push(&mut self, at: i64, ev: Event, keep_ticks: i64) {
        self.events.push_back((at, ev));
        while self.events.front().is_some_and(|(t, _)| *t < at - keep_ticks) {
            self.events.pop_front();
        }
    }

    pub fn len(&self) -> usize {
        self.events.len()
    }

    pub fn is_empty(&self) -> bool {
        self.events.is_empty()
    }

    /// Events in `[from, to]`, timed from `from` and stretched by `1/speed`.
    pub fn between(&self, from: i64, to: i64, speed: f64) -> Vec<Event> {
        self.events
            .iter()
            .filter(|(t, _)| (from..=to).contains(t))
            .map(|(t, e)| Event { t: (t - from) as f64 / TICKS_PER_SEC as f64 / speed, ..e.clone() })
            .collect()
    }
}

/// `<clip dir>/<stem>-input.json`.
pub fn sidecar_path(clip: &Path) -> PathBuf {
    let dir = clip.parent().unwrap_or(Path::new("."));
    let stem = clip.file_stem().unwrap_or_default().to_string_lossy();
    dir.join(format!("{stem}-input.json"))
}

#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
pub struct Screen {
    pub width: u32,
    pub height: u32,
}

/// `<clip>-input.json`.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Sidecar {
    pub version: u32,
    /// The recorded monitor, which `x`/`y` are relative to.
    pub screen: Screen,
    pub events: Vec<Event>,
}

pub fn write_sidecar(clip: &Path, screen: Screen, events: Vec<Event>) -> Result<()> {
    let s = Sidecar { version: 1, screen, events };
    crate::settings::write_atomic(&sidecar_path(clip), serde_json::to_string(&s)?.as_bytes())
}

pub fn read_sidecar(clip: &Path) -> Option<Sidecar> {
    let text = std::fs::read_to_string(sidecar_path(clip)).ok()?;
    serde_json::from_str(&text).ok()
}

/// The part of `src`'s input that a trim from `from` to `to` seconds keeps,
/// written next to `dst`. No-op when `src` has none.
pub fn trim_sidecar(src: &Path, dst: &Path, from: f64, to: f64) -> Result<()> {
    let Some(mut s) = read_sidecar(src) else { return Ok(()) };
    s.events.retain(|e| (from..=to).contains(&e.t));
    for e in &mut s.events {
        e.t -= from;
    }
    write_sidecar(dst, s.screen, s.events)
}

/// State the input hook works from. `cfg` is swapped on settings changes.
pub struct Shared {
    pub cfg: RwLock<KeystrokeSettings>,
    pub log: Mutex<Log>,
    /// Whether events go into `log` for clips; off when only the live
    /// overlay keeps the hook installed.
    pub record: AtomicBool,
    /// Buffer length to keep, QPC ticks.
    pub keep_ticks: AtomicI64,
    /// Set while a privacy-listed app has focus: nothing is recorded.
    pub privacy: Arc<crate::pipeline::Flags>,
}

/// True while the Windows input hooks are installed. Stays false for anyone
/// who never turns the overlay on.
static HOOKED: AtomicBool = AtomicBool::new(false);

pub fn hook_installed() -> bool {
    HOOKED.load(Ordering::Relaxed)
}

/// Where the hook also sends presses and releases while the live overlay is
/// on. `None` (the default) sends nothing.
static LIVE: RwLock<Option<crossbeam_channel::Sender<Event>>> = RwLock::new(None);

pub fn set_live_sink(tx: Option<crossbeam_channel::Sender<Event>>) {
    *LIVE.write() = tx;
}

/// Hand an event to the live overlay, if it's on. Never blocks the hook: a
/// full queue drops the event.
pub(crate) fn send_live(ev: &Event) {
    if let Some(tx) = LIVE.read().as_ref() {
        let _ = tx.try_send(ev.clone());
    }
}

/// Owns the hook thread, which exists only while recording is on.
pub struct Recorder {
    shared: Option<Arc<Shared>>,
    #[cfg(windows)]
    thread: Option<hook::HookThread>,
    privacy: Arc<crate::pipeline::Flags>,
}

impl Recorder {
    /// Nothing is started here: [`Recorder::apply`] decides.
    pub fn new(privacy: Arc<crate::pipeline::Flags>) -> Self {
        Recorder {
            shared: None,
            #[cfg(windows)]
            thread: None,
            privacy,
        }
    }

    pub fn running(&self) -> bool {
        self.shared.is_some()
    }

    /// Install or remove the hooks to match `cfg`. The setting is checked
    /// first: off (neither recording nor the live overlay) means stop (if
    /// running) and return.
    pub fn apply(&mut self, cfg: &KeystrokeSettings, keep_secs: u32) {
        if !cfg.hooks() {
            self.stop();
            return;
        }
        let keep = keep_secs as i64 * TICKS_PER_SEC;
        if let Some(sh) = &self.shared {
            *sh.cfg.write() = cfg.clone();
            sh.keep_ticks.store(keep, Ordering::Relaxed);
            sh.record.store(cfg.records(), Ordering::Relaxed);
            if !cfg.records() {
                // Recording off, live overlay still on: keep nothing.
                *sh.log.lock() = Log::default();
            }
            // A different set of input types needs a different set of hooks.
            #[cfg(windows)]
            let same = self.thread.as_ref().map(|t| (t.keyboard, t.mouse)) == Some(hook::needs(cfg));
            #[cfg(not(windows))]
            let same = true;
            if same {
                return;
            }
            self.stop();
        }
        let sh = Arc::new(Shared {
            cfg: RwLock::new(cfg.clone()),
            log: Mutex::new(Log::default()),
            record: AtomicBool::new(cfg.records()),
            keep_ticks: AtomicI64::new(keep),
            privacy: self.privacy.clone(),
        });
        #[cfg(windows)]
        match hook::HookThread::start(sh.clone()) {
            Ok(t) => {
                self.thread = Some(t);
                HOOKED.store(true, Ordering::Relaxed);
            }
            Err(e) => {
                tracing::error!("keystroke overlay: {e:#}");
                return;
            }
        }
        self.shared = Some(sh);
    }

    pub fn stop(&mut self) {
        #[cfg(windows)]
        if let Some(t) = self.thread.take() {
            t.stop();
            HOOKED.store(false, Ordering::Relaxed);
        }
        self.shared = None;
    }

    /// Input between two QPC times, ready for a sidecar. `None` when
    /// recording is off, even if the live overlay has the hook installed.
    pub fn between(&self, from: i64, to: i64, speed: f64) -> Option<Vec<Event>> {
        let s = self.shared.as_ref().filter(|s| s.record.load(Ordering::Relaxed))?;
        Some(s.log.lock().between(from, to, speed))
    }
}

impl Drop for Recorder {
    fn drop(&mut self) {
        self.stop();
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::settings::Settings;

    #[test]
    fn names() {
        assert_eq!(key_name(0x57), "W");
        assert_eq!(key_name(0xA1), "Shift");
        assert_eq!(key_name(0x74), "F5");
        assert_eq!(key_name(0x20), "Space");
        assert_eq!(key_name(0xFF), "Key FF");
    }

    #[test]
    fn filters_and_presets() {
        let mut k = Settings::default().keystrokes;
        assert!(key_allowed(&k, "Q"));
        assert!(apply_preset(&mut k, "wasd_clicks"));
        assert!(key_allowed(&k, "w") && !key_allowed(&k, "Q") && k.clicks);
        k.filter = KeyFilterMode::Block;
        assert!(!key_allowed(&k, "W") && key_allowed(&k, "Q"));
        assert!(!apply_preset(&mut k, "nope"));
        assert!(presets().iter().all(|p| p.id != "custom"));
    }

    #[test]
    fn log_trims_and_slices() {
        let s = TICKS_PER_SEC;
        let mut log = Log::default();
        for i in 0..10 {
            log.push(i * s, Event::key(0.0, Kind::KeyDown, "W", vec![]), 5 * s);
        }
        assert_eq!(log.len(), 6);
        let ev = log.between(6 * s, 8 * s, 1.0);
        assert_eq!(ev.iter().map(|e| e.t).collect::<Vec<_>>(), vec![0.0, 1.0, 2.0]);
        assert_eq!(log.between(6 * s, 8 * s, 0.5)[2].t, 4.0);
    }

    #[test]
    fn sidecar_roundtrip_and_trim() {
        let dir = std::env::temp_dir().join(format!("rewind-input-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        let clip = dir.join("A.mp4");
        let events = vec![
            Event::key(1.0, Kind::KeyDown, "W", vec!["Shift".into()]),
            Event { t: 3.0, kind: Kind::MouseDown, key: "LMB".into(), mods: vec![], x: Some(10), y: Some(20), delta: None },
        ];
        write_sidecar(&clip, Screen { width: 1920, height: 1080 }, events.clone()).unwrap();
        assert_eq!(sidecar_path(&clip), dir.join("A-input.json"));
        assert_eq!(read_sidecar(&clip).unwrap().events, events);
        let cut = dir.join("B.mp4");
        trim_sidecar(&clip, &cut, 2.0, 5.0).unwrap();
        let s = read_sidecar(&cut).unwrap();
        assert_eq!((s.events.len(), s.events[0].t), (1, 1.0));
        std::fs::remove_dir_all(&dir).ok();
    }

    /// Zero impact when disabled: with default settings `apply` installs no
    /// hook, starts no thread and keeps no log.
    #[test]
    fn off_by_default_installs_nothing() {
        let s = Settings::default();
        let mut r = Recorder::new(Arc::new(crate::pipeline::Flags::default()));
        r.apply(&s.keystrokes, 30);
        assert!(!r.running());
        assert!(!hook_installed());
        assert!(r.between(0, i64::MAX, 1.0).is_none());
        // Enabled with every input type off: still nothing to hook.
        let mut k = s.keystrokes.clone();
        k.enabled = true;
        k.keyboard = false;
        r.apply(&k, 30);
        assert!(!r.running() && !hook_installed());
        // On installs the hook; flipping it off again removes it entirely.
        #[cfg(windows)]
        {
            k.keyboard = true;
            r.apply(&k, 30);
            assert!(r.running() && hook_installed());
            r.apply(&s.keystrokes, 30);
            assert!(!r.running() && !hook_installed());
            // Live overlay alone: hook installed, but nothing kept for clips.
            let mut live = s.keystrokes.clone();
            live.live = true;
            r.apply(&live, 30);
            assert!(r.running() && hook_installed());
            assert!(r.between(0, i64::MAX, 1.0).is_none());
            live.live = false;
            r.apply(&live, 30);
            assert!(!r.running() && !hook_installed());
        }
    }
}
