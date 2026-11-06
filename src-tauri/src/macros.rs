//! Custom macros: one hotkey runs a chain of actions, in order, on its own
//! thread. Nothing exists until a macro is configured and its key pressed:
//! no macros means no extra hotkey, no thread, no polling.
//!
//! Steps that need a clip (post to Discord, export, open) work on the one
//! saved most recently, so a chain starting with a save waits for that save to
//! finish before the next step runs.

use std::collections::HashSet;
use std::sync::OnceLock;
use std::time::{Duration, Instant};

use anyhow::{anyhow, bail, Result};
use parking_lot::Mutex;

use crate::engine::{Engine, Level};
use crate::hotkey::Action;
use crate::settings::{Macro, MacroStep};

/// Macros mid-run, so a double press doesn't start the chain twice.
fn running() -> &'static Mutex<HashSet<usize>> {
    static R: OnceLock<Mutex<HashSet<usize>>> = OnceLock::new();
    R.get_or_init(Default::default)
}

/// Run macro number `index` (as listed in settings), if it exists and is on.
pub fn run(engine: &Engine, index: usize) {
    let Some(m) = engine.settings().macros.get(index).cloned() else { return };
    if !m.enabled || m.steps.is_empty() {
        return;
    }
    if !running().lock().insert(index) {
        engine.notify(Level::Warning, &format!("\"{}\" is still running", m.name));
        return;
    }
    let engine = engine.clone();
    let spawned = std::thread::Builder::new().name("rewind-macro".into()).spawn(move || {
        crate::platform::com_init();
        let result = execute(&engine, &m);
        running().lock().remove(&index);
        if let Err(e) = result {
            engine.notify(Level::Error, &format!("Macro \"{}\" stopped: {e:#}", m.name));
        }
    });
    if spawned.is_err() {
        running().lock().remove(&index);
    }
}

fn execute(e: &Engine, m: &Macro) -> Result<()> {
    for step in &m.steps {
        match step {
            MacroStep::SaveClip => save_and_wait(e, |e| e.save_clip(None))?,
            MacroStep::SlowMo => save_and_wait(e, |e| e.save_slow_mo())?,
            MacroStep::ExtendLast => save_and_wait(e, |e| e.extend_last())?,
            MacroStep::SetMarker => e.handle(Action::Marker),
            MacroStep::Screenshot => e.handle(Action::Screenshot),
            MacroStep::ToggleHud => e.handle(Action::ToggleHud),
            MacroStep::PauseBuffer => e.handle(Action::TogglePause),
            MacroStep::OpenLastClip => e.open_clip(&last_clip(e)?),
            MacroStep::RunExport { preset } => {
                let out = crate::export::mf::export(&last_clip(e)?, *preset, None)?;
                e.library_changed();
                e.notify(Level::Success, &format!("Exported {}", out.file_name().unwrap_or_default().to_string_lossy()));
            }
            MacroStep::PostDiscord { webhook } => post_to_discord(e, *webhook as usize)?,
            MacroStep::ShowToast { message } => e.notify(Level::Info, message),
        }
    }
    Ok(())
}

fn last_clip(e: &Engine) -> Result<std::path::PathBuf> {
    let p = e.last_clip_path().ok_or_else(|| anyhow!("no clip has been saved yet"))?;
    if !p.exists() {
        bail!("the last clip was renamed or deleted");
    }
    Ok(p)
}

/// Start a save, then wait for it to finish (success or not).
fn save_and_wait(e: &Engine, start: impl FnOnce(&Engine) -> Result<()>) -> Result<()> {
    let before = e.save_seq();
    start(e)?;
    // Post-roll adds its own seconds to the wait; encoding a long clip a few more.
    let limit = Instant::now() + Duration::from_secs(90 + e.settings().post_roll_secs() as u64);
    while e.save_seq() == before {
        if Instant::now() > limit {
            bail!("the clip took too long to save");
        }
        std::thread::sleep(Duration::from_millis(100));
    }
    if e.last_save_failed() {
        bail!("the clip could not be saved");
    }
    Ok(())
}

/// The last clip to Discord webhook `index`, as a Discord-sized copy if it is
/// over that webhook's limit.
fn post_to_discord(e: &Engine, index: usize) -> Result<()> {
    let w = e.settings().sharing.discord_webhooks.get(index).cloned().ok_or_else(|| anyhow!("that Discord webhook was removed"))?;
    let mut path = last_clip(e)?;
    if std::fs::metadata(&path)?.len() > crate::share::discord::limit_bytes(&w) {
        e.notify(Level::Info, &format!("Exporting a {} MB copy for {}…", w.limit_mb, w.name));
        let preset = if w.limit_mb >= 25 { crate::export::Preset::Discord25 } else { crate::export::Preset::Discord10 };
        path = crate::export::mf::export(&path, preset, None)?;
        e.library_changed();
    }
    crate::share::discord::post(&w, &path)?;
    e.notify(Level::Success, &format!("Posted to {}", w.name));
    Ok(())
}
