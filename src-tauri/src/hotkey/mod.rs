//! Global hotkeys via RegisterHotKey (OS-level, no keyboard hooks).

use std::collections::HashMap;

use anyhow::{bail, Result};
use crossbeam_channel::Sender;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, serde::Serialize)]
#[serde(rename_all = "snake_case")]
pub enum Action {
    SaveClip,
    SaveLongClip,
    Marker,
    Screenshot,
    ExtendLast,
}

/// Parsed hotkey: Win32 modifier flags + virtual key.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Combo {
    pub mods: u32,
    pub vk: u32,
}

const MOD_ALT: u32 = 0x1;
const MOD_CONTROL: u32 = 0x2;
const MOD_SHIFT: u32 = 0x4;
const MOD_WIN: u32 = 0x8;
const MOD_NOREPEAT: u32 = 0x4000;

fn key_code(name: &str) -> Option<u32> {
    let n = name.to_ascii_uppercase();
    if n.len() == 1 {
        let c = n.as_bytes()[0];
        if c.is_ascii_alphanumeric() {
            return Some(c as u32);
        }
    }
    if let Some(f) = n.strip_prefix('F').and_then(|d| d.parse::<u32>().ok()) {
        if (1..=24).contains(&f) {
            return Some(0x70 + f - 1);
        }
    }
    if let Some(d) = n.strip_prefix("NUM").and_then(|d| d.parse::<u32>().ok()) {
        if d <= 9 {
            return Some(0x60 + d);
        }
    }
    Some(match n.as_str() {
        "SPACE" => 0x20,
        "PAGEUP" => 0x21,
        "PAGEDOWN" => 0x22,
        "END" => 0x23,
        "HOME" => 0x24,
        "LEFT" => 0x25,
        "UP" => 0x26,
        "RIGHT" => 0x27,
        "DOWN" => 0x28,
        "PRINTSCREEN" => 0x2C,
        "INSERT" => 0x2D,
        "DELETE" => 0x2E,
        "PAUSE" => 0x13,
        "SCROLLLOCK" => 0x91,
        "`" | "BACKQUOTE" => 0xC0,
        "-" | "MINUS" => 0xBD,
        "=" | "EQUAL" => 0xBB,
        "[" => 0xDB,
        "]" => 0xDD,
        ";" => 0xBA,
        "'" => 0xDE,
        "," => 0xBC,
        "." => 0xBE,
        "/" => 0xBF,
        "\\" => 0xDC,
        _ => return None,
    })
}

/// Parse "Ctrl+Shift+F10" style strings. Empty string = unbound.
pub fn parse(s: &str) -> Result<Option<Combo>> {
    let s = s.trim();
    if s.is_empty() {
        return Ok(None);
    }
    let mut mods = 0;
    let mut vk = None;
    for part in s.split('+').map(str::trim) {
        match part.to_ascii_lowercase().as_str() {
            "ctrl" | "control" => mods |= MOD_CONTROL,
            "alt" => mods |= MOD_ALT,
            "shift" => mods |= MOD_SHIFT,
            "win" | "super" | "meta" => mods |= MOD_WIN,
            _ => {
                if vk.is_some() {
                    bail!("hotkey \"{s}\" has more than one key");
                }
                vk = Some(key_code(part).ok_or_else(|| anyhow::anyhow!("unknown key \"{part}\""))?);
            }
        }
    }
    match vk {
        Some(vk) => Ok(Some(Combo { mods, vk })),
        None => bail!("hotkey \"{s}\" has no key"),
    }
}

pub fn bindings(h: &crate::settings::Hotkeys) -> Vec<(Action, String)> {
    vec![
        (Action::SaveClip, h.save_clip.clone()),
        (Action::SaveLongClip, h.save_long_clip.clone()),
        (Action::Marker, h.marker.clone()),
        (Action::Screenshot, h.screenshot.clone()),
        (Action::ExtendLast, h.extend_last.clone()),
    ]
}

#[cfg(windows)]
pub use win::HotkeyThread;

#[cfg(windows)]
mod win {
    use super::*;
    use windows::Win32::UI::Input::KeyboardAndMouse::{RegisterHotKey, UnregisterHotKey, HOT_KEY_MODIFIERS};
    use windows::Win32::UI::WindowsAndMessaging::{GetMessageW, PostThreadMessageW, MSG, WM_APP, WM_HOTKEY};
    use windows::Win32::System::Threading::GetCurrentThreadId;

    const WM_REBIND: u32 = WM_APP + 1;
    const WM_QUIT_LOOP: u32 = WM_APP + 2;

    pub struct HotkeyThread {
        thread_id: u32,
        pending: Sender<Vec<(Action, Combo)>>,
    }

    impl HotkeyThread {
        /// Spawn the hotkey message loop. Pressed actions go to `actions`;
        /// registration failures (key taken by another app) go to `errors`.
        pub fn spawn(actions: Sender<Action>, errors: Sender<String>) -> Self {
            let (pending_tx, pending_rx) = crossbeam_channel::unbounded::<Vec<(Action, Combo)>>();
            let (id_tx, id_rx) = crossbeam_channel::bounded(1);
            std::thread::Builder::new()
                .name("rewind-hotkeys".into())
                .spawn(move || unsafe {
                    let _ = id_tx.send(GetCurrentThreadId());
                    let mut registered: HashMap<i32, Action> = HashMap::new();
                    let mut msg = MSG::default();
                    while GetMessageW(&mut msg, None, 0, 0).as_bool() {
                        match msg.message {
                            WM_HOTKEY => {
                                if let Some(a) = registered.get(&(msg.wParam.0 as i32)) {
                                    let _ = actions.send(*a);
                                }
                            }
                            WM_REBIND => {
                                let Some(list) = pending_rx.try_iter().last() else { continue };
                                for id in registered.keys() {
                                    let _ = UnregisterHotKey(None, *id);
                                }
                                registered.clear();
                                for (i, (action, combo)) in list.into_iter().enumerate() {
                                    let id = i as i32 + 1;
                                    let mods = HOT_KEY_MODIFIERS(combo.mods | MOD_NOREPEAT);
                                    if RegisterHotKey(None, id, mods, combo.vk).is_ok() {
                                        registered.insert(id, action);
                                    } else {
                                        let _ = errors.send(format!("{action:?}"));
                                    }
                                }
                            }
                            WM_QUIT_LOOP => break,
                            _ => {}
                        }
                    }
                })
                .expect("spawn hotkey thread");
            let thread_id = id_rx.recv().unwrap_or(0);
            Self { thread_id, pending: pending_tx }
        }

        /// Replace all registrations. Returns parse errors immediately.
        pub fn bind(&self, hotkeys: &crate::settings::Hotkeys) -> Vec<String> {
            let mut list = Vec::new();
            let mut errors = Vec::new();
            for (action, text) in bindings(hotkeys) {
                match parse(&text) {
                    Ok(Some(c)) => list.push((action, c)),
                    Ok(None) => {}
                    Err(e) => errors.push(format!("{action:?}: {e}")),
                }
            }
            let _ = self.pending.send(list);
            // The thread may not have entered GetMessage yet; retry briefly.
            for _ in 0..50 {
                if unsafe { PostThreadMessageW(self.thread_id, WM_REBIND, Default::default(), Default::default()) }.is_ok() {
                    break;
                }
                std::thread::sleep(std::time::Duration::from_millis(10));
            }
            errors
        }
    }

    impl Drop for HotkeyThread {
        fn drop(&mut self) {
            unsafe {
                let _ = PostThreadMessageW(self.thread_id, WM_QUIT_LOOP, Default::default(), Default::default());
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_combos() {
        assert_eq!(parse("Alt+F10").unwrap(), Some(Combo { mods: MOD_ALT, vk: 0x79 }));
        assert_eq!(parse("ctrl + shift + s").unwrap(), Some(Combo { mods: MOD_CONTROL | MOD_SHIFT, vk: b'S' as u32 }));
        assert_eq!(parse("").unwrap(), None);
        assert!(parse("Ctrl+Shift").is_err());
        assert!(parse("Alt+Banana").is_err());
    }
}
