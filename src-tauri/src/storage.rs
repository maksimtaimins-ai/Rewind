//! Settings > Storage: what Rewind's files take up, by kind. Measured only
//! when the UI asks (opening Storage, the dashboard, after a save); there is
//! no background scan.

use std::path::Path;

use serde::Serialize;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum Kind {
    Clips,
    Screenshots,
    Exports,
    Recycled,
    Thumbnails,
    AppData,
}

pub const KINDS: [Kind; 6] = [Kind::Clips, Kind::Screenshots, Kind::Exports, Kind::Recycled, Kind::Thumbnails, Kind::AppData];

#[derive(Debug, Clone, Default, PartialEq, Serialize)]
pub struct Usage {
    pub bytes: u64,
    pub files: u64,
}

#[derive(Debug, Clone, Serialize)]
pub struct Category {
    pub kind: Kind,
    pub bytes: u64,
    pub files: u64,
}

#[derive(Debug, Clone, Serialize)]
pub struct Breakdown {
    /// In [`KINDS`] order.
    pub categories: Vec<Category>,
    pub total: u64,
    /// The clips drive; `None` if it couldn't be read.
    pub drive_total: Option<u64>,
    pub drive_free: Option<u64>,
}

/// Export suffixes (`export::Preset::suffix`).
const EXPORT_SUFFIXES: [&str; 5] = ["-discord10", "-discord25", "-shorts", "-twitter", "-gif"];

/// Which kind a file in the clips folder is. `in_thumbs`: it sits in a
/// `.thumbs` folder.
pub fn classify(path: &Path, in_thumbs: bool) -> Kind {
    let name = path.file_name().unwrap_or_default().to_string_lossy().to_lowercase();
    let stem = path.file_stem().unwrap_or_default().to_string_lossy().to_lowercase();
    let ext = path.extension().unwrap_or_default().to_string_lossy().to_lowercase();
    if in_thumbs || name.ends_with("-thumb.jpg") {
        Kind::Thumbnails
    } else if ext == "png" {
        Kind::Screenshots
    } else if ext == "gif" || (ext == "mp4" && (stem.starts_with("montage ") || EXPORT_SUFFIXES.iter().any(|s| stem.contains(s)))) {
        Kind::Exports
    } else {
        // Clips and their recorded keystrokes.
        Kind::Clips
    }
}

fn add(u: &mut [Usage; 6], kind: Kind, bytes: u64) {
    let i = KINDS.iter().position(|k| *k == kind).unwrap();
    u[i].bytes += bytes;
    u[i].files += 1;
}

/// Files in `dir` and (for `depth` > 0) its subfolders.
fn walk(dir: &Path, depth: u32, in_thumbs: bool, f: &mut dyn FnMut(&Path, u64, bool)) {
    let Ok(rd) = std::fs::read_dir(dir) else { return };
    for e in rd.flatten() {
        let Ok(meta) = e.metadata() else { continue };
        if meta.is_dir() {
            if depth > 0 {
                let thumbs = in_thumbs || e.file_name() == ".thumbs";
                walk(&e.path(), depth - 1, thumbs, f);
            }
        } else {
            f(&e.path(), meta.len(), in_thumbs);
        }
    }
}

/// Measure the clips folder (every level a folder template can make, and the
/// `.thumbs` inside them), the Recently deleted bin, and the app's own files.
pub fn measure(clips: &Path, bin: &Path, app_data: &Path) -> [Usage; 6] {
    let mut u: [Usage; 6] = Default::default();
    walk(clips, crate::naming::MAX_FOLDER_DEPTH as u32 + 2, false, &mut |p, len, thumbs| add(&mut u, classify(p, thumbs), len));
    walk(bin, 0, false, &mut |_, len, _| add(&mut u, Kind::Recycled, len));
    // Settings, library.json, usage, log: top-level files only, so the clips
    // folder (which defaults to living in here) isn't counted twice.
    walk(app_data, 0, false, &mut |_, len, _| add(&mut u, Kind::AppData, len));
    u
}

pub fn breakdown(clips: &Path) -> Breakdown {
    let u = measure(clips, &crate::recycled::bin_dir(), &crate::settings::app_data_dir());
    #[cfg(windows)]
    let drive = crate::platform::disk_space(clips);
    #[cfg(not(windows))]
    let drive: Option<(u64, u64)> = None;
    Breakdown {
        total: u.iter().map(|x| x.bytes).sum(),
        categories: KINDS.iter().zip(u).map(|(k, x)| Category { kind: *k, bytes: x.bytes, files: x.files }).collect(),
        drive_total: drive.map(|d| d.0),
        drive_free: drive.map(|d| d.1),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn classifies() {
        let p = Path::new;
        assert_eq!(classify(p("C:/c/CS2 2026-10-31 21-14-02.mp4"), false), Kind::Clips);
        assert_eq!(classify(p("C:/c/CS2 2026-10-31 21-14-02-input.json"), false), Kind::Clips);
        assert_eq!(classify(p("C:/c/CS2 x-discord10.mp4"), false), Kind::Exports);
        assert_eq!(classify(p("C:/c/CS2 x-shorts (2).mp4"), false), Kind::Exports);
        assert_eq!(classify(p("C:/c/Montage 2026-11-01 10-00-00.mp4"), false), Kind::Exports);
        assert_eq!(classify(p("C:/c/x-gif.gif"), false), Kind::Exports);
        assert_eq!(classify(p("C:/c/Screenshot 2026.png"), false), Kind::Screenshots);
        assert_eq!(classify(p("C:/c/A-thumb.jpg"), false), Kind::Thumbnails);
        assert_eq!(classify(p("C:/c/.thumbs/A.jpg"), true), Kind::Thumbnails);
    }

    #[test]
    fn measures_folders() {
        let root = std::env::temp_dir().join(format!("rewind-storage-{}", std::process::id()));
        let clips = root.join("Clips");
        for d in [clips.join(".thumbs"), clips.join("Rocket League").join(".thumbs"), root.join("recycled")] {
            std::fs::create_dir_all(d).unwrap();
        }
        let put = |p: std::path::PathBuf, n: usize| std::fs::write(p, vec![0u8; n]).unwrap();
        put(clips.join("a.mp4"), 1000);
        put(clips.join("Rocket League").join("b.mp4"), 500);
        put(clips.join("Rocket League").join(".thumbs").join("b.jpg"), 30);
        put(clips.join(".thumbs").join("a.jpg"), 20);
        put(clips.join("a-discord10.mp4"), 300);
        put(clips.join("Screenshot.png"), 70);
        put(root.join("recycled").join("old.mp4"), 900);
        put(root.join("settings.toml"), 5);
        let u = measure(&clips, &root.join("recycled"), &root);
        let get = |k: Kind| u[KINDS.iter().position(|x| *x == k).unwrap()].clone();
        assert_eq!(get(Kind::Clips), Usage { bytes: 1500, files: 2 });
        assert_eq!(get(Kind::Thumbnails), Usage { bytes: 50, files: 2 });
        assert_eq!(get(Kind::Exports).bytes, 300);
        assert_eq!(get(Kind::Screenshots).bytes, 70);
        assert_eq!(get(Kind::Recycled).bytes, 900);
        // Only top-level app files: the Clips folder inside isn't counted again.
        assert_eq!(get(Kind::AppData), Usage { bytes: 5, files: 1 });
        std::fs::remove_dir_all(&root).ok();
    }
}
