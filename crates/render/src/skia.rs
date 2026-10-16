//! Small tiny-skia helpers shared by the CPU overlay painters.

use tiny_skia::{Color, FillRule, Paint, Path, PathBuilder, Pixmap, PixmapPaint, Rect, Transform};

pub fn color(c: [f32; 4]) -> Color {
    Color::from_rgba(c[0].clamp(0.0, 1.0), c[1].clamp(0.0, 1.0), c[2].clamp(0.0, 1.0), c[3].clamp(0.0, 1.0)).unwrap_or(Color::BLACK)
}

pub fn paint(c: [f32; 4]) -> Paint<'static> {
    let mut p = Paint::default();
    p.set_color(color(c));
    p.anti_alias = true;
    p
}

pub fn with_alpha(mut c: [f32; 4], a: f32) -> [f32; 4] {
    c[3] *= a;
    c
}

/// A rounded rectangle with corner radius `r` (clamped to half the shorter side).
pub fn round_rect(x: f32, y: f32, w: f32, h: f32, r: f32) -> Option<Path> {
    let (w, h) = (w.max(0.01), h.max(0.01));
    let r = r.max(0.0).min(w.abs() / 2.0).min(h.abs() / 2.0);
    if r < 0.01 {
        return PathBuilder::from_rect(Rect::from_xywh(x, y, w, h)?).into();
    }
    const K: f32 = 0.552_284_75;
    let k = r * K;
    let mut pb = PathBuilder::new();
    pb.move_to(x + r, y);
    pb.line_to(x + w - r, y);
    pb.cubic_to(x + w - r + k, y, x + w, y + r - k, x + w, y + r);
    pb.line_to(x + w, y + h - r);
    pb.cubic_to(x + w, y + h - r + k, x + w - r + k, y + h, x + w - r, y + h);
    pb.line_to(x + r, y + h);
    pb.cubic_to(x + r - k, y + h, x, y + h - r + k, x, y + h - r);
    pb.line_to(x, y + r);
    pb.cubic_to(x, y + r - k, x + r - k, y, x + r, y);
    pb.close();
    pb.finish()
}

pub fn ellipse(x: f32, y: f32, w: f32, h: f32) -> Option<Path> {
    PathBuilder::from_oval(Rect::from_xywh(x, y, w.max(0.01), h.max(0.01))?)
}

/// Box blur (three passes ≈ gaussian) over premultiplied RGBA, in place.
pub fn blur(px: &mut Pixmap, radius: f32) {
    let r = radius.round() as i32;
    if r < 1 {
        return;
    }
    let (w, h) = (px.width() as usize, px.height() as usize);
    let data = px.data_mut();
    let mut tmp = vec![0u8; data.len()];
    for _ in 0..3 {
        box_pass(data, &mut tmp, w, h, r as usize, true);
        box_pass(&tmp, data, w, h, r as usize, false);
    }
}

fn box_pass(src: &[u8], dst: &mut [u8], w: usize, h: usize, r: usize, horiz: bool) {
    let (n, lines, stride, step) = if horiz { (w, h, w * 4, 4) } else { (h, w, 4, w * 4) };
    let div = (2 * r + 1) as u32;
    for l in 0..lines {
        let base = if horiz { l * stride } else { l * 4 };
        for c in 0..4 {
            let at = |i: isize| -> u32 { src[base + (i.clamp(0, n as isize - 1) as usize) * step + c] as u32 };
            let mut acc: u32 = 0;
            for i in -(r as isize)..=(r as isize) {
                acc += at(i);
            }
            for i in 0..n {
                dst[base + i * step + c] = (acc / div) as u8;
                acc += at(i as isize + r as isize + 1);
                acc -= at(i as isize - r as isize);
            }
        }
    }
}

/// Composite `src` onto `dst` at (x, y) with source-over.
pub fn draw(dst: &mut Pixmap, src: &Pixmap, x: i32, y: i32, alpha: f32) {
    let mut pp = PixmapPaint::default();
    pp.opacity = alpha.clamp(0.0, 1.0);
    dst.draw_pixmap(x, y, src.as_ref(), &pp, Transform::identity(), None);
}

/// Fill `path` with a soft drop shadow underneath (what canvas `shadowBlur` does).
pub fn fill_with_shadow(dst: &mut Pixmap, path: &Path, fill: [f32; 4], shadow: Option<(f32, [f32; 4], [f32; 2])>) {
    if let Some((blur_px, col, off)) = shadow {
        // canvas shadowBlur B has a gaussian sigma of B/2
        let sigma = blur_px / 2.0;
        let b = path.bounds();
        let pad = (sigma * 3.0).ceil() + 2.0;
        let (x0, y0) = ((b.left() - pad + off[0]).floor() as i32, (b.top() - pad + off[1]).floor() as i32);
        let (w, h) = (((b.width() + pad * 2.0).ceil() as u32).max(1), ((b.height() + pad * 2.0).ceil() as u32).max(1));
        if let Some(mut tmp) = Pixmap::new(w.min(8192), h.min(8192)) {
            let t = Transform::from_translate(-(b.left() - pad), -(b.top() - pad));
            tmp.fill_path(path, &paint(col), FillRule::Winding, t, None);
            blur(&mut tmp, sigma * 0.9);
            draw(dst, &tmp, x0, y0, 1.0);
        }
    }
    dst.fill_path(path, &paint(fill), FillRule::Winding, Transform::identity(), None);
}
