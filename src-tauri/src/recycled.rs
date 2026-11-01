//! Soft delete: deleted clips move to `%APPDATA%\Rewind\recycled\` with their
//! thumbnail and metadata, can be restored, and are purged after a while.

use std::path::{Path, PathBuf};

use anyhow::{Context, Result};
use serde::{Deserialize, Serialize};

use crate::library::{self, ClipMeta};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Entry {
    /// File name inside the bin; also the id the UI uses.
    pub id: String,
    /// Where the clip lived, for restoring.
    pub original: PathBuf,
    /// Unix milliseconds.
    pub deleted_ms: i64,
    #[serde(default)]
    pub meta: ClipMeta,
}

#[derive(Debug, Clone, Serialize)]
pub struct RecycledClip {
    pub id: String,
    pub name: String,
    pub original: PathBuf,
    pub deleted_ms: i64,
    pub size: u64,
    pub duration: f64,
    pub thumb: Option<PathBuf>,
}

pub fn bin_dir() -> PathBuf {
    crate::settings::app_data_dir().join("recycled")
}

fn index_path(bin: &Path) -> PathBuf {
    bin.join("index.json")
}

fn load(bin: &Path) -> Vec<Entry> {
    std::fs::read_to_string(index_path(bin)).ok().and_then(|t| serde_json::from_str(&t).ok()).unwrap_or_default()
}

fn store(bin: &Path, entries: &[Entry]) -> Result<()> {
    crate::settings::write_atomic(&index_path(bin), serde_json::to_string_pretty(entries)?.as_bytes())
}

fn thumb_in_bin(bin: &Path, id: &str, kind: library::ThumbKind) -> PathBuf {
    bin.join(format!("{id}.{}.jpg", kind.tag()))
}

/// Rename, or copy and remove when the bin is on another drive.
fn move_file(from: &Path, to: &Path) -> Result<()> {
    if std::fs::rename(from, to).is_ok() {
        return Ok(());
    }
    std::fs::copy(from, to).with_context(|| format!("copy {}", from.display()))?;
    std::fs::remove_file(from)?;
    Ok(())
}

static LOCK: parking_lot::Mutex<()> = parking_lot::Mutex::new(());

/// Move clips into the bin. Returns how many moved.
pub fn delete_into(bin: &Path, paths: &[PathBuf], now_ms: i64, take_meta: &mut dyn FnMut(&Path) -> ClipMeta) -> Result<usize> {
    let _g = LOCK.lock();
    std::fs::create_dir_all(bin)?;
    let mut entries = load(bin);
    let mut moved = 0;
    for p in paths {
        let stem = p.file_stem().unwrap_or_default().to_string_lossy();
        let dest = library::unique_path(bin, &stem, "mp4");
        let id = dest.file_name().unwrap_or_default().to_string_lossy().to_string();
        move_file(p, &dest)?;
        for kind in library::ThumbKind::ALL {
            let thumb = kind.path_for(p);
            if thumb.exists() {
                let _ = move_file(&thumb, &thumb_in_bin(bin, &id, kind));
            }
        }
        entries.push(Entry { id, original: p.clone(), deleted_ms: now_ms, meta: take_meta(p) });
        moved += 1;
        store(bin, &entries)?;
    }
    Ok(moved)
}

/// Move clips back to where they were (or a free name there). Returns the
/// restored paths with their metadata.
pub fn restore_from(bin: &Path, ids: &[String]) -> Result<Vec<(PathBuf, ClipMeta)>> {
    let _g = LOCK.lock();
    let mut entries = load(bin);
    let mut out = Vec::new();
    for id in ids {
        let Some(i) = entries.iter().position(|e| &e.id == id) else { continue };
        let e = entries.remove(i);
        let dir = e.original.parent().map(Path::to_path_buf).unwrap_or_default();
        std::fs::create_dir_all(&dir)?;
        let stem = e.original.file_stem().unwrap_or_default().to_string_lossy();
        let dest = library::unique_path(&dir, &stem, "mp4");
        move_file(&bin.join(&e.id), &dest)?;
        for kind in library::ThumbKind::ALL {
            let thumb = thumb_in_bin(bin, &e.id, kind);
            if thumb.exists() {
                let t = kind.path_for(&dest);
                if let Some(d) = t.parent() {
                    let _ = std::fs::create_dir_all(d);
                }
                let _ = move_file(&thumb, &t);
            }
        }
        out.push((dest, e.meta));
        store(bin, &entries)?;
    }
    Ok(out)
}

/// Permanently delete clips that have been in the bin longer than `days`, and
/// any stray file there just as old that no entry owns (left by a crash
/// mid-delete). Returns how many clips and strays went.
pub fn purge_older(bin: &Path, now_ms: i64, days: u32) -> Result<usize> {
    let _g = LOCK.lock();
    let mut entries = load(bin);
    let cutoff = now_ms - days as i64 * 86_400_000;
    let before = entries.len();
    let mut strays = 0;
    entries.retain(|e| {
        let old = e.deleted_ms < cutoff;
        if old {
            let _ = std::fs::remove_file(bin.join(&e.id));
            for kind in library::ThumbKind::ALL {
                let _ = std::fs::remove_file(thumb_in_bin(bin, &e.id, kind));
            }
        }
        !old
    });
    if entries.len() != before {
        store(bin, &entries)?;
    }
    for f in std::fs::read_dir(bin).into_iter().flatten().flatten() {
        let name = f.file_name().to_string_lossy().to_string();
        // Thumbnails are `<id>.<kind>.jpg`; the index and its temp file are ours.
        let owned = name.starts_with("index.") || entries.iter().any(|e| name == e.id || name.starts_with(&format!("{}.", e.id)));
        let modified_ms = f.metadata().and_then(|m| m.modified()).ok().and_then(|t| t.duration_since(std::time::UNIX_EPOCH).ok()).map(|d| d.as_millis() as i64);
        if !owned && f.path().is_file() && modified_ms.is_some_and(|ms| ms < cutoff) && std::fs::remove_file(f.path()).is_ok() {
            strays += 1;
        }
    }
    Ok(before - entries.len() + strays)
}

pub fn list_in(bin: &Path) -> Vec<RecycledClip> {
    let mut v: Vec<RecycledClip> = load(bin)
        .into_iter()
        .filter_map(|e| {
            let path = bin.join(&e.id);
            let meta = std::fs::metadata(&path).ok()?;
            let thumb = [library::ThumbKind::Custom, library::ThumbKind::Auto].into_iter().map(|k| thumb_in_bin(bin, &e.id, k)).find(|t| t.exists());
            Some(RecycledClip {
                name: e.original.file_stem().unwrap_or_default().to_string_lossy().to_string(),
                size: meta.len(),
                duration: library::probe_duration(&path).unwrap_or(0.0),
                thumb,
                id: e.id,
                original: e.original,
                deleted_ms: e.deleted_ms,
            })
        })
        .collect();
    v.sort_by(|a, b| b.deleted_ms.cmp(&a.deleted_ms));
    v
}

fn now_ms() -> i64 {
    chrono::Local::now().timestamp_millis()
}

/// Soft-delete clips, taking their metadata out of the library.
pub fn delete(paths: &[PathBuf]) -> Result<usize> {
    let mut take = |p: &Path| {
        library::update_meta(|m| {
            let c = m.clip(p);
            m.set_clip(p, ClipMeta::default());
            c
        })
        .unwrap_or_default()
    };
    delete_into(&bin_dir(), paths, now_ms(), &mut take)
}

pub fn restore(ids: &[String]) -> Result<usize> {
    let restored = restore_from(&bin_dir(), ids)?;
    library::update_meta(|m| {
        for (p, meta) in &restored {
            m.set_clip(p, meta.clone());
        }
    })?;
    Ok(restored.len())
}

pub fn purge(days: u32) -> Result<usize> {
    purge_older(&bin_dir(), now_ms(), days)
}

pub fn list() -> Vec<RecycledClip> {
    list_in(&bin_dir())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn delete_restore_purge() {
        let root = std::env::temp_dir().join(format!("rewind-bin-{}", std::process::id()));
        let clips = root.join("clips");
        let bin = root.join("bin");
        std::fs::create_dir_all(library::thumbs_dir(&clips)).unwrap();
        let a = clips.join("A.mp4");
        std::fs::write(&a, b"video").unwrap();
        std::fs::write(library::thumb_path(&a), b"jpg").unwrap();
        std::fs::write(library::custom_thumb_path(&a), b"custom").unwrap();

        let meta = ClipMeta { favorite: true, tags: vec!["x".into()] };
        let n = delete_into(&bin, &[a.clone()], 1_000, &mut |_| meta.clone()).unwrap();
        assert_eq!(n, 1);
        assert!(!a.exists() && !library::thumb_path(&a).exists());
        let listed = list_in(&bin);
        assert_eq!(listed.len(), 1);
        assert!(listed[0].thumb.is_some());

        let back = restore_from(&bin, &[listed[0].id.clone()]).unwrap();
        assert_eq!(back[0].0, a);
        assert_eq!(back[0].1, meta);
        assert!(a.exists() && library::thumb_path(&a).exists());
        assert_eq!(std::fs::read(library::custom_thumb_path(&a)).unwrap(), b"custom");
        assert!(list_in(&bin).is_empty());

        delete_into(&bin, &[a.clone()], 1_000, &mut |_| ClipMeta::default()).unwrap();
        // A stray file nothing in the index owns, written "now".
        let stray = bin.join("orphan.mp4");
        std::fs::write(&stray, b"x").unwrap();
        let now = std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).unwrap().as_millis() as i64;
        assert_eq!(purge_older(&bin, 1_000 + 29 * 86_400_000, 30).unwrap(), 0);
        assert_eq!(purge_older(&bin, 1_000 + 31 * 86_400_000, 30).unwrap(), 1);
        assert!(list_in(&bin).is_empty());
        assert!(stray.exists());
        assert_eq!(purge_older(&bin, now + 31 * 86_400_000, 30).unwrap(), 1);
        assert!(!stray.exists() && bin.join("index.json").exists());
        std::fs::remove_dir_all(&root).ok();
    }
}
