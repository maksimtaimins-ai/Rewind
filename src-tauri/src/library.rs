//! Clip library on disk: listing, probing, thumbnails, rename, recycle.

use std::io::{Read, Seek, SeekFrom};
use std::path::{Path, PathBuf};

use anyhow::{bail, Context, Result};
use serde::Serialize;

#[derive(Debug, Clone, Serialize)]
pub struct ClipInfo {
    pub path: PathBuf,
    pub name: String,
    pub size: u64,
    /// Unix milliseconds.
    pub modified: i64,
    pub duration: f64,
    pub thumb: Option<PathBuf>,
}

pub fn thumbs_dir(clips_dir: &Path) -> PathBuf {
    clips_dir.join(".thumbs")
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
    let meta = std::fs::metadata(path)?;
    let modified = meta
        .modified()
        .ok()
        .and_then(|t| t.duration_since(std::time::UNIX_EPOCH).ok())
        .map(|d| d.as_millis() as i64)
        .unwrap_or(0);
    let thumb = thumb_path(path);
    Ok(ClipInfo {
        path: path.to_path_buf(),
        name: path.file_stem().unwrap_or_default().to_string_lossy().to_string(),
        size: meta.len(),
        modified,
        duration: probe_duration(path).unwrap_or(0.0),
        thumb: thumb.exists().then_some(thumb),
    })
}

pub fn list(dir: &Path) -> Vec<ClipInfo> {
    let Ok(rd) = std::fs::read_dir(dir) else { return Vec::new() };
    let mut out: Vec<ClipInfo> = rd
        .filter_map(|e| e.ok())
        .map(|e| e.path())
        .filter(|p| p.extension().is_some_and(|e| e.eq_ignore_ascii_case("mp4")))
        .filter_map(|p| info(&p).ok())
        .collect();
    out.sort_by(|a, b| b.modified.cmp(&a.modified));
    out
}

fn sanitize(name: &str) -> String {
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
    let old_thumb = thumb_path(path);
    if old_thumb.exists() {
        let _ = std::fs::rename(&old_thumb, thumb_path(&target));
    }
    Ok(target)
}

/// Send clips (and their thumbnails) to the Recycle Bin.
#[cfg(windows)]
pub fn recycle(paths: &[PathBuf]) -> Result<()> {
    let mut all: Vec<PathBuf> = Vec::new();
    for p in paths {
        all.push(p.clone());
        let t = thumb_path(p);
        if t.exists() {
            all.push(t);
        }
    }
    let refs: Vec<&Path> = all.iter().map(|p| p.as_path()).collect();
    crate::platform::recycle(&refs)
}

pub fn copy_thumb(from_clip: &Path, to_clip: &Path) {
    let src = thumb_path(from_clip);
    if src.exists() {
        let _ = std::fs::copy(src, thumb_path(to_clip));
    }
}
