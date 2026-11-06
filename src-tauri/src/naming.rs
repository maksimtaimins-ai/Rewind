//! Clip file names and folders from user templates. Pure functions, so the
//! rules are testable; the engine only supplies the values.
//!
//! Name tokens: `{game}` `{date}` `{time}` `{duration}` `{counter}` `{hotkey}`
//! `{monitor}`. Unknown `{words}` are kept as typed.

use std::path::PathBuf;

use chrono::{Datelike, NaiveDateTime};
use serde::{Deserialize, Serialize};

pub const DEFAULT_CLIP_TEMPLATE: &str = "{game} {date} {time}";
pub const MAX_TEMPLATE_LEN: usize = 120;

pub struct Ctx<'a> {
    pub game: &'a str,
    pub now: NaiveDateTime,
    /// Length of the clip being saved.
    pub duration_secs: u32,
    /// The hotkey that triggered the save (`Alt+F8`), if known.
    pub hotkey: &'a str,
    /// 1-based, as the user counts monitors.
    pub monitor: u32,
    /// Running number among the clips already in the target folder.
    pub counter: u32,
}

/// `30s`, `2m`, `1m30s`.
pub fn fmt_duration(secs: u32) -> String {
    match (secs / 60, secs % 60) {
        (0, s) => format!("{s}s"),
        (m, 0) => format!("{m}m"),
        (m, s) => format!("{m}m{s}s"),
    }
}

/// The file stem (no extension) for `template`.
pub fn render(template: &str, ctx: &Ctx) -> String {
    let out = expand(template, |name| {
        Some(match name {
            "game" => ctx.game.to_string(),
            "date" => ctx.now.format("%Y-%m-%d").to_string(),
            "time" => ctx.now.format("%H-%M-%S").to_string(),
            "duration" => fmt_duration(ctx.duration_secs),
            "counter" => format!("{:03}", ctx.counter),
            "hotkey" => ctx.hotkey.to_string(),
            "monitor" => format!("Monitor {}", ctx.monitor),
            _ => return None,
        })
    });
    let cleaned = crate::library::sanitize(&out);
    if cleaned.is_empty() {
        crate::library::sanitize(&render(DEFAULT_CLIP_TEMPLATE, ctx))
    } else {
        cleaned
    }
}

/// Replace every known `{token}`; `value` returns `None` for unknown ones,
/// which stay as typed.
pub(crate) fn expand(template: &str, value: impl Fn(&str) -> Option<String>) -> String {
    let mut out = String::with_capacity(template.len() + 16);
    let mut rest = template;
    while let Some(open) = rest.find('{') {
        out.push_str(&rest[..open]);
        let after = &rest[open + 1..];
        match after.find('}') {
            Some(close) => {
                let name = after[..close].trim().to_ascii_lowercase();
                match value(&name) {
                    Some(v) => out.push_str(&v),
                    None => out.push_str(&rest[open..open + close + 2]),
                }
                rest = &after[close + 1..];
            }
            None => {
                out.push_str(&rest[open..]);
                rest = "";
            }
        }
    }
    out.push_str(rest);
    out
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum FolderMode {
    /// Everything in the clips folder.
    Flat,
    Game,
    Date,
    GameDate,
    /// `NamingSettings::folder_template`.
    Custom,
}

/// Folders deeper than this are cut off; the library looks this deep.
pub const MAX_FOLDER_DEPTH: usize = 3;

/// The template a mode stands for (`custom` is the user's own).
pub fn folder_template(mode: FolderMode, custom: &str) -> String {
    match mode {
        FolderMode::Flat => String::new(),
        FolderMode::Game => "{game}".into(),
        FolderMode::Date => "{year}-{month}-{day}".into(),
        FolderMode::GameDate => "{game}/{year}-{month}-{day}".into(),
        FolderMode::Custom => custom.to_string(),
    }
}

/// The subfolder (relative to the clips folder) for `template`. Each part is
/// cleaned; empty parts, `.` and `..` are dropped so a template can never
/// leave the clips folder.
pub fn render_folder(template: &str, ctx: &Ctx) -> PathBuf {
    // Split before filling tokens in, or a "/" from the template and one from
    // a game's name would look the same.
    let mut path = PathBuf::new();
    for part in template.split(['/', '\\']) {
        let part = expand(part, |name| {
            Some(match name {
                "game" => ctx.game.to_string(),
                "year" => format!("{:04}", ctx.now.year()),
                "month" => format!("{:02}", ctx.now.month()),
                "day" => format!("{:02}", ctx.now.day()),
                "week" => format!("{:02}", ctx.now.iso_week().week()),
                _ => return None,
            })
        });
        let part = crate::library::sanitize(&part);
        if !part.is_empty() && part != "." && part != ".." && path.components().count() < MAX_FOLDER_DEPTH {
            path.push(part);
        }
    }
    path
}

#[cfg(test)]
mod tests {
    use super::*;
    use chrono::NaiveDate;

    fn ctx() -> Ctx<'static> {
        Ctx {
            game: "Valorant",
            now: NaiveDate::from_ymd_opt(2026, 11, 6).unwrap().and_hms_opt(14, 3, 9).unwrap(),
            duration_secs: 90,
            hotkey: "Alt+F8",
            monitor: 2,
            counter: 7,
        }
    }

    #[test]
    fn default_matches_the_old_names() {
        assert_eq!(render(DEFAULT_CLIP_TEMPLATE, &ctx()), "Valorant 2026-11-06 14-03-09");
    }

    #[test]
    fn every_token() {
        assert_eq!(render("{game}_{duration}_{counter}_{hotkey}_{monitor}", &ctx()), "Valorant_1m30s_007_Alt+F8_Monitor 2");
    }

    #[test]
    fn unknown_tokens_and_stray_braces_stay() {
        assert_eq!(render("{nope} {game", &ctx()), "{nope} {game");
        assert_eq!(render("{GAME}", &ctx()), "Valorant");
    }

    #[test]
    fn illegal_characters_are_replaced_and_empty_falls_back() {
        assert_eq!(render("a/b:c", &ctx()), "a_b_c");
        assert_eq!(render("   ", &ctx()), "Valorant 2026-11-06 14-03-09");
    }

    #[test]
    fn folders() {
        let c = ctx();
        assert_eq!(render_folder("{game}/{year}-{month}/", &c), PathBuf::from("Valorant").join("2026-11"));
        assert_eq!(render_folder(&folder_template(FolderMode::GameDate, ""), &c), PathBuf::from("Valorant").join("2026-11-06"));
        assert_eq!(render_folder("", &c), PathBuf::new());
        assert_eq!(render_folder("W{week}", &c), PathBuf::from("W45"));
    }

    #[test]
    fn folders_cannot_escape() {
        let c = ctx();
        assert_eq!(render_folder("../../x/./{game}", &c), PathBuf::from("x").join("Valorant"));
        assert_eq!(render_folder("C:\\evil", &c), PathBuf::from("C_").join("evil"));
        assert_eq!(render_folder("a/b/c/d/e", &c).components().count(), MAX_FOLDER_DEPTH);
        let weird = Ctx { game: "AC/DC: Live", ..ctx() };
        assert_eq!(render_folder("{game}", &weird), PathBuf::from("AC_DC_ Live"));
    }

    #[test]
    fn durations() {
        assert_eq!([5, 60, 125].map(fmt_duration), ["5s", "1m", "2m5s"]);
    }
}
