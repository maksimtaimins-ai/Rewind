//! The engine owns the replay buffer and pipeline lifecycle, reacts to
//! hotkeys, and writes clips on a background thread so capture never blocks.

use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicBool, AtomicU32, AtomicU64, Ordering};
use std::sync::Arc;
use std::time::{Duration, Instant};

use anyhow::{anyhow, Result};
use crossbeam_channel::{unbounded, Receiver, Sender};
use parking_lot::{Mutex, RwLock};
use serde::Serialize;

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
    pub privacy: bool,
    pub markers: Vec<MarkerInfo>,
    pub error: Option<String>,
    pub written_today_gb: f64,
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
}

struct SaveJob {
    from: i64,
    to: i64,
    path: PathBuf,
    thumb: Option<Receiver<FrameGrab>>,
    replaces: bool,
}

#[derive(Clone)]
struct LastSave {
    from: i64,
    to: i64,
    path: PathBuf,
}

struct Inner {
    settings: RwLock<Settings>,
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
    save_tx: Sender<SaveJob>,
    error: Mutex<Option<String>>,
    hotkeys: Mutex<Option<HotkeyThread>>,
    written_today: AtomicU64,
    shutdown: AtomicBool,
    /// Buffer length in effect (a game profile may override the setting).
    buffer_secs: AtomicU32,
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
        let settings = settings::load();
        tracing::info!("settings: buffer {}s, first run done {}, clips {}", settings.buffer.seconds, settings.first_run_complete, settings.clips_dir.display());
        let (save_tx, save_rx) = unbounded();
        let inner = Arc::new(Inner {
            buffer: Arc::new(Mutex::new(ReplayBuffer::new(1, MB))),
            flags: Arc::new(Flags::default()),
            pipeline: Mutex::new(None),
            encoders: Mutex::new(Vec::new()),
            events,
            restart: AtomicBool::new(false),
            game: Mutex::new(None),
            markers: Mutex::new(Vec::new()),
            next_marker: AtomicU64::new(1),
            last_save: Mutex::new(None),
            save_tx,
            error: Mutex::new(None),
            hotkeys: Mutex::new(None),
            written_today: AtomicU64::new(load_usage()),
            shutdown: AtomicBool::new(false),
            buffer_secs: AtomicU32::new(settings.buffer.seconds),
            settings: RwLock::new(settings),
        });
        let engine = Engine { inner };

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
                        e.inner.events.notice(Level::Warning, &format!("{m} hotkey is already used by another app"));
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

    /// Persist new settings; restarts the pipeline if capture settings changed.
    pub fn set_settings(&self, new: Settings) -> Result<()> {
        settings::save(&new)?;
        let old = std::mem::replace(&mut *self.inner.settings.write(), new.clone());
        if old.hotkeys != new.hotkeys || old.buffer.extra_slots != new.buffer.extra_slots {
            if let Some(hk) = self.inner.hotkeys.lock().as_ref() {
                for e in hk.bind(&new) {
                    self.inner.events.notice(Level::Warning, &format!("Hotkey not set: {e}"));
                }
            }
        }
        if old.video != new.video || old.buffer != new.buffer || old.audio != new.audio || old.profiles != new.profiles {
            self.inner.restart.store(true, Ordering::Relaxed);
        }
        if old.launch_on_startup != new.launch_on_startup {
            platform::set_autostart(new.launch_on_startup)?;
        }
        Ok(())
    }

    pub fn encoders(&self) -> Vec<EncoderInfo> {
        let mut list = self.inner.encoders.lock();
        if list.is_empty() {
            *list = encode::enumerate_all();
        }
        list.clone()
    }

    pub fn benchmark(&self, secs: f64) -> Vec<BenchResult> {
        let monitors = capture::monitors();
        let m = monitors.first();
        let (w, h) = crate::gpu::output_size(
            m.map(|m| (m.width, m.height)).unwrap_or((1920, 1080)),
            self.inner.settings.read().video.height,
        );
        let list = self.encoders();
        let per = secs / list.len().max(1) as f64;
        encode::benchmark(&list, w, h, per.max(0.4))
    }

    pub fn shutdown(&self) {
        self.inner.shutdown.store(true, Ordering::Relaxed);
        if let Some(mut p) = self.inner.pipeline.lock().take() {
            p.stop();
        }
        save_usage(self.inner.written_today.load(Ordering::Relaxed));
    }

    pub fn handle(&self, action: Action) {
        tracing::info!("action {action:?}");
        let r = match action {
            Action::SaveClip => self.save_clip(None),
            Action::SaveSlot(i) => {
                let slot = self.inner.settings.read().buffer.extra_slots.get(i as usize).cloned();
                match slot {
                    Some(slot) if slot.enabled => self.save_clip(Some(slot.seconds)),
                    _ => Ok(()),
                }
            }
            Action::Marker => self.add_marker(),
            Action::Screenshot => self.screenshot(),
            Action::ExtendLast => self.extend_last(),
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
        let dir = self.inner.settings.read().clips_dir.clone();
        std::fs::create_dir_all(&dir)?;
        std::fs::create_dir_all(library::thumbs_dir(&dir))?;
        Ok(dir)
    }

    /// Clip length for a request: the main buffer length by default, never
    /// more than the longest enabled slot.
    fn clip_ticks(&self, secs: Option<u32>) -> i64 {
        let main = self.inner.buffer_secs.load(Ordering::Relaxed);
        let max = self.inner.settings.read().longest_slot_secs().max(main);
        secs.unwrap_or(main).clamp(1, max) as i64 * TICKS_PER_SEC
    }

    /// Save the last `secs` (default: the main buffer length), ending at the
    /// keypress.
    pub fn save_clip(&self, secs: Option<u32>) -> Result<()> {
        let pipeline = self.inner.pipeline.lock();
        let Some(p) = pipeline.as_ref() else {
            return Err(anyhow!("Capture is still starting"));
        };
        let now = qpc_now();
        let job = SaveJob {
            from: now - self.clip_ticks(secs),
            to: now,
            path: library::unique_path(&self.clips_dir()?, &self.clip_stem(), "mp4"),
            thumb: Some(p.request_frame()),
            replaces: false,
        };
        drop(pipeline);
        self.inner.save_tx.send(job)?;
        Ok(())
    }

    /// Save `secs` (at most the buffer length) around a marker, or as close to
    /// it as the buffer still holds.
    pub fn save_marker(&self, id: u64, secs: u32) -> Result<()> {
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

    /// Re-save the last clip in place with up to `extend_secs` more footage
    /// before its start, taken from the buffer.
    fn extend_last(&self) -> Result<()> {
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
        self.inner.save_tx.send(SaveJob { from, to: last.to, path: last.path, thumb: None, replaces: true })?;
        Ok(())
    }

    pub fn screenshot(&self) -> Result<()> {
        let rx = {
            let p = self.inner.pipeline.lock();
            p.as_ref().ok_or_else(|| anyhow!("Capture is still starting"))?.request_frame()
        };
        let dir = self.clips_dir()?.join("Screenshots");
        let path = library::unique_path(&dir, &self.clip_stem(), "png");
        let events = self.inner.clone();
        std::thread::spawn(move || {
            platform::com_init();
            let r = rx
                .recv_timeout(Duration::from_secs(2))
                .map_err(|_| anyhow!("No new frame to capture (screen hasn't changed)"))
                .and_then(|(w, h, px)| platform::write_image(&path, w, h, &px, ImageFormat::Png));
            match r {
                Ok(()) => events.events.notice(Level::Success, &format!("Screenshot saved: {}", path.display())),
                Err(e) => events.events.notice(Level::Error, &format!("Screenshot failed: {e:#}")),
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
        let snapshot = self.inner.buffer.lock().snapshot_within(job.from, job.to);
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
            } else {
                mux::write_snapshot(&job.path, &snap)?
            };
            self.add_written(bytes);
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
                *self.inner.last_save.lock() = Some(LastSave { from: start, to: end, path: job.path.clone() });
                self.inner.events.clip_saved(&info);
                let verb = if job.replaces { "Extended" } else { "Saved" };
                self.inner.events.notice(Level::Success, &format!("{verb} {:.0}s clip", info.duration));
            }
            Err(e) => self.inner.events.notice(Level::Error, &format!("Saving clip failed: {e:#}")),
        }
    }

    fn add_written(&self, bytes: u64) {
        let total = self.inner.written_today.fetch_add(bytes, Ordering::Relaxed) + bytes;
        save_usage(total);
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
        let s = self.inner.settings.read().clone();
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
            privacy: self.inner.flags.privacy.load(Ordering::Relaxed),
            markers,
            error,
            written_today_gb: self.inner.written_today.load(Ordering::Relaxed) as f64 / 1e9,
        }
    }

    fn start_pipeline(&self, s: &Settings) -> Result<Pipeline> {
        let game = self.inner.game.lock().clone();
        let profile = game.as_ref().and_then(|g| profiles::profile_for(&g.exe, &s.profiles)).cloned();
        let v = &s.video;
        let monitors = capture::monitors();
        let monitor = monitors
            .iter()
            .find(|m| m.index == v.monitor)
            .or_else(|| monitors.first())
            .cloned()
            .ok_or_else(|| anyhow!("No monitor found"))?;
        let list = self.encoders();
        let encoder = encode::pick(&list, v.encoder, v.codec)
            .or_else(|| encode::pick(&list, crate::settings::EncoderChoice::Auto, v.codec))
            .ok_or_else(|| anyhow!("No video encoder available for {:?}", v.codec))?;
        let buffer_secs = profile.as_ref().and_then(|p| p.buffer_seconds).unwrap_or(s.buffer.seconds);
        self.inner.buffer_secs.store(buffer_secs, Ordering::Relaxed);
        // Slack past the clip length: GOP rounding, plus room for "extend last clip".
        let longest = s.buffer.extra_slots.iter().filter(|x| x.enabled).map(|x| x.seconds).fold(buffer_secs, u32::max);
        let keep = longest + s.buffer.extend_secs + 2;
        *self.inner.buffer.lock() = ReplayBuffer::new(keep, s.buffer.max_memory_mb as usize * MB);
        Pipeline::start(
            PipelineConfig {
                monitor,
                cursor: v.capture_cursor,
                target_height: profile.as_ref().and_then(|p| p.height).unwrap_or(v.height),
                encoder,
                params: EncoderParams {
                    codec: v.codec,
                    width: 0,
                    height: 0,
                    fps: profile.as_ref().and_then(|p| p.fps).unwrap_or(v.fps),
                    rate_control: v.rate_control,
                    bitrate_kbps: profile.as_ref().and_then(|p| p.bitrate_kbps).unwrap_or(v.bitrate_kbps),
                    quality: v.quality,
                    keyframe_secs: v.keyframe_secs,
                },
                audio: s.audio.clone(),
            },
            self.inner.buffer.clone(),
            self.inner.flags.clone(),
        )
    }

    fn control_loop(&self) {
        let mut detector = Detector::default();
        let mut last_detect = Instant::now() - Duration::from_secs(10);
        let mut last_status = Instant::now();
        let mut last_frames = 0u64;
        let mut last_bytes = 0u64;
        let mut retry_after = Instant::now();
        let mut current_game_exe: Option<String> = None;
        while !self.inner.shutdown.load(Ordering::Relaxed) {
            let s = self.inner.settings.read().clone();
            self.inner.flags.privacy.store(profiles::privacy_focused(&s), Ordering::Relaxed);

            if last_detect.elapsed() >= Duration::from_secs(2) {
                last_detect = Instant::now();
                let g = detector.poll(&s);
                let exe = g.as_ref().map(|g| g.exe.clone());
                // A different game may have its own profile: restart to apply it.
                if exe != current_game_exe && self.inner.pipeline.lock().is_some() {
                    let had_profile = |e: &Option<String>| e.as_ref().is_some_and(|e| profiles::profile_for(e, &s.profiles).is_some());
                    if had_profile(&exe) || had_profile(&current_game_exe) {
                        self.inner.restart.store(true, Ordering::Relaxed);
                    }
                }
                current_game_exe = exe;
                *self.inner.game.lock() = g;
            }

            // The buffer is always on while the app runs; saving never starts or stops it.
            let running = self.inner.pipeline.lock().is_some();
            if self.inner.restart.swap(false, Ordering::Relaxed) && running {
                if let Some(mut p) = self.inner.pipeline.lock().take() {
                    p.stop();
                }
                *self.inner.error.lock() = None;
            }
            let running = self.inner.pipeline.lock().is_some();
            if !running && Instant::now() >= retry_after {
                match self.start_pipeline(&s) {
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

    pub fn clips(&self) -> Vec<ClipInfo> {
        library::list(&self.inner.settings.read().clips_dir)
    }

    pub fn clips_dir_path(&self) -> PathBuf {
        self.inner.settings.read().clips_dir.clone()
    }

    /// Keyframe-aligned trim into a new file next to the source.
    pub fn trim(&self, path: &Path, from: f64, to: f64) -> Result<ClipInfo> {
        let tracks = mux::reader::read_file(path)?;
        let (cut, _) = mux::trim(&tracks, from, to)?;
        let stem = format!("{} trim", path.file_stem().unwrap_or_default().to_string_lossy());
        let out = library::unique_path(path.parent().unwrap_or(Path::new(".")), &stem, "mp4");
        let bytes = mux::fmp4::write_file(&out, &cut)?;
        self.add_written(bytes);
        library::copy_thumb(path, &out);
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

fn usage_path() -> PathBuf {
    settings::app_data_dir().join("usage.json")
}

/// Bytes written today (resets when the date changes).
fn load_usage() -> u64 {
    let today = chrono::Local::now().format("%Y-%m-%d").to_string();
    std::fs::read_to_string(usage_path())
        .ok()
        .and_then(|t| serde_json::from_str::<serde_json::Value>(&t).ok())
        .filter(|v| v["date"] == today.as_str())
        .and_then(|v| v["bytes"].as_u64())
        .unwrap_or(0)
}

fn save_usage(bytes: u64) {
    let today = chrono::Local::now().format("%Y-%m-%d").to_string();
    let _ = settings::write_atomic(&usage_path(), serde_json::json!({ "date": today, "bytes": bytes }).to_string().as_bytes());
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
}
