//! Clip library on disk: listing, probing, thumbnails, rename, and
//! per-clip metadata (favorites, tags, session names) in `library.json`.

use std::collections::BTreeMap;
use std::io::{Read, Seek, SeekFrom};
use std::path::{Path, PathBuf};

use anyhow::{bail, Context, Result};
use parking_lot::Mutex;
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize)]
pub struct ClipInfo {
    pub path: PathBuf,
    pub name: String,
    pub size: u64,
    /// Unix milliseconds.
    pub modified: i64,
    pub duration: f64,
    pub thumb: Option<PathBuf>,
    /// Thumbnail file's modified time (Unix ms), so the UI can reload a replaced one.
    pub thumb_rev: i64,
    pub favorite: bool,
    pub tags: Vec<String>,
    /// Smart-highlight moments; `None` until the clip has been analysed.
    pub highlights: Option<Vec<crate::analysis::Moment>>,
    /// Keystrokes were recorded with it (`<clip>-input.json`).
    pub has_input: bool,
}

/// What Rewind knows about a clip beyond the file itself.
#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
pub struct ClipMeta {
    #[serde(default)]
    pub favorite: bool,
    #[serde(default)]
    pub tags: Vec<String>,
    /// Smart-highlight moments. `None`: not analysed; empty: nothing found.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub highlights: Option<Vec<crate::analysis::Moment>>,
}

impl ClipMeta {
    fn is_empty(&self) -> bool {
        *self == ClipMeta::default()
    }
}

/// `library.json`. Clips are keyed by file name, so moving the clips folder
/// keeps their metadata.
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct Meta {
    #[serde(default)]
    pub clips: BTreeMap<String, ClipMeta>,
    /// Tag name -> CSS colour, chosen the first time a tag is used.
    #[serde(default)]
    pub tag_colors: BTreeMap<String, String>,
    /// Session key (first clip's Unix ms) -> user-given name.
    #[serde(default)]
    pub sessions: BTreeMap<String, String>,
}

impl Meta {
    pub fn clip(&self, path: &Path) -> ClipMeta {
        self.clips.get(&key(path)).cloned().unwrap_or_default()
    }

    pub fn set_clip(&mut self, path: &Path, m: ClipMeta) {
        if m.is_empty() {
            self.clips.remove(&key(path));
        } else {
            self.clips.insert(key(path), m);
        }
    }

    /// Carry metadata over when a clip's file is renamed or moved.
    pub fn rename(&mut self, from: &Path, to: &Path) {
        if let Some(m) = self.clips.remove(&key(from)) {
            self.clips.insert(key(to), m);
        }
    }
}

fn key(path: &Path) -> String {
    path.file_name().unwrap_or_default().to_string_lossy().to_lowercase()
}

static META_LOCK: Mutex<()> = Mutex::new(());

pub fn meta_path() -> PathBuf {
    crate::settings::app_data_dir().join("library.json")
}

fn read_meta(path: &Path) -> Meta {
    std::fs::read_to_string(path).ok().and_then(|t| serde_json::from_str(&t).ok()).unwrap_or_default()
}

pub fn load_meta() -> Meta {
    let _g = META_LOCK.lock();
    read_meta(&meta_path())
}

fn update_meta_at<T>(path: &Path, f: impl FnOnce(&mut Meta) -> T) -> Result<T> {
    let _g = META_LOCK.lock();
    let mut m = read_meta(path);
    let out = f(&mut m);
    crate::settings::write_atomic(path, serde_json::to_string_pretty(&m)?.as_bytes())?;
    Ok(out)
}

/// Load, change and save `library.json` under a lock.
pub fn update_meta<T>(f: impl FnOnce(&mut Meta) -> T) -> Result<T> {
    update_meta_at(&meta_path(), f)
}

fn apply_meta(mut c: ClipInfo, m: &Meta) -> ClipInfo {
    let cm = m.clip(&c.path);
    c.favorite = cm.favorite;
    c.tags = cm.tags;
    c.highlights = cm.highlights;
    c
}

pub fn thumbs_dir(clips_dir: &Path) -> PathBuf {
    clips_dir.join(".thumbs")
}

/// A thumbnail the user picked: `<clip name>-thumb.jpg` next to the clip.
/// Wins over the automatic one.
pub fn custom_thumb_path(clip: &Path) -> PathBuf {
    let dir = clip.parent().unwrap_or(Path::new("."));
    let stem = clip.file_stem().unwrap_or_default().to_string_lossy();
    dir.join(format!("{stem}-thumb.jpg"))
}

pub fn thumb_path(clip: &Path) -> PathBuf {
    let dir = clip.parent().unwrap_or(Path::new("."));
    let stem = clip.file_stem().unwrap_or_default().to_string_lossy();
    thumbs_dir(dir).join(format!("{stem}.jpg"))
}

/// Duration in seconds from `mvex/mehd` (fragmented) or `mvhd` (progressive),
/// reading only the box headers and `moov`.
pub fn probe_duration(path: &Path) -> Result<f64> {
    let mut f = std::fs::File::open(path)?;
    let len = f.metadata()?.len();
    let mut pos = 0u64;
    while pos + 8 <= len {
        f.seek(SeekFrom::Start(pos))?;
        let mut h = [0u8; 8];
        f.read_exact(&mut h)?;
        let size = u32::from_be_bytes(h[..4].try_into()?) as u64;
        if size < 8 {
            break;
        }
        if &h[4..] == b"moov" && size < 64 << 20 {
            let mut body = vec![0u8; (size - 8) as usize];
            f.read_exact(&mut body)?;
            return Ok(moov_duration(&body));
        }
        pos += size;
    }
    bail!("no moov box")
}

fn moov_duration(moov: &[u8]) -> f64 {
    let mut timescale = 1000u32;
    let mut mvhd_dur = 0u64;
    let mut mehd_dur = 0u64;
    let mut p = 0usize;
    while p + 8 <= moov.len() {
        let size = u32::from_be_bytes(moov[p..p + 4].try_into().unwrap()) as usize;
        if size < 8 || p + size > moov.len() {
            break;
        }
        let body = &moov[p + 8..p + size];
        match &moov[p + 4..p + 8] {
            b"mvhd" if body.len() >= 20 => {
                if body[0] == 1 && body.len() >= 32 {
                    timescale = u32::from_be_bytes(body[20..24].try_into().unwrap());
                    mvhd_dur = u64::from_be_bytes(body[24..32].try_into().unwrap());
                } else {
                    timescale = u32::from_be_bytes(body[12..16].try_into().unwrap());
                    mvhd_dur = u32::from_be_bytes(body[16..20].try_into().unwrap()) as u64;
                }
            }
            b"mvex" => {
                let mut q = 0;
                while q + 8 <= body.len() {
                    let s = u32::from_be_bytes(body[q..q + 4].try_into().unwrap()) as usize;
                    if s < 8 || q + s > body.len() {
                        break;
                    }
                    if &body[q + 4..q + 8] == b"mehd" {
                        let b = &body[q + 8..q + s];
                        mehd_dur = if b[0] == 1 && b.len() >= 12 {
                            u64::from_be_bytes(b[4..12].try_into().unwrap())
                        } else if b.len() >= 8 {
                            u32::from_be_bytes(b[4..8].try_into().unwrap()) as u64
                        } else {
                            0
                        };
                    }
                    q += s;
                }
            }
            _ => {}
        }
        p += size;
    }
    mehd_dur.max(mvhd_dur) as f64 / timescale.max(1) as f64
}

pub fn info(path: &Path) -> Result<ClipInfo> {
    let c = info_bare(path)?;
    Ok(apply_meta(c, &load_meta()))
}

fn info_bare(path: &Path) -> Result<ClipInfo> {
    let meta = std::fs::metadata(path)?;
    let modified = meta
        .modified()
        .ok()
        .and_then(|t| t.duration_since(std::time::UNIX_EPOCH).ok())
        .map(|d| d.as_millis() as i64)
        .unwrap_or(0);
    let custom = custom_thumb_path(path);
    let thumb = if custom.exists() { custom } else { thumb_path(path) };
    let thumb_rev = std::fs::metadata(&thumb)
        .and_then(|m| m.modified())
        .ok()
        .and_then(|t| t.duration_since(std::time::UNIX_EPOCH).ok())
        .map(|d| d.as_millis() as i64)
        .unwrap_or(0);
    let c = ClipInfo {
        path: path.to_path_buf(),
        name: path.file_stem().unwrap_or_default().to_string_lossy().to_string(),
        size: meta.len(),
        modified,
        duration: probe_duration(path).unwrap_or(0.0),
        thumb: thumb.exists().then_some(thumb),
        thumb_rev,
        favorite: false,
        tags: Vec::new(),
        highlights: None,
        has_input: crate::input::sidecar_path(path).exists(),
    };
    Ok(c)
}

/// Files in `dir` and its subfolders one level down (game profiles can save
/// into `<clips>/<game>/`). Hidden folders like `.thumbs` are skipped.
pub fn files_one_deep(dir: &Path) -> Vec<PathBuf> {
    let Ok(rd) = std::fs::read_dir(dir) else { return Vec::new() };
    let mut out = Vec::new();
    for e in rd.flatten() {
        let p = e.path();
        if e.file_type().is_ok_and(|t| t.is_dir()) {
            if !e.file_name().to_string_lossy().starts_with('.') {
                out.extend(std::fs::read_dir(&p).into_iter().flatten().flatten().filter(|f| f.file_type().is_ok_and(|t| t.is_file())).map(|f| f.path()));
            }
        } else {
            out.push(p);
        }
    }
    out
}

pub fn list(dir: &Path) -> Vec<ClipInfo> {
    let meta = load_meta();
    let mut out: Vec<ClipInfo> = files_one_deep(dir)
        .into_iter()
        .filter(|p| p.extension().is_some_and(|e| e.eq_ignore_ascii_case("mp4")))
        .filter_map(|p| info_bare(&p).ok())
        .map(|c| apply_meta(c, &meta))
        .collect();
    out.sort_by_key(|c| std::cmp::Reverse(c.modified));
    out
}

/// A screenshot in the clips folder.
#[derive(Debug, Clone, Serialize)]
pub struct ShotInfo {
    pub path: PathBuf,
    pub name: String,
    pub size: u64,
    /// Unix milliseconds.
    pub modified: i64,
}

/// PNGs in the clips folder (and one level down), newest first.
pub fn list_screenshots(dir: &Path) -> Vec<ShotInfo> {
    let mut out: Vec<ShotInfo> = files_one_deep(dir)
        .into_iter()
        .filter(|p| p.extension().is_some_and(|e| e.eq_ignore_ascii_case("png")))
        .filter_map(|p| {
            let md = std::fs::metadata(&p).ok()?;
            let modified = md.modified().ok()?.duration_since(std::time::UNIX_EPOCH).ok()?.as_millis() as i64;
            let name = p.file_stem()?.to_string_lossy().into_owned();
            Some(ShotInfo { path: p, name, size: md.len(), modified })
        })
        .collect();
    out.sort_by_key(|s| std::cmp::Reverse(s.modified));
    out
}

pub(crate) fn sanitize(name: &str) -> String {
    let cleaned: String = name
        .chars()
        .map(|c| if matches!(c, '<' | '>' | ':' | '"' | '/' | '\\' | '|' | '?' | '*') || c.is_control() { '_' } else { c })
        .collect();
    cleaned.trim().trim_end_matches('.').to_string()
}

/// A free path `dir/<stem>.<ext>`, adding " (2)", " (3)"... if taken.
pub fn unique_path(dir: &Path, stem: &str, ext: &str) -> PathBuf {
    let stem = sanitize(stem);
    let mut p = dir.join(format!("{stem}.{ext}"));
    let mut n = 2;
    while p.exists() {
        p = dir.join(format!("{stem} ({n}).{ext}"));
        n += 1;
    }
    p
}

pub fn rename(path: &Path, new_name: &str) -> Result<PathBuf> {
    let new_name = sanitize(new_name);
    if new_name.is_empty() {
        bail!("name can't be empty");
    }
    let dir = path.parent().context("clip has no folder")?;
    let target = dir.join(format!("{new_name}.mp4"));
    if target.exists() {
        bail!("a clip named \"{new_name}\" already exists");
    }
    std::fs::rename(path, &target)?;
    let _ = update_meta(|m| m.rename(path, &target));
    let input = (crate::input::sidecar_path(path), crate::input::sidecar_path(&target));
    for (from, to) in [(thumb_path(path), thumb_path(&target)), (custom_thumb_path(path), custom_thumb_path(&target)), input] {
        if from.exists() {
            let _ = std::fs::rename(&from, to);
        }
    }
    Ok(target)
}

/// Kinds of thumbnail file that belong to a clip and move with it.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ThumbKind {
    Auto,
    Custom,
}

impl ThumbKind {
    pub const ALL: [ThumbKind; 2] = [ThumbKind::Auto, ThumbKind::Custom];

    pub fn path_for(self, clip: &Path) -> PathBuf {
        match self {
            ThumbKind::Auto => thumb_path(clip),
            ThumbKind::Custom => custom_thumb_path(clip),
        }
    }

    pub fn tag(self) -> &'static str {
        match self {
            ThumbKind::Auto => "thumb",
            ThumbKind::Custom => "custom",
        }
    }
}

/// Star or unstar a clip.
pub fn set_favorite(path: &Path, favorite: bool) -> Result<()> {
    update_meta(|m| {
        let mut c = m.clip(path);
        c.favorite = favorite;
        m.set_clip(path, c);
    })
}

/// Tidy user-typed tags: trimmed, at most 32 characters, no duplicates
/// (ignoring case), at most 20 per clip.
pub fn clean_tags(tags: &[String]) -> Vec<String> {
    let mut out: Vec<String> = Vec::new();
    for t in tags {
        let t: String = t.trim().chars().take(32).collect();
        if !t.is_empty() && !out.iter().any(|o| o.eq_ignore_ascii_case(&t)) {
            out.push(t);
        }
    }
    out.truncate(20);
    out
}

pub fn set_tags(path: &Path, tags: &[String]) -> Result<()> {
    update_meta(|m| {
        let mut c = m.clip(path);
        c.tags = clean_tags(tags);
        m.set_clip(path, c);
    })
}

/// Store (or with `None`, forget) a clip's smart-highlight moments.
pub fn set_highlights(path: &Path, moments: Option<Vec<crate::analysis::Moment>>) -> Result<()> {
    update_meta(|m| {
        let mut c = m.clip(path);
        c.highlights = moments;
        m.set_clip(path, c);
    })
}

pub fn copy_thumb(from_clip: &Path, to_clip: &Path) {
    let src = thumb_path(from_clip);
    if src.exists() {
        let _ = std::fs::copy(src, thumb_path(to_clip));
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn tags_cleaned() {
        let t = clean_tags(&[" Clutch ".into(), "clutch".into(), "".into(), "x".repeat(40)]);
        assert_eq!(t, vec!["Clutch".to_string(), "x".repeat(32)]);
    }

    #[test]
    fn meta_roundtrip_and_rename() {
        let dir = std::env::temp_dir().join(format!("rewind-meta-{}", std::process::id()));
        let path = dir.join("library.json");
        let a = Path::new("C:/clips/A.mp4");
        let b = Path::new("D:/elsewhere/B.mp4");
        update_meta_at(&path, |m| m.set_clip(a, ClipMeta { favorite: true, tags: vec!["clutch".into()], highlights: None })).unwrap();
        update_meta_at(&path, |m| m.rename(a, b)).unwrap();
        let m = read_meta(&path);
        assert_eq!(m.clip(Path::new("C:/other/b.MP4")).tags, vec!["clutch".to_string()]);
        assert!(m.clip(a).is_empty());
        update_meta_at(&path, |m| m.set_clip(b, ClipMeta::default())).unwrap();
        assert!(read_meta(&path).clips.is_empty());
        std::fs::remove_dir_all(&dir).ok();
    }
}
