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
        "laptop" => Insets { side: 0.032, top: 0.04, bottom: 0.085 },
        "monitor" => Insets { side: 0.03, top: 0.03, bottom: 0.2 },
        "tablet" => Insets { side: 0.05, top: 0.05, bottom: 0.05 },
        "phone" => Insets { side: 0.032, top: 0.036, bottom: 0.036 },
        "browser" => Insets { side: 0.0, top: 0.07, bottom: 0.0 },
        _ => NONE,
    }
}

/// The screen's corner radius for a device: real displays have rounded corners of their own, so a
/// device never shows a squarer screen than it has. `screen_h` is the screen's height in px.
pub fn screen_radius(kind: &str, user: f32, screen_h: f32) -> f32 {
    let min = match kind {
        "laptop" => 0.013,
        "monitor" => 0.010,
        "tablet" => 0.032,
        "phone" => 0.075,
        _ => 0.0,
    };
    user.max(min * screen_h)
}

fn hex(s: &str) -> [f32; 4] {
    crate::wallpapers::parse_color(s)
}

fn vgrad(stops: &[(f32, &str)], y0: f32, y1: f32) -> Paint<'static> {
    let mut p = Paint::default();
    p.anti_alias = true;
    let st: Vec<GradientStop> = stops
        .iter()
        .map(|(o, c)| {
            let a = hex(c);
            GradientStop::new(*o, Color::from_rgba(a[0], a[1], a[2], a[3]).unwrap())
        })
        .collect();
    p.shader = LinearGradient::new(Point::from_xy(0.0, y0), Point::from_xy(0.0, y1.max(y0 + 0.01)), st, SpreadMode::Pad, Transform::identity()).unwrap_or(tiny_skia::Shader::SolidColor(Color::BLACK));
    p
}

fn hgrad(stops: &[(f32, &str)], x0: f32, x1: f32) -> Paint<'static> {
    let mut p = vgrad(stops, 0.0, 1.0);
    let st: Vec<GradientStop> = stops
        .iter()
        .map(|(o, c)| {
            let a = hex(c);
            GradientStop::new(*o, Color::from_rgba(a[0], a[1], a[2], a[3]).unwrap())
        })
        .collect();
    p.shader = LinearGradient::new(Point::from_xy(x0, 0.0), Point::from_xy(x1.max(x0 + 0.01), 0.0), st, SpreadMode::Pad, Transform::identity()).unwrap_or(tiny_skia::Shader::SolidColor(Color::BLACK));
    p
}

fn fill_rr(px: &mut Pixmap, x: f32, y: f32, w: f32, h: f32, r: f32, p: &Paint) {
    if let Some(path) = skia::round_rect(x, y, w, h, r) {
        px.fill_path(&path, p, FillRule::Winding, Transform::identity(), None);
    }
}

fn stroke_rr(px: &mut Pixmap, x: f32, y: f32, w: f32, h: f32, r: f32, col: [f32; 4], width: f32) {
    if let Some(path) = skia::round_rect(x, y, w, h, r) {
        let mut st = Stroke::default();
        st.width = width.max(0.5);
        px.stroke_path(&path, &paint(col), &st, Transform::identity(), None);
    }
}

fn circle(px: &mut Pixmap, cx: f32, cy: f32, r: f32, p: &Paint) {
    if let Some(path) = PathBuilder::from_circle(cx, cy, r.max(0.1)) {
        px.fill_path(&path, p, FillRule::Winding, Transform::identity(), None);
    }
}

/// A rounded rectangle whose top corners are square and bottom corners round.
fn bottom_round(x: f32, y: f32, w: f32, h: f32, r: f32) -> Option<tiny_skia::Path> {
    let r = r.min(w / 2.0).min(h);
    const K: f32 = 0.552_284_75;
    let mut pb = PathBuilder::new();
    pb.move_to(x, y);
    pb.line_to(x + w, y);
    pb.line_to(x + w, y + h - r);
    pb.cubic_to(x + w, y + h - r + r * K, x + w - r + r * K, y + h, x + w - r, y + h);
    pb.line_to(x + r, y + h);
    pb.cubic_to(x + r - r * K, y + h, x, y + h - r + r * K, x, y + h - r);
    pb.close();
    pb.finish()
}

fn fill_path(px: &mut Pixmap, path: &tiny_skia::Path, p: &Paint) {
    px.fill_path(path, p, FillRule::Winding, Transform::identity(), None);
}

/// Paint the frame around the screen rectangle (x, y, w, h). `rad` is the screen's corner radius,
/// `s` the output scale (height / 1080). Shapes follow Apple hardware: a MacBook Pro with notch and
/// hinge cut-out, a thin-bezel monitor on a tilted stand, an iPad, an iPhone with a titanium frame,
/// side buttons and Dynamic Island, and a Safari-style window.
pub fn draw(px: &mut Pixmap, kind: &str, x: f32, y: f32, w: f32, h: f32, rad: f32, s: f32) {
    let k = insets(kind);
    if k == NONE {
        return;
    }
    let (side, top, bot) = (k.side as f32 * h, k.top as f32 * h, k.bottom as f32 * h);
    let (xx, yy, tw) = (x - side, y - top, w + side * 2.0);
    let hair = s.max(1.0) * 0.8;
    match kind {
        "laptop" => {
            // lid: black glass bezel in an aluminium shell
            let lid_h = top + h + side;
            let r_out = rad + side * 0.85;
            fill_rr(px, xx - hair, yy - hair, tw + hair * 2.0, lid_h + hair, r_out + hair, &vgrad(&[(0.0, "#c9ccd2"), (1.0, "#8f939c")], yy, yy + lid_h));
            fill_rr(px, xx, yy, tw, lid_h, r_out, &paint(hex("#050507")));
            stroke_rr(px, xx + hair, yy + hair, tw - hair * 2.0, lid_h - hair * 2.0, r_out, [1.0, 1.0, 1.0, 0.07], hair);
            // notch in the top of the display, with the camera in it
            let (nw, nh) = (w * 0.115, top * 0.92);
            if let Some(p) = bottom_round(x + w / 2.0 - nw / 2.0, y - 0.5, nw, nh, nh * 0.5) {
                fill_path(px, &p, &paint(hex("#050507")));
            }
            circle(px, x + w / 2.0, y + nh * 0.45, (nh * 0.13).max(1.2), &paint(hex("#10131c")));
            circle(px, x + w / 2.0 - (nh * 0.04), y + nh * 0.43, (nh * 0.05).max(0.6), &paint([0.25, 0.35, 0.6, 0.8]));
            // base: anodised aluminium with a finger notch
            let (by, bh) = (yy + lid_h, bot - side);
            let lip = side * 1.1;
            let (bx, bw) = (xx - lip, tw + lip * 2.0);
            if let Some(p) = bottom_round(bx, by, bw, bh, bh * 1.05) {
                fill_path(px, &p, &vgrad(&[(0.0, "#e7e9ee"), (0.35, "#d3d6dc"), (1.0, "#9a9ea8")], by, by + bh));
            }
            // contact shadow under the lid and the bright front edge
            fill_rr(px, bx, by, bw, bh * 0.16, 0.0, &vgrad(&[(0.0, "#00000066"), (1.0, "#00000000")], by, by + bh * 0.16));
            fill_rr(px, bx + bh, by + bh - hair * 1.5, bw - bh * 2.0, hair * 1.5, 0.0, &paint([1.0, 1.0, 1.0, 0.35]));
            // hinge cut-out
            let nw = bw * 0.17;
            if let Some(p) = bottom_round(bx + bw / 2.0 - nw / 2.0, by, nw, bh * 0.52, bh * 0.52) {
                fill_path(px, &p, &vgrad(&[(0.0, "#a9adb6"), (1.0, "#cfd2d9")], by, by + bh * 0.52));
            }
        }
        "monitor" => {
            // thin aluminium rim, black glass, tilted stand
            let body_h = top + h + side * 1.15;
            let r_out = rad + side * 0.9;
            fill_rr(px, xx - hair, yy - hair, tw + hair * 2.0, body_h + hair * 2.0, r_out + hair, &vgrad(&[(0.0, "#d4d7dc"), (1.0, "#9ea2ab")], yy, yy + body_h));
            fill_rr(px, xx, yy, tw, body_h, r_out, &paint(hex("#060608")));
            stroke_rr(px, xx + hair, yy + hair, tw - hair * 2.0, body_h - hair * 2.0, r_out, [1.0, 1.0, 1.0, 0.06], hair);
            // chin carries the tilt: a short, wide neck and a flat foot
            let neck_top = yy + body_h - hair;
            let (neck_w, neck_h) = (tw * 0.16, bot - body_h + top + 0.0);
            let neck_h = neck_h.max(h * 0.05).min(bot * 0.7);
            let cx = x + w / 2.0;
            let mut pb = PathBuilder::new();
            pb.move_to(cx - neck_w * 0.5, neck_top);
            pb.line_to(cx + neck_w * 0.5, neck_top);
            pb.line_to(cx + neck_w * 0.62, neck_top + neck_h);
            pb.line_to(cx - neck_w * 0.62, neck_top + neck_h);
            pb.close();
            if let Some(p) = pb.finish() {
                fill_path(px, &p, &hgrad(&[(0.0, "#8d919a"), (0.5, "#d9dce1"), (1.0, "#8d919a")], cx - neck_w * 0.62, cx + neck_w * 0.62));
            }
            let (foot_w, foot_h) = (tw * 0.27, h * 0.028);
            fill_rr(px, cx - foot_w / 2.0, neck_top + neck_h - foot_h * 0.3, foot_w, foot_h, foot_h * 0.5, &vgrad(&[(0.0, "#e2e4e8"), (1.0, "#a4a8b1")], neck_top + neck_h, neck_top + neck_h + foot_h));
        }
        "tablet" => {
            let (bh_, r_out) = (top + h + bot, rad + side * 0.95);
            fill_rr(px, xx - hair, yy - hair, tw + hair * 2.0, bh_ + hair * 2.0, r_out + hair, &vgrad(&[(0.0, "#d6d8dd"), (1.0, "#9a9ea7")], yy, yy + bh_));
            fill_rr(px, xx, yy, tw, bh_, r_out, &paint(hex("#050507")));
            stroke_rr(px, xx + hair, yy + hair, tw - hair * 2.0, bh_ - hair * 2.0, r_out, [1.0, 1.0, 1.0, 0.06], hair);
            circle(px, x + w / 2.0, yy + top * 0.5, (top * 0.09).max(1.2), &paint(hex("#12151d")));
        }
        "phone" => {
            // titanium frame, black bezel, side buttons, Dynamic Island
            let (bh_, r_out) = (top + h + bot, rad + side * 0.95);
            let f = side * 0.34; // frame thickness
            // buttons first, so the frame covers their roots
            let btn = paint(hex("#7d7b77"));
            let bx_l = xx - f * 0.7;
            let bx_r = xx + tw - f * 0.3;
            for (off, len) in [(0.16, 0.035), (0.23, 0.065), (0.31, 0.065)] {
                fill_rr(px, bx_l, yy + bh_ * off, f, bh_ * len, f * 0.5, &btn);
            }
            fill_rr(px, bx_r, yy + bh_ * 0.27, f, bh_ * 0.11, f * 0.5, &btn);
            fill_rr(px, xx - f, yy - f, tw + f * 2.0, bh_ + f * 2.0, r_out + f, &hgrad(&[(0.0, "#6f6d69"), (0.12, "#bdbab4"), (0.5, "#8a8883"), (0.88, "#bdbab4"), (1.0, "#6a6864")], xx - f, xx + tw + f));
            fill_rr(px, xx, yy, tw, bh_, r_out, &paint(hex("#030304")));
            stroke_rr(px, xx - f * 0.5, yy - f * 0.5, tw + f, bh_ + f, r_out + f * 0.5, [1.0, 1.0, 1.0, 0.22], hair);
            // Dynamic Island, inside the top of the screen
            let (iw, ih) = (w * 0.29, h * 0.027);
            fill_rr(px, x + w / 2.0 - iw / 2.0, y + h * 0.012, iw, ih, ih * 0.5, &paint(hex("#000000")));
            circle(px, x + w / 2.0 + iw * 0.3, y + h * 0.012 + ih * 0.5, ih * 0.2, &paint(hex("#0b0e18")));
            // home indicator
            fill_rr(px, x + w / 2.0 - w * 0.17, y + h - h * 0.014 - hair * 2.0, w * 0.34, hair * 2.4, hair * 1.2, &paint([1.0, 1.0, 1.0, 0.55]));
        }
        "browser" => {
            // Safari-style window: a unified title bar with traffic lights and the address field
            let r_out = rad.max(10.0 * s);
            fill_rr(px, xx, yy, tw, top + h, r_out, &vgrad(&[(0.0, "#38383c"), (1.0, "#2a2a2d")], yy, yy + top));
            stroke_rr(px, xx + hair * 0.5, yy + hair * 0.5, tw - hair, top + h - hair, r_out, [1.0, 1.0, 1.0, 0.10], hair);
            fill_rr(px, xx, yy + top - hair, tw, hair, 0.0, &paint([0.0, 0.0, 0.0, 0.45]));
            let (cy, r0) = (yy + top / 2.0, top * 0.15);
            for (i, c) in ["#ff5f57", "#febc2e", "#28c840"].iter().enumerate() {
                circle(px, xx + top * 0.5 + i as f32 * r0 * 3.4, cy, r0, &paint(hex(c)));
            }
            let uw = (tw * 0.42).min(tw - top * 4.0);
            fill_rr(px, xx + tw / 2.0 - uw / 2.0, cy - top * 0.27, uw, top * 0.54, top * 0.16, &paint(hex("#1f1f22")));
            stroke_rr(px, xx + tw / 2.0 - uw / 2.0, cy - top * 0.27, uw, top * 0.54, top * 0.16, [1.0, 1.0, 1.0, 0.07], hair);
            // lock glyph
            fill_rr(px, xx + tw / 2.0 - top * 0.12, cy - top * 0.06, top * 0.24, top * 0.17, top * 0.03, &paint([1.0, 1.0, 1.0, 0.4]));
        }
        _ => {}
    }
}
