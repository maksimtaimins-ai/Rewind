//! Situational profiles: settings that switch in by circumstance (a game is
//! running, nothing is, one app has focus, on battery, time of day), layered
//! under any per-game profile. With none switched on, nothing is looked at.

use crate::settings::{Settings, SituationalProfile, Trigger};

/// What the engine knows right now. Only the parts a switched-on profile
/// asks about are ever filled in.
#[derive(Debug, Default, Clone)]
pub struct Context {
    pub game_running: bool,
    pub focused_exe: Option<String>,
    pub on_battery: bool,
    /// Minutes since local midnight.
    pub minutes: u32,
}

/// Whether any profile is on, i.e. whether the engine needs to look at all.
pub fn any_on(s: &Settings) -> bool {
    s.situational.iter().any(|p| p.enabled)
}

pub fn needs_focus(s: &Settings) -> bool {
    s.situational.iter().any(|p| p.enabled && matches!(p.trigger, Trigger::AppFocused { .. }))
}

pub fn needs_power(s: &Settings) -> bool {
    s.situational.iter().any(|p| p.enabled && matches!(p.trigger, Trigger::OnBattery | Trigger::PluggedIn))
}

/// `HH:MM` as minutes since midnight.
pub fn parse_time(t: &str) -> Option<u32> {
    let (h, m) = t.trim().split_once(':')?;
    let (h, m): (u32, u32) = (h.parse().ok()?, m.parse().ok()?);
    (h < 24 && m < 60).then_some(h * 60 + m)
}

fn matches(t: &Trigger, c: &Context) -> bool {
    match t {
        Trigger::GameDetected => c.game_running,
        Trigger::Idle => !c.game_running,
        Trigger::AppFocused { exe } => c.focused_exe.as_deref().is_some_and(|f| f.eq_ignore_ascii_case(exe)),
        Trigger::OnBattery => c.on_battery,
        Trigger::PluggedIn => !c.on_battery,
        Trigger::TimeOfDay { from, to } => {
            let (Some(from), Some(to)) = (parse_time(from), parse_time(to)) else { return false };
            if from <= to { (from..to).contains(&c.minutes) } else { c.minutes >= from || c.minutes < to }
        }
    }
}

/// The first switched-on profile whose condition holds.
pub fn active<'a>(s: &'a Settings, c: &Context) -> Option<&'a SituationalProfile> {
    s.situational.iter().find(|p| p.enabled && matches(&p.trigger, c))
}

/// `s` with `p`'s overrides applied.
pub fn apply(s: &Settings, p: &SituationalProfile) -> Settings {
    let mut e = s.clone();
    let v = &mut e.video;
    v.fps = p.fps.unwrap_or(v.fps);
    v.height = p.height.unwrap_or(v.height);
    v.bitrate_kbps = p.bitrate_kbps.unwrap_or(v.bitrate_kbps);
    v.codec = p.codec.unwrap_or(v.codec);
    e.buffer.seconds = p.buffer_seconds.unwrap_or(e.buffer.seconds);
    if let Some(h) = &p.hotkeys {
        e.hotkeys = h.clone();
    }
    if p.overlays_off {
        e.keystrokes.enabled = false;
        e.keystrokes.live = false;
    }
    e
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::settings::Codec;

    fn profile(trigger: Trigger) -> SituationalProfile {
        SituationalProfile { name: "p".into(), enabled: true, trigger, fps: Some(30), height: None, bitrate_kbps: None, codec: None, buffer_seconds: Some(60), hotkeys: None, overlays_off: true }
    }

    #[test]
    fn none_configured_is_inert() {
        let s = Settings::default();
        assert!(!any_on(&s) && !needs_focus(&s) && !needs_power(&s));
        assert!(active(&s, &Context::default()).is_none());
    }

    #[test]
    fn triggers() {
        let mut c = Context { game_running: true, minutes: 23 * 60, ..Default::default() };
        assert!(matches(&Trigger::GameDetected, &c) && !matches(&Trigger::Idle, &c));
        c.game_running = false;
        assert!(matches(&Trigger::Idle, &c));
        c.focused_exe = Some("Code.exe".into());
        assert!(matches(&Trigger::AppFocused { exe: "code.exe".into() }, &c));
        assert!(matches(&Trigger::PluggedIn, &c) && !matches(&Trigger::OnBattery, &c));
        let night = Trigger::TimeOfDay { from: "22:00".into(), to: "06:00".into() };
        assert!(matches(&night, &c));
        assert!(!matches(&night, &Context { minutes: 12 * 60, ..Default::default() }));
        let day = Trigger::TimeOfDay { from: "09:00".into(), to: "17:00".into() };
        assert!(matches(&day, &Context { minutes: 9 * 60, ..Default::default() }));
        assert!(!matches(&day, &Context { minutes: 17 * 60, ..Default::default() }));
        assert!(!matches(&Trigger::TimeOfDay { from: "x".into(), to: "y".into() }, &c));
    }

    #[test]
    fn first_enabled_match_wins_and_overrides_apply() {
        let mut s = Settings::default();
        let mut off = profile(Trigger::Idle);
        off.enabled = false;
        let mut second = profile(Trigger::Idle);
        second.name = "second".into();
        s.situational = vec![off, second.clone(), profile(Trigger::Idle)];
        assert_eq!(active(&s, &Context::default()).unwrap().name, "second");
        let e = apply(&s, &second);
        assert_eq!((e.video.fps, e.buffer.seconds, e.video.height, e.video.codec), (30, 60, s.video.height, Codec::H264));
        assert!(!e.keystrokes.enabled);
    }

    #[test]
    fn time_parsing() {
        assert_eq!(parse_time("07:05"), Some(425));
        assert_eq!(parse_time("24:00"), None);
        assert_eq!(parse_time("nope"), None);
    }
}
