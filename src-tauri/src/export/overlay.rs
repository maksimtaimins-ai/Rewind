//! Draws the keystroke overlay into exported frames with Direct2D and
//! DirectWrite on a WIC bitmap (CPU, anti-aliased), sized to just the part of
//! the frame the overlay covers, then blends it onto the BGRA frame.

use std::collections::HashMap;
use std::path::Path;

use anyhow::Result;
use windows::core::{w, HSTRING};
use windows::Win32::Graphics::Direct2D::Common::*;
use windows::Win32::Graphics::Direct2D::*;
use windows::Win32::Graphics::DirectWrite::*;
use windows::Win32::Graphics::Dxgi::Common::DXGI_FORMAT_B8G8R8A8_UNORM;
use windows::Win32::Graphics::Imaging::*;
use windows::Win32::System::Com::{CoCreateInstance, CLSCTX_INPROC_SERVER};
use windows_numerics::Vector2;

use super::Rect;
use crate::input::overlay::{Cap, Scene, Timeline};
use crate::input::{read_sidecar, Screen};
use crate::settings::{ClickStyle, KeycapStyle, KeystrokeSettings, OverlayFont};

type Rgb = [f32; 3];

fn hex(c: &str, fallback: Rgb) -> Rgb {
    if !crate::settings::is_hex_color(c) {
        return fallback;
    }
    let v = |i: usize| u8::from_str_radix(&c[i..i + 2], 16).unwrap_or(0) as f32 / 255.0;
    [v(1), v(3), v(5)]
}

fn luminance(c: Rgb) -> f32 {
    0.2126 * c[0] + 0.7152 * c[1] + 0.0722 * c[2]
}

fn mix(a: Rgb, b: Rgb, k: f32) -> Rgb {
    [a[0] + (b[0] - a[0]) * k, a[1] + (b[1] - a[1]) * k, a[2] + (b[2] - a[2]) * k]
}

fn shade(c: Rgb, k: f32) -> Rgb {
    [(c[0] * k).min(1.0), (c[1] * k).min(1.0), (c[2] * k).min(1.0)]
}

const WHITE: Rgb = [1.0, 1.0, 1.0];
const BLACK: Rgb = [0.0, 0.0, 0.0];

fn rect(l: f32, t: f32, r: f32, b: f32) -> D2D_RECT_F {
    D2D_RECT_F { left: l, top: t, right: r, bottom: b }
}

fn rounded(r: D2D_RECT_F, rad: f32) -> D2D1_ROUNDED_RECT {
    let rad = rad.min((r.bottom - r.top) / 2.0).min((r.right - r.left) / 2.0).max(0.0);
    D2D1_ROUNDED_RECT { rect: r, radiusX: rad, radiusY: rad }
}

struct Target {
    bitmap: IWICBitmap,
    rt: ID2D1RenderTarget,
    brush: ID2D1SolidColorBrush,
    w: u32,
    h: u32,
}

pub struct Renderer {
    cfg: KeystrokeSettings,
    timeline: Timeline,
    screen: Screen,
    d2d: ID2D1Factory,
    dw: IDWriteFactory,
    wic: IWICImagingFactory,
    target: Option<Target>,
    /// Text formats by pixel size.
    formats: HashMap<u32, IDWriteTextFormat>,
    base: Rgb,
    text: Rgb,
    border: Rgb,
    buf: Vec<u8>,
}

impl Renderer {
    /// `None` when the clip has no recorded input.
    pub fn open(clip: &Path, cfg: &KeystrokeSettings) -> Result<Option<Self>> {
        let Some(side) = read_sidecar(clip) else { return Ok(None) };
        let timeline = Timeline::new(&side.events);
        if timeline.is_empty() {
            return Ok(None);
        }
        let base = hex(&cfg.style.base_color, [0.906, 0.906, 0.898]);
        let text = hex(&cfg.style.text_color, if luminance(base) > 0.5 { [0.09, 0.094, 0.102] } else { WHITE });
        unsafe {
            Ok(Some(Renderer {
                cfg: cfg.clone(),
                timeline,
                screen: side.screen,
                d2d: D2D1CreateFactory(D2D1_FACTORY_TYPE_SINGLE_THREADED, None)?,
                dw: DWriteCreateFactory(DWRITE_FACTORY_TYPE_SHARED)?,
                wic: CoCreateInstance(&CLSID_WICImagingFactory, None, CLSCTX_INPROC_SERVER)?,
                target: None,
                formats: HashMap::new(),
                base,
                text,
                border: hex(&cfg.style.border_color, WHITE),
                buf: Vec::new(),
            }))
        }
    }

    /// Draw the overlay at `t` seconds onto `px` (`w`x`h` BGRA). The frame
    /// came from a `src` sized video, cropped to `crop` and scaled to `w`x`h`.
    pub fn draw(&mut self, px: &mut [u8], (w, h): (u32, u32), t: f64, src: (u32, u32), crop: Rect) -> Result<()> {
        let (sw, sh) = (self.screen.width.max(1) as f32, self.screen.height.max(1) as f32);
        let map = |x: i32, y: i32| {
            let vx = x as f32 * src.0 as f32 / sw;
            let vy = y as f32 * src.1 as f32 / sh;
            ((vx - crop.0 as f32) * w as f32 / crop.2.max(1) as f32, (vy - crop.1 as f32) * h as f32 / crop.3.max(1) as f32)
        };
        let scene = self.timeline.scene_at(t, &self.cfg, (w, h), &map);
        if scene.is_empty() {
            return Ok(());
        }
        let Some((x0, y0, x1, y1)) = bounds(&scene, w, h) else { return Ok(()) };
        let (bw, bh) = (x1 - x0, y1 - y0);
        self.ensure_target(bw, bh)?;
        let (rt, bitmap) = self.target.as_ref().map(|t| (t.rt.clone(), t.bitmap.clone())).unwrap();
        unsafe {
            rt.BeginDraw();
            rt.Clear(Some(&D2D1_COLOR_F { r: 0.0, g: 0.0, b: 0.0, a: 0.0 }));
            self.paint(&scene, (x0 as f32, y0 as f32));
            rt.EndDraw(None, None)?;
            self.buf.resize(bw as usize * bh as usize * 4, 0);
            let r = WICRect { X: 0, Y: 0, Width: bw as i32, Height: bh as i32 };
            bitmap.CopyPixels(&r, bw * 4, &mut self.buf)?;
        }
        // Premultiplied source over the frame.
        for row in 0..bh as usize {
            let src_row = &self.buf[row * bw as usize * 4..(row + 1) * bw as usize * 4];
            let d = ((y0 as usize + row) * w as usize + x0 as usize) * 4;
            let dst_row = &mut px[d..d + bw as usize * 4];
            for (s, o) in src_row.chunks_exact(4).zip(dst_row.chunks_exact_mut(4)) {
                let a = s[3] as u32;
                if a == 0 {
                    continue;
                }
                for c in 0..3 {
                    o[c] = (s[c] as u32 + o[c] as u32 * (255 - a) / 255).min(255) as u8;
                }
            }
        }
        Ok(())
    }

    fn ensure_target(&mut self, w: u32, h: u32) -> Result<()> {
        if self.target.as_ref().is_some_and(|t| t.w >= w && t.h >= h) {
            return Ok(());
        }
        let (w, h) = self.target.as_ref().map(|t| (t.w.max(w), t.h.max(h))).unwrap_or((w, h));
        unsafe {
            let bitmap = self.wic.CreateBitmap(w, h, &GUID_WICPixelFormat32bppPBGRA, WICBitmapCacheOnLoad)?;
            let props = D2D1_RENDER_TARGET_PROPERTIES {
                r#type: D2D1_RENDER_TARGET_TYPE_SOFTWARE,
                pixelFormat: D2D1_PIXEL_FORMAT { format: DXGI_FORMAT_B8G8R8A8_UNORM, alphaMode: D2D1_ALPHA_MODE_PREMULTIPLIED },
                dpiX: 96.0,
                dpiY: 96.0,
                ..Default::default()
            };
            let rt = self.d2d.CreateWicBitmapRenderTarget(&bitmap, &props)?;
            rt.SetTextAntialiasMode(D2D1_TEXT_ANTIALIAS_MODE_GRAYSCALE);
            let brush = rt.CreateSolidColorBrush(&D2D1_COLOR_F { r: 1.0, g: 1.0, b: 1.0, a: 1.0 }, None)?;
            self.target = Some(Target { bitmap, rt, brush, w, h });
        }
        Ok(())
    }

    fn format(&mut self, px: f32) -> Result<IDWriteTextFormat> {
        let size = (px.round() as u32).max(6);
        if let Some(f) = self.formats.get(&size) {
            return Ok(f.clone());
        }
        let (family, weight, stretch) = match self.cfg.style.font {
            OverlayFont::Theme => (w!("Segoe UI"), DWRITE_FONT_WEIGHT_SEMI_BOLD, DWRITE_FONT_STRETCH_NORMAL),
            OverlayFont::Sans => (w!("Arial"), DWRITE_FONT_WEIGHT_BOLD, DWRITE_FONT_STRETCH_NORMAL),
            OverlayFont::Mono => (w!("Consolas"), DWRITE_FONT_WEIGHT_BOLD, DWRITE_FONT_STRETCH_NORMAL),
            OverlayFont::Condensed => (w!("Bahnschrift"), DWRITE_FONT_WEIGHT_SEMI_BOLD, DWRITE_FONT_STRETCH_CONDENSED),
        };
        let f = unsafe {
            let f = self.dw.CreateTextFormat(family, None, weight, DWRITE_FONT_STYLE_NORMAL, stretch, size as f32, w!("en-us"))?;
            f.SetTextAlignment(DWRITE_TEXT_ALIGNMENT_CENTER)?;
            f.SetParagraphAlignment(DWRITE_PARAGRAPH_ALIGNMENT_CENTER)?;
            f.SetWordWrapping(DWRITE_WORD_WRAPPING_NO_WRAP)?;
            f
        };
        self.formats.insert(size, f.clone());
        Ok(f)
    }

    fn color(&self, c: Rgb, a: f32) {
        let b = &self.target.as_ref().unwrap().brush;
        unsafe { b.SetColor(&D2D1_COLOR_F { r: c[0], g: c[1], b: c[2], a: a.clamp(0.0, 1.0) }) };
    }

    unsafe fn paint(&mut self, scene: &Scene, off: (f32, f32)) {
        let u = scene.unit;
        let rt = self.target.as_ref().unwrap().rt.clone();
        let brush = self.target.as_ref().unwrap().brush.clone();
        unsafe {
            // Trail under everything else.
            for pair in scene.trail.windows(2) {
                let (a, b) = (pair[0], pair[1]);
                self.color(self.base, b.2 * 0.8);
                rt.DrawLine(Vector2 { X: a.0 - off.0, Y: a.1 - off.1 }, Vector2 { X: b.0 - off.0, Y: b.1 - off.1 }, &brush, (u * 0.09).max(1.5), None);
            }
            for c in &scene.caps {
                let _ = self.cap(c, u, off);
            }
            for &(x, y, a) in &scene.pluses {
                if let Ok(f) = self.format(u * 0.5) {
                    let r = rect(x - off.0 - u * 0.25, y - off.1 - u * 0.5, x - off.0 + u * 0.25, y - off.1 + u * 0.5);
                    self.color(BLACK, a * 0.35);
                    let shadow = rect(r.left, r.top + u * 0.03, r.right, r.bottom + u * 0.03);
                    rt.DrawText(&HSTRING::from("+"), &f, &shadow, &brush, D2D1_DRAW_TEXT_OPTIONS_NONE, DWRITE_MEASURING_MODE_NATURAL);
                    self.color(self.base, a);
                    rt.DrawText(&HSTRING::from("+"), &f, &r, &brush, D2D1_DRAW_TEXT_OPTIONS_NONE, DWRITE_MEASURING_MODE_NATURAL);
                }
            }
            for c in &scene.clicks {
                let col = if c.secondary { mix(self.base, WHITE, 0.5) } else { self.base };
                let p = Vector2 { X: c.x - off.0, Y: c.y - off.1 };
                match self.cfg.position.click_style {
                    ClickStyle::Dot => {
                        self.color(col, c.opacity * 0.9);
                        let r = u * 0.22;
                        rt.FillEllipse(&D2D1_ELLIPSE { point: p, radiusX: r, radiusY: r }, &brush);
                    }
                    ClickStyle::Ring => {
                        let r = u * 0.18 + c.age.min(0.4) * u * 1.5;
                        self.color(col, c.opacity * (1.0 - (c.age / 0.6).min(1.0) * 0.5));
                        rt.DrawEllipse(&D2D1_ELLIPSE { point: p, radiusX: r, radiusY: r }, &brush, (u * 0.08).max(1.5), None);
                    }
                    ClickStyle::Flash => {
                        let k = (1.0 - c.age / 0.35).max(0.0);
                        self.color(WHITE, 0.4 * c.opacity * k);
                        let r = u * 0.8;
                        rt.FillEllipse(&D2D1_ELLIPSE { point: p, radiusX: r, radiusY: r }, &brush);
                        self.color(col, c.opacity);
                        rt.FillEllipse(&D2D1_ELLIPSE { point: p, radiusX: u * 0.12, radiusY: u * 0.12 }, &brush);
                    }
                }
            }
        }
    }

    unsafe fn cap(&mut self, c: &Cap, u: f32, off: (f32, f32)) -> Result<()> {
        let st = self.cfg.style.clone();
        let rt = self.target.as_ref().unwrap().rt.clone();
        let brush = self.target.as_ref().unwrap().brush.clone();
        let k = u / st.size.max(1) as f32;
        let (cw, ch) = (c.w * c.scale, c.h * c.scale);
        let (cx, cy) = (c.x + c.w / 2.0 - off.0, c.y + c.h / 2.0 - off.1);
        let r = rect(cx - cw / 2.0, cy - ch / 2.0, cx + cw / 2.0, cy + ch / 2.0);
        let rad = st.radius as f32 * k * c.scale * (ch / c.h.max(1.0));
        let a = c.opacity;
        if a <= 0.004 {
            return Ok(());
        }
        let fill = mix(self.base, WHITE, c.flash * 0.7);
        // Released keys are outlined when the hold indicator is on.
        let outlined = st.keycap == KeycapStyle::Outlined || (st.hold_indicator && !c.held && st.keycap != KeycapStyle::Minimal);
        unsafe {
            if st.shadow && st.keycap != KeycapStyle::Minimal {
                for i in 0..3 {
                    let s = (i + 1) as f32 * ch * 0.035;
                    self.color(BLACK, st.shadow_intensity as f32 / 100.0 * 0.35 * a / (i + 1) as f32);
                    let sr = rect(r.left - s, r.top - s + ch * 0.07, r.right + s, r.bottom + s + ch * 0.07);
                    rt.FillRoundedRectangle(&rounded(sr, rad + s), &brush);
                }
            }
            let mut face = r;
            let mut text_col = self.text;
            if outlined {
                self.color(BLACK, 0.25 * a);
                rt.FillRoundedRectangle(&rounded(r, rad), &brush);
                self.color(fill, a);
                let sw = (ch * 0.07).max(1.5);
                let inner = rect(r.left + sw / 2.0, r.top + sw / 2.0, r.right - sw / 2.0, r.bottom - sw / 2.0);
                rt.DrawRoundedRectangle(&rounded(inner, rad), &brush, sw, None);
                if st.text_color.is_empty() {
                    text_col = fill;
                }
            } else {
                match st.keycap {
                    KeycapStyle::Flat => {
                        self.color(fill, a);
                        rt.FillRoundedRectangle(&rounded(r, rad), &brush);
                    }
                    KeycapStyle::ThreeD => {
                        let full = ch * 0.1;
                        let depth = if c.held { full * 0.35 } else { full };
                        self.color(shade(fill, 0.55), a);
                        rt.FillRoundedRectangle(&rounded(r, rad), &brush);
                        face = rect(r.left, r.top + (full - depth), r.right, r.bottom - depth);
                        self.color(fill, a);
                        rt.FillRoundedRectangle(&rounded(face, rad), &brush);
                    }
                    KeycapStyle::Mechanical => {
                        self.color(shade(fill, 0.62), a);
                        rt.FillRoundedRectangle(&rounded(r, rad), &brush);
                        let press = if c.held { ch * 0.05 } else { 0.0 };
                        face = rect(r.left + ch * 0.1, r.top + ch * 0.05 + press, r.right - ch * 0.1, r.bottom - ch * 0.17 + press);
                        self.color(shade(fill, 1.12), a);
                        rt.FillRoundedRectangle(&rounded(face, rad * 0.75), &brush);
                    }
                    KeycapStyle::Minimal => {
                        self.color(BLACK, 0.45 * a);
                        rt.FillRoundedRectangle(&rounded(r, rad), &brush);
                        if st.text_color.is_empty() {
                            text_col = WHITE;
                        }
                    }
                    KeycapStyle::Outlined => unreachable!(),
                }
            }
            if st.border {
                self.color(self.border, a);
                let bw = (ch * 0.05).max(1.0);
                let inner = rect(r.left + bw / 2.0, r.top + bw / 2.0, r.right - bw / 2.0, r.bottom - bw / 2.0);
                rt.DrawRoundedRectangle(&rounded(inner, rad), &brush, bw, None);
            }
            let f = self.format((face.bottom - face.top) * 0.46)?;
            self.color(text_col, a);
            rt.DrawText(&HSTRING::from(c.label.as_str()), &f, &face, &brush, D2D1_DRAW_TEXT_OPTIONS_CLIP, DWRITE_MEASURING_MODE_NATURAL);
        }
        Ok(())
    }
}

/// Pixel box covering everything in the scene (with room for shadows and
/// press animations), clipped to the frame.
fn bounds(s: &Scene, w: u32, h: u32) -> Option<(u32, u32, u32, u32)> {
    let u = s.unit;
    let mut b = (f32::MAX, f32::MAX, f32::MIN, f32::MIN);
    let mut add = |l: f32, t: f32, r: f32, btm: f32| {
        b = (b.0.min(l), b.1.min(t), b.2.max(r), b.3.max(btm));
    };
    for c in &s.caps {
        let pad = u * 0.35 + c.w.max(c.h) * (c.scale - 1.0).max(0.0);
        add(c.x - pad, c.y - pad, c.x + c.w + pad, c.y + c.h + pad);
    }
    for &(x, y, _) in &s.pluses {
        add(x - u, y - u, x + u, y + u);
    }
    for c in &s.clicks {
        add(c.x - u * 1.3, c.y - u * 1.3, c.x + u * 1.3, c.y + u * 1.3);
    }
    for &(x, y, _) in &s.trail {
        add(x - u * 0.2, y - u * 0.2, x + u * 0.2, y + u * 0.2);
    }
    let x0 = b.0.floor().max(0.0) as u32;
    let y0 = b.1.floor().max(0.0) as u32;
    let x1 = (b.2.ceil().max(0.0) as u32).min(w);
    let y1 = (b.3.ceil().max(0.0) as u32).min(h);
    (x1 > x0 && y1 > y0).then_some((x0, y0, x1, y1))
}

/// Ignore errors per frame: a missing font must not fail the whole export.
pub fn draw_or_warn(r: &mut Option<Renderer>, px: &mut [u8], size: (u32, u32), t: f64, src: (u32, u32), crop: Rect) {
    if let Some(rr) = r.as_mut() {
        if let Err(e) = rr.draw(px, size, t, src, crop) {
            tracing::warn!("keystroke overlay: {e:#}; exporting without it");
            *r = None;
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::input::{write_sidecar, Event, Kind};
    use crate::settings::{PressAnimation, Settings};

    /// Draws every keycap style onto a black frame and checks that keycaps,
    /// a click ring and nothing else appeared where expected.
    #[test]
    fn renders_onto_frame() {
        crate::platform::com_init();
        let dir = std::env::temp_dir().join(format!("rewind-overlay-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        let clip = dir.join("c.mp4");
        let events = vec![
            Event::key(0.5, Kind::KeyDown, "P", vec!["Ctrl".into()]),
            Event { t: 0.6, kind: Kind::MouseDown, key: "LMB".into(), mods: vec![], x: Some(1280), y: Some(720), delta: None },
        ];
        write_sidecar(&clip, Screen { width: 2560, height: 1440 }, events).unwrap();
        for style in [KeycapStyle::Flat, KeycapStyle::ThreeD, KeycapStyle::Minimal, KeycapStyle::Outlined, KeycapStyle::Mechanical] {
            let mut cfg = Settings::default().keystrokes;
            cfg.style.keycap = style;
            cfg.style.border = true;
            cfg.style.animation = PressAnimation::Spring;
            cfg.style.base_color = "#4488ff".into();
            let mut r = Renderer::open(&clip, &cfg).unwrap().expect("sidecar");
            let (w, h) = (640u32, 360u32);
            let mut px = vec![0u8; (w * h * 4) as usize];
            r.draw(&mut px, (w, h), 0.7, (1920, 1080), (0, 0, 1920, 1080)).unwrap();
            // Set REWIND_OVERLAY_PNG=<dir> to look at the result.
            if let Some(out) = std::env::var_os("REWIND_OVERLAY_PNG") {
                let path = std::path::Path::new(&out).join(format!("{style:?}.png"));
                crate::platform::write_image(&path, w, h, &px, crate::platform::ImageFormat::Png).unwrap();
            }
            let lit = |x: u32, y: u32| px[((y * w + x) * 4) as usize..][..3].iter().any(|&c| c > 0);
            // Keys: bottom-left; ring: around the centre; top-right untouched.
            assert!((0..w / 3).any(|x| (h * 3 / 4..h).any(|y| lit(x, y))), "{style:?}: no keycaps");
            assert!((w / 2 - 20..w / 2 + 20).any(|x| (h / 2 - 20..h / 2 + 20).any(|y| lit(x, y))), "{style:?}: no click");
            assert!(!(w - 60..w).any(|x| (0..60).any(|y| lit(x, y))), "{style:?}: stray pixels");
        }
        // Without a sidecar there's nothing to draw.
        assert!(Renderer::open(&dir.join("none.mp4"), &Settings::default().keystrokes).unwrap().is_none());
        std::fs::remove_dir_all(&dir).ok();
    }
}
