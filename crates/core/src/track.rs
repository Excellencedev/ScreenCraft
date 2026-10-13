//! Per-frame render track: where the camera and cursor are at every output
//! frame. Computed once in Rust; the UI just paints it, so preview and export
//! always agree.

use crate::input::{CursorKind, InputLog};
use crate::project::{Project, Rect, ZoomKind};
use crate::spring::{ease, smooth_damp, Spring};
use crate::timeline::{output_duration_ms, output_to_source};
use serde::{Deserialize, Serialize};

#[derive(Clone, Debug, Serialize, Deserialize, Default)]
#[serde(rename_all = "camelCase")]
pub struct ClickMark {
    pub t_out_ms: f64,
    pub x: f64,
    pub y: f64,
}

/// A key press in output time. `kind`: 0 = character, 1 = space, 2 = return, 3 = backspace/delete, 4 = tab.
#[derive(Clone, Debug, Serialize, Deserialize, Default)]
#[serde(rename_all = "camelCase")]
pub struct KeyMark {
    pub t_out_ms: f64,
    pub kind: u8,
}

/// A shortcut / special key to show on screen, in output time.
#[derive(Clone, Debug, Serialize, Deserialize, Default)]
#[serde(rename_all = "camelCase")]
pub struct ComboMark {
    pub t_out_ms: f64,
    pub label: String,
    pub mods: u8,
}

/// Camera layout ids shared with the UI: 0 bubble, 1 hidden, 2 large, 3 camera, 4 side-right, 5 side-left.
pub fn layout_id(name: &str) -> u8 {
    match name {
        "hidden" => 1,
        "large" => 2,
        "camera" => 3,
        "side-right" => 4,
        "side-left" => 5,
        _ => 0,
    }
}

/// Transition ids shared with the UI: 0 none, 1 fade, 2 flash, 3 blur, 4 slide, 5 zoom.
pub fn transition_id(name: &str) -> u8 {
    match name {
        "fade" => 1,
        "flash" => 2,
        "blur" => 3,
        "slide" => 4,
        "zoom" => 5,
        _ => 0,
    }
}

/// Struct-of-arrays so the JSON stays compact.
#[derive(Clone, Debug, Serialize, Deserialize, Default)]
#[serde(rename_all = "camelCase")]
pub struct RenderTrack {
    pub fps: f64,
    pub duration_ms: f64,
    pub count: usize,
    pub t_src: Vec<f64>,
    /// Camera zoom factor (1 = whole frame).
    pub zoom: Vec<f64>,
    /// Camera centre, normalised to the full source frame.
    pub cx: Vec<f64>,
    pub cy: Vec<f64>,
    /// Smoothed cursor position, normalised to the full source frame.
    pub mx: Vec<f64>,
    pub my: Vec<f64>,
    pub cursor_kind: Vec<u8>,
    /// 1 = normal, <1 while the button is held.
    pub cursor_scale: Vec<f64>,
    pub cursor_alpha: Vec<f64>,
    /// Cursor tilt in radians when velocity rotation is on.
    pub cursor_rotation: Vec<f64>,
    pub clicks: Vec<ClickMark>,
    /// Key presses (for the typing sound effects), in output time.
    pub keys: Vec<KeyMark>,
    /// Shortcuts and special keys for the on-screen key display.
    pub combos: Vec<ComboMark>,
    /// Transition at each frame: id, and strength (1 at the cut; positive before the cut, negative after).
    pub trans_kind: Vec<u8>,
    pub trans_p: Vec<f64>,
    /// Camera layout at each frame, the one before it and how far the change has progressed (0..1).
    pub layout_cur: Vec<u8>,
    pub layout_prev: Vec<u8>,
    pub layout_mix: Vec<f64>,
}

fn r4(v: f64) -> f64 {
    (v * 10000.0).round() / 10000.0
}

fn clamp_center(c: f64, half: f64, lo: f64, hi: f64) -> f64 {
    if hi - lo <= 2.0 * half {
        (lo + hi) * 0.5
    } else {
        c.clamp(lo + half, hi - half)
    }
}

pub fn compute_track(p: &Project, input: &InputLog, fps: f64) -> RenderTrack {
    let slices = &p.scene.slices;
    let duration = output_duration_ms(slices);
    let count = ((duration / 1000.0) * fps).floor() as usize + 1;
    let dt = 1.0 / fps;
    let crop: Rect = p.config.crop;
    let anim = &p.config.animations;

    let mut tr = RenderTrack { fps, duration_ms: duration, count, ..Default::default() };
    for v in [
        &mut tr.t_src,
        &mut tr.zoom,
        &mut tr.cx,
        &mut tr.cy,
        &mut tr.mx,
        &mut tr.my,
        &mut tr.cursor_scale,
        &mut tr.cursor_alpha,
        &mut tr.cursor_rotation,
    ] {
        v.reserve(count);
    }

    // camera layout state: eased over ~0.35 s whenever the layout changes
    let mut lay_cur = 0u8;
    let mut lay_prev = 0u8;
    let mut lay_mix = 1.0f64;
    let mut lay_init = false;
    let starts = crate::timeline::slice_output_starts(slices);

    let crop_cx = crop.x + crop.width / 2.0;
    let crop_cy = crop.y + crop.height / 2.0;

    // smoothed cursor + camera focus (critically damped, so no snapping)
    let mut mx = 0.5;
    let mut my = 0.5;
    let (mut mvx, mut mvy) = (0.0, 0.0);
    let (mut fx, mut fy) = (crop_cx, crop_cy);
    let (mut fvx, mut fvy) = (0.0, 0.0);
    let mut initialised = false;
    let mut press = Spring::new(1.0);
    // cursor opacity fades linearly over half a second (hide / typing / idle)
    let mut alpha = 1.0f64;
    let mut rotation = Spring::new(0.0);
    // zoom level is eased in log space (constant perceived speed) and then low-passed
    // so its velocity never jumps at the start or end of a region
    let mut zoom_s = 1.0f64;
    let mut zoom_v = 0.0f64;
    let mut last_move_t = 0.0;
    let mut last_raw = (0.0, 0.0);
    let press_cfg = crate::spring::SpringConfig { stiffness: 500.0, damping: 30.0, mass: 1.0, clamp: true, precision: 0.001 };
    let rot_cfg = crate::spring::SpringConfig { stiffness: 200.0, damping: 20.0, mass: 1.0, clamp: false, precision: 0.001 };
    let edge = anim.follow == "edge";

    for i in 0..count {
        let t_out = (i as f64) * 1000.0 / fps;
        let (t_src, slice_idx) = output_to_source(slices, t_out.min(duration)).unwrap_or((0.0, 0));
        let slice = &slices[slice_idx.min(slices.len().saturating_sub(1))];

        // --- cursor ---
        // Smoothing delays the cursor by about its smoothing time, so aim at where the
        // mouse will be then: the eased cursor then lands on the real position.
        let lead = if slice.disable_smooth_mouse { 0.0 } else { anim.cursor_smooth_ms };
        let (rx, ry, kind) = input.cursor_at(t_src + lead).unwrap_or((0.5, 0.5, CursorKind::Arrow));
        if !initialised || slice.disable_smooth_mouse {
            mx = rx;
            my = ry;
            mvx = 0.0;
            mvy = 0.0;
        } else {
            let st = anim.cursor_smooth_ms / 1000.0;
            mx = smooth_damp(mx, rx, &mut mvx, st, dt);
            my = smooth_damp(my, ry, &mut mvy, st, dt);
        }
        if (rx - last_raw.0).abs() + (ry - last_raw.1).abs() > 1e-4 {
            last_move_t = t_out;
        }
        last_raw = (rx, ry);

        let down = input.button_down_at(t_src);
        press.step(if down { 0.78 } else { 1.0 }, dt, &press_cfg);
        let hide_idle = p.config.cursor.hide_not_moving_after_ms.map(|ms| t_out - last_move_t > ms).unwrap_or(false);
        // the cursor steps out of the way while typing and returns when typing stops
        let typing_now = input.typing_at(t_src, 400.0);
        let want_alpha = if p.config.cursor.hide || slice.hide_cursor || hide_idle || typing_now { 0.0 } else { 1.0 };
        let step = dt / 0.5;
        alpha = if want_alpha > alpha { (alpha + step).min(want_alpha) } else { (alpha - step).max(want_alpha) };
        rotation.step(0.0, dt, &rot_cfg);

        // --- zoom level: eased envelope over every active region ---
        let mut ln_zoom = 0.0f64;
        let mut any_instant = false;
        let mut manual_w = 0.0f64;
        let mut manual_pt = (crop_cx, crop_cy);
        for z in p.scene.zooms.iter().filter(|z| !z.is_disabled) {
            if t_src < z.source_start_ms || t_src >= z.source_end_ms {
                continue;
            }
            let env = if z.instant {
                1.0
            } else {
                let ein = z.ease_in_ms.unwrap_or(anim.ease_in_ms).max(1.0);
                let eout = z.ease_out_ms.unwrap_or(anim.ease_out_ms).max(1.0);
                let a = ease((t_src - z.source_start_ms) / ein);
                let b = ease((z.source_end_ms - t_src) / eout);
                a.min(b)
            };
            let level = z.zoom.max(1.0).ln() * env;
            if level > ln_zoom {
                ln_zoom = level;
            }
            any_instant |= z.instant;
            if z.kind == ZoomKind::Manual && env > manual_w {
                manual_w = env;
                manual_pt = (z.manual_target_point.x, z.manual_target_point.y);
            }
        }

        let zoom_target = ln_zoom.exp();
        if !initialised || any_instant {
            zoom_s = zoom_target;
            zoom_v = 0.0;
        } else {
            zoom_s = smooth_damp(zoom_s, zoom_target, &mut zoom_v, 0.14, dt);
        }
        let zoom = zoom_s.max(1.0);

        // --- camera focus: caret while typing, otherwise the cursor ---
        // (look ahead too, so the lead-in of a typing zoom already heads for the caret)
        let typing = input.typing_at(t_src, 1500.0) || input.typing_ahead(t_src, 900.0);
        let caret = if typing { input.caret_smooth(t_src) } else { None };
        // the camera also aims slightly ahead so the cursor stays close to the middle
        let ahead = input
            .cursor_at(t_src + anim.follow_smooth_ms * 0.7)
            .map(|(x, y, _)| (x, y))
            .unwrap_or((mx, my));
        let (tx, ty) = caret.unwrap_or(ahead);
        let (tx, ty) = (tx.clamp(crop.x, crop.x + crop.width), ty.clamp(crop.y, crop.y + crop.height));
        if !initialised {
            fx = tx;
            fy = ty;
        } else if edge {
            // only pan when the focus leaves the inner 60% of the viewport
            let half_w = crop.width / zoom * 0.5 * 0.6;
            let half_h = crop.height / zoom * 0.5 * 0.6;
            let (dx, dy) = (tx - fx, ty - fy);
            let gx = if dx.abs() > half_w { dx - dx.signum() * half_w } else { 0.0 };
            let gy = if dy.abs() > half_h { dy - dy.signum() * half_h } else { 0.0 };
            let st = anim.follow_smooth_ms / 1000.0;
            fx = smooth_damp(fx, fx + gx, &mut fvx, st, dt);
            fy = smooth_damp(fy, fy + gy, &mut fvy, st, dt);
        } else {
            // follow the caret tightly so it stays as close to the middle as the frame allows
            let st = if caret.is_some() { anim.typing_smooth_ms } else { anim.follow_smooth_ms } / 1000.0;
            fx = smooth_damp(fx, tx, &mut fvx, st, dt);
            fy = smooth_damp(fy, ty, &mut fvy, st, dt);
        }
        initialised = true;

        // manual regions pull the camera toward their fixed point
        let wx = fx + (manual_pt.0 - fx) * manual_w;
        let wy = fy + (manual_pt.1 - fy) * manual_w;

        let half_w = crop.width / zoom.max(1.0) * 0.5;
        let half_h = crop.height / zoom.max(1.0) * 0.5;
        let cxo = clamp_center(wx, half_w, crop.x, crop.x + crop.width);
        let cyo = clamp_center(wy, half_h, crop.y, crop.y + crop.height);

        tr.t_src.push(r4(t_src));
        tr.zoom.push(r4(zoom));
        tr.cx.push(r4(cxo));
        tr.cy.push(r4(cyo));
        tr.mx.push(r4(mx));
        tr.my.push(r4(my));
        tr.cursor_kind.push(kind.index() as u8);
        tr.cursor_scale.push(r4(press.x));
        tr.cursor_alpha.push(r4(alpha));
        tr.cursor_rotation.push(r4(rotation.x));

        // --- transition across a cut (centred on the start of a slice) ---
        let mut tk = 0u8;
        let mut tp = 0.0f64;
        for (si, sl) in slices.iter().enumerate().skip(1) {
            if let Some(t) = &sl.transition {
                let half = (t.duration_ms * 0.5).max(1.0);
                let d = (t_out - starts[si]).abs();
                if d < half {
                    tk = transition_id(&t.kind);
                    // positive before the cut (outgoing picture), negative after it (incoming)
                    let k = 1.0 - d / half;
                    tp = if t_out < starts[si] { k } else { -k };
                    break;
                }
            }
        }
        tr.trans_kind.push(tk);
        tr.trans_p.push(r4(tp));

        // --- camera layout ---
        let want = p
            .scene
            .camera_layouts
            .iter()
            .rev()
            .find(|r| t_src >= r.source_start_ms && t_src < r.source_end_ms)
            .map(|r| layout_id(&r.layout))
            .unwrap_or(0);
        if !lay_init {
            lay_cur = want;
            lay_prev = want;
            lay_init = true;
        } else if want != lay_cur {
            lay_prev = lay_cur;
            lay_cur = want;
            lay_mix = 0.0;
        } else {
            lay_mix = (lay_mix + dt / 0.35).min(1.0);
        }
        tr.layout_cur.push(lay_cur);
        tr.layout_prev.push(lay_prev);
        tr.layout_mix.push(r4(lay_mix));
    }

    // click marks in output time
    for c in input.clicks.iter().filter(|c| c.down) {
        if let Some(t_out) = crate::timeline::source_to_output(slices, c.t) {
            let (nx, ny) = input.normalize(c.x, c.y);
            tr.clicks.push(ClickMark { t_out_ms: r4(t_out), x: r4(nx), y: r4(ny) });
        }
    }
    // shortcuts / special keys for the on-screen key display
    for k in input.keys.iter().filter(|k| k.down && !k.label.is_empty()) {
        if let Some(t_out) = crate::timeline::source_to_output(slices, k.t) {
            tr.combos.push(ComboMark { t_out_ms: r4(t_out), label: k.label.clone(), mods: k.mods });
        }
    }
    // key presses in output time (macOS key codes; the Windows logger maps onto the same ones)
    for k in input.keys.iter().filter(|k| k.down) {
        if let Some(t_out) = crate::timeline::source_to_output(slices, k.t) {
            let kind = match k.key.as_str() {
                "49" => 1,
                "36" | "76" => 2,
                "51" | "117" => 3,
                "48" => 4,
                _ => 0,
            };
            tr.keys.push(KeyMark { t_out_ms: r4(t_out), kind });
        }
    }
    tr
}

#[allow(dead_code)]
/// Optional "loop cursor position": near the end the cursor eases back to
/// where it started so the video loops cleanly.
fn loop_position(p: &Project, input: &InputLog, t_src: f64, raw: (f64, f64)) -> (f64, f64) {
    let Some(before) = p.config.cursor.loop_position_before_end_ms else { return raw };
    let end = p.media.duration_ms;
    if before <= 0.0 || t_src < end - before {
        return raw;
    }
    let Some((sx, sy, _)) = input.cursor_at(0.0) else { return raw };
    let f = ((t_src - (end - before)) / before).clamp(0.0, 1.0);
    let f = f * f * (3.0 - 2.0 * f);
    (raw.0 + (sx - raw.0) * f, raw.1 + (sy - raw.1) * f)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::input::{Button, ClickEvent, MoveEvent};
    use crate::project::Slice;
    use crate::project::{Media, ZoomRegion};

    fn project() -> (Project, InputLog) {
        let mut p = Project::from_recording("t", Media { duration_ms: 6000.0, ..Default::default() });
        p.scene.zooms.push(ZoomRegion {
            id: "z".into(),
            zoom: 2.0,
            source_start_ms: 1000.0,
            source_end_ms: 4000.0,
            ..Default::default()
        });
        let log = InputLog {
            bounds: Rect { x: 0.0, y: 0.0, width: 1000.0, height: 500.0 },
            moves: vec![
                MoveEvent { t: 0.0, x: 800.0, y: 100.0, cursor: CursorKind::Arrow },
                MoveEvent { t: 6000.0, x: 800.0, y: 100.0, cursor: CursorKind::Arrow },
            ],
            clicks: vec![ClickEvent { t: 1300.0, x: 800.0, y: 100.0, button: Button::Left, down: true }],
            ..Default::default()
        };
        (p, log)
    }

    #[test]
    fn zooms_in_toward_cursor_then_out() {
        let (p, log) = project();
        let tr = compute_track(&p, &log, 60.0);
        assert_eq!(tr.count, 361);
        let f = |ms: f64| (ms * 60.0 / 1000.0) as usize;
        assert!((tr.zoom[f(500.0)] - 1.0).abs() < 1e-6);
        assert!((tr.zoom[f(3000.0)] - 2.0).abs() < 0.05);
        assert!(tr.cx[f(3000.0)] > 0.6, "camera should pan right toward the cursor");
        assert!((tr.zoom[f(5900.0)] - 1.0).abs() < 0.05);
        assert_eq!(tr.clicks.len(), 1);
    }

    #[test]
    fn camera_viewport_stays_inside_frame() {
        let (p, log) = project();
        let tr = compute_track(&p, &log, 60.0);
        for i in 0..tr.count {
            let half = 0.5 / tr.zoom[i];
            assert!(tr.cx[i] - half >= -1e-3 && tr.cx[i] + half <= 1.0 + 1e-3);
            assert!(tr.cy[i] - half >= -1e-3 && tr.cy[i] + half <= 1.0 + 1e-3);
        }
    }

    #[test]
    fn typing_moves_focus_to_caret() {
        use crate::input::{CaretEvent, KeyEvent};
        let (p, mut log) = project();
        // cursor parked top-right, text caret on the left side of the screen
        log.keys.push(KeyEvent { t: 3000.0, key: "0".into(), down: true, ..Default::default() });
        log.carets.push(CaretEvent { t: 3050.0, x: 100.0, y: 250.0, w: 2.0, h: 20.0 });
        let tr = compute_track(&p, &log, 60.0);
        let f = |ms: f64| (ms * 60.0 / 1000.0) as usize;
        // well before typing the camera sits on the cursor (x = 0.8)...
        assert!(tr.cx[f(1900.0)] > 0.6);
        // ...and while typing it has moved toward the caret (x ≈ 0.1)
        assert!(tr.cx[f(3800.0)] < tr.cx[f(1900.0)] - 0.2, "camera should pan toward the caret");
    }

    #[test]
    fn center_mode_keeps_cursor_centred() {
        let (p, log) = project();
        let tr = compute_track(&p, &log, 60.0);
        let f = |ms: f64| (ms * 60.0 / 1000.0) as usize;
        let i = f(3500.0);
        // viewport half-width is 0.25; cursor sits at 0.8 so the clamped centre is 0.75
        assert!((tr.cx[i] - 0.75).abs() < 0.02);
    }

    #[test]
    fn zoom_eases_in_over_the_configured_duration() {
        let (mut p, log) = project();
        p.config.animations.ease_in_ms = 400.0;
        let tr = compute_track(&p, &log, 60.0);
        let f = |ms: f64| (ms * 60.0 / 1000.0).round() as usize;
        // region starts at 1000 ms: starts from rest, passes through the middle, and settles
        assert!((tr.zoom[f(1000.0)] - 1.0).abs() < 1e-3);
        let mid = tr.zoom[f(1250.0)];
        assert!(mid > 1.1 && mid < 1.9, "mid-ease zoom {mid}");
        assert!((tr.zoom[f(1900.0)] - 2.0).abs() < 0.02);
        // monotonic on the way in, no sudden jumps anywhere
        for i in f(1000.0)..f(1700.0) {
            assert!(tr.zoom[i + 1] >= tr.zoom[i] - 1e-6, "zoom must not go backwards while easing in");
        }
        for i in 1..tr.count {
            assert!((tr.zoom[i] - tr.zoom[i - 1]).abs() < 0.08);
        }
    }

    #[test]
    fn camera_pan_is_smooth_between_frames() {
        let (p, mut log) = project();
        // cursor jumps across the screen at 2 s
        log.moves = vec![
            crate::input::MoveEvent { t: 0.0, x: 800.0, y: 100.0, cursor: CursorKind::Arrow },
            crate::input::MoveEvent { t: 2000.0, x: 800.0, y: 100.0, cursor: CursorKind::Arrow },
            crate::input::MoveEvent { t: 2010.0, x: 100.0, y: 400.0, cursor: CursorKind::Arrow },
            crate::input::MoveEvent { t: 6000.0, x: 100.0, y: 400.0, cursor: CursorKind::Arrow },
        ];
        let tr = compute_track(&p, &log, 60.0);
        for i in 1..tr.count {
            assert!((tr.cx[i] - tr.cx[i - 1]).abs() < 0.03, "camera snapped at frame {i}");
        }
    }

    #[test]
    fn camera_heads_for_the_caret_before_typing_starts() {
        use crate::input::{CaretEvent, KeyEvent};
        let (p, mut log) = project();
        log.keys.push(KeyEvent { t: 2500.0, key: "0".into(), down: true, ..Default::default() });
        log.carets.push(CaretEvent { t: 2550.0, x: 100.0, y: 250.0, w: 2.0, h: 20.0 });
        let tr = compute_track(&p, &log, 60.0);
        let f = |ms: f64| (ms * 60.0 / 1000.0) as usize;
        // 800 ms before the first key the camera has already started moving left
        assert!(tr.cx[f(2450.0)] < tr.cx[f(1500.0)] - 0.01);
    }

    #[test]
    fn typing_keeps_the_caret_near_the_centre() {
        use crate::input::{CaretEvent, KeyEvent};
        let (mut p, mut log) = project();
        p.scene.zooms[0].source_start_ms = 500.0;
        p.scene.zooms[0].source_end_ms = 5500.0;
        // caret walks steadily left to right across the middle of the screen, 20 Hz samples
        for i in 0..80 {
            let t = 1000.0 + i as f64 * 50.0;
            log.keys.push(KeyEvent { t, key: "0".into(), down: true, ..Default::default() });
            let x = 300.0 + i as f64 * 5.0; // 300..695 px of a 1000 px wide capture
            log.carets.push(CaretEvent { t, x, y: 250.0, w: 2.0, h: 20.0 });
        }
        let tr = compute_track(&p, &log, 60.0);
        let f = |ms: f64| (ms * 60.0 / 1000.0).round() as usize;
        for ms in [2000.0, 2600.0, 3200.0, 3800.0] {
            let i = f(ms);
            let caret_x = (300.0 + ((ms - 1000.0) / 50.0) * 5.0 + 1.0) / 1000.0;
            let off = (tr.cx[i] - caret_x).abs();
            assert!(off < 0.04, "caret {caret_x:.3} vs camera {:.3} at {ms} ms", tr.cx[i]);
        }
    }

    #[test]
    fn eased_cursor_does_not_lag_the_real_mouse() {
        let (p, mut log) = project();
        log.moves.clear();
        for i in 0..=100 {
            let t = i as f64 * 50.0;
            log.moves.push(MoveEvent { t, x: 100.0 + i as f64 * 6.0, y: 500.0, cursor: CursorKind::Arrow });
        }
        let tr = compute_track(&p, &log, 60.0);
        for ms in [1500.0_f64, 2500.0, 3500.0] {
            let i = (ms * 60.0 / 1000.0).round() as usize;
            let real = (100.0 + ms / 50.0 * 6.0) / 1000.0;
            assert!((tr.mx[i] - real).abs() < 0.01, "cursor {:.3} vs real {real:.3} at {ms}", tr.mx[i]);
        }
    }

    #[test]
    fn key_marks_follow_the_timeline() {
        use crate::input::KeyEvent;
        let (p, mut log) = project();
        log.keys.push(KeyEvent { t: 2000.0, key: "49".into(), down: true, ..Default::default() });
        log.keys.push(KeyEvent { t: 2100.0, key: "0".into(), down: true, ..Default::default() });
        log.keys.push(KeyEvent { t: 2150.0, key: "0".into(), down: false, ..Default::default() });
        let tr = compute_track(&p, &log, 60.0);
        assert_eq!(tr.keys.len(), 2);
        assert_eq!(tr.keys[0].kind, 1);
        assert!((tr.keys[0].t_out_ms - 2000.0).abs() < 1.0);
    }

    #[test]
    fn layout_and_transition_marks() {
        use crate::project::{CameraLayoutRegion, Transition};
        let (mut p, log) = project();
        p.scene.camera_layouts.push(CameraLayoutRegion {
            id: "l".into(),
            source_start_ms: 2000.0,
            source_end_ms: 4000.0,
            layout: "side-right".into(),
        });
        // cut the recording in two at 3000 ms with a fade across the join
        p.scene.slices = vec![
            Slice { id: "a".into(), source_start_ms: 0.0, source_end_ms: 3000.0, ..Default::default() },
            Slice {
                id: "b".into(),
                source_start_ms: 3000.0,
                source_end_ms: 6000.0,
                transition: Some(Transition { kind: "fade".into(), duration_ms: 400.0 }),
                ..Default::default()
            },
        ];
        let tr = compute_track(&p, &log, 60.0);
        let f = |ms: f64| (ms * 60.0 / 1000.0) as usize;
        assert_eq!(tr.layout_cur[f(1000.0)], 0);
        assert_eq!(tr.layout_cur[f(2500.0)], 4);
        assert!(tr.layout_mix[f(2010.0)] < 1.0);
        assert_eq!(tr.layout_prev[f(2010.0)], 0);
        assert_eq!(tr.trans_kind[f(3000.0)], 1);
        assert!(tr.trans_p[f(3000.0)] > 0.9);
        assert_eq!(tr.trans_kind[f(1000.0)], 0);
    }

    #[test]
    fn cursor_fades_out_while_typing_and_back_in_over_half_a_second() {
        use crate::input::KeyEvent;
        let (p, mut log) = project();
        for i in 0..20 {
            log.keys.push(KeyEvent { t: 1000.0 + i as f64 * 100.0, key: "0".into(), down: true, ..Default::default() });
        }
        let tr = compute_track(&p, &log, 60.0);
        let f = |ms: f64| (ms * 60.0 / 1000.0).round() as usize;
        assert!(tr.cursor_alpha[f(500.0)] > 0.99);
        assert!(tr.cursor_alpha[f(2000.0)] < 0.01, "hidden while typing");
        // last key at 2900, hold 400 ms, then 0.5 s ramp back to full
        assert!(tr.cursor_alpha[f(3600.0)] > 0.2 && tr.cursor_alpha[f(3600.0)] < 0.8);
        assert!(tr.cursor_alpha[f(4200.0)] > 0.99);
    }
}
