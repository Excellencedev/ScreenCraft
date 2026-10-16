//! Everything painted on the CPU over the recording: device frames, annotations, the shortcut
//! display, the watermark and captions. A port of `overlays.js`, drawn with tiny-skia into
//! premultiplied RGBA layers that the GPU compositor blends in. The functions are pure, so the
//! preview and the exporter produce the same pixels.

use crate::skia::{self, paint, with_alpha};
use crate::text::{fill_text_path, Font};
use crate::wallpapers::parse_color;
use screencraft_core::project::{Annotation, Project};
use screencraft_core::track::ComboMark;
use std::collections::HashMap;
use std::sync::{Arc, Mutex, OnceLock};
use tiny_skia::{FillRule, LineCap, LineJoin, Mask, PathBuilder, Pixmap, PixmapPaint, Stroke, Transform};

pub type R = [f32; 4];

/// An on-canvas box of an annotation (for the editor's drag handles).
#[derive(Clone, Debug)]
pub struct Bound {
    pub id: String,
    pub rect: R,
    pub arrow: Option<[f32; 4]>,
}

/// Region effects the shader applies to the screen picture.
#[derive(Clone, Copy, Debug)]
pub struct Effect {
    pub rect: R,
    /// 1 = blur, 2 = pixelate.
    pub kind: u32,
    pub cell_px: f32,
    pub radius_px: f32,
}

#[derive(Clone, Default)]
pub struct Layers {
    /// Device frame, drawn before the screen.
    pub under: Option<Arc<Pixmap>>,
    /// Content-space annotations, drawn over the screen and under the cursor.
    pub content: Option<Arc<Pixmap>>,
    /// Frame-space annotations, watermark, shortcuts and captions.
    pub top: Option<Arc<Pixmap>>,
    pub bounds: Vec<Bound>,
    pub effects: Vec<Effect>,
}

impl std::fmt::Debug for Layers {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("Layers").field("under", &self.under.is_some()).field("content", &self.content.is_some()).field("top", &self.top.is_some()).finish()
    }
}

/// The geometry annotations need.
pub struct Geom {
    pub w: f32,
    pub h: f32,
    /// Where the screen is drawn.
    pub sr: R,
    /// The plain content rectangle (before any camera layout moved the screen).
    pub content_r: R,
    /// Visible part of the recording, normalised: x, y, w, h.
    pub view: R,
    pub zoom: f32,
    pub screen_radius: f32,
    pub t_src: f64,
    pub t_out_ms: f64,
    pub screen_alpha: f32,
}

fn ease(t: f32) -> f32 {
    t * t * (3.0 - 2.0 * t)
}

fn c01(v: f32) -> f32 {
    v.clamp(0.0, 1.0)
}

fn anim_k(a: &Annotation, t: f64) -> f32 {
    if a.anim == "none" {
        return 1.0;
    }
    let ms = a.anim_ms.max(1.0);
    ease(c01(((t - a.source_start_ms) / ms).min((a.source_end_ms - t) / ms) as f32))
}

fn pm(w: f32, h: f32) -> Option<Pixmap> {
    Pixmap::new((w.round() as u32).max(1), (h.round() as u32).max(1))
}

fn color_of(s: &str) -> [f32; 4] {
    parse_color(s)
}

// ---- device frame --------------------------------------------------------------------------

type UnderKey = (String, [i32; 4], u32, u32, i32);

fn under_cache() -> &'static Mutex<Option<(UnderKey, Arc<Pixmap>)>> {
    static C: OnceLock<Mutex<Option<(UnderKey, Arc<Pixmap>)>>> = OnceLock::new();
    C.get_or_init(|| Mutex::new(None))
}

/// The device frame around the screen rectangle `r`, cached while nothing about it changes.
pub fn device_layer(kind: &str, r: R, out: (u32, u32), rad: f32, s: f32) -> Option<Arc<Pixmap>> {
    if kind.is_empty() || kind == "none" {
        return None;
    }
    let key: UnderKey = (kind.to_string(), [r[0].round() as i32, r[1].round() as i32, r[2].round() as i32, r[3].round() as i32], out.0, out.1, (rad * 10.0) as i32);
    let mut c = under_cache().lock().unwrap();
    if let Some((k, v)) = c.as_ref() {
        if *k == key {
            return Some(v.clone());
        }
    }
    let mut px = Pixmap::new(out.0, out.1)?;
    crate::devices::draw(&mut px, kind, r[0], r[1], r[2], r[3], rad, s);
    let a = Arc::new(px);
    *c = Some((key, a.clone()));
    Some(a)
}

// ---- watermark image cache -------------------------------------------------------------------

fn image_cache() -> &'static Mutex<HashMap<u64, Option<Arc<Pixmap>>>> {
    static C: OnceLock<Mutex<HashMap<u64, Option<Arc<Pixmap>>>>> = OnceLock::new();
    C.get_or_init(|| Mutex::new(HashMap::new()))
}

fn hash_str(s: &str) -> u64 {
    use std::hash::{Hash, Hasher};
    let mut h = std::collections::hash_map::DefaultHasher::new();
    s.len().hash(&mut h);
    s.get(..s.len().min(4096)).unwrap_or("").hash(&mut h);
    s.get(s.len().saturating_sub(4096)..).unwrap_or("").hash(&mut h);
    h.finish()
}

/// Decode a `data:image/...;base64,` URL to RGBA (cached by content hash).
pub fn decode_image(url: &str) -> Option<(u32, u32, Vec<u8>)> {
    use base64::Engine;
    let b64 = url.split_once("base64,")?.1;
    let bytes = base64::engine::general_purpose::STANDARD.decode(b64.trim()).ok()?;
    let img = image::load_from_memory(&bytes).ok()?.to_rgba8();
    Some((img.width(), img.height(), img.into_raw()))
}

fn watermark_pixmap(url: &str) -> Option<Arc<Pixmap>> {
    let key = hash_str(url);
    let mut c = image_cache().lock().unwrap();
    if let Some(v) = c.get(&key) {
        return v.clone();
    }
    let v = decode_image(url).and_then(|(w, h, rgba)| {
        let mut px = Pixmap::new(w, h)?;
        for (d, s) in px.data_mut().chunks_exact_mut(4).zip(rgba.chunks_exact(4)) {
            let a = s[3] as u32;
            d[0] = (s[0] as u32 * a / 255) as u8;
            d[1] = (s[1] as u32 * a / 255) as u8;
            d[2] = (s[2] as u32 * a / 255) as u8;
            d[3] = s[3];
        }
        Some(Arc::new(px))
    });
    c.insert(key, v.clone());
    v
}

// ---- annotations --------------------------------------------------------------------------

struct Ctx<'a> {
    g: &'a Geom,
    s: f32,
    font: Option<Font<'static>>,
    font_bold: Option<Font<'static>>,
}

fn rr_path(x: f32, y: f32, w: f32, h: f32, r: f32) -> Option<tiny_skia::Path> {
    // negative sizes (a box dragged up and left) flip around the origin
    let (x, w) = if w < 0.0 { (x + w, -w) } else { (x, w) };
    let (y, h) = if h < 0.0 { (y + h, -h) } else { (y, h) };
    skia::round_rect(x, y, w, h, r)
}

fn fill(px: &mut Pixmap, path: &tiny_skia::Path, c: [f32; 4], mask: Option<&Mask>) {
    px.fill_path(path, &paint(c), FillRule::Winding, Transform::identity(), mask);
}

fn stroke(px: &mut Pixmap, path: &tiny_skia::Path, c: [f32; 4], w: f32, mask: Option<&Mask>) {
    let mut st = Stroke::default();
    st.width = w;
    st.line_join = LineJoin::Round;
    st.line_cap = LineCap::Round;
    px.stroke_path(path, &paint(c), &st, Transform::identity(), mask);
}

/// Annotations in `space` ("content" or "screen"). Returns the layer (None when nothing drew).
fn annotations(p: &Project, cx: &Ctx, space: &str, bounds: &mut Vec<Bound>, effects: &mut Vec<Effect>) -> Option<Arc<Pixmap>> {
    let g = cx.g;
    let list = &p.scene.annotations;
    let mut layer: Option<Pixmap> = None;
    let content = space == "content";
    // content annotations are clipped to the rounded screen
    let mask = if content {
        let mut m = Mask::new(g.w as u32, g.h as u32)?;
        m.fill_path(&skia::round_rect(g.sr[0], g.sr[1], g.sr[2], g.sr[3], g.screen_radius)?, FillRule::Winding, true, Transform::identity());
        Some(m)
    } else {
        None
    };
    let to_x = |n: f64| g.sr[0] + ((n as f32 - g.view[0]) / g.view[2]) * g.sr[2];
    let to_y = |n: f64| g.sr[1] + ((n as f32 - g.view[1]) / g.view[3]) * g.sr[3];
    let zoom_k = g.zoom * (g.sr[2] / g.content_r[2].max(1.0));
    for a in list {
        let a_space = if a.space.is_empty() { "screen" } else { a.space.as_str() };
        if a_space != space || g.t_src < a.source_start_ms || g.t_src >= a.source_end_ms {
            continue;
        }
        let k = anim_k(a, g.t_src);
        if k <= 0.003 {
            continue;
        }
        let x = if content { to_x(a.x) } else { a.x as f32 * g.w };
        let y = if content { to_y(a.y) } else { a.y as f32 * g.h };
        let w = if content { a.w as f32 / g.view[2] * g.sr[2] } else { a.w as f32 * g.w };
        let h = if content { a.h as f32 / g.view[3] * g.sr[3] } else { a.h as f32 * g.h };
        let unit = cx.s * if content { zoom_k } else { 1.0 };
        let lw = (a.thickness.max(0.0) as f32 * unit).max(1.0);
        let lw = if a.thickness == 0.0 { 5.0 * unit } else { lw };
        let pop = if a.anim == "pop" { 0.85 + 0.15 * k } else { 1.0 };
        let ga = if a.anim == "pop" { (k * 1.6).min(1.0) } else { k };
        let col = color_of(if a.color.is_empty() { "#ffffff" } else { &a.color });
        let ensure = |layer: &mut Option<Pixmap>| -> bool {
            if layer.is_none() {
                *layer = pm(g.w, g.h);
            }
            layer.is_some()
        };
        match a.kind.as_str() {
            "text" => {
                let Some(font) = cx.font_bold.as_ref().or(cx.font.as_ref()) else { continue };
                if !ensure(&mut layer) {
                    continue;
                }
                let px = layer.as_mut().unwrap();
                let fs = (if a.font_size > 0.0 { a.font_size as f32 } else { 44.0 } * unit * pop).max(6.0);
                let (pad_x, pad_y) = (fs * 0.5, fs * 0.3);
                let max_w = (fs * 3.0).max((if a.w > 0.0 { w } else { g.w * 0.8 }) - pad_x * 2.0);
                let lines = font.wrap(&a.text, fs, max_w);
                let tw = lines.iter().map(|l| font.width(l, fs)).fold(1.0, f32::max);
                let lh = fs * 1.22;
                let (bw, bh) = (tw + pad_x * 2.0, lines.len() as f32 * lh + pad_y * 2.0);
                let (bx, by) = (x - bw * (pop - 1.0) / 2.0, y - bh * (pop - 1.0) / 2.0);
                let has_fill = !a.fill.is_empty() && a.fill != "none";
                if has_fill {
                    if let Some(path) = rr_path(bx, by, bw, bh, fs * 0.28) {
                        fill(px, &path, with_alpha(color_of(&a.fill), ga), mask.as_ref());
                    }
                }
                let (asc, desc) = font.metrics(fs);
                let top_to_base = asc / (asc + desc) * fs;
                for (i, l) in lines.iter().enumerate() {
                    let base = by + pad_y + i as f32 * lh + top_to_base;
                    let Some(path) = font.path(l, bx + pad_x, base, fs) else { continue };
                    if !has_fill {
                        // a soft shadow keeps text readable on busy pictures
                        let mut sh = pm(g.w, g.h);
                        if let Some(t) = sh.as_mut() {
                            t.fill_path(&path, &paint([0.0, 0.0, 0.0, 0.5 * ga]), FillRule::Winding, Transform::identity(), None);
                            skia::blur(t, fs * 0.15 / 2.0);
                            let mut pp = PixmapPaint::default();
                            pp.opacity = 1.0;
                            px.draw_pixmap(0, 0, t.as_ref(), &pp, Transform::identity(), mask.as_ref());
                        }
                    }
                    px.fill_path(&path, &paint(with_alpha(col, ga)), FillRule::Winding, Transform::identity(), mask.as_ref());
                }
                bounds.push(Bound { id: a.id.clone(), rect: [bx, by, bw, bh], arrow: None });
            }
            "box" | "ellipse" | "highlight" => {
                if !ensure(&mut layer) {
                    continue;
                }
                let px = layer.as_mut().unwrap();
                let (bw, bh) = (w * pop, h * pop);
                let (bx, by) = (x + (w - bw) / 2.0, y + (h - bh) / 2.0);
                match a.kind.as_str() {
                    "ellipse" => {
                        if let Some(path) = skia::ellipse(bx.min(bx + bw), by.min(by + bh), bw.abs(), bh.abs()) {
                            stroke(px, &path, with_alpha(col, ga), lw, mask.as_ref());
                        }
                    }
                    "highlight" => {
                        if let Some(path) = rr_path(bx, by, bw, bh, 6.0 * unit) {
                            fill(px, &path, with_alpha(col, ga * 0.35), mask.as_ref());
                        }
                    }
                    _ => {
                        if let Some(path) = rr_path(bx, by, bw, bh, 8.0 * unit) {
                            stroke(px, &path, with_alpha(col, ga), lw, mask.as_ref());
                        }
                    }
                }
                bounds.push(Bound { id: a.id.clone(), rect: [bx, by, bw, bh], arrow: None });
            }
            "arrow" => {
                if !ensure(&mut layer) {
                    continue;
                }
                let px = layer.as_mut().unwrap();
                let (x2, y2) = if content { (to_x(a.x2), to_y(a.y2)) } else { (a.x2 as f32 * g.w, a.y2 as f32 * g.h) };
                let (dx, dy) = (x2 - x, y2 - y);
                let full = (dx * dx + dy * dy).sqrt();
                let len = full * pop;
                let len = if len == 0.0 { 1.0 } else { len };
                let (ux, uy) = if full > 0.0 { (dx / full, dy / full) } else { (0.0, 0.0) };
                let (ex, ey) = (x + ux * len, y + uy * len);
                let head = (len * 0.6).min(lw * 4.0 + 10.0 * unit);
                let ac = with_alpha(if a.color.is_empty() { color_of("#ff453a") } else { col }, ga);
                let mut pb = PathBuilder::new();
                pb.move_to(x, y);
                pb.line_to(ex - ux * head * 0.6, ey - uy * head * 0.6);
                if let Some(path) = pb.finish() {
                    stroke(px, &path, ac, lw, mask.as_ref());
                }
                let mut pb = PathBuilder::new();
                pb.move_to(ex, ey);
                pb.line_to(ex - ux * head + -uy * head * 0.5, ey - uy * head + ux * head * 0.5);
                pb.line_to(ex - ux * head + uy * head * 0.5, ey - uy * head - ux * head * 0.5);
                pb.close();
                if let Some(path) = pb.finish() {
                    fill(px, &path, ac, mask.as_ref());
                }
                bounds.push(Bound { id: a.id.clone(), rect: [x.min(x2), y.min(y2), dx.abs(), dy.abs()], arrow: Some([x, y, x2, y2]) });
            }
            "blur" | "pixelate" => {
                if content && w > 1.0 && h > 1.0 {
                    let str_ = c01(a.strength as f32);
                    let cell = (4.0 + str_ * 28.0) * cx.s * 0.9;
                    effects.push(Effect { rect: [x, y, w, h], kind: if a.kind == "blur" { 1 } else { 2 }, cell_px: cell.max(2.0), radius_px: 4.0 * unit });
                }
                bounds.push(Bound { id: a.id.clone(), rect: [x, y, w, h], arrow: None });
            }
            "spotlight" => {
                if !ensure(&mut layer) {
                    continue;
                }
                let px = layer.as_mut().unwrap();
                let mut pb = PathBuilder::new();
                if let Some(r) = tiny_skia::Rect::from_xywh(g.sr[0], g.sr[1], g.sr[2], g.sr[3]) {
                    pb.push_rect(r);
                }
                if let Some(rp) = rr_path(x, y, w, h, 14.0 * unit) {
                    pb.push_path(&rp);
                }
                if let Some(path) = pb.finish() {
                    px.fill_path(&path, &paint([0.0, 0.0, 0.0, c01(a.strength as f32) * k]), FillRule::EvenOdd, Transform::identity(), mask.as_ref());
                }
                bounds.push(Bound { id: a.id.clone(), rect: [x, y, w, h], arrow: None });
            }
            _ => {}
        }
    }
    layer.map(Arc::new)
}

// ---- shortcut display -------------------------------------------------------------------------

pub fn combo_text(c: &ComboMark) -> String {
    const MODS: [(u8, &str, &str); 4] = [(8, "⌃", "Ctrl"), (4, "⌥", "Alt"), (2, "⇧", "Shift"), (1, "⌘", "Win")];
    let mods: Vec<&(u8, &str, &str)> = MODS.iter().filter(|m| c.mods & m.0 != 0).collect();
    if cfg!(target_os = "macos") {
        let sym: String = mods.iter().map(|m| m.1).collect();
        if mods.is_empty() {
            c.label.clone()
        } else {
            format!("{sym} {}", c.label)
        }
    } else {
        let mut parts: Vec<String> = mods.iter().map(|m| if m.0 == 1 { "Win".to_string() } else { m.2.to_string() }).collect();
        parts.push(c.label.clone());
        parts.join(" + ")
    }
}

fn keys(p: &Project, cx: &Ctx, combos: &[ComboMark], px: &mut Option<Pixmap>) {
    let cfg = &p.config.keystrokes;
    if !cfg.enabled || combos.is_empty() {
        return;
    }
    let g = cx.g;
    let t = g.t_out_ms;
    let idx = combos.partition_point(|c| c.t_out_ms <= t);
    let Some(c) = idx.checked_sub(1).and_then(|i| combos.get(i)) else { return };
    let hold = if cfg.hold_ms > 0.0 { cfg.hold_ms } else { 1400.0 };
    let age = t - c.t_out_ms;
    if age < 0.0 || age > hold {
        return;
    }
    let a = ((age / 120.0).min((hold - age) / 250.0).min(1.0)) as f32;
    let fs = (if cfg.size > 0.0 { cfg.size as f32 } else { 30.0 }) * cx.s;
    let Some(font) = cx.font_bold.as_ref().or(cx.font.as_ref()) else { return };
    let text = combo_text(c);
    let tw = font.width(&text, fs);
    let (pad_x, bh) = (fs * 0.75, fs * 1.9);
    let bw = tw + pad_x * 2.0;
    let margin = fs * 1.4;
    let pos = if cfg.position.is_empty() { "bottom-center" } else { cfg.position.as_str() };
    let mut x = (g.w - bw) / 2.0;
    if pos.ends_with("left") {
        x = margin;
    } else if pos.ends_with("right") {
        x = g.w - bw - margin;
    }
    let y = if pos.starts_with("top") { margin } else { g.h - bh - margin };
    let lift = (1.0 - ease(c01((age / 120.0) as f32))) * fs * 0.4;
    let ga = ease(c01(a));
    if px.is_none() {
        *px = pm(g.w, g.h);
    }
    let Some(px) = px.as_mut() else { return };
    let Some(path) = skia::round_rect(x, y + lift, bw, bh, bh / 2.0) else { return };
    skia::fill_with_shadow(px, &path, [18.0 / 255.0, 20.0 / 255.0, 28.0 / 255.0, 0.82 * ga], Some((fs * 0.5, [0.0, 0.0, 0.0, 0.35 * ga], [0.0, 0.0])));
    let mut st = Stroke::default();
    st.width = (fs * 0.04).max(1.0);
    px.stroke_path(&path, &paint([1.0, 1.0, 1.0, 0.18 * ga]), &st, Transform::identity(), None);
    let (asc, desc) = font.metrics(fs);
    let base = y + lift + bh / 2.0 + (asc - desc) / 2.0 + fs * 0.03;
    if let Some(tp) = font.path(&text, x + pad_x, base, fs) {
        px.fill_path(&tp, &paint([1.0, 1.0, 1.0, ga]), FillRule::Winding, Transform::identity(), None);
    }
}

// ---- watermark ------------------------------------------------------------------------------

fn watermark(p: &Project, cx: &Ctx, px: &mut Option<Pixmap>) {
    let wm = &p.config.watermark;
    if !wm.enabled || wm.image.is_empty() {
        return;
    }
    let Some(img) = watermark_pixmap(&wm.image) else { return };
    let g = cx.g;
    let (iw, ih) = (img.width() as f32, img.height() as f32);
    let w = (if wm.size > 0.0 { wm.size as f32 } else { 0.12 } * g.w).max(8.0);
    let h = w * ih / iw;
    let margin = 0.025 * g.w.min(g.h) + 6.0 * cx.s;
    let x = margin + wm.x as f32 * (g.w - w - 2.0 * margin);
    let y = margin + wm.y as f32 * (g.h - h - 2.0 * margin);
    if px.is_none() {
        *px = pm(g.w, g.h);
    }
    let Some(px) = px.as_mut() else { return };
    let mut pp = PixmapPaint::default();
    pp.opacity = c01(wm.opacity as f32);
    pp.quality = tiny_skia::FilterQuality::Bicubic;
    px.draw_pixmap(0, 0, (*img).as_ref(), &pp, Transform::from_row(w / iw, 0.0, 0.0, h / ih, x, y), None);
}

// ---- captions -------------------------------------------------------------------------------

fn subtitles(p: &Project, cx: &Ctx, px: &mut Option<Pixmap>) {
    let list = &p.scene.subtitles;
    if list.is_empty() {
        return;
    }
    let g = cx.g;
    let t = g.t_src;
    let Some(cap) = list.iter().find(|c| t >= c.source_start_ms && t < c.source_end_ms && !c.text.trim().is_empty()) else { return };
    let st = &p.config.subtitles;
    let Some(font) = cx.font_bold.as_ref().or(cx.font.as_ref()) else { return };
    let fs = (if st.size > 0.0 { st.size as f32 } else { 56.0 }) * cx.s;
    let text = if st.uppercase { cap.text.to_uppercase() } else { cap.text.clone() };
    let max_w = (fs * 4.0).max(g.w * if st.width > 0.0 { st.width as f32 } else { 0.34 });
    let lines = font.wrap(&text, fs, max_w);
    let lh = fs * 1.16;
    let block_h = lines.len() as f32 * lh;
    let (cx_, cy_) = (g.w / 2.0, g.h * st.y as f32);
    // Captions that follow each other swap instantly; only a line after a pause eases in, and
    // every line fades out softly.
    let mut k = 1.0f32;
    let mut alpha = 1.0f32;
    let prev_end = list.iter().filter(|c| c.id != cap.id && c.source_end_ms <= cap.source_start_ms + 1.0).map(|c| c.source_end_ms).fold(-1e9, f64::max);
    let continuing = cap.source_start_ms - prev_end < 120.0;
    if st.pop && !continuing {
        let tt = c01(((t - cap.source_start_ms) / 140.0) as f32);
        k = 0.92 + 0.08 * (1.0 - (1.0 - tt).powi(3));
        alpha = tt;
    }
    let next_start = list.iter().filter(|c| c.id != cap.id && c.source_start_ms >= cap.source_end_ms - 1.0).map(|c| c.source_start_ms).fold(1e9, f64::min);
    if next_start - cap.source_end_ms >= 120.0 {
        alpha *= c01(((cap.source_end_ms - t) / 110.0) as f32);
    }
    if px.is_none() {
        *px = pm(g.w, g.h);
    }
    let Some(px) = px.as_mut() else { return };
    let tr = Transform::from_translate(cx_, cy_).pre_scale(k, k);
    let space = font.advance(' ', fs);
    if !st.background.is_empty() {
        let tw = lines.iter().map(|l| font.width(l, fs)).fold(0.0, f32::max);
        if let Some(path) = skia::round_rect(-tw / 2.0 - fs * 0.45, -block_h / 2.0 - fs * 0.2, tw + fs * 0.9, block_h + fs * 0.4, fs * 0.3) {
            px.fill_path(&path, &paint(with_alpha(color_of(&st.background), alpha)), FillRule::Winding, tr, None);
        }
    }
    let ow = st.outline_width as f32 * cx.s;
    let toks: Vec<&str> = text.split_whitespace().collect();
    let mut active: i64 = -1;
    if !st.highlight.is_empty() && !toks.is_empty() {
        if cap.word_starts.len() == toks.len() {
            for (i, w) in cap.word_starts.iter().enumerate() {
                if *w <= t {
                    active = i as i64;
                }
            }
        } else {
            let total: usize = toks.iter().map(|w| w.chars().count() + 1).sum();
            let f = c01(((t - cap.source_start_ms) / (cap.source_end_ms - cap.source_start_ms).max(1.0)) as f32);
            let mut acc = 0.0;
            active = toks.len() as i64 - 1;
            for (i, w) in toks.iter().enumerate() {
                acc += (w.chars().count() + 1) as f32 / total as f32;
                if f < acc {
                    active = i as i64;
                    break;
                }
            }
        }
    }
    let (asc, desc) = font.metrics(fs);
    let mut wi: i64 = 0;
    let text_col = color_of(if st.color.is_empty() { "#ffffff" } else { &st.color });
    let outline_col = color_of(if st.outline_color.is_empty() { "#000000" } else { &st.outline_color });
    let hi_col = if st.highlight.is_empty() { text_col } else { color_of(&st.highlight) };
    for (i, l) in lines.iter().enumerate() {
        let yc = -block_h / 2.0 + lh * (i as f32 + 0.5);
        let base = yc + (asc - desc) / 2.0;
        let ws: Vec<&str> = l.split_whitespace().collect();
        let widths: Vec<f32> = ws.iter().map(|w| font.width(w, fs)).collect();
        let total = widths.iter().sum::<f32>() + space * (ws.len().saturating_sub(1)) as f32;
        let first = wi;
        let mut x = -total / 2.0;
        for (k2, w) in ws.iter().enumerate() {
            if let Some(path) = font.path(w, x, base, fs) {
                let outline = if ow > 0.0 { Some((with_alpha(outline_col, alpha), ow * 2.0)) } else { None };
                let c = if first + k2 as i64 == active { hi_col } else { text_col };
                fill_text_path(px, &path, with_alpha(c, alpha), outline, tr);
            }
            x += widths[k2] + space;
        }
        wi += ws.len() as i64;
    }
}

// ---- entry point ----------------------------------------------------------------------------

/// Paint every CPU layer for one frame.
pub fn build(p: &Project, g: &Geom, combos: &[ComboMark], out: (u32, u32), device_rect: R) -> Layers {
    let s = g.h / 1080.0;
    let cx = Ctx { g, s, font: Font::at_weight(400.0), font_bold: Font::at_weight(600.0) };
    let mut layers = Layers::default();
    layers.under = device_layer(&p.config.styles.device, device_rect, out, p.config.styles.corner_radius as f32 * s, s);
    if g.screen_alpha > 0.01 {
        layers.content = annotations(p, &cx, "content", &mut layers.bounds, &mut layers.effects);
    }
    let mut top = annotations(p, &cx, "screen", &mut layers.bounds, &mut layers.effects).map(|a| Arc::try_unwrap(a).unwrap_or_else(|a| (*a).clone()));
    watermark(p, &cx, &mut top);
    keys(p, &cx, combos, &mut top);
    // captions use the heavier weight
    let cx2 = Ctx { g, s, font: Font::at_weight(400.0), font_bold: Font::at_weight(800.0) };
    subtitles(p, &cx2, &mut top);
    layers.top = top.map(Arc::new);
    layers
}
