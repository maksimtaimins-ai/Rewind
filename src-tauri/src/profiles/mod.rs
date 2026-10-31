//! Game detection (process + foreground polling) and per-game profiles.

use std::collections::HashSet;

use serde::Serialize;

use crate::settings::{GameProfile, Settings};

/// Well-known game executables, matched case-insensitively.
const KNOWN_GAMES: &[(&str, &str)] = &[
    ("cs2.exe", "Counter-Strike 2"),
    ("valorant-win64-shipping.exe", "VALORANT"),
    ("fortniteclient-win64-shipping.exe", "Fortnite"),
    ("r5apex.exe", "Apex Legends"),
    ("r5apex_dx12.exe", "Apex Legends"),
    ("league of legends.exe", "League of Legends"),
    ("overwatch.exe", "Overwatch 2"),
    ("rocketleague.exe", "Rocket League"),
    ("dota2.exe", "Dota 2"),
    ("gta5.exe", "GTA V"),
    ("gta5_enhanced.exe", "GTA V"),
    ("eldenring.exe", "Elden Ring"),
    ("rainbowsix.exe", "Rainbow Six Siege"),
    ("rainbowsix_vulkan.exe", "Rainbow Six Siege"),
    ("cod.exe", "Call of Duty"),
    ("destiny2.exe", "Destiny 2"),
    ("tslgame.exe", "PUBG"),
    ("marvel-win64-shipping.exe", "Marvel Rivals"),
    ("deadlock.exe", "Deadlock"),
    ("project8.exe", "Deadlock"),
    ("thefinals.exe", "THE FINALS"),
    ("helldivers2.exe", "Helldivers 2"),
    ("minecraft.windows.exe", "Minecraft"),
    ("robloxplayerbeta.exe", "Roblox"),
    ("witcher3.exe", "The Witcher 3"),
    ("cyberpunk2077.exe", "Cyberpunk 2077"),
    ("bf2042.exe", "Battlefield 2042"),
    ("escapefromtarkov.exe", "Escape from Tarkov"),
    ("rustclient.exe", "Rust"),
    ("hunt.exe", "Hunt: Showdown"),
];

/// Fullscreen apps that are never games.
const NOT_GAMES: &[&str] = &[
    "explorer.exe",
    "chrome.exe",
    "msedge.exe",
    "firefox.exe",
    "opera.exe",
    "brave.exe",
    "vlc.exe",
    "mpv.exe",
    "mpc-hc64.exe",
    "applicationframehost.exe",
    "rewind.exe",
    "powerpnt.exe",
    "lockapp.exe",
    "searchhost.exe",
    "shellexperiencehost.exe",
    "startmenuexperiencehost.exe",
    "textinputhost.exe",
    "code.exe",
];

#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct Game {
    pub exe: String,
    pub name: String,
    pub pid: u32,
}

pub fn display_name(exe: &str, profiles: &[GameProfile]) -> String {
    let lower = exe.to_ascii_lowercase();
    if let Some(p) = profiles.iter().find(|p| p.exe.eq_ignore_ascii_case(exe)) {
        return p.name.clone();
    }
    if let Some((_, n)) = KNOWN_GAMES.iter().find(|(e, _)| *e == lower) {
        return n.to_string();
    }
    exe.strip_suffix(".exe").or_else(|| exe.strip_suffix(".EXE")).unwrap_or(exe).to_string()
}

pub fn profile_for<'a>(exe: &str, profiles: &'a [GameProfile]) -> Option<&'a GameProfile> {
    profiles.iter().find(|p| p.exe.eq_ignore_ascii_case(exe))
}

/// Remembers fullscreen apps seen in the foreground so a game stays "running"
/// while alt-tabbed, until its process exits.
#[derive(Default)]
pub struct Detector {
    sticky: Option<Game>,
}

impl Detector {
    /// `processes` = lower-cased running exe names with pids.
    #[cfg(windows)]
    pub fn poll(&mut self, settings: &Settings) -> Option<Game> {
        let procs = crate::platform::processes();
        let running: HashSet<(String, u32)> = procs.iter().map(|p| (p.exe.to_ascii_lowercase(), p.pid)).collect();

        if let Some(g) = &self.sticky {
            if !running.contains(&(g.exe.to_ascii_lowercase(), g.pid)) {
                self.sticky = None;
            }
        }

        let fg = crate::platform::foreground();
        if let Some(fg) = &fg {
            let lower = fg.exe.to_ascii_lowercase();
            let known = KNOWN_GAMES.iter().any(|(e, _)| *e == lower)
                || settings.profiles.iter().any(|p| p.exe.eq_ignore_ascii_case(&fg.exe));
            let fullscreen_app = fg.fullscreen
                && !NOT_GAMES.contains(&lower.as_str())
                && !fg.path.to_ascii_lowercase().contains("\\windows\\");
            if known || fullscreen_app {
                self.sticky = Some(Game { exe: fg.exe.clone(), name: display_name(&fg.exe, &settings.profiles), pid: fg.pid });
            }
        }
        if self.sticky.is_some() {
            return self.sticky.clone();
        }
        // A known game running in the background (e.g. launched, still loading).
        for p in &procs {
            let lower = p.exe.to_ascii_lowercase();
            if KNOWN_GAMES.iter().any(|(e, _)| *e == lower) || settings.profiles.iter().any(|x| x.exe.eq_ignore_ascii_case(&p.exe)) {
                return Some(Game { exe: p.exe.clone(), name: display_name(&p.exe, &settings.profiles), pid: p.pid });
            }
        }
        None
    }
}

/// True if a privacy-listed app currently has focus.
#[cfg(windows)]
pub fn privacy_focused(settings: &Settings) -> bool {
    match crate::platform::foreground() {
        Some(fg) => settings.privacy_apps.iter().any(|a| a.eq_ignore_ascii_case(&fg.exe)),
        None => false,
    }
}
