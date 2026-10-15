//! Vector cursors rasterised once into an atlas texture. Row = cursor style, column = shape
//! (order matches `CursorKind::ALL`). Each cell is `CELL` px, the shape is drawn for a cursor
//! height of `REF_H` px with its hotspot at `HOTSPOT[kind]`.

use tiny_skia::{Color, FillRule, LineCap, LineJoin, Paint, PathBuilder, Pixmap, Stroke, Transform};

pub const CELL: u32 = 128;
pub const REF_H: f32 = 80.0;
pub const KINDS: u32 = 9;
pub const STYLES: u32 = 4; // macos, light, accent, dot

/// Hotspot inside a cell, in px.
pub const HOTSPOT: [(f32, f32); 9] = [
    (32.0, 24.0),  // arrow: tip
    (64.0, 64.0),  // i-beam: centre
    (46.0, 30.0),  // pointer: fingertip
    (64.0, 64.0),  // crosshair
    (64.0, 64.0),  // open hand
    (64.0, 64.0),  // closed hand
    (64.0, 64.0),  // resize left-right
    (64.0, 64.0),  // resize up-down
    (64.0, 64.0),  // not allowed
];

const ARROW: [(f32, f32); 7] =
    [(0.0, 0.0), (0.0, 0.86), (0.19, 0.69), (0.31, 0.97), (0.43, 0.92), (0.31, 0.64), (0.56, 0.64)];

fn paint(c: [u8; 4]) -> Paint<'static> {
    let mut p = Paint::default();
    p.set_color(Color::from_rgba8(c[0], c[1], c[2], c[3]));
    p.anti_alias = true;
    p
}

fn style_colors(style: u32) -> ([u8; 4], [u8; 4]) {
    match style {
        1 => ([255, 255, 255, 255], [17, 17, 17, 255]),
        2 => ([59, 130, 196, 255], [255, 255, 255, 255]),
        3 => ([255, 255, 255, 235], [0, 0, 0, 130]),
        _ => ([17, 17, 17, 255], [255, 255, 255, 255]),
    }
}

fn poly(pts: &[(f32, f32)], s: f32, ox: f32, oy: f32) -> tiny_skia::Path {
    let mut pb = PathBuilder::new();
    for (i, (x, y)) in pts.iter().enumerate() {
        let (x, y) = (ox + x * s, oy + y * s);
        if i == 0 {
            pb.move_to(x, y);
        } else {
            pb.line_to(x, y);
        }
    }
    pb.close();
    pb.finish().unwrap()
}

fn draw_shape(px: &mut Pixmap, kind: u32, style: u32) {
    let (fill, stroke_c) = style_colors(style);
    let s = REF_H;
    let (hx, hy) = HOTSPOT[kind as usize];
    let lw = (s * 0.065).max(1.5);
    let stroke = Stroke { width: lw, line_cap: LineCap::Round, line_join: LineJoin::Round, ..Default::default() };
    let ident = Transform::identity();

    // Cheap soft shadow: the same silhouette offset down, repeated with growing width.
    let shadow = |px: &mut Pixmap, path: &tiny_skia::Path| {
        for (i, a) in [26u8, 18, 10, 6].iter().enumerate() {
            let w = lw + 2.0 + i as f32 * 2.4;
            let st = Stroke { width: w, line_cap: LineCap::Round, line_join: LineJoin::Round, ..Default::default() };
            px.stroke_path(path, &paint([0, 0, 0, *a]), &st, Transform::from_translate(0.0, s * 0.045), None);
        }
        px.fill_path(path, &paint([0, 0, 0, 40]), FillRule::Winding, Transform::from_translate(0.0, s * 0.045), None);
    };

    if style == 3 {
        // Dot style: every kind is a round marker.
        let mut pb = PathBuilder::new();
        pb.push_circle(hx, hy, s * 0.28);
        let p = pb.finish().unwrap();
        shadow(px, &p);
        px.fill_path(&p, &paint(fill), FillRule::Winding, ident, None);
        px.stroke_path(&p, &paint(stroke_c), &stroke, ident, None);
        return;
    }

    match kind {
        1 => {
            // I-beam
            let w = s * 0.18;
            let mut pb = PathBuilder::new();
            pb.move_to(hx - w, hy - s * 0.5);
            pb.line_to(hx + w, hy - s * 0.5);
            pb.move_to(hx, hy - s * 0.5);
            pb.line_to(hx, hy + s * 0.5);
            pb.move_to(hx - w, hy + s * 0.5);
            pb.line_to(hx + w, hy + s * 0.5);
            let p = pb.finish().unwrap();
            shadow(px, &p);
            let outer = Stroke { width: (s * 0.16).max(3.0), ..stroke.clone() };
            px.stroke_path(&p, &paint(stroke_c), &outer, ident, None);
            let inner = Stroke { width: (s * 0.08).max(1.5), ..stroke.clone() };
            px.stroke_path(&p, &paint(fill), &inner, ident, None);
        }
        2 | 4 | 5 => {
            // Hand: palm + (pointing) finger
            let (ox, oy) = (hx - s * 0.26, hy - s * 0.0);
            let mut pb = PathBuilder::new();
            let f = kind == 2;
            let pt = |x: f32, y: f32| (ox + x * s, oy + y * s);
            let m = pt(0.18, 0.0);
            pb.move_to(m.0, m.1);
            let c = pt(0.26, -0.02);
            let e = pt(0.26, 0.1);
            pb.quad_to(c.0, c.1, e.0, e.1);
            let a = pt(0.26, 0.38);
            pb.line_to(a.0, a.1);
            let b = pt(0.62, 0.42);
            pb.line_to(b.0, b.1);
            let c2 = pt(0.78, 0.46);
            let e2 = pt(0.76, 0.62);
            pb.quad_to(c2.0, c2.1, e2.0, e2.1);
            let d = pt(0.7, 0.9);
            pb.line_to(d.0, d.1);
            let g = pt(0.22, 0.9);
            pb.line_to(g.0, g.1);
            let h = pt(0.0, 0.56);
            pb.line_to(h.0, h.1);
            let c3 = pt(0.0, 0.48);
            let e3 = pt(0.1, 0.5);
            pb.quad_to(c3.0, c3.1, e3.0, e3.1);
            let i = pt(0.18, 0.6);
            pb.line_to(i.0, i.1);
            pb.close();
            let p = pb.finish().unwrap();
            let _ = f;
            shadow(px, &p);
            px.fill_path(&p, &paint(stroke_c), FillRule::Winding, ident, None);
            px.stroke_path(&p, &paint(fill), &stroke, ident, None);
        }
        3 => {
            let r = s * 0.32;
            let mut pb = PathBuilder::new();
            pb.move_to(hx - r, hy);
            pb.line_to(hx + r, hy);
            pb.move_to(hx, hy - r);
            pb.line_to(hx, hy + r);
            let p = pb.finish().unwrap();
            shadow(px, &p);
            let outer = Stroke { width: lw * 2.4, ..stroke.clone() };
            px.stroke_path(&p, &paint(stroke_c), &outer, ident, None);
            px.stroke_path(&p, &paint(fill), &stroke, ident, None);
        }
        6 | 7 => {
            let r = s * 0.34;
            let a = s * 0.12;
            let mut pb = PathBuilder::new();
            if kind == 6 {
                pb.move_to(hx - r, hy);
                pb.line_to(hx + r, hy);
                pb.move_to(hx - r + a, hy - a);
                pb.line_to(hx - r, hy);
                pb.line_to(hx - r + a, hy + a);
                pb.move_to(hx + r - a, hy - a);
                pb.line_to(hx + r, hy);
                pb.line_to(hx + r - a, hy + a);
            } else {
                pb.move_to(hx, hy - r);
                pb.line_to(hx, hy + r);
                pb.move_to(hx - a, hy - r + a);
                pb.line_to(hx, hy - r);
                pb.line_to(hx + a, hy - r + a);
                pb.move_to(hx - a, hy + r - a);
                pb.line_to(hx, hy + r);
                pb.line_to(hx + a, hy + r - a);
            }
            let p = pb.finish().unwrap();
            shadow(px, &p);
            px.stroke_path(&p, &paint(stroke_c), &Stroke { width: lw * 2.2, ..stroke.clone() }, ident, None);
            px.stroke_path(&p, &paint(fill), &stroke, ident, None);
        }
        8 => {
            let r = s * 0.3;
            let mut pb = PathBuilder::new();
            pb.push_circle(hx, hy, r);
            pb.move_to(hx - r * 0.7, hy + r * 0.7);
            pb.line_to(hx + r * 0.7, hy - r * 0.7);
            let p = pb.finish().unwrap();
            shadow(px, &p);
            px.stroke_path(&p, &paint(stroke_c), &Stroke { width: lw * 2.4, ..stroke.clone() }, ident, None);
            px.stroke_path(&p, &paint([220, 38, 38, 255]), &stroke, ident, None);
        }
        _ => {
            // Arrow, tip at the hotspot
            let path = poly(&ARROW, s, hx, hy);
            shadow(px, &path);
            px.fill_path(&path, &paint(fill), FillRule::Winding, ident, None);
            px.stroke_path(&path, &paint(stroke_c), &stroke, ident, None);
        }
    }
}

/// RGBA8 straight-alpha atlas, `KINDS * CELL` wide and `STYLES * CELL` tall.
pub fn build_atlas() -> (u32, u32, Vec<u8>) {
    let (w, h) = (KINDS * CELL, STYLES * CELL);
    let mut data = vec![0u8; (w * h * 4) as usize];
    for style in 0..STYLES {
        for kind in 0..KINDS {
            let mut cell = Pixmap::new(CELL, CELL).unwrap();
            draw_shape(&mut cell, kind, style);
            // tiny-skia is premultiplied; demultiply into the straight-alpha atlas.
            for y in 0..CELL {
                for x in 0..CELL {
                    let p = cell.pixel(x, y).unwrap().demultiply();
                    let o = (((style * CELL + y) * w) + kind * CELL + x) as usize * 4;
                    data[o..o + 4].copy_from_slice(&[p.red(), p.green(), p.blue(), p.alpha()]);
                }
            }
        }
    }
    (w, h, data)
}

pub fn style_index(name: &str) -> u32 {
    match name {
        "light" => 1,
        "accent" => 2,
        "dot" => 3,
        _ => 0,
    }
}
