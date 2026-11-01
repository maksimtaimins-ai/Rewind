//! Low-level keyboard and mouse hooks on their own message-loop thread.
//! Installed only while the keystroke overlay records or the live overlay is
//! on; each hook is set only
//! for the input types turned on. Callbacks do a few comparisons and a push,
//! so they stay far under Windows' hook timeout.

use std::collections::HashSet;
use std::sync::atomic::Ordering;
use std::sync::Arc;
use std::thread::JoinHandle;

use anyhow::{anyhow, Result};
use parking_lot::{Mutex, RwLock};
use windows::Win32::Foundation::{HINSTANCE, LPARAM, LRESULT, WPARAM};
use windows::Win32::System::LibraryLoader::GetModuleHandleW;
use windows::Win32::System::Threading::GetCurrentThreadId;
use windows::Win32::UI::Input::KeyboardAndMouse::GetAsyncKeyState;
use windows::Win32::UI::WindowsAndMessaging::*;

use super::{key_allowed, key_name, Event, Kind, Shared, MOUSE_BUTTONS};
use crate::platform::qpc_now;

/// The hook callbacks take no context, so they find the recorder here.
static SHARED: RwLock<Option<Arc<Shared>>> = RwLock::new(None);

#[derive(Default)]
struct KeyState {
    /// Keys physically down, to drop auto-repeat.
    down: HashSet<u32>,
    /// Keys whose press was recorded, so their release is too.
    recorded: HashSet<u32>,
    /// Mouse buttons whose press was recorded.
    buttons: HashSet<&'static str>,
    last_move: i64,
}

static STATE: Mutex<Option<KeyState>> = Mutex::new(None);

pub struct HookThread {
    tid: u32,
    join: Option<JoinHandle<()>>,
    pub keyboard: bool,
    pub mouse: bool,
}

/// Which hooks a configuration needs: (keyboard, mouse).
pub fn needs(cfg: &crate::settings::KeystrokeSettings) -> (bool, bool) {
    (cfg.keyboard, cfg.clicks || cfg.movement || cfg.scroll)
}

impl HookThread {
    pub fn start(shared: Arc<Shared>) -> Result<Self> {
        let (keyboard, mouse) = needs(&shared.cfg.read());
        *SHARED.write() = Some(shared);
        *STATE.lock() = Some(KeyState::default());
        let (tx, rx) = crossbeam_channel::bounded::<Result<u32, String>>(1);
        let join = std::thread::Builder::new().name("rewind-input".into()).spawn(move || unsafe {
            let module = GetModuleHandleW(None).ok().map(|m| HINSTANCE(m.0));
            let kb = if keyboard { SetWindowsHookExW(WH_KEYBOARD_LL, Some(kb_proc), module, 0).map(Some) } else { Ok(None) };
            let ms = if mouse { SetWindowsHookExW(WH_MOUSE_LL, Some(mouse_proc), module, 0).map(Some) } else { Ok(None) };
            let (kb, ms) = match (kb, ms) {
                (Ok(k), Ok(m)) => (k, m),
                (k, m) => {
                    let e = k.as_ref().err().or(m.as_ref().err()).map(|e| e.to_string()).unwrap_or_default();
                    for h in [k.ok().flatten(), m.ok().flatten()].into_iter().flatten() {
                        let _ = UnhookWindowsHookEx(h);
                    }
                    let _ = tx.send(Err(e));
                    return;
                }
            };
            let _ = tx.send(Ok(GetCurrentThreadId()));
            let mut msg = MSG::default();
            while GetMessageW(&mut msg, None, 0, 0).as_bool() {}
            for h in [kb, ms].into_iter().flatten() {
                let _ = UnhookWindowsHookEx(h);
            }
        })?;
        match rx.recv() {
            Ok(Ok(tid)) => Ok(HookThread { tid, join: Some(join), keyboard, mouse }),
            Ok(Err(e)) => {
                let _ = join.join();
                *SHARED.write() = None;
                Err(anyhow!("couldn't install the input hook: {e}"))
            }
            Err(_) => Err(anyhow!("input hook thread exited")),
        }
    }

    pub fn stop(mut self) {
        unsafe {
            let _ = PostThreadMessageW(self.tid, WM_QUIT, WPARAM(0), LPARAM(0));
        }
        if let Some(j) = self.join.take() {
            let _ = j.join();
        }
        *SHARED.write() = None;
        *STATE.lock() = None;
    }
}

/// A text box somewhere has the blinking caret (chat, a browser field, a
/// login form): keys typed now shouldn't be recorded.
fn typing_in_text_field() -> bool {
    unsafe {
        let fg = GetForegroundWindow();
        if fg.0.is_null() {
            return false;
        }
        let tid = GetWindowThreadProcessId(fg, None);
        let mut gi = GUITHREADINFO { cbSize: std::mem::size_of::<GUITHREADINFO>() as u32, ..Default::default() };
        GetGUIThreadInfo(tid, &mut gi).is_ok() && !gi.hwndCaret.0.is_null()
    }
}

fn held_modifiers(except: &str) -> Vec<String> {
    let down = |vk: i32| unsafe { GetAsyncKeyState(vk) as u16 & 0x8000 != 0 };
    let mut m = Vec::new();
    for (name, vk) in [("Ctrl", 0x11), ("Shift", 0x10), ("Alt", 0x12)] {
        if name != except && down(vk) {
            m.push(name.to_string());
        }
    }
    if except != "Win" && (down(0x5B) || down(0x5C)) {
        m.push("Win".into());
    }
    m
}

fn push(sh: &Shared, ev: Event) {
    if ev.kind != Kind::Move {
        super::send_live(&ev);
    }
    if sh.record.load(Ordering::Relaxed) {
        let keep = sh.keep_ticks.load(Ordering::Relaxed);
        sh.log.lock().push(qpc_now(), ev, keep);
    }
}

fn on_key(vk: u32, pressed: bool) {
    let Some(sh) = SHARED.read().clone() else { return };
    let mut guard = STATE.lock();
    let Some(st) = guard.as_mut() else { return };
    let name = key_name(vk);
    if !pressed {
        st.down.remove(&vk);
        if st.recorded.remove(&vk) {
            push(&sh, Event::key(0.0, Kind::KeyUp, &name, Vec::new()));
        }
        return;
    }
    if !st.down.insert(vk) {
        return; // auto-repeat
    }
    let cfg = sh.cfg.read();
    if sh.privacy.blocked() || (cfg.pause_in_text_fields && typing_in_text_field()) || !key_allowed(&cfg, &name) {
        return;
    }
    st.recorded.insert(vk);
    push(&sh, Event::key(0.0, Kind::KeyDown, &name, held_modifiers(&name)));
}

unsafe extern "system" fn kb_proc(code: i32, wp: WPARAM, lp: LPARAM) -> LRESULT {
    if code == HC_ACTION as i32 {
        let k = unsafe { &*(lp.0 as *const KBDLLHOOKSTRUCT) };
        let pressed = matches!(wp.0 as u32, WM_KEYDOWN | WM_SYSKEYDOWN);
        on_key(k.vkCode, pressed);
    }
    unsafe { CallNextHookEx(None, code, wp, lp) }
}

fn on_mouse(msg: u32, m: &MSLLHOOKSTRUCT) {
    let Some(sh) = SHARED.read().clone() else { return };
    let mut guard = STATE.lock();
    let Some(st) = guard.as_mut() else { return };
    let cfg = sh.cfg.read();
    let (x, y) = (Some(m.pt.x), Some(m.pt.y));
    let hi = (m.mouseData >> 16) as u16;
    let button = match msg {
        WM_LBUTTONDOWN | WM_LBUTTONUP => Some(0),
        WM_RBUTTONDOWN | WM_RBUTTONUP => Some(1),
        WM_MBUTTONDOWN | WM_MBUTTONUP => Some(2),
        WM_XBUTTONDOWN | WM_XBUTTONUP => Some(if hi == 1 { 3 } else { 4 }),
        _ => None,
    };
    if let Some(b) = button {
        let key = MOUSE_BUTTONS[b];
        let down = matches!(msg, WM_LBUTTONDOWN | WM_RBUTTONDOWN | WM_MBUTTONDOWN | WM_XBUTTONDOWN);
        if !down {
            if st.buttons.remove(key) {
                push(&sh, Event { t: 0.0, kind: Kind::MouseUp, key: key.into(), mods: Vec::new(), x, y, delta: None });
            }
            return;
        }
        if cfg.clicks && !sh.privacy.blocked() {
            st.buttons.insert(key);
            push(&sh, Event { t: 0.0, kind: Kind::MouseDown, key: key.into(), mods: held_modifiers(""), x, y, delta: None });
        }
        return;
    }
    if sh.privacy.blocked() {
        return;
    }
    match msg {
        WM_MOUSEMOVE if cfg.movement => {
            // About 60 a second is plenty for a trail.
            let now = qpc_now();
            if now - st.last_move >= crate::buffer::TICKS_PER_SEC / 60 {
                st.last_move = now;
                push(&sh, Event { t: 0.0, kind: Kind::Move, key: String::new(), mods: Vec::new(), x, y, delta: None });
            }
        }
        WM_MOUSEWHEEL if cfg.scroll => {
            let notches = (hi as i16) as i32 / 120;
            if notches != 0 {
                let key = if notches > 0 { "Wheel Up" } else { "Wheel Down" };
                push(&sh, Event { t: 0.0, kind: Kind::Scroll, key: key.into(), mods: Vec::new(), x, y, delta: Some(notches) });
            }
        }
        _ => {}
    }
}

unsafe extern "system" fn mouse_proc(code: i32, wp: WPARAM, lp: LPARAM) -> LRESULT {
    if code == HC_ACTION as i32 {
        on_mouse(wp.0 as u32, unsafe { &*(lp.0 as *const MSLLHOOKSTRUCT) });
    }
    unsafe { CallNextHookEx(None, code, wp, lp) }
}
