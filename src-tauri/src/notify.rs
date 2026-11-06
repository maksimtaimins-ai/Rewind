//! Notification sounds and "do not disturb". Nothing here runs unless an event
//! with a sound fires; with every sound `none` (and no toasts wanted) the
//! module is never touched.
//!
//! A sound setting is `none`, a built-in id (`blip`, `chime`, `click`, `pop`,
//! `low`) or `file:<path to a .wav>`.

use std::sync::atomic::{AtomicBool, Ordering};

use crate::settings::NotificationSettings;

/// Events with their own toast switch and sound.
pub const EVENTS: [&str; 5] = ["clip_saved", "marker", "screenshot", "update", "benchmark"];

pub const BUILTIN_SOUNDS: [&str; 5] = ["blip", "chime", "click", "pop", "low"];

/// Silence everything until Rewind restarts. Never saved.
static DND: AtomicBool = AtomicBool::new(false);

pub fn set_dnd(on: bool) {
    DND.store(on, Ordering::Relaxed);
}

pub fn dnd() -> bool {
    DND.load(Ordering::Relaxed)
}

/// Whether `s` is a value a sound setting may hold.
pub fn valid_sound(s: &str) -> bool {
    s == "none" || BUILTIN_SOUNDS.contains(&s) || s.strip_prefix("file:").is_some_and(|p| p.to_ascii_lowercase().ends_with(".wav"))
}

/// The sound configured for `event`, if any.
pub fn sound_for<'a>(n: &'a NotificationSettings, event: &str) -> Option<&'a str> {
    let s = match event {
        "clip_saved" => &n.sounds.clip_saved,
        "marker" => &n.sounds.marker,
        "screenshot" => &n.sounds.screenshot,
        "update" => &n.sounds.update,
        "benchmark" => &n.sounds.benchmark,
        _ => return None,
    };
    (s != "none").then_some(s.as_str())
}

/// Whether the toast for `event` is switched on.
pub fn toast_enabled(n: &NotificationSettings, event: &str) -> bool {
    match event {
        "clip_saved" => n.events.clip_saved,
        "marker" => n.events.marker,
        "screenshot" => n.events.screenshot,
        "update" => n.events.update,
        "benchmark" => n.events.benchmark,
        _ => true,
    }
}

/// Play `event`'s sound now, unless do-not-disturb is on or it has none.
pub fn play_event(n: &NotificationSettings, event: &str) {
    if dnd() {
        return;
    }
    if let Some(s) = sound_for(n, event) {
        play(s);
    }
}

/// Play a sound setting value. Unknown values are ignored.
pub fn play(sound: &str) {
    if !valid_sound(sound) || sound == "none" {
        return;
    }
    if let Some(path) = sound.strip_prefix("file:") {
        crate::platform::play_wav_file(path);
    } else if let Some(wav) = builtin_wav(sound) {
        crate::platform::play_wav_bytes(wav);
    }
}

/// `(frequency Hz, milliseconds)` steps of a built-in sound.
fn steps(id: &str) -> &'static [(f32, u32)] {
    match id {
        "blip" => &[(880.0, 60), (1320.0, 90)],
        "chime" => &[(1047.0, 70), (1319.0, 70), (1568.0, 160)],
        "click" => &[(2400.0, 14)],
        "pop" => &[(700.0, 35), (420.0, 70)],
        "low" => &[(220.0, 90), (165.0, 150)],
        _ => &[],
    }
}

/// A 16-bit mono WAV of a built-in sound, or `None` for an unknown id. Cached
/// per id, so each is built once.
pub fn builtin_wav(id: &str) -> Option<&'static [u8]> {
    use std::collections::HashMap;
    use std::sync::{Mutex, OnceLock};
    static CACHE: OnceLock<Mutex<HashMap<String, &'static [u8]>>> = OnceLock::new();
    let st = steps(id);
    if st.is_empty() {
        return None;
    }
    let mut cache = CACHE.get_or_init(Default::default).lock().ok()?;
    if let Some(w) = cache.get(id) {
        return Some(w);
    }
    let wav: &'static [u8] = Box::leak(synth(st).into_boxed_slice());
    cache.insert(id.to_string(), wav);
    Some(wav)
}

fn synth(steps: &[(f32, u32)]) -> Vec<u8> {
    let rate = 44_100u32;
    let mut pcm: Vec<i16> = Vec::new();
    for &(freq, ms) in steps {
        let n = rate * ms / 1000;
        for i in 0..n {
            let t = i as f32 / rate as f32;
            let env = (1.0 - i as f32 / n as f32).powi(2);
            pcm.push(((t * freq * std::f32::consts::TAU).sin() * env * 9000.0) as i16);
        }
    }
    let data_len = (pcm.len() * 2) as u32;
    let mut w = Vec::with_capacity(44 + data_len as usize);
    w.extend_from_slice(b"RIFF");
    w.extend_from_slice(&(36 + data_len).to_le_bytes());
    w.extend_from_slice(b"WAVEfmt ");
    w.extend_from_slice(&16u32.to_le_bytes());
    w.extend_from_slice(&1u16.to_le_bytes());
    w.extend_from_slice(&1u16.to_le_bytes());
    w.extend_from_slice(&rate.to_le_bytes());
    w.extend_from_slice(&(rate * 2).to_le_bytes());
    w.extend_from_slice(&2u16.to_le_bytes());
    w.extend_from_slice(&16u16.to_le_bytes());
    w.extend_from_slice(b"data");
    w.extend_from_slice(&data_len.to_le_bytes());
    for s in pcm {
        w.extend_from_slice(&s.to_le_bytes());
    }
    w
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn sound_values() {
        assert!(valid_sound("none") && valid_sound("chime") && valid_sound("file:C:\\x\\ding.WAV"));
        assert!(!valid_sound("bogus") && !valid_sound("file:C:\\x\\song.mp3") && !valid_sound(""));
    }

    #[test]
    fn builtins_are_wavs() {
        for id in BUILTIN_SOUNDS {
            let w = builtin_wav(id).unwrap();
            assert_eq!(&w[..4], b"RIFF");
            assert!(w.len() > 44);
        }
        assert!(builtin_wav("none").is_none());
    }

    #[test]
    fn dnd_is_a_switch() {
        set_dnd(true);
        assert!(dnd());
        set_dnd(false);
        assert!(!dnd());
    }
}
