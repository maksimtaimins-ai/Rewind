//! Settings benchmark: watch the live pipeline (which already runs at the
//! current settings) for a few seconds, measure the encoder and the system,
//! and say in plain words whether the settings hold up.

use serde::Serialize;

#[derive(Debug, Clone, Default, Serialize)]
pub struct Report {
    pub secs: f64,
    pub encoder: String,
    pub width: u32,
    pub height: u32,
    /// The frame-rate cap in effect (a game profile may override the setting).
    pub target_fps: u32,
    /// Frames the screen delivered per second.
    pub capture_fps: f64,
    /// Frames encoded per second.
    pub encoded_fps: f64,
    /// Highest rate the encoder could keep up, from how busy it was.
    pub encoder_max_fps: f64,
    /// Share of the time the encoder was working, 0-1.
    pub encoder_load: f64,
    pub dropped: u64,
    pub bitrate_kbps: f64,
    /// Whole-system CPU, percent.
    pub cpu_pct: f64,
    /// Rewind's own CPU, percent of the whole machine.
    pub rewind_cpu_pct: f64,
    /// Busiest GPU 3D engine and video-encode engine, percent. `None` when
    /// Windows doesn't expose GPU counters.
    pub gpu_pct: Option<f64>,
    pub gpu_encode_pct: Option<f64>,
    /// Rewind's working set, MB.
    pub ram_mb: f64,
    /// Of which the replay buffer, MB.
    pub buffer_mb: f64,
    pub verdict: Verdict,
}

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum Grade {
    #[default]
    Pass,
    Warn,
    Fail,
}

#[derive(Debug, Clone, Default, Serialize, PartialEq)]
pub struct Verdict {
    pub grade: Grade,
    pub headline: String,
    pub detail: Vec<String>,
}

/// Plain-language judgement of a run.
pub fn verdict(r: &Report) -> Verdict {
    let target = r.target_fps.max(1) as f64;
    let res = format!("{}p {}fps", r.height, r.target_fps);
    let load = (r.encoder_load * 100.0).round();
    let frames = r.capture_fps * r.secs;
    let drop_share = if frames > 0.0 { r.dropped as f64 / (frames + r.dropped as f64) } else { 0.0 };
    let mut detail = Vec::new();
    // A still desktop delivers few frames; judge capacity, not the count.
    if r.capture_fps < target * 0.9 {
        detail.push(format!(
            "The screen changed about {:.0} times a second during the test, under the {} fps cap. That's normal on a still desktop. Run it while a game is moving for a full reading.",
            r.capture_fps, r.target_fps
        ));
    }
    if r.cpu_pct >= 90.0 {
        detail.push(format!("The CPU was {:.0}% busy overall, so games may stutter. Rewind itself used {:.0}%.", r.cpu_pct, r.rewind_cpu_pct));
    }
    if let Some(g) = r.gpu_pct.filter(|g| *g >= 95.0) {
        detail.push(format!("The GPU was {g:.0}% busy. If the game is GPU-bound, a lower capture resolution frees some headroom."));
    }
    let (grade, headline) = if drop_share > 0.02 || r.encoder_load >= 0.9 {
        (Grade::Fail, format!("Encoder is at {load:.0}% capacity. Reduce resolution or bitrate."))
    } else if r.encoder_max_fps > 0.0 && r.encoder_max_fps < target * 0.95 {
        (Grade::Fail, format!("The encoder tops out around {:.0} fps, under the {} fps cap. Reduce resolution or frame rate.", r.encoder_max_fps, r.target_fps))
    } else if r.encoder_load >= 0.75 || r.cpu_pct >= 90.0 {
        (Grade::Warn, format!("Your settings work for {res}, with little headroom (encoder {load:.0}% busy)."))
    } else {
        (Grade::Pass, format!("Your current settings are fine for {res} gameplay."))
    };
    Verdict { grade, headline, detail }
}

/// System counters sampled at the start and end of a run.
#[cfg(windows)]
pub mod sys {
    use windows::core::w;
    use windows::Win32::Foundation::FILETIME;
    use windows::Win32::System::Performance::*;
    use windows::Win32::System::ProcessStatus::{GetProcessMemoryInfo, PROCESS_MEMORY_COUNTERS};
    use windows::Win32::System::Threading::{GetCurrentProcess, GetProcessTimes, GetSystemTimes};

    fn ft(t: FILETIME) -> u64 {
        ((t.dwHighDateTime as u64) << 32) | t.dwLowDateTime as u64
    }

    /// (system busy, system total, this process) in 100 ns units.
    #[derive(Clone, Copy, Default)]
    pub struct Cpu(u64, u64, u64);

    pub fn cpu() -> Cpu {
        let (mut idle, mut kernel, mut user) = (FILETIME::default(), FILETIME::default(), FILETIME::default());
        let (mut c, mut e, mut pk, mut pu) = (FILETIME::default(), FILETIME::default(), FILETIME::default(), FILETIME::default());
        unsafe {
            let _ = GetSystemTimes(Some(&mut idle), Some(&mut kernel), Some(&mut user));
            let _ = GetProcessTimes(GetCurrentProcess(), &mut c, &mut e, &mut pk, &mut pu);
        }
        // Kernel time includes idle time.
        let total = ft(kernel) + ft(user);
        Cpu(total - ft(idle), total, ft(pk) + ft(pu))
    }

    /// (system %, this process % of the whole machine) between two samples.
    pub fn cpu_pct(a: Cpu, b: Cpu) -> (f64, f64) {
        let total = b.1.saturating_sub(a.1).max(1) as f64;
        (b.0.saturating_sub(a.0) as f64 / total * 100.0, b.2.saturating_sub(a.2) as f64 / total * 100.0)
    }

    pub fn ram_mb() -> f64 {
        let mut m = PROCESS_MEMORY_COUNTERS { cb: std::mem::size_of::<PROCESS_MEMORY_COUNTERS>() as u32, ..Default::default() };
        unsafe {
            let _ = GetProcessMemoryInfo(GetCurrentProcess(), &mut m, m.cb);
        }
        m.WorkingSetSize as f64 / (1024.0 * 1024.0)
    }

    /// GPU engine utilisation through the same counters Task Manager uses.
    pub struct Gpu {
        query: PDH_HQUERY,
        three_d: PDH_HCOUNTER,
        encode: PDH_HCOUNTER,
        /// Highest reading per engine type over the run.
        pub peak: (Option<f64>, Option<f64>),
        sum: (f64, f64, u32),
    }

    impl Gpu {
        pub fn open() -> Option<Self> {
            unsafe {
                let mut query = PDH_HQUERY::default();
                if PdhOpenQueryW(None, 0, &mut query) != 0 {
                    return None;
                }
                let (mut three_d, mut encode) = (PDH_HCOUNTER::default(), PDH_HCOUNTER::default());
                let a = PdhAddEnglishCounterW(query, w!("\\GPU Engine(*engtype_3D)\\Utilization Percentage"), 0, &mut three_d);
                let b = PdhAddEnglishCounterW(query, w!("\\GPU Engine(*engtype_VideoEncode)\\Utilization Percentage"), 0, &mut encode);
                if a != 0 && b != 0 {
                    PdhCloseQuery(query);
                    return None;
                }
                PdhCollectQueryData(query);
                Some(Gpu { query, three_d, encode, peak: (None, None), sum: (0.0, 0.0, 0) })
            }
        }

        /// Sum of every instance of a counter, i.e. the engine's total use
        /// across processes, per physical engine (`luid_..._phys_N_eng_M`).
        fn total(counter: PDH_HCOUNTER) -> Option<f64> {
            if counter.is_invalid() {
                return None;
            }
            unsafe {
                let (mut size, mut count) = (0u32, 0u32);
                PdhGetFormattedCounterArrayW(counter, PDH_FMT_DOUBLE, &mut size, &mut count, None);
                if size == 0 {
                    return Some(0.0);
                }
                let n = size as usize / std::mem::size_of::<PDH_FMT_COUNTERVALUE_ITEM_W>() + 1;
                let mut items = vec![PDH_FMT_COUNTERVALUE_ITEM_W::default(); n];
                if PdhGetFormattedCounterArrayW(counter, PDH_FMT_DOUBLE, &mut size, &mut count, Some(items.as_mut_ptr())) != 0 {
                    return None;
                }
                let mut per_engine: std::collections::HashMap<String, f64> = std::collections::HashMap::new();
                for it in &items[..count as usize] {
                    let name = it.szName.to_string().unwrap_or_default();
                    // "pid_123_luid_0x..._phys_0_eng_3_engtype_3D": group by engine.
                    let engine = name.find("luid_").map(|i| name[i..].to_string()).unwrap_or(name);
                    *per_engine.entry(engine).or_default() += it.FmtValue.Anonymous.doubleValue;
                }
                Some(per_engine.values().cloned().fold(0.0, f64::max).min(100.0))
            }
        }

        /// Take a reading; call about once a second.
        pub fn sample(&mut self) {
            unsafe {
                PdhCollectQueryData(self.query);
            }
            let (a, b) = (Self::total(self.three_d), Self::total(self.encode));
            if let Some(a) = a {
                self.sum.0 += a;
            }
            if let Some(b) = b {
                self.sum.1 += b;
            }
            self.sum.2 += 1;
            self.peak = (a.map(|a| self.peak.0.unwrap_or(0.0).max(a)), b.map(|b| self.peak.1.unwrap_or(0.0).max(b)));
        }

        /// Average readings over the run.
        pub fn average(&self) -> (Option<f64>, Option<f64>) {
            let n = self.sum.2.max(1) as f64;
            (self.peak.0.map(|_| self.sum.0 / n), self.peak.1.map(|_| self.sum.1 / n))
        }
    }

    impl Drop for Gpu {
        fn drop(&mut self) {
            unsafe {
                PdhCloseQuery(self.query);
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn run(capture: f64, max: f64, load: f64, dropped: u64) -> Report {
        Report { secs: 10.0, height: 1440, target_fps: 60, capture_fps: capture, encoded_fps: capture, encoder_max_fps: max, encoder_load: load, dropped, cpu_pct: 30.0, ..Default::default() }
    }

    /// Reads the real counters. `cargo test -- --ignored system_counters --nocapture`
    #[cfg(windows)]
    #[test]
    #[ignore]
    fn system_counters() {
        let a = sys::cpu();
        let mut g = sys::Gpu::open().expect("GPU counters");
        for _ in 0..2 {
            std::thread::sleep(std::time::Duration::from_secs(1));
            g.sample();
        }
        let (cpu, mine) = sys::cpu_pct(a, sys::cpu());
        println!("cpu {cpu:.1}% (self {mine:.2}%), ram {:.0} MB, gpu {:?}", sys::ram_mb(), g.average());
        assert!((0.0..=100.0).contains(&cpu) && sys::ram_mb() > 1.0);
    }

    #[test]
    fn verdicts() {
        let v = verdict(&run(60.0, 240.0, 0.25, 0));
        assert_eq!(v.grade, Grade::Pass);
        assert_eq!(v.headline, "Your current settings are fine for 1440p 60fps gameplay.");
        assert!(v.detail.is_empty());

        let v = verdict(&run(58.0, 61.0, 0.95, 0));
        assert_eq!(v.grade, Grade::Fail);
        assert_eq!(v.headline, "Encoder is at 95% capacity. Reduce resolution or bitrate.");

        // Dropped frames fail even when load looks fine.
        assert_eq!(verdict(&run(60.0, 200.0, 0.3, 50)).grade, Grade::Fail);
        assert_eq!(verdict(&run(60.0, 70.0, 0.8, 0)).grade, Grade::Warn);

        // A still desktop: few frames, but plenty of capacity.
        let v = verdict(&run(4.0, 300.0, 0.02, 0));
        assert_eq!(v.grade, Grade::Pass);
        assert_eq!(v.detail.len(), 1);
    }
}
