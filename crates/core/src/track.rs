//! Per-frame render track: where the camera and cursor are at every output
//! frame. Computed once in Rust; the UI just paints it, so preview and export
//! always agree.

use crate::input::{CursorKind, InputLog};
use crate::project::{Project, Rect, ZoomKind};
use crate::spring::Spring;
use crate::timeline::{output_duration_ms, output_to_source};
use serde::{Deserialize, Serialize};

#[derive(Clone, Debug, Serialize, Deserialize, Default)]
#[serde(rename_all = "camelCase")]
pub struct ClickMark {
    pub t_out_ms: f64,
    pub x: f64,
    pub y: f64,
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

    let crop_cx = crop.x + crop.width / 2.0;
    let crop_cy = crop.y + crop.height / 2.0;
    let mut zoom = Spring::new(1.0);
    let mut cam_x = Spring::new(crop_cx);
    let mut cam_y = Spring::new(crop_cy);
    let mut mx = Spring::default();
    let mut my = Spring::default();
    let mut press = Spring::new(1.0);
    let mut alpha = Spring::new(1.0);
    let mut focus = (crop_cx, crop_cy);
    let mut active_region: Option<usize> = None;
    let mut initialised = false;
    let mut last_move_t = 0.0;
    let mut last_raw = (0.0, 0.0);
    let mut rotation = Spring::new(0.0);
    let press_cfg = crate::spring::SpringConfig { stiffness: 500.0, damping: 30.0, mass: 1.0, clamp: true, precision: 0.001 };
    let fade_cfg = crate::spring::SpringConfig { stiffness: 300.0, damping: 34.0, mass: 1.0, clamp: true, precision: 0.001 };
    let rot_cfg = crate::spring::SpringConfig { stiffness: 200.0, damping: 20.0, mass: 1.0, clamp: false, precision: 0.001 };

    for i in 0..count {
        let t_out = (i as f64) * 1000.0 / fps;
        let (t_src, slice_idx) = output_to_source(slices, t_out.min(duration)).unwrap_or((0.0, 0));
        let slice = &slices[slice_idx.min(slices.len().saturating_sub(1))];

        // --- cursor ---
        let (rx, ry, kind) = input.cursor_at(t_src).unwrap_or((0.5, 0.5, CursorKind::Arrow));
        let (rx, ry) = loop_position(p, input, t_src, (rx, ry));
        if !initialised {
            mx = Spring::new(rx);
            my = Spring::new(ry);
            focus = (rx.clamp(crop.x, crop.x + crop.width), ry.clamp(crop.y, crop.y + crop.height));
        }
        if slice.disable_smooth_mouse || !initialised {
            mx = Spring::new(rx);
            my = Spring::new(ry);
        } else {
            mx.step(rx, dt, &anim.mouse_spring);
            my.step(ry, dt, &anim.mouse_spring);
        }
        if (rx - last_raw.0).abs() + (ry - last_raw.1).abs() > 1e-4 {
            last_move_t = t_out;
        }
        last_raw = (rx, ry);

        let down = input.button_down_at(t_src);
        press.step(if down { 0.78 } else { 1.0 }, dt, &press_cfg);
        let hide_idle = p
            .config
            .cursor
            .hide_not_moving_after_ms
            .map(|ms| t_out - last_move_t > ms)
            .unwrap_or(false);
        let want_alpha = if p.config.cursor.hide || slice.hide_cursor || hide_idle { 0.0 } else { 1.0 };
        alpha.step(want_alpha, dt, &fade_cfg);
        let vx = (mx.v).clamp(-3.0, 3.0);
        rotation.step(if p.config.cursor.rotate_with_velocity { vx * 0.35 } else { 0.0 }, dt, &rot_cfg);

        // --- camera ---
        let region_idx = p.scene.zooms.iter().position(|z| {
            !z.is_disabled && t_src >= z.source_start_ms && t_src < z.source_end_ms
        });
        let (target_zoom, target_center) = match region_idx {
            Some(idx) => {
                let z = &p.scene.zooms[idx];
                let zf = z.zoom.max(1.0);
                let half_w = crop.width / zf * 0.5;
                let half_h = crop.height / zf * 0.5;
                if active_region != Some(idx) {
                    // entering a region: start focused on the cursor
                    focus = (rx, ry);
                }
                let centre = match z.kind {
                    ZoomKind::Manual => (z.manual_target_point.x, z.manual_target_point.y),
                    ZoomKind::FollowCursor => {
                        let allow_x = half_w * 0.6;
                        let allow_y = half_h * 0.6;
                        let (px, py) = (mx.x, my.x);
                        let dx = px - focus.0;
                        let dy = py - focus.1;
                        if dx.abs() > allow_x {
                            focus.0 += dx - dx.signum() * allow_x;
                        }
                        if dy.abs() > allow_y {
                            focus.1 += dy - dy.signum() * allow_y;
                        }
                        focus
                    }
                };
                let cxx = clamp_center(centre.0, half_w, crop.x, crop.x + crop.width);
                let cyy = clamp_center(centre.1, half_h, crop.y, crop.y + crop.height);
                (zf, (cxx, cyy))
            }
            None => (1.0, (crop_cx, crop_cy)),
        };
        let instant = region_idx.map(|idx| p.scene.zooms[idx].instant).unwrap_or(false);
        if !initialised {
            zoom = Spring::new(target_zoom);
            cam_x = Spring::new(target_center.0);
            cam_y = Spring::new(target_center.1);
        } else if instant && active_region != region_idx {
            zoom = Spring::new(target_zoom);
            cam_x = Spring::new(target_center.0);
            cam_y = Spring::new(target_center.1);
        } else {
            zoom.step(target_zoom, dt, &anim.screen_spring);
            cam_x.step(target_center.0, dt, &anim.screen_spring);
            cam_y.step(target_center.1, dt, &anim.screen_spring);
        }
        active_region = region_idx;
        initialised = true;

        // keep the viewport inside the crop while the spring is mid-flight
        let half_w = crop.width / zoom.x.max(1.0) * 0.5;
        let half_h = crop.height / zoom.x.max(1.0) * 0.5;
        let cxo = clamp_center(cam_x.x, half_w, crop.x, crop.x + crop.width);
        let cyo = clamp_center(cam_y.x, half_h, crop.y, crop.y + crop.height);

        tr.t_src.push(r4(t_src));
        tr.zoom.push(r4(zoom.x));
        tr.cx.push(r4(cxo));
        tr.cy.push(r4(cyo));
        tr.mx.push(r4(mx.x));
        tr.my.push(r4(my.x));
        tr.cursor_kind.push(kind.index() as u8);
        tr.cursor_scale.push(r4(press.x));
        tr.cursor_alpha.push(r4(alpha.x));
        tr.cursor_rotation.push(r4(rotation.x));
    }

    // click marks in output time
    for c in input.clicks.iter().filter(|c| c.down) {
        if let Some(t_out) = crate::timeline::source_to_output(slices, c.t) {
            let (nx, ny) = input.normalize(c.x, c.y);
            tr.clicks.push(ClickMark { t_out_ms: r4(t_out), x: r4(nx), y: r4(ny) });
        }
    }
    tr
}

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
            assert!(tr.cx[i] - half >= -1e-6 && tr.cx[i] + half <= 1.0 + 1e-6);
            assert!(tr.cy[i] - half >= -1e-6 && tr.cy[i] + half <= 1.0 + 1e-6);
        }
    }
}
