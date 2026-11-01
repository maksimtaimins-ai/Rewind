# Rewind

Local-only instant replay for Windows. Rewind keeps the last few minutes of your screen and audio in RAM as already-encoded video, and writes a clip to disk when you press a hotkey. No account, no cloud, no telemetry.

## Build

Requirements: Rust (stable), Node 20+, WebView2 (preinstalled on Windows 10/11), and the Tauri CLI (`cargo install tauri-cli --version "^2" --locked`).

```bash
npm install
```

```bash
npm run package
```

`npm run package` runs `cargo tauri build` and copies the installer to `src-tauri/target/release/bundle/nsis/Rewind-Setup.exe`. Plain `cargo tauri build` also works; it leaves the installer as `Rewind_<version>_x64-setup.exe` in the same folder (Tauri hard-codes that name).

Development:

```bash
cargo tauri dev
```

`npm run dev` alone serves the UI in a browser with sample data, which is useful for design work.

## How it works

```
WGC frame (GPU) -> D3D11 video processor: BGRA -> NV12 + scale (GPU)
               -> Media Foundation encoder MFT (NVENC / AMF / QSV, zero-copy)
               -> H.264/HEVC NAL units -> ring buffer (RAM)
WASAPI (loopback / per-process / mic) -> AAC MFT -> ring buffer, one track each
Hotkey -> snapshot buffer -> fragmented MP4 written on a background thread
```

- **Capture:** Windows.Graphics.Capture, frame-arrived callbacks with no polling. FPS is capped from frame timestamps, so VRR is respected. No hooks or DLL injection.
- **Encode:** hardware MFTs are enumerated per GPU (`MFTEnum2`), so on multi-GPU machines each encoder gets a D3D device on its own adapter. `Auto` picks NVENC > AMF > QSV and never the CPU encoder while hardware exists. Keyframes are forced on a timer (default every 1 s), and there are no B-frames.
- **Buffer:** whole GOPs of encoded packets, trimmed by time and by a RAM cap. A saved clip is the last N seconds (the buffer length) ending at the keypress. With the opt-in post-roll setting, a normal save keeps recording 1-120 s past the keypress before it's written; presses during the wait queue up in order.
- **Mux:** hand-written fMP4 (`src-tauri/src/mux`): one fragment per GOP plus an `mfra` index. Each audio source is its own named track. Trim and merge are keyframe-aligned remuxes, so there's no re-encode.
- **Audio:** each track runs its own thread. Gaps (e.g. silent loopback) are filled with silence, and clock drift is corrected against QPC.
- **Settings:** versioned TOML in `%APPDATA%\Rewind\settings.toml` with migrations (`settings::migrate`). Log: `%APPDATA%\Rewind\rewind.log`.

Platform-specific code sits behind `#[cfg(windows)]` modules (`capture::wgc`, `encode::mf`, `audio::wasapi`, `hotkey::win`); the buffer, muxer and settings are portable.

## Installer

NSIS via Tauri (`src-tauri/tauri.conf.json`, template in `src-tauri/windows/installer.nsi`):

- Per-user or per-machine install, chosen on the first screen
- Start Menu shortcut; desktop shortcut checkbox on the finish page
- "Launch Rewind when Windows starts" page (writes `HKCU\...\Run\Rewind`, which the in-app setting also controls)
- Uninstaller registered in Apps & Features (it also removes the startup entry)

**Code signing:** `src-tauri/windows/sign.ps1` runs for every binary. It does nothing until you set `REWIND_SIGN_PFX` (+ `REWIND_SIGN_PFX_PASSWORD`) or `REWIND_SIGN_THUMBPRINT`.

## Diagnostics

`cargo run --release --bin poc -- <out_dir> [--hevc]` benchmarks every encoder, records 8 s of the main screen plus desktop audio, writes a clip and thumbnail, then decodes the clip with Windows' own MP4 demuxer to check it's valid.

## Status

Done (MVP): ring buffer + hotkey save, settings UI with a Pro mode toggle, clip library (play, rename, soft delete with a 30-day Recently deleted bin, open folder, search and filters, sessions, favorites, tags, custom thumbnails), keyframe-aligned trim, multi-track audio (system / per-app / everything-except-app / mic), first-run wizard with encoder benchmark, installer.

Also in: session markers, extend last clip, up to three replay lengths, slow-mo replay (retimed, video only), export presets (Discord 10/25 MB, Shorts, X, GIF; Media Foundation decode/encode, WIC for GIF), per-game profiles with game detection, screenshots, pause hotkey, privacy apps (capture pauses while they're focused), clip merge, benchmark in settings, settings import/export, daily SSD write warning, tray menu, Discord webhook posting (Settings > Sharing; WinHTTP multipart upload, per-webhook 10/25 MB cap with an export offer when a clip is over it).

Not yet:
- Frame-exact trim (re-encode): trim currently snaps the start to the previous keyframe.
- HDR tonemapping: HDR desktops are captured as 8-bit BGRA; the tonemap shader is still to do, and the option is hidden until it exists.
- Window capture (a single game window instead of the whole monitor).
- The rest of the advanced list: overlay, annotate/blur, auto-crop, LAN send, HUD mode, highlights.
