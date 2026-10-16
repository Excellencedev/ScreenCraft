//! Device frames drawn around the screen rectangle (laptop, monitor, tablet, phone, browser
//! window): all vector, no image assets. Insets are fractions of the screen's height.

use crate::skia::{self, paint};
use tiny_skia::{Color, FillRule, GradientStop, LinearGradient, Paint, PathBuilder, Pixmap, Point, SpreadMode, Stroke, Transform};

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Insets {
    pub side: f64,
    pub top: f64,
    pub bottom: f64,
}

pub const NONE: Insets = Insets { side: 0.0, top: 0.0, bottom: 0.0 };

pub fn insets(kind: &str) -> Insets {
    match kind {
        "laptop" => Insets { side: 0.035, top: 0.05, bottom: 0.1 },
        "monitor" => Insets { side: 0.028, top: 0.028, bottom: 0.17 },
        "tablet" => Insets { side: 0.06, top: 0.06, bottom: 0.06 },
        "phone" => Insets { side: 0.04, top: 0.05, bottom: 0.05 },
        "browser" => Insets { side: 0.0, top: 0.07, bottom: 0.0 },
        _ => NONE,
    }
}

fn hex(s: &str) -> [f32; 4] {
    crate::wallpapers::parse_color(s)
}

fn gradient(c0: &str, c1: &str, y0: f32, y1: f32) -> Paint<'static> {
    let mut p = Paint::default();
    p.anti_alias = true;
    let (a, b) = (hex(c0), hex(c1));
    p.shader = LinearGradient::new(
        Point::from_xy(0.0, y0),
        Point::from_xy(0.0, y1.max(y0 + 0.01)),
        vec![GradientStop::new(0.0, Color::from_rgba(a[0], a[1], a[2], a[3]).unwrap()), GradientStop::new(1.0, Color::from_rgba(b[0], b[1], b[2], b[3]).unwrap())],
        SpreadMode::Pad,
        Transform::identity(),
    )
    .unwrap_or(tiny_skia::Shader::SolidColor(Color::BLACK));
    p
}

fn fill_rr(px: &mut Pixmap, x: f32, y: f32, w: f32, h: f32, r: f32, p: &Paint) {
    if let Some(path) = skia::round_rect(x, y, w, h, r) {
        px.fill_path(&path, p, FillRule::Winding, Transform::identity(), None);
    }
}

fn body(px: &mut Pixmap, x: f32, y: f32, w: f32, h: f32, r: f32, fill: &str, s: f32) {
    fill_rr(px, x, y, w, h, r, &paint(hex(fill)));
    if let Some(path) = skia::round_rect(x, y, w, h, r) {
        let mut st = Stroke::default();
        st.width = s.max(1.0);
        px.stroke_path(&path, &paint([1.0, 1.0, 1.0, 0.14]), &st, Transform::identity(), None);
    }
}

fn circle(px: &mut Pixmap, cx: f32, cy: f32, r: f32, p: &Paint) {
    if let Some(path) = PathBuilder::from_circle(cx, cy, r.max(0.1)) {
        px.fill_path(&path, p, FillRule::Winding, Transform::identity(), None);
    }
}

/// Paint the frame around the screen rectangle (x, y, w, h). `rad` is the screen's corner radius,
/// `s` the output scale (height / 1080).
pub fn draw(px: &mut Pixmap, kind: &str, x: f32, y: f32, w: f32, h: f32, rad: f32, s: f32) {
    let k = insets(kind);
    if k == NONE {
        return;
    }
    let (side, top, bot) = (k.side as f32 * h, k.top as f32 * h, k.bottom as f32 * h);
    let (xx, yy, tw) = (x - side, y - top, w + side * 2.0);
    match kind {
        "laptop" => {
            let lid_h = top + h + side;
            body(px, xx, yy, tw, lid_h, rad + side * 0.8, "#0c0e13", s);
            circle(px, x + w / 2.0, yy + top / 2.0, (top * 0.14).max(2.0), &paint(hex("#1f2430")));
            let (by, bh) = (yy + lid_h, bot - side);
            let lip = side * 0.9;
            let p = gradient("#d9dde5", "#8b93a3", by, by + bh);
            // base: square top corners, round bottom corners
            let r = bh * 0.9;
            let mut pb = PathBuilder::new();
            let (bx, bw) = (xx - lip, tw + lip * 2.0);
            let r = r.min(bw / 2.0).min(bh);
            const K: f32 = 0.552_284_75;
            pb.move_to(bx, by);
            pb.line_to(bx + bw, by);
            pb.line_to(bx + bw, by + bh - r);
            pb.cubic_to(bx + bw, by + bh - r + r * K, bx + bw - r + r * K, by + bh, bx + bw - r, by + bh);
            pb.line_to(bx + r, by + bh);
            pb.cubic_to(bx + r - r * K, by + bh, bx, by + bh - r + r * K, bx, by + bh - r);
            pb.close();
            if let Some(path) = pb.finish() {
                px.fill_path(&path, &p, FillRule::Winding, Transform::identity(), None);
            }
            fill_rr(px, x + w / 2.0 - tw * 0.09, by, tw * 0.18, bh * 0.45, bh * 0.3, &paint([0.0, 0.0, 0.0, 0.22]));
        }
        "monitor" => {
            let bezel_h = top + h + side * 1.2;
            body(px, xx, yy, tw, bezel_h + h * 0.045, rad + side * 0.8, "#0d0f14", s);
            let stand_top = yy + bezel_h + h * 0.045;
            let (neck_w, neck_h) = (tw * 0.1, h * 0.06);
            let p = gradient("#9aa2b2", "#6f7686", stand_top, stand_top + neck_h);
            if let Some(r) = tiny_skia::Rect::from_xywh(x + w / 2.0 - neck_w / 2.0, stand_top - 1.0, neck_w, neck_h + 1.0) {
                px.fill_rect(r, &p, Transform::identity(), None);
            }
            let (foot_w, foot_h) = (tw * 0.28, h * 0.035);
            let p = gradient("#d9dde5", "#8b93a3", stand_top + neck_h, stand_top + neck_h + foot_h);
            fill_rr(px, x + w / 2.0 - foot_w / 2.0, stand_top + neck_h, foot_w, foot_h, foot_h * 0.5, &p);
        }
        "tablet" => body(px, xx, yy, tw, top + h + bot, rad + side * 0.9, "#0b0d11", s),
        "phone" => {
            body(px, xx, yy, tw, top + h + bot, rad + side * 1.6, "#08090c", s);
            fill_rr(px, x + w / 2.0 - w * 0.09, yy + top * 0.35, w * 0.18, top * 0.3, top * 0.15, &paint(hex("#1a1d26")));
        }
        "browser" => {
            body(px, xx, yy, tw, top + h, rad, "#1a1e29", s);
            let (cy, r0) = (yy + top / 2.0, top * 0.17);
            for (i, c) in ["#ff5f57", "#febc2e", "#28c840"].iter().enumerate() {
                circle(px, xx + top * 0.55 + i as f32 * r0 * 3.0, cy, r0, &paint(hex(c)));
            }
            let uw = (tw * 0.5).min(tw - top * 3.5);
            fill_rr(px, xx + tw / 2.0 - uw / 2.0, cy - top * 0.26, uw, top * 0.52, top * 0.26, &paint(hex("#0e1118")));
        }
        _ => {}
    }
}
