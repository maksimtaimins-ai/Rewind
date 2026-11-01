//! What the keystroke overlay shows at a moment in a clip: keycaps, combo
//! plus signs, click markers and the pointer trail, laid out in output-frame
//! pixels. Drawing is `export::overlay`'s job; this part is plain math.

use crate::settings::{ClickPosition, ComboStyle, KeyFilterMode, KeystrokeSettings, OverlayAnchor, PressAnimation};

use super::{is_modifier, Event, Kind, MODIFIERS};

#[derive(Debug, Clone, PartialEq)]
pub struct Cap {
    pub label: String,
    pub x: f32,
    pub y: f32,
    pub w: f32,
    pub h: f32,
    /// 0-1.
    pub opacity: f32,
    /// Press animation, 1 = at rest. Applied around the cap's centre.
    pub scale: f32,
    /// 0-1 towards white.
    pub flash: f32,
    /// Down right now (drawn filled when the hold indicator is on).
    pub held: bool,
}

#[derive(Debug, Clone, PartialEq)]
pub struct Click {
    pub x: f32,
    pub y: f32,
    /// Seconds since the press.
    pub age: f32,
    pub opacity: f32,
    /// Not the left button.
    pub secondary: bool,
}

#[derive(Debug, Clone, Default, PartialEq)]
pub struct Scene {
    /// Keycap height in pixels; everything else is sized from it.
    pub unit: f32,
    pub caps: Vec<Cap>,
    /// Centres of `+` signs between chained combo keys, with opacity.
    pub pluses: Vec<(f32, f32, f32)>,
    pub clicks: Vec<Click>,
    /// Pointer path, oldest first, with opacity.
    pub trail: Vec<(f32, f32, f32)>,
}

impl Scene {
    pub fn is_empty(&self) -> bool {
        self.caps.is_empty() && self.clicks.is_empty() && self.trail.len() < 2
    }
}

#[derive(Debug, Clone)]
struct Press {
    key: String,
    mods: Vec<String>,
    down: f64,
    up: f64,
    mouse: bool,
    pos: Option<(i32, i32)>,
    /// A modifier held for a combo: shown inside it, not on its own.
    in_combo: bool,
}

/// Keys shown at most at once in history mode.
const HISTORY: usize = 6;
/// Seconds a click marker lasts after release.
const CLICK_SECS: f64 = 0.5;

pub struct Timeline {
    presses: Vec<Press>,
    moves: Vec<(f64, i32, i32)>,
}

impl Timeline {
    pub fn new(events: &[Event]) -> Self {
        let mut presses: Vec<Press> = Vec::new();
        let mut open: Vec<(String, usize)> = Vec::new();
        let mut moves = Vec::new();
        let mut sorted: Vec<&Event> = events.iter().collect();
        sorted.sort_by(|a, b| a.t.total_cmp(&b.t));
        for e in sorted {
            let pos = e.x.zip(e.y);
            match e.kind {
                Kind::KeyDown | Kind::MouseDown => {
                    open.push((e.key.clone(), presses.len()));
                    presses.push(Press { key: e.key.clone(), mods: e.mods.clone(), down: e.t, up: f64::INFINITY, mouse: e.kind == Kind::MouseDown, pos, in_combo: false });
                }
                Kind::KeyUp | Kind::MouseUp => {
                    if let Some(i) = open.iter().position(|(k, _)| *k == e.key) {
                        let (_, idx) = open.remove(i);
                        presses[idx].up = e.t;
                    }
                }
                Kind::Scroll => presses.push(Press { key: e.key.clone(), mods: e.mods.clone(), down: e.t, up: e.t + 0.15, mouse: false, pos, in_combo: false }),
                Kind::Move => {
                    if let Some((x, y)) = pos {
                        moves.push((e.t, x, y));
                    }
                }
            }
        }
        for i in 0..presses.len() {
            if is_modifier(&presses[i].key) {
                let (d, u) = (presses[i].down, presses[i].up);
                presses[i].in_combo = presses.iter().any(|q| !q.mouse && !is_modifier(&q.key) && q.down >= d && q.down <= u);
            }
        }
        Timeline { presses, moves }
    }

    pub fn is_empty(&self) -> bool {
        self.presses.is_empty() && self.moves.is_empty()
    }

    /// The overlay at `t` seconds into the clip, for a `w`x`h` frame.
    /// `map` turns recorded screen pixels into frame pixels.
    pub fn scene_at(&self, t: f64, cfg: &KeystrokeSettings, (w, h): (u32, u32), map: &dyn Fn(i32, i32) -> (f32, f32)) -> Scene {
        let st = &cfg.style;
        let k = h as f32 / 1080.0;
        let u = st.size as f32 * k;
        let fade = st.fade_secs as f64;
        let pressed = st.pressed_opacity as f32 / 100.0;
        let mut scene = Scene { unit: u, ..Default::default() };
        let mouse_caps = cfg.position.click_position == ClickPosition::WithKeys;

        if cfg.filter == KeyFilterMode::Allow && !cfg.keys.is_empty() {
            self.fixed_layout(t, cfg, u, &mut scene, mouse_caps);
        } else {
            self.history_layout(t, cfg, u, fade, pressed, &mut scene, mouse_caps);
        }
        place(&mut scene, cfg, (w as f32, h as f32), k);

        if !mouse_caps {
            for p in self.presses.iter().filter(|p| p.mouse && p.down <= t) {
                let end = p.up.max(p.down) + CLICK_SECS;
                let Some((x, y)) = p.pos else { continue };
                if t >= end {
                    continue;
                }
                let opacity = if t < p.up { 1.0 } else { (1.0 - (t - p.up) / CLICK_SECS) as f32 };
                let (fx, fy) = map(x, y);
                scene.clicks.push(Click { x: fx, y: fy, age: (t - p.down) as f32, opacity, secondary: p.key != "LMB" });
            }
        }
        if cfg.position.trail {
            let span = cfg.position.trail_ms as f64 / 1000.0;
            for &(mt, x, y) in self.moves.iter().filter(|(mt, _, _)| *mt <= t && t - *mt < span) {
                let (fx, fy) = map(x, y);
                scene.trail.push((fx, fy, (1.0 - (t - mt) / span) as f32));
            }
        }
        scene
    }

    fn history_layout(&self, t: f64, cfg: &KeystrokeSettings, u: f32, fade: f64, pressed: f32, scene: &mut Scene, mouse_caps: bool) {
        let st = &cfg.style;
        let visible: Vec<&Press> = self
            .presses
            .iter()
            .filter(|p| !p.in_combo && (!p.mouse || mouse_caps) && p.down <= t && t < p.up + fade)
            .collect();
        let start = visible.len().saturating_sub(HISTORY);
        let mut x = 0.0f32;
        let small = u * 0.62;
        let stack_gap = u * 0.12;
        let groups: Vec<(&Press, Vec<String>)> = visible[start..]
            .iter()
            .map(|p| {
                let mods: Vec<String> = MODIFIERS.iter().filter(|m| p.mods.iter().any(|x| x == *m) && **m != p.key).map(|m| m.to_string()).collect();
                (*p, mods)
            })
            .collect();
        let tallest = groups.iter().map(|(_, m)| m.len()).max().unwrap_or(0) as f32;
        let base_y = if st.combo == ComboStyle::Stack { tallest * (small + stack_gap) } else { 0.0 };
        for (gi, (p, mods)) in groups.iter().enumerate() {
            let held = t < p.up;
            let opacity = if held { pressed } else { pressed * (1.0 - ((t - p.up) / fade) as f32).max(0.0) };
            let (scale, flash) = animate(st.animation, t - p.down, held);
            let cap = |label: &str, x: f32, y: f32, w: f32, h: f32| Cap { label: label.into(), x, y, w, h, opacity, scale, flash, held };
            match st.combo {
                ComboStyle::Chain => {
                    let labels: Vec<&str> = mods.iter().map(String::as_str).chain([p.key.as_str()]).collect();
                    for (i, l) in labels.iter().enumerate() {
                        let cw = cap_width(l, u);
                        scene.caps.push(cap(l, x, 0.0, cw, u));
                        x += cw;
                        if i + 1 < labels.len() {
                            scene.pluses.push((x + u * 0.25, u * 0.5, opacity));
                            x += u * 0.5;
                        }
                    }
                }
                ComboStyle::Stack => {
                    let main_w = cap_width(&p.key, u);
                    let gw = mods.iter().map(|m| cap_width(m, small)).fold(main_w, f32::max);
                    scene.caps.push(cap(&p.key, x, base_y, gw, u));
                    for (i, m) in mods.iter().enumerate() {
                        let y = base_y - (i as f32 + 1.0) * (small + stack_gap);
                        scene.caps.push(cap(m, x, y, gw, small));
                    }
                    x += gw;
                }
            }
            if gi + 1 < groups.len() {
                x += u * 0.4;
            }
        }
    }

    fn fixed_layout(&self, t: f64, cfg: &KeystrokeSettings, u: f32, scene: &mut Scene, mouse_caps: bool) {
        let st = &cfg.style;
        let fade = st.fade_secs as f64;
        let pressed = st.pressed_opacity as f32 / 100.0;
        let idle = st.idle_opacity as f32 / 100.0;
        let pitch = u * 1.12;
        let mut keys: Vec<String> = cfg.keys.clone();
        if mouse_caps && cfg.clicks {
            keys.extend(["LMB", "MMB", "RMB"].map(String::from));
        }
        // Keys without a spot on the board go in a row underneath.
        let mut spots: Vec<(String, f32, f32, f32)> = Vec::new();
        let mut extra = 0.0f32;
        let mut unplaced = Vec::new();
        for key in &keys {
            match board_spot(key) {
                Some((x, y, w)) => spots.push((key.clone(), x, y, w)),
                None => unplaced.push(key.clone()),
            }
        }
        let bottom = spots.iter().map(|s| s.2).fold(f32::MIN, f32::max);
        let left = spots.iter().map(|s| s.1).fold(f32::MAX, f32::min);
        for key in unplaced {
            let w = (cap_width(&key, u) / pitch).max(1.0);
            spots.push((key, if left == f32::MAX { 0.0 } else { left } + extra, if bottom == f32::MIN { 0.0 } else { bottom + 1.0 }, w));
            extra += w;
        }
        let min_x = spots.iter().map(|s| s.1).fold(f32::MAX, f32::min);
        let min_y = spots.iter().map(|s| s.2).fold(f32::MAX, f32::min);
        for (key, x, y, w) in spots {
            let last = self.presses.iter().filter(|p| p.key == key && p.down <= t).last();
            let (opacity, scale, flash, held) = match last {
                Some(p) if t < p.up => {
                    let (s, f) = animate(st.animation, t - p.down, true);
                    (pressed, s, f, true)
                }
                Some(p) if t - p.up < fade => {
                    let r = ((t - p.up) / fade) as f32;
                    let (s, f) = animate(st.animation, t - p.down, false);
                    (pressed + (idle - pressed) * r, s, f, false)
                }
                _ => (idle, 1.0, 0.0, false),
            };
            scene.caps.push(Cap { label: key, x: (x - min_x) * pitch, y: (y - min_y) * pitch, w: w * pitch - (pitch - u), h: u, opacity, scale, flash, held });
        }
    }
}

/// Keycap width for a label: square for one character, wider for words.
pub fn cap_width(label: &str, u: f32) -> f32 {
    if label == "Space" {
        return u * 2.6;
    }
    (label.chars().count() as f32 * 0.25 + 0.55).max(1.0) * u
}

/// (scale, flash) for a key pressed `d` seconds ago.
fn animate(a: PressAnimation, d: f64, held: bool) -> (f32, f32) {
    let d = d.max(0.0);
    match a {
        PressAnimation::None => (1.0, 0.0),
        PressAnimation::Pop => (1.0 + 0.14 * (1.0 - d / 0.15).max(0.0) as f32, 0.0),
        PressAnimation::Flash => (1.0, (1.0 - d / 0.3).max(0.0) as f32),
        PressAnimation::Spring => ((1.0 - 0.16 * (-9.0 * d).exp() * (28.0 * d).cos()) as f32, 0.0),
        PressAnimation::Pulse if held => ((1.0 + 0.05 * (d * std::f64::consts::TAU * 2.5).sin()) as f32, 0.0),
        PressAnimation::Pulse => (1.0, 0.0),
    }
}

/// Where a key sits on a US keyboard: (x, row, width) in key units.
fn board_spot(key: &str) -> Option<(f32, f32, f32)> {
    let rows: [(f32, &[(&str, f32)]); 6] = [
        (0.0, &[("Esc", 1.0), ("", 1.0), ("F1", 1.0), ("F2", 1.0), ("F3", 1.0), ("F4", 1.0), ("", 0.5), ("F5", 1.0), ("F6", 1.0), ("F7", 1.0), ("F8", 1.0), ("", 0.5), ("F9", 1.0), ("F10", 1.0), ("F11", 1.0), ("F12", 1.0)]),
        (1.25, &[("`", 1.0), ("1", 1.0), ("2", 1.0), ("3", 1.0), ("4", 1.0), ("5", 1.0), ("6", 1.0), ("7", 1.0), ("8", 1.0), ("9", 1.0), ("0", 1.0), ("-", 1.0), ("=", 1.0), ("Backspace", 2.0)]),
        (2.25, &[("Tab", 1.5), ("Q", 1.0), ("W", 1.0), ("E", 1.0), ("R", 1.0), ("T", 1.0), ("Y", 1.0), ("U", 1.0), ("I", 1.0), ("O", 1.0), ("P", 1.0), ("[", 1.0), ("]", 1.0), ("\\", 1.5)]),
        (3.25, &[("Caps", 1.75), ("A", 1.0), ("S", 1.0), ("D", 1.0), ("F", 1.0), ("G", 1.0), ("H", 1.0), ("J", 1.0), ("K", 1.0), ("L", 1.0), (";", 1.0), ("'", 1.0), ("Enter", 2.25)]),
        (4.25, &[("Shift", 2.25), ("Z", 1.0), ("X", 1.0), ("C", 1.0), ("V", 1.0), ("B", 1.0), ("N", 1.0), ("M", 1.0), (",", 1.0), (".", 1.0), ("/", 1.0)]),
        (5.25, &[("Ctrl", 1.5), ("Win", 1.25), ("Alt", 1.25), ("Space", 6.25)]),
    ];
    for (y, keys) in rows {
        let mut x = 0.0;
        for &(k, w) in keys {
            if !k.is_empty() && k.eq_ignore_ascii_case(key) {
                return Some((x, y, w));
            }
            x += w;
        }
    }
    let side: &[(&str, f32, f32)] = &[
        ("Insert", 15.25, 1.25),
        ("Home", 16.25, 1.25),
        ("PgUp", 17.25, 1.25),
        ("Delete", 15.25, 2.25),
        ("End", 16.25, 2.25),
        ("PgDn", 17.25, 2.25),
        ("Up", 16.25, 4.25),
        ("Left", 15.25, 5.25),
        ("Down", 16.25, 5.25),
        ("Right", 17.25, 5.25),
        ("LMB", 18.75, 4.25),
        ("MMB", 19.75, 4.25),
        ("RMB", 20.75, 4.25),
    ];
    side.iter().find(|(k, _, _)| k.eq_ignore_ascii_case(key)).map(|&(_, x, y)| (x, y, 1.0))
}

/// Move the laid-out caps to their anchor in the frame.
fn place(scene: &mut Scene, cfg: &KeystrokeSettings, (fw, fh): (f32, f32), k: f32) {
    if scene.caps.is_empty() {
        return;
    }
    let min_x = scene.caps.iter().map(|c| c.x).fold(f32::MAX, f32::min);
    let min_y = scene.caps.iter().map(|c| c.y).fold(f32::MAX, f32::min);
    let bw = scene.caps.iter().map(|c| c.x + c.w).fold(f32::MIN, f32::max) - min_x;
    let bh = scene.caps.iter().map(|c| c.y + c.h).fold(f32::MIN, f32::max) - min_y;
    let m = cfg.position.margin as f32 * k;
    let (x, y) = match cfg.position.anchor {
        OverlayAnchor::TopLeft => (m, m),
        OverlayAnchor::TopRight => (fw - bw - m, m),
        OverlayAnchor::BottomLeft => (m, fh - bh - m),
        OverlayAnchor::BottomRight => (fw - bw - m, fh - bh - m),
        OverlayAnchor::TopCenter => ((fw - bw) / 2.0, m),
        OverlayAnchor::BottomCenter => ((fw - bw) / 2.0, fh - bh - m),
        OverlayAnchor::Custom => (cfg.position.x * fw, cfg.position.y * fh),
    };
    let x = x.clamp(0.0, (fw - bw).max(0.0)) - min_x;
    let y = y.clamp(0.0, (fh - bh).max(0.0)) - min_y;
    for c in &mut scene.caps {
        c.x += x;
        c.y += y;
    }
    for p in &mut scene.pluses {
        p.0 += x;
        p.1 += y;
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::settings::Settings;

    fn cfg() -> KeystrokeSettings {
        Settings::default().keystrokes
    }

    const FRAME: (u32, u32) = (1920, 1080);

    fn same(x: i32, y: i32) -> (f32, f32) {
        (x as f32, y as f32)
    }

    fn down(t: f64, k: &str, mods: &[&str]) -> Event {
        Event::key(t, Kind::KeyDown, k, mods.iter().map(|m| m.to_string()).collect())
    }

    fn up(t: f64, k: &str) -> Event {
        Event::key(t, Kind::KeyUp, k, vec![])
    }

    #[test]
    fn chain_combo_hides_its_modifiers() {
        let ev = [down(1.0, "Ctrl", &[]), down(1.1, "Shift", &["Ctrl"]), down(1.2, "P", &["Ctrl", "Shift"]), up(1.3, "P"), up(1.4, "Shift"), up(1.5, "Ctrl")];
        let tl = Timeline::new(&ev);
        let s = tl.scene_at(1.25, &cfg(), FRAME, &same);
        assert_eq!(s.caps.iter().map(|c| c.label.as_str()).collect::<Vec<_>>(), ["Ctrl", "Shift", "P"]);
        assert_eq!(s.pluses.len(), 2);
        assert!(s.caps.iter().all(|c| c.held));
        // Fading after release, gone once the fade is over.
        let s = tl.scene_at(2.3, &cfg(), FRAME, &same);
        assert!(s.caps[0].opacity > 0.0 && s.caps[0].opacity < 1.0 && !s.caps[0].held);
        assert!(tl.scene_at(3.4, &cfg(), FRAME, &same).caps.is_empty());
    }

    #[test]
    fn stack_puts_modifiers_above() {
        let mut c = cfg();
        c.style.combo = ComboStyle::Stack;
        let tl = Timeline::new(&[down(0.0, "P", &["Ctrl"])]);
        let s = tl.scene_at(0.5, &c, FRAME, &same);
        let (p, ctrl) = (&s.caps[0], &s.caps[1]);
        assert_eq!((p.label.as_str(), ctrl.label.as_str()), ("P", "Ctrl"));
        assert!(ctrl.y + ctrl.h <= p.y && ctrl.h < p.h);
    }

    #[test]
    fn solo_modifier_still_shows() {
        let tl = Timeline::new(&[down(0.0, "Shift", &[]), up(0.5, "Shift")]);
        assert_eq!(tl.scene_at(0.2, &cfg(), FRAME, &same).caps[0].label, "Shift");
    }

    #[test]
    fn history_keeps_last_six() {
        let ev: Vec<Event> = (0..10).map(|i| down(i as f64 * 0.1, &i.to_string(), &[])).collect();
        let s = Timeline::new(&ev).scene_at(1.0, &cfg(), FRAME, &same);
        assert_eq!(s.caps.len(), HISTORY);
        assert_eq!(s.caps.last().unwrap().label, "9");
    }

    #[test]
    fn wasd_is_a_cluster_at_rest() {
        let mut c = cfg();
        crate::input::apply_preset(&mut c, "wasd");
        let s = Timeline::new(&[down(1.0, "W", &[])]).scene_at(1.05, &c, FRAME, &same);
        let get = |k: &str| s.caps.iter().find(|c| c.label == k).unwrap().clone();
        let (w, a, sk, d) = (get("W"), get("A"), get("S"), get("D"));
        assert!(w.y < sk.y && a.x < sk.x && sk.x < d.x && w.x > a.x && w.x < sk.x + sk.w);
        assert!(w.held && w.opacity == 1.0);
        assert!((a.opacity - 0.3).abs() < 1e-6 && !a.held);
        // Bottom-left anchor with the default 40 px margin.
        let bottom = s.caps.iter().map(|c| c.y + c.h).fold(0.0, f32::max);
        assert!((bottom - 1040.0).abs() < 0.5 && (a.x - 40.0).abs() < 0.5, "{bottom} {}", a.x);
    }

    #[test]
    fn anchors_keep_overlay_inside() {
        let mut c = cfg();
        c.position.anchor = OverlayAnchor::BottomRight;
        let s = Timeline::new(&[down(0.0, "Wheel Down", &[])]).scene_at(0.1, &c, (1280, 720), &same);
        let cap = &s.caps[0];
        assert!(cap.x + cap.w <= 1280.0 && cap.y + cap.h <= 720.0 && cap.x > 640.0);
        assert!((s.unit - 44.0 * 720.0 / 1080.0).abs() < 1e-3);
        c.position.anchor = OverlayAnchor::Custom;
        c.position.x = 1.0;
        let s = Timeline::new(&[down(0.0, "Q", &[])]).scene_at(0.1, &c, (1280, 720), &same);
        assert!(s.caps[0].x + s.caps[0].w <= 1280.0);
    }

    #[test]
    fn clicks_follow_cursor_and_trail_fades() {
        let mut c = cfg();
        c.position.trail = true;
        let ev = [
            Event { t: 1.0, kind: Kind::MouseDown, key: "LMB".into(), mods: vec![], x: Some(100), y: Some(50), delta: None },
            Event { t: 1.1, kind: Kind::MouseUp, key: "LMB".into(), mods: vec![], x: Some(100), y: Some(50), delta: None },
            Event { t: 1.0, kind: Kind::Move, key: String::new(), mods: vec![], x: Some(0), y: Some(0), delta: None },
            Event { t: 1.2, kind: Kind::Move, key: String::new(), mods: vec![], x: Some(10), y: Some(0), delta: None },
        ];
        let half = |x: i32, y: i32| (x as f32 / 2.0, y as f32 / 2.0);
        let s = Timeline::new(&ev).scene_at(1.3, &c, FRAME, &half);
        assert_eq!((s.clicks[0].x, s.clicks[0].y, s.clicks[0].secondary), (50.0, 25.0, false));
        assert!(s.caps.is_empty());
        assert_eq!(s.trail.len(), 2);
        assert!(s.trail[0].2 < s.trail[1].2);
        assert!(Timeline::new(&ev).scene_at(1.7, &c, FRAME, &half).clicks.is_empty());
        // Drawn as a keycap instead when asked.
        c.position.click_position = ClickPosition::WithKeys;
        let s = Timeline::new(&ev).scene_at(1.05, &c, FRAME, &half);
        assert!(s.clicks.is_empty() && s.caps[0].label == "LMB");
    }

    #[test]
    fn animations_settle() {
        for a in [PressAnimation::Pop, PressAnimation::Flash, PressAnimation::Spring] {
            let (s, f) = animate(a, 2.0, false);
            assert!((s - 1.0).abs() < 0.01 && f == 0.0, "{a:?}");
        }
        assert!(animate(PressAnimation::Pop, 0.0, true).0 > 1.1);
        assert_eq!(animate(PressAnimation::Flash, 0.0, true).1, 1.0);
    }
}
