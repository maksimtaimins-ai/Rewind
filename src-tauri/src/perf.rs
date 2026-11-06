//! Performance presets. Low-spec mode is an overlay on the settings the
//! engine runs with: the user's own values stay on disk, so turning it off
//! (or plugging in the charger) puts everything back.

use crate::settings::{Codec, FpsMode, Settings};

/// Whether the low preset is in force: switched on, or throttled on battery.
pub fn throttled(s: &Settings, on_battery: bool) -> bool {
    s.performance.low_spec || (s.performance.battery_throttle && on_battery)
}

/// `s` with the performance settings applied. `display_hz` is the recorded
/// screen's refresh rate when known.
pub fn apply(s: &mut Settings, on_battery: bool, display_hz: Option<u32>) {
    if s.performance.fps_mode == FpsMode::Display {
        if let Some(hz) = display_hz {
            s.video.fps = hz.clamp(24, 240);
        }
    }
    if throttled(s, on_battery) {
        low_spec(s);
    }
}

/// 720p30 HEVC at ~4 Mbps, a 30 s buffer, and nothing optional running. HEVC
/// falls back to H.264 when the machine has no HEVC encoder.
pub fn low_spec(s: &mut Settings) {
    let v = &mut s.video;
    v.height = 720;
    v.fps = 30;
    v.codec = Codec::Hevc;
    v.bitrate_kbps = 4_000;
    v.keyframe_secs = 1.0;
    s.buffer.seconds = 30;
    for slot in &mut s.buffer.extra_slots {
        slot.enabled = false;
    }
    s.post_roll.enabled = false;
    s.keystrokes.enabled = false;
    s.keystrokes.live = false;
    s.smart_highlights.enabled = false;
    s.smart_highlights.analyze_on_save = false;
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Preset {
    Low,
    Medium,
    High,
}

/// The preset a measured encoder speed (frames per second at the output
/// size) can carry with headroom left for the game.
pub fn suggest(encoder_fps: f64, hardware: bool) -> Preset {
    if !hardware || encoder_fps < 90.0 {
        Preset::Low
    } else if encoder_fps < 180.0 {
        Preset::Medium
    } else {
        Preset::High
    }
}

/// Whether the PC runs on battery right now. `None` for desktops and when
/// Windows can't say.
#[cfg(windows)]
pub fn on_battery() -> Option<bool> {
    use windows::Win32::System::Power::{GetSystemPowerStatus, SYSTEM_POWER_STATUS};
    let mut st = SYSTEM_POWER_STATUS::default();
    unsafe { GetSystemPowerStatus(&mut st) }.ok()?;
    // 128: no battery. 255 / 255: unknown.
    if st.BatteryFlag == 128 || st.ACLineStatus == 255 {
        return None;
    }
    Some(st.ACLineStatus == 0)
}

#[cfg(not(windows))]
pub fn on_battery() -> Option<bool> {
    None
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn low_spec_overlay() {
        let mut s = Settings::default();
        s.keystrokes.enabled = true;
        s.performance.low_spec = true;
        apply(&mut s, false, None);
        assert_eq!((s.video.height, s.video.fps, s.video.codec, s.video.bitrate_kbps), (720, 30, Codec::Hevc, 4_000));
        assert_eq!(s.buffer.seconds, 30);
        assert!(s.buffer.extra_slots.iter().all(|x| !x.enabled));
        assert!(!s.keystrokes.enabled && s.video.keyframe_secs == 1.0);
        assert_eq!(s.longest_slot_secs(), 30);
    }

    #[test]
    fn default_is_untouched() {
        let mut s = Settings::default();
        let before = s.clone();
        apply(&mut s, true, None);
        assert_eq!(s, before);
    }

    #[test]
    fn battery_throttle_only_when_enabled_and_unplugged() {
        let mut s = Settings::default();
        s.performance.battery_throttle = true;
        assert!(!throttled(&s, false));
        assert!(throttled(&s, true));
        s.performance.battery_throttle = false;
        assert!(!throttled(&s, true));
    }

    #[test]
    fn display_fps_follows_refresh() {
        let mut s = Settings::default();
        s.performance.fps_mode = FpsMode::Display;
        apply(&mut s, false, Some(144));
        assert_eq!(s.video.fps, 144);
        apply(&mut s, false, Some(1000));
        assert_eq!(s.video.fps, 240);
        let mut fixed = Settings::default();
        apply(&mut fixed, false, Some(144));
        assert_eq!(fixed.video.fps, 60);
    }

    #[test]
    fn suggestions() {
        assert_eq!(suggest(250.0, true), Preset::High);
        assert_eq!(suggest(120.0, true), Preset::Medium);
        assert_eq!(suggest(60.0, true), Preset::Low);
        assert_eq!(suggest(400.0, false), Preset::Low);
    }
}
