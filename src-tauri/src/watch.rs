//! Watch folders: a new .mp4 dropped into an input folder is exported with
//! that folder's preset into its output folder. The last few operations are
//! kept for Settings, and every handled file is remembered so restarting
//! Rewind doesn't export it again.

use std::collections::{HashMap, HashSet};
use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::time::{Duration, Instant, SystemTime, UNIX_EPOCH};

use anyhow::{ensure, Result};
use serde::{Deserialize, Serialize};

use crate::engine::Engine;
use crate::export::Preset;
use crate::settings::{self, WatchFolder};

/// Operations shown in Settings > Watch folders > History.
pub const HISTORY_LEN: usize = 10;
/// Handled files remembered, newest kept.
const SEEN_LEN: usize = 5000;
const POLL: Duration = Duration::from_secs(3);

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct Op {
    /// Unix milliseconds.
    pub at_ms: i64,
    pub input: PathBuf,
    pub output: Option<PathBuf>,
    pub preset: Preset,
    pub error: Option<String>,
    /// The original was deleted after exporting.
    pub deleted: bool,
}

#[derive(Debug, Default, Serialize, Deserialize)]
struct State {
    history: Vec<Op>,
    /// `path|size|mtime` of files already handled.
    seen: Vec<String>,
}

fn state_path() -> PathBuf {
    settings::app_data_dir().join("watch.json")
}

fn load() -> State {
    std::fs::read_to_string(state_path()).ok().and_then(|t| serde_json::from_str(&t).ok()).unwrap_or_default()
}

fn store(s: &State) {
    if let Ok(text) = serde_json::to_string(s) {
        let _ = settings::write_atomic(&state_path(), text.as_bytes());
    }
}

pub fn history() -> Vec<Op> {
    load().history
}

fn key(path: &Path, size: u64, mtime: SystemTime) -> String {
    let ms = mtime.duration_since(UNIX_EPOCH).map(|d| d.as_millis()).unwrap_or(0);
    format!("{}|{size}|{ms}", path.display())
}

/// Why a pair can't run, if it can't.
pub fn check(w: &WatchFolder) -> Result<()> {
    ensure!(!w.input.as_os_str().is_empty() && !w.output.as_os_str().is_empty(), "pick both folders");
    let same = match (w.input.canonicalize(), w.output.canonicalize()) {
        (Ok(a), Ok(b)) => a == b,
        _ => w.input == w.output,
    };
    // Exports would land in the folder being watched and be exported again.
    ensure!(!same, "the output folder must differ from the input folder");
    Ok(())
}

/// Files whose size and modified time held still across two polls: whatever
/// was copying them in has finished.
#[derive(Default)]
pub struct Settle {
    last: HashMap<PathBuf, (u64, SystemTime)>,
}

impl Settle {
    pub fn poll(&mut self, files: Vec<(PathBuf, u64, SystemTime)>) -> Vec<(PathBuf, u64, SystemTime)> {
        let mut ready = Vec::new();
        let mut next = HashMap::new();
        for (p, size, mtime) in files {
            if size > 0 && self.last.get(&p) == Some(&(size, mtime)) {
                ready.push((p.clone(), size, mtime));
            }
            next.insert(p, (size, mtime));
        }
        self.last = next;
        ready
    }
}

fn mp4s(dir: &Path) -> Vec<(PathBuf, u64, SystemTime)> {
    let Ok(rd) = std::fs::read_dir(dir) else { return Vec::new() };
    rd.flatten()
        .filter(|e| e.path().extension().is_some_and(|x| x.eq_ignore_ascii_case("mp4")))
        .filter_map(|e| {
            let m = e.metadata().ok()?;
            m.is_file().then(|| (e.path(), m.len(), m.modified().unwrap_or(UNIX_EPOCH)))
        })
        .collect()
}

pub type Emit = Arc<dyn Fn(&str, serde_json::Value) + Send + Sync>;

/// Poll every watch folder until the app exits.
pub fn start(engine: Engine, emit: Emit) {
    std::thread::Builder::new()
        .name("rewind-watch".into())
        .spawn(move || {
            crate::platform::com_init();
            let mut state = load();
            let mut seen: HashSet<String> = state.seen.iter().cloned().collect();
            let mut settle: HashMap<PathBuf, Settle> = HashMap::new();
            let mut warned: HashSet<PathBuf> = HashSet::new();
            loop {
                let started = Instant::now();
                let pairs = engine.settings().watch_folders;
                settle.retain(|dir, _| pairs.iter().any(|w| &w.input == dir));
                for w in pairs.iter().filter(|w| w.enabled) {
                    if let Err(e) = check(w) {
                        if warned.insert(w.input.clone()) {
                            tracing::warn!("watch folder {}: {e:#}", w.input.display());
                        }
                        continue;
                    }
                    let ready = settle.entry(w.input.clone()).or_default().poll(mp4s(&w.input));
                    for (path, size, mtime) in ready {
                        let k = key(&path, size, mtime);
                        if seen.contains(&k) {
                            continue;
                        }
                        let op = run(w, &path);
                        let ok = op.error.is_none();
                        let msg = match (&op.output, &op.error) {
                            (Some(out), None) => format!("Watch folder: exported \"{}\" to {}", file_name(&path), out.parent().unwrap_or(Path::new("")).display()),
                            (_, Some(e)) => format!("Watch folder: couldn't export \"{}\": {e}", file_name(&path)),
                            _ => String::new(),
                        };
                        seen.insert(k.clone());
                        state.seen.push(k);
                        if state.seen.len() > SEEN_LEN {
                            let drop = state.seen.len() - SEEN_LEN;
                            for old in state.seen.drain(..drop) {
                                seen.remove(&old);
                            }
                        }
                        state.history.insert(0, op);
                        state.history.truncate(HISTORY_LEN);
                        store(&state);
                        emit("notice", serde_json::json!({ "level": if ok { "success" } else { "error" }, "message": msg }));
                        emit("watch-history", serde_json::to_value(&state.history).unwrap_or_default());
                    }
                }
                std::thread::sleep(POLL.saturating_sub(started.elapsed()).max(Duration::from_millis(500)));
            }
        })
        .expect("spawn watch thread");
}

fn file_name(p: &Path) -> String {
    p.file_name().unwrap_or_default().to_string_lossy().into()
}

fn run(w: &WatchFolder, path: &Path) -> Op {
    let at_ms = chrono::Local::now().timestamp_millis();
    let result = (|| -> Result<PathBuf> {
        std::fs::create_dir_all(&w.output)?;
        let out = crate::export::output_path_in(path, w.preset, &w.output);
        export(path, w.preset, &out)?;
        Ok(out)
    })();
    match result {
        Ok(out) => {
            let deleted = w.delete_original && std::fs::remove_file(path).is_ok();
            Op { at_ms, input: path.into(), output: Some(out), preset: w.preset, error: None, deleted }
        }
        Err(e) => Op { at_ms, input: path.into(), output: None, preset: w.preset, error: Some(format!("{e:#}")), deleted: false },
    }
}

#[cfg(windows)]
fn export(src: &Path, preset: Preset, out: &Path) -> Result<()> {
    crate::export::mf::export_to(src, preset, out)
}

#[cfg(not(windows))]
fn export(_src: &Path, _preset: Preset, _out: &Path) -> Result<()> {
    anyhow::bail!("exporting needs Windows")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn settles_after_two_equal_polls() {
        let t = UNIX_EPOCH + Duration::from_secs(100);
        let a = PathBuf::from("a.mp4");
        let mut s = Settle::default();
        assert!(s.poll(vec![(a.clone(), 10, t)]).is_empty());
        // Still growing.
        assert!(s.poll(vec![(a.clone(), 20, t)]).is_empty());
        assert_eq!(s.poll(vec![(a.clone(), 20, t)]).len(), 1);
        // Empty files are never ready.
        let b = PathBuf::from("b.mp4");
        s.poll(vec![(b.clone(), 0, t)]);
        assert!(s.poll(vec![(b, 0, t)]).is_empty());
    }

    /// Real export through a watch pair. Set REWIND_EXPORT_CLIP and run
    /// `cargo test -- --ignored watch_exports_real_clip`.
    #[cfg(windows)]
    #[test]
    #[ignore]
    fn watch_exports_real_clip() {
        let Some(clip) = std::env::var_os("REWIND_EXPORT_CLIP") else { return };
        crate::platform::com_init();
        crate::encode::mf::mf_startup();
        let root = std::env::temp_dir().join(format!("rewind-watch-{}", std::process::id()));
        let (input, output) = (root.join("in"), root.join("out"));
        std::fs::create_dir_all(&input).unwrap();
        let src = input.join("clip.mp4");
        std::fs::copy(&clip, &src).unwrap();
        let w = WatchFolder { enabled: true, input, output: output.clone(), preset: Preset::Discord10, delete_original: true };
        let op = run(&w, &src);
        assert_eq!(op.error, None);
        let out = op.output.unwrap();
        assert!(out.starts_with(&output) && std::fs::metadata(&out).unwrap().len() > 1000);
        assert!(op.deleted && !src.exists());
        std::fs::remove_dir_all(&root).ok();
    }

    #[test]
    fn same_folder_refused() {
        let w = WatchFolder { enabled: true, input: "C:\\x".into(), output: "C:\\x".into(), preset: Preset::Shorts, delete_original: false };
        assert!(check(&w).is_err());
        let w = WatchFolder { output: "C:\\y".into(), ..w };
        assert!(check(&w).is_ok());
    }
}
