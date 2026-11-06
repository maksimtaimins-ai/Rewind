//! The engine owns the replay buffer and pipeline lifecycle, reacts to
//! hotkeys, and writes clips on a background thread so capture never blocks.

use std::collections::{BTreeMap, VecDeque};
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicBool, AtomicU32, AtomicU64, Ordering};
use std::sync::Arc;
use std::time::{Duration, Instant};

use anyhow::{anyhow, bail, Result};
use crossbeam_channel::{unbounded, Receiver, Sender};
use parking_lot::{Mutex, RwLock};
use serde::{Deserialize, Serialize};

use crate::buffer::{ReplayBuffer, TICKS_PER_SEC};
use crate::capture;
use crate::encode::{self, BenchResult, EncoderInfo, EncoderParams};
use crate::hotkey::{Action, HotkeyThread};
use crate::library::{self, ClipInfo};
use crate::mux;
use crate::pipeline::{FrameGrab, Flags, Pipeline, PipelineConfig};
use crate::platform::{self, qpc_now, ImageFormat};
use crate::profiles::{self, Detector, Game};
use crate::settings::{self, Settings};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum State {
    /// Pipeline not running yet (starting up, or restarting after a settings change).
    Idle,
    /// Replay buffer is filling.
    Recording,
    /// Buffer running, but frames are being dropped while a privacy app has focus.
    Paused,
    Error,
}

#[derive(Debug, Clone, Serialize)]
pub struct Status {
    pub state: State,
    pub buffered_secs: f64,
    pub buffer_secs: u32,
    /// Every enabled replay length, main one first.
    pub slots: Vec<SlotStatus>,
    pub fps: f64,
    pub bitrate_kbps: f64,
    pub dropped: u64,
    pub memory_mb: f64,
    pub encoder: Option<String>,
    pub resolution: Option<(u32, u32)>,
    pub game: Option<String>,
    /// Name of the game profile in effect, if any.
    pub profile: Option<String>,
    pub privacy: bool,
    pub markers: Vec<MarkerInfo>,
    /// A save waiting for its post-roll to finish, if any.
    pub post_roll: Option<PostRollStatus>,
    pub error: Option<String>,
    pub written_today_gb: f64,
    /// Since Monday.
    pub written_week_gb: f64,
    /// Free space on the clips drive; `None` until first measured.
    pub disk_free_gb: Option<f64>,
    pub disk: DiskLevel,
    /// Captured audio loudness (RMS 0-1) in 100 ms steps since the last status, oldest first.
    pub levels: Vec<f32>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum DiskLevel {
    Ok,
    /// Under the warning limit.
    Low,
    /// Under the save limit: new clip saves are refused.
    Full,
}

pub fn disk_level(free: Option<u64>, s: &settings::DiskSettings) -> DiskLevel {
    match free {
        Some(b) if b < s.block_gb as u64 * GB => DiskLevel::Full,
        Some(b) if b < s.warn_gb as u64 * GB => DiskLevel::Low,
        _ => DiskLevel::Ok,
    }
}

/// Decimal, like Explorer's drive sizes are often read.
const GB: u64 = 1_000_000_000;

#[derive(Debug, Clone, Serialize)]
pub struct PostRollStatus {
    /// Seconds until the current save is written.
    pub remaining: f64,
    /// Post-roll length of the current save.
    pub seconds: u32,
    /// Further presses waiting behind it.
    pub queued: u32,
}

#[derive(Debug, Clone, Serialize)]
pub struct SlotStatus {
    /// "A", "B" or "C".
    pub name: String,
    pub seconds: u32,
    /// Seconds a save from this slot would hold right now.
    pub held: f64,
    pub hotkey: String,
}

#[derive(Debug, Clone, Serialize)]
pub struct MarkerInfo {
    pub id: u64,
    /// Seconds since the marker was set.
    pub ago: f64,
    /// Wall clock, Unix milliseconds.
    pub at_ms: i64,
    /// The buffer still holds footage at the marker.
    pub in_buffer: bool,
}

#[derive(Clone, Copy)]
struct Marker {
    id: u64,
    at: i64,
    at_ms: i64,
}

/// Where a marker clip of `len` ticks lands: centred on `at`, then shifted
/// into what the buffer holds (`[start, end]`). `None` when the marker itself
/// is older than the buffer. The flag says the window had to move.
fn marker_window(at: i64, len: i64, start: i64, end: i64) -> Option<(i64, i64, bool)> {
    if at < start || start >= end {
        return None;
    }
    let mut from = at - len / 2;
    let mut to = from + len;
    let mut shifted = false;
    if to > end {
        to = end;
        from = end - len;
        shifted = true;
    }
    if from < start {
        from = start;
        to = (start + len).min(end);
        shifted = true;
    }
    Some((from, to, shifted))
}

#[derive(Debug, Clone, Copy, Serialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum Level {
    Info,
    Success,
    Warning,
    Error,
}

pub trait Events: Send + Sync {
    fn status(&self, status: &Status);
    fn clip_saved(&self, clip: &ClipInfo);
    fn notice(&self, level: Level, message: &str);
    /// Clip metadata changed outside the UI (e.g. a hotkey); reload the library.
    fn library_changed(&self);
    /// The compact HUD hotkey was pressed.
    fn toggle_hud(&self) {}
    /// A game profile switched in or out; `effective` is what's in force now.
    fn profile_changed(&self, _effective: &Settings) {}
}

struct SaveJob {
    from: i64,
    to: i64,
    path: PathBuf,
    thumb: Option<Receiver<FrameGrab>>,
    replaces: bool,
    /// Write as slow motion at this speed.
    slow_mo: Option<f64>,
}

#[derive(Clone)]
struct LastSave {
    from: i64,
    to: i64,
    path: PathBuf,
}

struct Inner {
    /// What the user set.
    settings: RwLock<Settings>,
    /// `settings` with the running game's profile applied: what the engine
    /// uses. The same as `settings` while profiles are off.
    effective: RwLock<Settings>,
    /// The profile in `effective`.
    profile: Mutex<Option<settings::GameProfile>>,
    buffer: Arc<Mutex<ReplayBuffer>>,
    flags: Arc<Flags>,
    pipeline: Mutex<Option<Pipeline>>,
    encoders: Mutex<Vec<EncoderInfo>>,
    events: Box<dyn Events>,
    restart: AtomicBool,
    game: Mutex<Option<Game>>,
    /// Markers set this session. Kept until the app quits, even once their
    /// footage has left the buffer.
    markers: Mutex<Vec<Marker>>,
    next_marker: AtomicU64,
    last_save: Mutex<Option<LastSave>>,
    /// Most recently written clip of any kind, for "favorite last clip".
    last_clip: Mutex<Option<PathBuf>>,
    save_tx: Sender<SaveJob>,
    /// Saves still recording their post-roll, handed to the saver in order.
    post_tx: Sender<SaveJob>,
    /// (end, post-roll seconds) of each save in `post_tx`, oldest first.
    post_pending: Mutex<VecDeque<(i64, u32)>>,
    error: Mutex<Option<String>>,
    hotkeys: Mutex<Option<HotkeyThread>>,
    usage: Mutex<Usage>,
    /// Bytes free on the clips drive; `u64::MAX` until measured.
    disk_free: AtomicU64,
    shutdown: AtomicBool,
    /// Buffer length in effect (a game profile may override the setting).
    buffer_secs: AtomicU32,
    /// Keystroke overlay recording; holds no hook or thread while it's off.
    input: Mutex<crate::input::Recorder>,
    /// The PC runs on battery (only polled while battery throttling is on).
    on_battery: AtomicBool,
}

#[derive(Clone)]
pub struct Engine {
    inner: Arc<Inner>,
}

const MB: usize = 1024 * 1024;

impl Engine {
    pub fn start(events: Box<dyn Events>) -> Self {
        platform::com_init();
        encode::mf::mf_startup();
        let mut settings = settings::load();
        if settings::ensure_device_id(&mut settings) {
            let _ = settings::save(&settings);
        }
        tracing::info!("settings: buffer {}s, first run done {}, clips {}", settings.buffer.seconds, settings.first_run_complete, settings.clips_dir.display());
        let (save_tx, save_rx) = unbounded();
        let (post_tx, post_rx) = unbounded::<SaveJob>();
        let flags = Arc::new(Flags::default());
        let inner = Arc::new(Inner {
            buffer: Arc::new(Mutex::new(ReplayBuffer::new(1, MB))),
            input: Mutex::new(crate::input::Recorder::new(flags.clone())),
            flags,
            pipeline: Mutex::new(None),
            encoders: Mutex::new(Vec::new()),
            events,
            restart: AtomicBool::new(false),
            game: Mutex::new(None),
            markers: Mutex::new(Vec::new()),
            next_marker: AtomicU64::new(1),
            last_save: Mutex::new(None),
            last_clip: Mutex::new(None),
            save_tx,
            post_tx,
            post_pending: Mutex::new(VecDeque::new()),
            error: Mutex::new(None),
            hotkeys: Mutex::new(None),
            usage: Mutex::new(Usage::load()),
            disk_free: AtomicU64::new(u64::MAX),
            shutdown: AtomicBool::new(false),
            buffer_secs: AtomicU32::new(settings.buffer.seconds),
            on_battery: AtomicBool::new(false),
            effective: RwLock::new(settings.clone()),
            profile: Mutex::new(None),
            settings: RwLock::new(settings),
        });
        let engine = Engine { inner };
        if engine.inner.settings.read().performance.battery_throttle {
            engine.inner.on_battery.store(crate::perf::on_battery().unwrap_or(false), Ordering::Relaxed);
        }
        {
            let s = engine.inner.settings.read().clone();
            *engine.inner.effective.write() = engine.compute_effective(&s, None);
        }
        engine.apply_input();

        let (action_tx, action_rx) = unbounded();
        let (hk_err_tx, hk_err_rx) = unbounded::<String>();
        let hk = HotkeyThread::spawn(action_tx, hk_err_tx);
        for e in hk.bind(&engine.inner.settings.read()) {
            engine.inner.events.notice(Level::Warning, &format!("Hotkey not set: {e}"));
        }
        *engine.inner.hotkeys.lock() = Some(hk);

        let e = engine.clone();
        std::thread::Builder::new().name("rewind-actions".into()).spawn(move || {
            platform::com_init();
            loop {
                crossbeam_channel::select! {
                    recv(action_rx) -> a => match a {
                        Ok(a) => e.handle(a),
                        Err(_) => break,
                    },
                    recv(hk_err_rx) -> m => if let Ok(m) = m {
                        e.inner.events.notice(Level::Warning, &format!("{m} is already used by another app. Pick a different key in Settings > Hotkeys."));
                    },
                }
            }
        }).expect("spawn action thread");

        let e = engine.clone();
        std::thread::Builder::new().name("rewind-saver".into()).spawn(move || {
            platform::com_init();
            for job in save_rx {
                e.run_save(job);
            }
        }).expect("spawn save thread");

        // Post-roll saves wait here, one after another, so the saver stays free
        // for markers, slow-mo and extend while they record.
        let e = engine.clone();
        std::thread::Builder::new().name("rewind-postroll".into()).spawn(move || {
            for job in post_rx {
                while qpc_now() < job.to && !e.inner.shutdown.load(Ordering::Relaxed) {
                    std::thread::sleep(Duration::from_millis(50));
                }
                e.inner.post_pending.lock().pop_front();
                let _ = e.inner.save_tx.send(job);
            }
        }).expect("spawn post-roll thread");

        let e = engine.clone();
        std::thread::Builder::new().name("rewind-control".into()).spawn(move || {
            platform::com_init();
            e.control_loop();
        }).expect("spawn control thread");

        engine
    }

    pub fn settings(&self) -> Settings {
        self.inner.settings.read().clone()
    }

    /// The settings in force: the user's, with a running game's profile applied.
    pub fn effective_settings(&self) -> Settings {
        self.inner.effective.read().clone()
    }

    /// Persist new settings; restarts the pipeline if capture settings changed.
    pub fn set_settings(&self, new: Settings) -> Result<()> {
        settings::save(&new)?;
        let old = std::mem::replace(&mut *self.inner.settings.write(), new.clone());
        let exe = self.inner.game.lock().as_ref().map(|g| g.exe.clone());
        let profile = profiles::active(&new, exe.as_deref()).cloned();
        *self.inner.profile.lock() = profile.clone();
        self.inner.on_battery.store(
            new.performance.battery_throttle && crate::perf::on_battery().unwrap_or(false),
            Ordering::Relaxed,
        );
        self.switch_effective(self.compute_effective(&new, profile.as_ref()));
        if old.launch_on_startup != new.launch_on_startup {
            platform::set_autostart(new.launch_on_startup)?;
        }
        Ok(())
    }

    /// `s` with a game profile and the performance settings applied.
    fn compute_effective(&self, s: &Settings, profile: Option<&settings::GameProfile>) -> Settings {
        let mut e = profiles::effective(s, profile);
        let hz = (e.performance.fps_mode == settings::FpsMode::Display)
            .then(|| capture::capture_monitor(&e).and_then(|m| capture::refresh_hz(&m.name)))
            .flatten();
        crate::perf::apply(&mut e, self.inner.on_battery.load(Ordering::Relaxed), hz);
        e
    }

    /// Re-derive the effective settings after the power source changed.
    fn refresh_effective(&self) {
        let s = self.inner.settings.read().clone();
        let profile = self.inner.profile.lock().clone();
        self.switch_effective(self.compute_effective(&s, profile.as_ref()));
    }

    /// Put new effective settings in force, doing only what changed requires:
    /// rebind hotkeys, restart capture, resize the buffer, (un)hook input.
    fn switch_effective(&self, new: Settings) {
        let old = std::mem::replace(&mut *self.inner.effective.write(), new.clone());
        if old.hotkeys != new.hotkeys || old.buffer.extra_slots != new.buffer.extra_slots {
            if let Some(hk) = self.inner.hotkeys.lock().as_ref() {
                for e in hk.bind(&new) {
                    self.inner.events.notice(Level::Warning, &format!("Hotkey not set: {e}"));
                }
            }
        }
        if old.video != new.video || old.buffer != new.buffer || old.audio != new.audio {
            self.inner.restart.store(true, Ordering::Relaxed);
        }
        if old.post_roll != new.post_roll {
            // Resize in place: toggling post-roll shouldn't empty the buffer.
            let keep = keep_secs(&new, self.inner.buffer_secs.load(Ordering::Relaxed));
            self.inner.buffer.lock().set_max_secs(keep);
        }
        if old.keystrokes != new.keystrokes || old.buffer != new.buffer || old.post_roll != new.post_roll {
            self.apply_input();
        }
    }

    /// Start or stop keystroke recording to match the settings. Off means
    /// no hook and no thread.
    fn apply_input(&self) {
        let s = self.inner.effective.read().clone();
        let keep = keep_secs(&s, self.inner.buffer_secs.load(Ordering::Relaxed));
        self.inner.input.lock().apply(&s.keystrokes, keep);
    }

    /// Whether the keystroke overlay's input hook is installed right now.
    pub fn input_recording(&self) -> bool {
        self.inner.input.lock().running()
    }

    pub fn encoders(&self) -> Vec<EncoderInfo> {
        let mut list = self.inner.encoders.lock();
        if list.is_empty() {
            *list = encode::enumerate_all();
        }
        list.clone()
    }

    pub fn benchmark(&self, secs: f64) -> Vec<BenchResult> {
        let monitors = capture::cached();
        let m = monitors.first();
        let (w, h) = crate::gpu::output_size(
            m.map(|m| (m.width, m.height)).unwrap_or((1920, 1080)),
            self.inner.settings.read().video.height,
        );
        let list = self.encoders();
        let per = secs / list.len().max(1) as f64;
        encode::benchmark(&list, w, h, per.max(0.4))
    }

    /// Watch the running pipeline for `secs` and judge the current settings.
    pub fn live_benchmark(&self, secs: f64) -> Result<crate::bench::Report> {
        use crate::bench::{self, sys};
        let (stats, encoder, (width, height), target_fps) = {
            let p = self.inner.pipeline.lock();
            let p = p.as_ref().ok_or_else(|| anyhow!("Capture is still starting. Try again in a few seconds."))?;
            (p.stats.clone(), p.encoder_name.clone(), p.output, p.fps)
        };
        let snap = || {
            let s = &stats;
            [&s.frames_in, &s.frames_out, &s.dropped, &s.bytes_out, &s.busy_ns].map(|v| v.load(Ordering::Relaxed))
        };
        let a = snap();
        let cpu_a = sys::cpu();
        let mut gpu = sys::Gpu::open();
        let start = Instant::now();
        while start.elapsed().as_secs_f64() < secs {
            std::thread::sleep(Duration::from_millis(1000));
            if let Some(g) = gpu.as_mut() {
                g.sample();
            }
            let same = self.inner.pipeline.lock().as_ref().is_some_and(|p| Arc::ptr_eq(&p.stats, &stats));
            if !same {
                return Err(anyhow!("Capture restarted during the test (settings or game changed). Run it again."));
            }
            if self.inner.flags.blocked() {
                return Err(anyhow!("A private app was focused, so capture paused. Run it again."));
            }
        }
        let b = snap();
        let wall = start.elapsed().as_secs_f64();
        let (cpu_pct, rewind_cpu_pct) = sys::cpu_pct(cpu_a, sys::cpu());
        let d = |i: usize| b[i].saturating_sub(a[i]);
        let busy = d(4) as f64 / 1e9;
        let (gpu_pct, gpu_encode_pct) = gpu.as_ref().map(|g| g.average()).unwrap_or((None, None));
        let mut r = bench::Report {
            secs: wall,
            encoder,
            width,
            height,
            target_fps,
            capture_fps: d(0) as f64 / wall,
            encoded_fps: d(1) as f64 / wall,
            encoder_max_fps: if busy > 0.0 { d(1) as f64 / busy } else { 0.0 },
            encoder_load: (busy / wall).min(1.0),
            dropped: d(2),
            bitrate_kbps: d(3) as f64 * 8.0 / 1000.0 / wall,
            cpu_pct,
            rewind_cpu_pct,
            gpu_pct,
            gpu_encode_pct,
            ram_mb: sys::ram_mb(),
            buffer_mb: self.inner.buffer.lock().total_bytes() as f64 / MB as f64,
            verdict: Default::default(),
        };
        r.verdict = bench::verdict(&r);
        Ok(r)
    }

    pub fn shutdown(&self) {
        self.inner.shutdown.store(true, Ordering::Relaxed);
        if let Some(mut p) = self.inner.pipeline.lock().take() {
            p.stop();
        }
        self.inner.usage.lock().save();
    }

    pub fn handle(&self, action: Action) {
        tracing::info!("action {action:?}");
        let r = match action {
            Action::SaveClip => self.save_clip(None),
            Action::SaveSlot(i) => {
                let slot = self.inner.effective.read().buffer.extra_slots.get(i as usize).cloned();
                match slot {
                    Some(slot) if slot.enabled => self.save_clip(Some(slot.seconds)),
                    _ => Ok(()),
                }
            }
            Action::Marker => self.add_marker(),
            Action::Screenshot => self.screenshot(),
            Action::ExtendLast => self.extend_last(),
            Action::SlowMo => self.save_slow_mo(),
            Action::FavoriteLast => self.favorite_last(),
            Action::ToggleHud => {
                self.inner.events.toggle_hud();
                Ok(())
            }
        };
        if let Err(e) = r {
            self.inner.events.notice(Level::Error, &format!("{e:#}"));
        }
    }

    fn clip_stem(&self) -> String {
        let game = self.inner.game.lock().as_ref().map(|g| g.name.clone()).unwrap_or_else(|| "Desktop".into());
        format!("{game} {}", chrono::Local::now().format("%Y-%m-%d %H-%M-%S"))
    }

    fn clips_dir(&self) -> Result<PathBuf> {
        let dir = self.inner.effective.read().clips_dir.clone();
        std::fs::create_dir_all(&dir)?;
        std::fs::create_dir_all(library::thumbs_dir(&dir))?;
        Ok(dir)
    }

    /// Clip length for a request: the main buffer length by default, never
    /// more than the longest enabled slot.
    fn clip_ticks(&self, secs: Option<u32>) -> i64 {
        let main = self.inner.buffer_secs.load(Ordering::Relaxed);
        let max = self.inner.effective.read().longest_slot_secs().max(main);
        secs.unwrap_or(main).clamp(1, max) as i64 * TICKS_PER_SEC
    }

    /// Save the last `secs` (default: the main buffer length), ending at the
    /// keypress, or post-roll seconds after it when that's on.
    pub fn save_clip(&self, secs: Option<u32>) -> Result<()> {
        self.check_disk()?;
        let pipeline = self.inner.pipeline.lock();
        let Some(p) = pipeline.as_ref() else {
            return Err(anyhow!("Capture is still starting"));
        };
        let now = qpc_now();
        let post = self.inner.settings.read().post_roll_secs();
        let job = SaveJob {
            from: now - self.clip_ticks(secs),
            to: now + post as i64 * TICKS_PER_SEC,
            path: library::unique_path(&self.clips_dir()?, &self.clip_stem(), "mp4"),
            thumb: Some(p.request_frame()),
            replaces: false,
            slow_mo: None,
        };
        drop(pipeline);
        if post == 0 {
            self.inner.save_tx.send(job)?;
        } else {
            self.inner.post_pending.lock().push_back((job.to, post));
            self.inner.post_tx.send(job)?;
        }
        Ok(())
    }

    /// Save `secs` (at most the buffer length) around a marker, or as close to
    /// it as the buffer still holds.
    pub fn save_marker(&self, id: u64, secs: u32) -> Result<()> {
        self.check_disk()?;
        let m = self.inner.markers.lock().iter().find(|m| m.id == id).copied().ok_or_else(|| anyhow!("Unknown marker"))?;
        let held = {
            let b = self.inner.buffer.lock();
            b.earliest_pts().zip(b.latest_pts())
        };
        let (start, end) = held.ok_or_else(|| anyhow!("Nothing in the buffer to save yet"))?;
        let (from, to, shifted) = marker_window(m.at, self.clip_ticks(Some(secs)), start, end)
            .ok_or_else(|| anyhow!("That marker is older than the buffer, so its footage is gone"))?;
        if shifted {
            self.inner.events.notice(Level::Warning, "The buffer doesn't reach far enough on one side, so the clip is shifted to what it still holds");
        }
        let job = SaveJob {
            from,
            to,
            path: library::unique_path(&self.clips_dir()?, &format!("{} marker", self.clip_stem()), "mp4"),
            thumb: None,
            replaces: false,
            slow_mo: None,
        };
        self.inner.save_tx.send(job)?;
        Ok(())
    }

    fn add_marker(&self) -> Result<()> {
        if self.inner.pipeline.lock().is_none() {
            return Err(anyhow!("Capture is still starting"));
        }
        let id = self.inner.next_marker.fetch_add(1, Ordering::Relaxed);
        let at_ms = chrono::Local::now().timestamp_millis();
        self.inner.markers.lock().push(Marker { id, at: qpc_now(), at_ms });
        self.inner.events.notice(Level::Info, "Marker set");
        Ok(())
    }

    fn favorite_last(&self) -> Result<()> {
        let path = self.inner.last_clip.lock().clone().ok_or_else(|| anyhow!("No clip saved yet"))?;
        if !path.exists() {
            return Err(anyhow!("The last clip was renamed or deleted"));
        }
        library::set_favorite(&path, true)?;
        self.inner.events.library_changed();
        self.inner.events.notice(Level::Success, "Added the last clip to favorites");
        Ok(())
    }

    /// Save the last few seconds as a slowed-down clip.
    fn save_slow_mo(&self) -> Result<()> {
        self.check_disk()?;
        let sm = self.inner.settings.read().slow_mo.clone();
        let pipeline = self.inner.pipeline.lock();
        let p = pipeline.as_ref().ok_or_else(|| anyhow!("Capture is still starting"))?;
        let now = qpc_now();
        let job = SaveJob {
            from: now - self.clip_ticks(Some(sm.seconds)),
            to: now,
            path: library::unique_path(&self.clips_dir()?, &format!("{} slowmo", self.clip_stem()), "mp4"),
            thumb: Some(p.request_frame()),
            replaces: false,
            slow_mo: Some(sm.speed as f64),
        };
        drop(pipeline);
        self.inner.save_tx.send(job)?;
        Ok(())
    }

    /// Re-save the last clip in place with up to `extend_secs` more footage
    /// before its start, taken from the buffer.
    fn extend_last(&self) -> Result<()> {
        self.check_disk()?;
        let last = self.inner.last_save.lock().clone().ok_or_else(|| anyhow!("No clip to extend yet"))?;
        if !last.path.exists() {
            return Err(anyhow!("The last clip was renamed or deleted"));
        }
        let extra = self.inner.settings.read().buffer.extend_secs as i64 * TICKS_PER_SEC;
        let start = {
            let b = self.inner.buffer.lock();
            b.earliest_pts().and_then(|oldest| b.keyframe_at_or_after((last.from - extra).max(oldest)))
        };
        let Some(from) = start.filter(|&s| s < last.from) else {
            self.inner.events.notice(Level::Warning, "Buffer no longer contains that time range");
            return Ok(());
        };
        self.inner.save_tx.send(SaveJob { from, to: last.to, path: last.path, thumb: None, replaces: true, slow_mo: None })?;
        Ok(())
    }

    /// Save the current frame as `Screenshot <date> <time>.png` in the clips folder.
    pub fn screenshot(&self) -> Result<()> {
        if self.inner.flags.blocked() {
            return Err(anyhow!("Screenshots are paused while a private app is focused"));
        }
        let rx = {
            let p = self.inner.pipeline.lock();
            p.as_ref().ok_or_else(|| anyhow!("Capture is still starting"))?.request_frame()
        };
        let stem = format!("Screenshot {}", chrono::Local::now().format("%Y-%m-%d %H-%M-%S"));
        let path = library::unique_path(&self.clips_dir()?, &stem, "png");
        let inner = self.inner.clone();
        std::thread::spawn(move || {
            platform::com_init();
            // A fresh frame if one arrives quickly; otherwise the screen is
            // static and the newest recorded frame is what's on it.
            let frame = rx
                .recv_timeout(Duration::from_millis(250))
                .ok()
                .or_else(|| inner.pipeline.lock().as_ref().and_then(|p| p.latest_frame()));
            let r = frame
                .ok_or_else(|| anyhow!("No frame captured yet"))
                .and_then(|(w, h, px)| platform::write_image(&path, w, h, &px, ImageFormat::Png));
            match r {
                Ok(()) => inner.events.notice(Level::Success, "Screenshot saved"),
                Err(e) => inner.events.notice(Level::Error, &format!("Screenshot failed: {e:#}")),
            }
        });
        Ok(())
    }

    fn run_save(&self, job: SaveJob) {
        // Wait for the frames up to the keypress to land in the buffer. A static
        // screen produces no new frames, so give up shortly after either way.
        let grace = TICKS_PER_SEC / 4;
        loop {
            let reached = self.inner.buffer.lock().latest_pts().is_some_and(|p| p >= job.to);
            if reached || qpc_now() >= job.to + grace || self.inner.pipeline.lock().is_none() {
                break;
            }
            std::thread::sleep(Duration::from_millis(30));
        }
        // Slow-mo starts at the keyframe nearest the asked-for length; normal saves
        // never run longer than asked.
        let snapshot = {
            let b = self.inner.buffer.lock();
            if job.slow_mo.is_some() { b.snapshot_nearest(job.from, job.to) } else { b.snapshot_within(job.from, job.to) }
        };
        let Some(snap) = snapshot else {
            self.inner.events.notice(Level::Error, "Nothing in the buffer to save yet");
            return;
        };
        let (start, end) = (snap.start(), snap.end());
        let result = (|| -> Result<ClipInfo> {
            let bytes = if job.replaces {
                // Write beside the clip, then swap, so a failure leaves the original.
                let tmp = job.path.with_extension("mp4.tmp");
                let bytes = mux::write_snapshot(&tmp, &snap)?;
                std::fs::rename(&tmp, &job.path).map_err(|e| {
                    let _ = std::fs::remove_file(&tmp);
                    anyhow!("couldn't replace the clip (is it open in a player?): {e}")
                })?;
                bytes
            } else if let Some(speed) = job.slow_mo {
                mux::fmp4::write_file(&job.path, &mux::slow_motion(mux::snapshot_to_tracks(&snap), speed)?)?
            } else {
                mux::write_snapshot(&job.path, &snap)?
            };
            self.add_written(bytes);
            self.write_input(&job.path, start, end, job.slow_mo.unwrap_or(1.0));
            if let Some(rx) = job.thumb {
                if let Ok((w, h, px)) = rx.recv_timeout(Duration::from_millis(500)) {
                    let (tw, th, small) = platform::downscale_bgra(w, h, &px, 640);
                    let _ = platform::write_image(&library::thumb_path(&job.path), tw, th, &small, ImageFormat::Jpeg);
                }
            }
            library::info(&job.path)
        })();
        match result {
            Ok(info) => {
                *self.inner.last_clip.lock() = Some(job.path.clone());
                // A slow-mo clip isn't a stretch of the buffer, so it can't be extended.
                if job.slow_mo.is_none() {
                    *self.inner.last_save.lock() = Some(LastSave { from: start, to: end, path: job.path.clone() });
                }
                self.inner.events.clip_saved(&info);
                let inner = self.inner.clone();
                crate::analysis::on_clip_saved(&self.inner.settings.read(), &job.path, move || inner.events.library_changed());
                let verb = if job.replaces { "Extended" } else if job.slow_mo.is_some() { "Saved slow-mo" } else { "Saved" };
                self.inner.events.notice(Level::Success, &format!("{verb} {:.0}s clip", info.duration));
            }
            Err(e) => self.inner.events.notice(Level::Error, &format!("Saving clip failed: {e:#}")),
        }
    }

    /// Save the keystrokes recorded during a clip next to it, if recording is on.
    fn write_input(&self, clip: &Path, from: i64, to: i64, speed: f64) {
        let Some(mut events) = self.inner.input.lock().between(from, to, speed) else { return };
        let Some((sx, sy, w, h)) = self.inner.pipeline.lock().as_ref().map(|p| p.screen) else { return };
        if events.is_empty() {
            let _ = std::fs::remove_file(crate::input::sidecar_path(clip));
            return;
        }
        for e in &mut events {
            if let (Some(x), Some(y)) = (e.x, e.y) {
                e.x = Some(x - sx);
                e.y = Some(y - sy);
            }
        }
        if let Err(e) = crate::input::write_sidecar(clip, crate::input::Screen { width: w, height: h }, events) {
            tracing::warn!("save keystrokes: {e:#}");
        }
    }

    /// Measure free space on the clips drive now.
    fn refresh_disk(&self) -> Option<u64> {
        let free = platform::disk_free(&self.inner.settings.read().clips_dir);
        self.inner.disk_free.store(free.unwrap_or(u64::MAX), Ordering::Relaxed);
        free
    }

    /// Refuse a clip save when the clips drive is under the save limit.
    /// Screenshots don't check: they're small and may be what's needed.
    fn check_disk(&self) -> Result<()> {
        let free = self.refresh_disk();
        let limits = self.inner.settings.read().disk.clone();
        if disk_level(free, &limits) == DiskLevel::Full {
            return Err(anyhow!(
                "Only {:.1} GB free on the clips drive. Saving clips is paused below {} GB: free up space, or lower the limit in Settings > Advanced.",
                free.unwrap_or(0) as f64 / GB as f64,
                limits.block_gb
            ));
        }
        Ok(())
    }

    fn add_written(&self, bytes: u64) {
        let total = {
            let mut u = self.inner.usage.lock();
            let t = u.add(today(), bytes);
            u.save();
            t
        };
        let limit = self.inner.settings.read().ssd_warn_gb_per_day as u64 * 1024 * MB as u64;
        if limit > 0 && total >= limit && total - bytes < limit {
            self.inner.events.notice(
                Level::Warning,
                &format!("Rewind has written {:.0} GB today. Consider a shorter buffer or lower bitrate to spare your SSD.", total as f64 / 1e9),
            );
        }
    }

    pub fn status(&self) -> Status {
        self.build_status(0.0, 0.0)
    }

    fn build_status(&self, fps: f64, kbps: f64) -> Status {
        let s = self.inner.effective.read().clone();
        let pipeline = self.inner.pipeline.lock();
        let buf = self.inner.buffer.lock();
        let buffered = if pipeline.is_some() { buf.buffered_ticks() as f64 / TICKS_PER_SEC as f64 } else { 0.0 };
        let main = self.inner.buffer_secs.load(Ordering::Relaxed);
        let slot = |name: &str, seconds: u32, hotkey: &str| SlotStatus {
            name: name.into(),
            seconds,
            held: buffered.min(seconds as f64),
            hotkey: hotkey.into(),
        };
        let mut slots = vec![slot("A", main, &s.hotkeys.save_clip)];
        for (i, x) in s.buffer.extra_slots.iter().enumerate().filter(|(_, x)| x.enabled) {
            slots.push(slot(["B", "C"][i.min(1)], x.seconds, &x.hotkey));
        }
        let paused = self.inner.flags.blocked();
        let start = buf.earliest_pts();
        let now = qpc_now();
        let markers = self
            .inner
            .markers
            .lock()
            .iter()
            .map(|m| MarkerInfo {
                id: m.id,
                ago: (now - m.at) as f64 / TICKS_PER_SEC as f64,
                at_ms: m.at_ms,
                in_buffer: pipeline.is_some() && start.is_some_and(|s| m.at >= s),
            })
            .collect();
        // Read once: a guard in the tail expression would live until the end of
        // the struct literal and deadlock a second lock below.
        let error = self.inner.error.lock().clone();
        let game = self.inner.game.lock().as_ref().map(|g| g.name.clone());
        let profile = self.inner.profile.lock().as_ref().map(|p| p.name.clone());
        let disk_free = Some(self.inner.disk_free.load(Ordering::Relaxed)).filter(|&b| b != u64::MAX);
        let post_roll = {
            let pending = self.inner.post_pending.lock();
            pending.front().map(|&(to, seconds)| PostRollStatus {
                remaining: ((to - now) as f64 / TICKS_PER_SEC as f64).max(0.0),
                seconds,
                queued: pending.len().saturating_sub(1) as u32,
            })
        };
        let (written_today, written_week) = {
            let u = self.inner.usage.lock();
            (u.day(today()), u.week(today()))
        };
        Status {
            state: match (&error, pipeline.is_some(), paused) {
                (Some(_), false, _) => State::Error,
                (_, true, true) => State::Paused,
                (_, true, false) => State::Recording,
                _ => State::Idle,
            },
            buffered_secs: buf.buffered_ticks() as f64 / TICKS_PER_SEC as f64,
            buffer_secs: main,
            slots,
            fps,
            bitrate_kbps: kbps,
            dropped: pipeline.as_ref().map(|p| p.stats.dropped.load(Ordering::Relaxed)).unwrap_or(0),
            memory_mb: buf.total_bytes() as f64 / MB as f64,
            encoder: pipeline.as_ref().map(|p| p.encoder_name.clone()),
            resolution: pipeline.as_ref().map(|p| p.output),
            game,
            profile,
            privacy: self.inner.flags.privacy.load(Ordering::Relaxed),
            markers,
            post_roll,
            error,
            written_today_gb: written_today as f64 / 1e9,
            written_week_gb: written_week as f64 / 1e9,
            disk_free_gb: disk_free.map(|b| b as f64 / GB as f64),
            disk: disk_level(disk_free, &s.disk),
            levels: crate::audio::meter::take(),
        }
    }

    /// `s` is the effective settings: a game profile is already applied.
    fn start_pipeline(&self, s: &Settings) -> Result<Pipeline> {
        let v = &s.video;
        let monitors = capture::cached();
        let monitor = capture::capture_monitor(s).ok_or_else(|| anyhow!("No monitor found"))?;
        capture::log_monitors(&monitors, &monitor);
        if !monitor.sane() {
            bail!("Screen {} reports {}x{} at {},{}; waiting for the display to settle", monitor.name, monitor.width, monitor.height, monitor.x, monitor.y);
        }
        let list = self.encoders();
        let mut v = v.clone();
        let pick = |codec| {
            encode::pick(&list, v.encoder, codec).or_else(|| encode::pick(&list, crate::settings::EncoderChoice::Auto, codec))
        };
        let encoder = match pick(v.codec) {
            Some(e) => e,
            // HEVC asked for (low-spec, a profile) on a PC without an HEVC encoder.
            None if v.codec == crate::settings::Codec::Hevc => {
                tracing::warn!("no HEVC encoder; using H.264");
                v.codec = crate::settings::Codec::H264;
                pick(v.codec).ok_or_else(|| anyhow!("No video encoder available"))?
            }
            None => return Err(anyhow!("No video encoder available for {:?}", v.codec)),
        };
        let buffer_secs = s.buffer.seconds;
        self.inner.buffer_secs.store(buffer_secs, Ordering::Relaxed);
        *self.inner.buffer.lock() = ReplayBuffer::new(keep_secs(s, buffer_secs), s.buffer.max_memory_mb as usize * MB);
        Pipeline::start(
            PipelineConfig {
                monitor,
                cursor: v.capture_cursor,
                target_height: v.height,
                encoder,
                params: EncoderParams {
                    codec: v.codec,
                    width: 0,
                    height: 0,
                    fps: v.fps,
                    rate_control: v.rate_control,
                    bitrate_kbps: v.bitrate_kbps,
                    quality: v.quality,
                    keyframe_secs: v.keyframe_secs,
                },
                audio: s.audio.clone(),
            },
            self.inner.buffer.clone(),
            self.inner.flags.clone(),
        )
    }

    /// Switch the running game's profile in or out when it changed.
    fn follow_profile(&self, s: &Settings, exe: Option<&str>) {
        let want = profiles::active(s, exe).cloned();
        if *self.inner.profile.lock() == want {
            return;
        }
        let msg = match (&want, self.inner.profile.lock().as_ref()) {
            (Some(p), _) => format!("Using the {} profile", p.name),
            (None, Some(p)) => format!("{} closed: back to your usual settings", p.name),
            (None, None) => String::new(),
        };
        *self.inner.profile.lock() = want.clone();
        let effective = self.compute_effective(s, want.as_ref());
        self.switch_effective(effective.clone());
        self.inner.events.profile_changed(&effective);
        self.inner.events.notice(Level::Info, &msg);
    }

    fn control_loop(&self) {
        let mut detector = Detector::default();
        let mut last_detect = Instant::now() - Duration::from_secs(10);
        let mut last_status = Instant::now();
        let mut last_frames = 0u64;
        let mut last_bytes = 0u64;
        let mut retry_after = Instant::now();
        let mut last_purge = Instant::now() - Duration::from_secs(3600);
        let mut last_disk = Instant::now() - Duration::from_secs(60);
        let mut disk_was = DiskLevel::Ok;
        let mut last_power = Instant::now();
        let mut first_run_codec_done = false;
        while !self.inner.shutdown.load(Ordering::Relaxed) {
            let s = self.inner.settings.read().clone();
            self.inner.flags.privacy.store(profiles::privacy_focused(&s), Ordering::Relaxed);

            if last_purge.elapsed() >= Duration::from_secs(3600) {
                last_purge = Instant::now();
                match crate::recycled::purge(s.library.purge_days) {
                    Ok(0) => {}
                    Ok(n) => tracing::info!("purged {n} deleted clips older than {} days", s.library.purge_days),
                    Err(e) => tracing::warn!("purge deleted clips: {e:#}"),
                }
            }

            if last_disk.elapsed() >= Duration::from_secs(10) {
                last_disk = Instant::now();
                let free = self.refresh_disk();
                let level = disk_level(free, &s.disk);
                let gb = free.unwrap_or(0) as f64 / GB as f64;
                match level {
                    _ if level == disk_was => {}
                    DiskLevel::Full => self.inner.events.notice(Level::Error, &format!("Only {gb:.1} GB free on the clips drive. Saving clips is paused until there's more than {} GB.", s.disk.block_gb)),
                    DiskLevel::Low if disk_was == DiskLevel::Ok => self.inner.events.notice(Level::Warning, &format!("Only {gb:.1} GB free on the clips drive.")),
                    _ => {}
                }
                disk_was = level;
            }

            // Only looked at while battery throttling is on.
            if s.performance.battery_throttle && last_power.elapsed() >= Duration::from_secs(5) {
                last_power = Instant::now();
                let now = crate::perf::on_battery().unwrap_or(false);
                if self.inner.on_battery.swap(now, Ordering::Relaxed) != now {
                    self.inner.events.notice(
                        Level::Info,
                        if now { "On battery: using the low-spec preset" } else { "Plugged in: back to your usual settings" },
                    );
                    self.refresh_effective();
                }
            }

            if !first_run_codec_done {
                first_run_codec_done = true;
                self.default_to_hevc();
            }

            if last_detect.elapsed() >= Duration::from_secs(2) {
                last_detect = Instant::now();
                let g = detector.poll(&s);
                let exe = g.as_ref().map(|g| g.exe.clone());
                *self.inner.game.lock() = g;
                // Profiles off: stop here, nothing is looked up or switched.
                if s.profiles_enabled {
                    self.follow_profile(&s, exe.as_deref());
                }
            }

            // The buffer is always on while the app runs; saving never starts or stops it.
            // Its screen was unplugged: start over on whatever is there now. Only
            // a gone handle counts; a mode change on the same screen is handled
            // in place and must not throw the buffer away.
            let gone = self.inner.pipeline.lock().as_ref().map(|p| p.monitor).filter(|h| !capture::cached().iter().any(|m| m.handle == *h));
            if let Some(h) = gone {
                tracing::warn!("recorded monitor {h:#x} is gone; restarting capture");
                self.inner.restart.store(true, Ordering::Relaxed);
            }
            let running = self.inner.pipeline.lock().is_some();
            if self.inner.restart.swap(false, Ordering::Relaxed) && running {
                if let Some(mut p) = self.inner.pipeline.lock().take() {
                    p.stop();
                }
                *self.inner.error.lock() = None;
            }
            let running = self.inner.pipeline.lock().is_some();
            if !running && Instant::now() >= retry_after {
                let effective = self.inner.effective.read().clone();
                match self.start_pipeline(&effective) {
                    Ok(p) => {
                        tracing::info!("recording: {} at {}x{}", p.encoder_name, p.output.0, p.output.1);
                        *self.inner.pipeline.lock() = Some(p);
                        *self.inner.error.lock() = None;
                    }
                    Err(e) => {
                        let msg = format!("{e:#}");
                        tracing::error!("pipeline start failed: {msg}");
                        if self.inner.error.lock().as_deref() != Some(&msg) {
                            self.inner.events.notice(Level::Error, &msg);
                        }
                        *self.inner.error.lock() = Some(msg);
                        retry_after = Instant::now() + Duration::from_secs(10);
                    }
                }
            }

            if last_status.elapsed() >= Duration::from_millis(500) {
                let dt = last_status.elapsed().as_secs_f64();
                last_status = Instant::now();
                let (frames, bytes) = self
                    .inner
                    .pipeline
                    .lock()
                    .as_ref()
                    .map(|p| (p.stats.frames_out.load(Ordering::Relaxed), p.stats.bytes_out.load(Ordering::Relaxed)))
                    .unwrap_or((0, 0));
                let fps = frames.saturating_sub(last_frames) as f64 / dt;
                let kbps = bytes.saturating_sub(last_bytes) as f64 * 8.0 / 1000.0 / dt;
                last_frames = frames;
                last_bytes = bytes;
                self.inner.events.status(&self.build_status(fps, kbps));
            }
            std::thread::sleep(Duration::from_millis(250));
        }
    }

    /// First launch only: record HEVC when a hardware HEVC encoder exists
    /// (smaller files, same quality). Anything else stays H.264.
    fn default_to_hevc(&self) {
        let mut s = self.settings();
        if s.first_run_complete || s.video.codec != settings::Codec::H264 || s.performance.low_spec {
            return;
        }
        let hevc = self.encoders().iter().any(|e| e.codec == settings::Codec::Hevc && e.kind != encode::EncoderKind::Software);
        if hevc {
            s.video.codec = settings::Codec::Hevc;
            tracing::info!("first launch: hardware HEVC encoder found, defaulting to HEVC");
            if let Err(e) = self.set_settings(s) {
                tracing::warn!("default to HEVC: {e:#}");
            }
        }
    }

    pub fn clips(&self) -> Vec<ClipInfo> {
        library::list(&self.inner.settings.read().clips_dir)
    }

    pub fn clips_dir_path(&self) -> PathBuf {
        self.inner.settings.read().clips_dir.clone()
    }

    /// Keyframe-aligned trim into a new file next to the source.
    pub fn trim(&self, path: &Path, from: f64, to: f64) -> Result<ClipInfo> {
        let tracks = mux::reader::read_file(path)?;
        let (cut, start) = mux::trim(&tracks, from, to)?;
        let stem = format!("{} trim", path.file_stem().unwrap_or_default().to_string_lossy());
        let out = library::unique_path(path.parent().unwrap_or(Path::new(".")), &stem, "mp4");
        let bytes = mux::fmp4::write_file(&out, &cut)?;
        self.add_written(bytes);
        library::copy_thumb(path, &out);
        let _ = crate::input::trim_sidecar(path, &out, start, to);
        library::info(&out)
    }

    pub fn merge(&self, paths: &[PathBuf]) -> Result<ClipInfo> {
        let clips = paths.iter().map(|p| mux::reader::read_file(p)).collect::<Result<Vec<_>>>()?;
        let merged = mux::merge(&clips)?;
        let first = paths.first().ok_or_else(|| anyhow!("no clips"))?;
        let stem = format!("{} merged", first.file_stem().unwrap_or_default().to_string_lossy());
        let out = library::unique_path(first.parent().unwrap_or(Path::new(".")), &stem, "mp4");
        let bytes = mux::fmp4::write_file(&out, &merged)?;
        self.add_written(bytes);
        library::copy_thumb(first, &out);
        library::info(&out)
    }
}

/// Seconds the buffer keeps: the longest slot (`buffer_secs` may come from a
/// game profile), plus the post-roll a pending save is still waiting on, plus
/// slack for GOP rounding and "extend last clip".
fn keep_secs(s: &Settings, buffer_secs: u32) -> u32 {
    let longest = s.buffer.extra_slots.iter().filter(|x| x.enabled).map(|x| x.seconds).fold(buffer_secs, u32::max);
    longest + s.post_roll_secs() + s.buffer.extend_secs + 2
}

fn usage_path() -> PathBuf {
    settings::app_data_dir().join("usage.json")
}

fn today() -> chrono::NaiveDate {
    chrono::Local::now().date_naive()
}

/// Bytes Rewind wrote per local day, for the SSD warning and Settings > Storage.
#[derive(Debug, Default, PartialEq, Serialize, Deserialize)]
struct Usage {
    /// `YYYY-MM-DD` -> bytes. Two weeks are kept.
    days: BTreeMap<String, u64>,
}

fn day_key(d: chrono::NaiveDate) -> String {
    d.format("%Y-%m-%d").to_string()
}

impl Usage {
    fn load() -> Self {
        let v = std::fs::read_to_string(usage_path()).ok().and_then(|t| serde_json::from_str::<serde_json::Value>(&t).ok());
        Self::from_json(v.unwrap_or_default())
    }

    fn from_json(v: serde_json::Value) -> Self {
        // Before per-day history the file held just today: {"date", "bytes"}.
        if let (Some(d), Some(b)) = (v["date"].as_str(), v["bytes"].as_u64()) {
            return Usage { days: BTreeMap::from([(d.to_string(), b)]) };
        }
        serde_json::from_value(v).unwrap_or_default()
    }

    fn save(&self) {
        if let Ok(t) = serde_json::to_string(self) {
            let _ = settings::write_atomic(&usage_path(), t.as_bytes());
        }
    }

    /// Add to `today`'s total and return it.
    fn add(&mut self, today: chrono::NaiveDate, bytes: u64) -> u64 {
        let oldest = day_key(today - chrono::Days::new(13));
        self.days.retain(|k, _| *k >= oldest);
        let t = self.days.entry(day_key(today)).or_default();
        *t += bytes;
        *t
    }

    fn day(&self, d: chrono::NaiveDate) -> u64 {
        self.days.get(&day_key(d)).copied().unwrap_or(0)
    }

    /// Monday through `today`.
    fn week(&self, today: chrono::NaiveDate) -> u64 {
        use chrono::Datelike;
        let back = today.weekday().num_days_from_monday() as u64;
        (0..=back).map(|i| self.day(today - chrono::Days::new(i))).sum()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const S: i64 = TICKS_PER_SEC;

    #[test]
    fn marker_window_centres_and_shifts() {
        // Buffer holds 100..200 s.
        assert_eq!(marker_window(150 * S, 30 * S, 100 * S, 200 * S), Some((135 * S, 165 * S, false)));
        // Too close to now: shifted back to end at the newest frame.
        assert_eq!(marker_window(195 * S, 30 * S, 100 * S, 200 * S), Some((170 * S, 200 * S, true)));
        // Near the oldest frame: shifted forward.
        assert_eq!(marker_window(105 * S, 30 * S, 100 * S, 200 * S), Some((100 * S, 130 * S, true)));
        // Older than the buffer.
        assert_eq!(marker_window(90 * S, 30 * S, 100 * S, 200 * S), None);
    }

    #[test]
    fn usage_days_and_week() {
        let d = |s: &str| chrono::NaiveDate::parse_from_str(s, "%Y-%m-%d").unwrap();
        let mut u = Usage::from_json(serde_json::json!({ "date": "2026-10-26", "bytes": 5 }));
        assert_eq!(u.day(d("2026-10-26")), 5);
        // 2026-10-26 is a Monday; Sunday the 1st closes that week.
        assert_eq!(u.add(d("2026-10-31"), 7), 7);
        assert_eq!(u.add(d("2026-10-31"), 1), 8);
        assert_eq!(u.week(d("2026-11-01")), 13);
        // A new week starts Monday.
        assert_eq!(u.week(d("2026-11-02")), 0);
        // Old days drop off.
        u.add(d("2026-11-20"), 1);
        assert_eq!(u.days.len(), 1);
        let back = Usage::from_json(serde_json::to_value(&u).unwrap());
        assert_eq!(back, u);
    }

    #[test]
    fn disk_levels() {
        let s = settings::DiskSettings { warn_gb: 10, block_gb: 2 };
        assert_eq!(disk_level(None, &s), DiskLevel::Ok);
        assert_eq!(disk_level(Some(11 * GB), &s), DiskLevel::Ok);
        assert_eq!(disk_level(Some(9 * GB), &s), DiskLevel::Low);
        assert_eq!(disk_level(Some(GB), &s), DiskLevel::Full);
        let off = settings::DiskSettings { warn_gb: 10, block_gb: 0 };
        assert_eq!(disk_level(Some(0), &off), DiskLevel::Low);
    }

    #[test]
    fn keep_covers_post_roll() {
        let mut s = Settings::default();
        // Slot B (120 s) is the longest; extend adds 15, slack 2.
        assert_eq!(keep_secs(&s, 30), 137);
        s.post_roll.enabled = true;
        s.post_roll.seconds = 20;
        assert_eq!(keep_secs(&s, 30), 157);
    }
}
