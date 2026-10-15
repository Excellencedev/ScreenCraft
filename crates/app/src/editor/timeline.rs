//! Timeline: ruler, zoom regions and slices in output time, with trim / move / split.

use super::{EditorState, Selection};
use crate::theme;
use eframe::egui::{self, Color32, CornerRadius, Pos2, Rect, Sense, Stroke, Vec2};
use screencraft_core::project::{Slice, ZoomKind, ZoomRegion};
use screencraft_core::timeline::{output_duration_ms, slice_output_starts, source_to_output};

#[derive(Default)]
pub struct View {
    pub start_ms: f64,
    pub px_per_ms: f64,
    pub fit_pending: bool,
    drag: Option<Drag>,
}

#[derive(Clone, Copy)]
enum Drag {
    Playhead,
    SliceEdge { idx: usize, left: bool, orig_source: f64, press_out: f64 },
    ZoomMove { idx: usize, grab_src: f64, orig_start: f64, orig_end: f64 },
    ZoomEdge { idx: usize, left: bool },
}

const RULER_H: f32 = 24.0;
const ZOOM_Y: f32 = 30.0;
const ZOOM_H: f32 = 30.0;
const SLICE_Y: f32 = 68.0;
const SLICE_H: f32 = 74.0;
const EDGE: f32 = 7.0;
const MIN_LEN_MS: f64 = 120.0;

fn src_to_out(slices: &[Slice], t: f64, total: f64) -> f64 {
    if let Some(o) = source_to_output(slices, t) {
        return o;
    }
    // Inside a cut: snap to the start of the next kept slice.
    let starts = slice_output_starts(slices);
    for (i, s) in slices.iter().enumerate() {
        if s.source_start_ms >= t {
            return starts[i];
        }
    }
    total
}

/// Output-time length of one source millisecond around `t` (the slice speed).
fn scale_at(slices: &[Slice], t_src: f64) -> f64 {
    slices
        .iter()
        .find(|s| t_src >= s.source_start_ms && t_src <= s.source_end_ms)
        .map(|s| s.time_scale.max(0.05))
        .unwrap_or(1.0)
}

fn nice_step_ms(px_per_ms: f64) -> f64 {
    let want = 90.0 / px_per_ms; // ms per ~90 px
    for s in [100.0, 250.0, 500.0, 1000.0, 2000.0, 5000.0, 10_000.0, 15_000.0, 30_000.0, 60_000.0, 120_000.0, 300_000.0, 600_000.0] {
        if s >= want {
            return s;
        }
    }
    1_200_000.0
}

fn ruler_label(t: f64, step: f64) -> String {
    let total = t.max(0.0) / 1000.0;
    let (m, s) = ((total / 60.0).floor() as u64, total % 60.0);
    if step < 1000.0 {
        format!("{m}:{s:04.1}")
    } else {
        format!("{m}:{:02}", s.floor() as u64)
    }
}

pub fn show(st: &mut EditorState, ui: &mut egui::Ui) {
    let p = theme::palette(ui.ctx());
    let total = st.duration_ms();

    // Toolbar
    ui.horizontal(|ui| {
        ui.add_space(10.0);
        if ui.button("Split").on_hover_text("Split at the playhead (S)").clicked() {
            st.split_at_playhead();
        }
        let can_delete = match st.selection {
            Selection::Slice(_) => st.project.scene.slices.len() > 1,
            Selection::Zoom(_) => true,
            Selection::None => false,
        };
        ui.add_enabled_ui(can_delete, |ui| {
            if ui.button("Delete").on_hover_text("Remove the selection (Delete)").clicked() {
                st.delete_selection();
            }
        });
        ui.separator();
        if ui.button("Add zoom").on_hover_text("A zoom region at the playhead").clicked() {
            let t = st.playhead_source_ms();
            let id = st.new_id("zoom");
            let dur = st.project.media.duration_ms;
            let zoom = st.project.config.animations.default_zoom;
            let n = st.project.scene.zooms.len();
            let (start, end) = (t, (t + 2000.0).min(dur));
            st.edit(|pr| {
                pr.scene.zooms.push(ZoomRegion {
                    id,
                    zoom,
                    kind: ZoomKind::FollowCursor,
                    source_start_ms: start,
                    source_end_ms: end.max(start + MIN_LEN_MS),
                    ..Default::default()
                })
            });
            st.selection = Selection::Zoom(n);
            st.tab = super::Tab::Zoom;
        }
        if ui.button("Auto zoom").on_hover_text("Generate zooms from clicks and typing").clicked() {
            st.auto_zoom();
        }
        ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
            ui.add_space(10.0);
            if ui.small_button("Fit").clicked() {
                st.timeline.fit_pending = true;
            }
            ui.label(egui::RichText::new("Ctrl/Cmd + scroll to zoom").small().color(p.dim));
        });
    });
    ui.add_space(2.0);

    let avail = ui.available_size();
    let (rect, resp) = ui.allocate_exact_size(Vec2::new(avail.x, avail.y.max(160.0)), Sense::click_and_drag());
    if rect.width() < 50.0 {
        return;
    }
    let width = rect.width() as f64;
    if st.timeline.fit_pending || st.timeline.px_per_ms <= 0.0 {
        st.timeline.px_per_ms = ((width - 40.0) / total.max(500.0)).max(0.001);
        st.timeline.start_ms = -20.0 / st.timeline.px_per_ms;
        st.timeline.fit_pending = false;
    }

    // Wheel: zoom with a modifier, pan otherwise.
    if resp.hovered() {
        let (scroll, zoom_mod, pointer) = ui.ctx().input(|i| {
            (i.smooth_scroll_delta, i.modifiers.command || i.modifiers.ctrl, i.pointer.hover_pos())
        });
        let v = &mut st.timeline;
        if zoom_mod && scroll.y != 0.0 {
            if let Some(pos) = pointer {
                let anchor = v.start_ms + (pos.x - rect.min.x) as f64 / v.px_per_ms;
                v.px_per_ms = (v.px_per_ms * (1.0 + scroll.y as f64 * 0.01)).clamp(0.0008, 2.0);
                v.start_ms = anchor - (pos.x - rect.min.x) as f64 / v.px_per_ms;
            }
        } else if scroll.x != 0.0 || scroll.y != 0.0 {
            let d = if scroll.x != 0.0 { scroll.x } else { scroll.y };
            v.start_ms -= d as f64 / v.px_per_ms;
        }
    }
    let ppm = st.timeline.px_per_ms;
    let start = st.timeline.start_ms;
    let x_of = |t: f64| rect.min.x + ((t - start) * ppm) as f32;
    let t_of = |x: f32| start + (x - rect.min.x) as f64 / ppm;

    let painter = ui.painter_at(rect);
    painter.rect_filled(rect, 0.0, p.bg);

    // Ruler
    let step = nice_step_ms(ppm);
    let first = (start / step).floor() * step;
    let mut t = first;
    while x_of(t) < rect.max.x {
        let x = x_of(t);
        if x >= rect.min.x - 1.0 {
            painter.line_segment([Pos2::new(x, rect.min.y + RULER_H - 7.0), Pos2::new(x, rect.min.y + RULER_H)], Stroke::new(1.0, p.line));
            painter.text(
                Pos2::new(x + 4.0, rect.min.y + 3.0),
                egui::Align2::LEFT_TOP,
                ruler_label(t, step),
                egui::FontId::proportional(10.5),
                p.dim,
            );
        }
        t += step;
    }
    painter.line_segment([Pos2::new(rect.min.x, rect.min.y + RULER_H), Pos2::new(rect.max.x, rect.min.y + RULER_H)], Stroke::new(1.0, p.line));

    // Slices
    let slices = st.project.scene.slices.clone();
    let starts = slice_output_starts(&slices);
    let slice_rect = |i: usize| -> Rect {
        let s = &slices[i];
        let len = (s.source_end_ms - s.source_start_ms) / s.time_scale.max(1e-3);
        Rect::from_min_max(
            Pos2::new(x_of(starts[i]), rect.min.y + SLICE_Y),
            Pos2::new(x_of(starts[i] + len), rect.min.y + SLICE_Y + SLICE_H),
        )
    };
    for (i, s) in slices.iter().enumerate() {
        let r = slice_rect(i);
        if r.max.x < rect.min.x || r.min.x > rect.max.x {
            continue;
        }
        let selected = st.selection == Selection::Slice(i);
        painter.rect_filled(r.shrink2(Vec2::new(0.5, 0.0)), CornerRadius::same(6), if selected { p.slice_sel } else { p.slice });
        if selected {
            painter.rect_stroke(r, CornerRadius::same(6), Stroke::new(1.5, p.accent), egui::StrokeKind::Inside);
        }
        // Waveform inside the slice
        if let Some(w) = &st.waveform {
            let clip = r.intersect(rect);
            let mid = r.center().y;
            let h = r.height() * 0.42;
            let mut x = clip.min.x;
            while x < clip.max.x {
                let t_out = t_of(x);
                let t_src = s.source_start_ms + (t_out - starts[i]) * s.time_scale;
                let idx = (t_src / 20.0).max(0.0) as usize;
                if let Some(v) = w.get(idx) {
                    let a = (v.sqrt() * h).max(0.5);
                    painter.line_segment(
                        [Pos2::new(x, mid - a), Pos2::new(x, mid + a)],
                        Stroke::new(1.0, Color32::from_white_alpha(70)),
                    );
                }
                x += 2.0;
            }
        }
        let label = if (s.time_scale - 1.0).abs() > 0.01 { format!("Clip {}  ·  {:.2}×", i + 1, s.time_scale) } else { format!("Clip {}", i + 1) };
        painter.with_clip_rect(r.intersect(rect)).text(
            r.min + Vec2::new(8.0, 6.0),
            egui::Align2::LEFT_TOP,
            label,
            egui::FontId::proportional(11.5),
            p.text,
        );
    }

    // Zoom regions
    let zooms = st.project.scene.zooms.clone();
    let zoom_rect = |z: &ZoomRegion| -> Rect {
        let a = src_to_out(&slices, z.source_start_ms, total);
        let b = src_to_out(&slices, z.source_end_ms, total).max(a);
        Rect::from_min_max(Pos2::new(x_of(a), rect.min.y + ZOOM_Y), Pos2::new(x_of(b).max(x_of(a) + 3.0), rect.min.y + ZOOM_Y + ZOOM_H))
    };
    for (i, z) in zooms.iter().enumerate() {
        let r = zoom_rect(z);
        if r.max.x < rect.min.x || r.min.x > rect.max.x {
            continue;
        }
        let selected = st.selection == Selection::Zoom(i);
        let col = if z.is_disabled { p.line } else if selected { p.zoom_sel } else { p.zoom };
        painter.rect_filled(r, CornerRadius::same(5), col);
        if selected {
            painter.rect_stroke(r, CornerRadius::same(5), Stroke::new(1.5, p.accent), egui::StrokeKind::Inside);
        }
        painter.with_clip_rect(r.intersect(rect)).text(
            r.min + Vec2::new(7.0, 7.0),
            egui::Align2::LEFT_TOP,
            format!("{:.1}×{}", z.zoom, if z.kind == ZoomKind::Manual { "  ·  fixed" } else { "" }),
            egui::FontId::proportional(11.0),
            p.text,
        );
    }

    // Click markers on the ruler
    let clicks: Vec<f64> = st.track().clicks.iter().map(|c| c.t_out_ms).collect();
    for c in clicks {
        let x = x_of(c);
        if x >= rect.min.x && x <= rect.max.x {
            painter.circle_filled(Pos2::new(x, rect.min.y + RULER_H - 3.0), 2.0, p.accent.gamma_multiply(0.8));
        }
    }

    // Playhead
    let px = x_of(st.playhead_ms);
    painter.line_segment([Pos2::new(px, rect.min.y), Pos2::new(px, rect.max.y)], Stroke::new(1.5, p.accent));
    painter.add(egui::Shape::convex_polygon(
        vec![Pos2::new(px - 6.0, rect.min.y), Pos2::new(px + 6.0, rect.min.y), Pos2::new(px, rect.min.y + 9.0)],
        p.accent,
        Stroke::NONE,
    ));

    // ---- interaction ----
    let pos = resp.interact_pointer_pos().or(resp.hover_pos());
    let in_lane = |y: f32, top: f32, h: f32| y >= rect.min.y + top && y <= rect.min.y + top + h;

    if resp.drag_started() || resp.clicked() {
        if let Some(pos) = pos {
            st.timeline.drag = None;
            let t = t_of(pos.x);
            if pos.y < rect.min.y + RULER_H + 2.0 {
                st.timeline.drag = Some(Drag::Playhead);
                st.seek(t);
            } else if in_lane(pos.y, ZOOM_Y, ZOOM_H) {
                let hit = zooms.iter().enumerate().rev().find(|(_, z)| zoom_rect(z).expand2(Vec2::new(EDGE * 0.5, 0.0)).contains(pos));
                match hit {
                    Some((i, z)) => {
                        let r = zoom_rect(z);
                        st.selection = Selection::Zoom(i);
                        st.tab = super::Tab::Zoom;
                        let left = (pos.x - r.min.x).abs() <= EDGE;
                        let right = (pos.x - r.max.x).abs() <= EDGE;
                        st.timeline.drag = Some(if left {
                            Drag::ZoomEdge { idx: i, left: true }
                        } else if right {
                            Drag::ZoomEdge { idx: i, left: false }
                        } else {
                            Drag::ZoomMove { idx: i, grab_src: z.source_start_ms, orig_start: z.source_start_ms, orig_end: z.source_end_ms }
                        });
                        // Remember where inside the block we grabbed.
                        if let Some(Drag::ZoomMove { grab_src, .. }) = st.timeline.drag.as_mut() {
                            *grab_src = t;
                        }
                    }
                    None => {
                        st.selection = Selection::None;
                        st.seek(t);
                    }
                }
            } else if in_lane(pos.y, SLICE_Y, SLICE_H) {
                let hit = (0..slices.len()).find(|&i| slice_rect(i).expand2(Vec2::new(EDGE * 0.5, 0.0)).contains(pos));
                match hit {
                    Some(i) => {
                        let r = slice_rect(i);
                        st.selection = Selection::Slice(i);
                        st.tab = super::Tab::Clip;
                        if (pos.x - r.min.x).abs() <= EDGE {
                            st.timeline.drag = Some(Drag::SliceEdge { idx: i, left: true, orig_source: slices[i].source_start_ms, press_out: t });
                        } else if (pos.x - r.max.x).abs() <= EDGE {
                            st.timeline.drag = Some(Drag::SliceEdge { idx: i, left: false, orig_source: slices[i].source_end_ms, press_out: t });
                        } else {
                            st.seek(t);
                        }
                    }
                    None => {
                        st.selection = Selection::None;
                        st.seek(t);
                    }
                }
            } else {
                st.selection = Selection::None;
                st.seek(t);
            }
        }
    }

    if resp.dragged() {
        if let (Some(pos), Some(drag)) = (pos, st.timeline.drag) {
            let t = t_of(pos.x);
            match drag {
                Drag::Playhead => st.seek(t),
                Drag::SliceEdge { idx, left, orig_source, press_out } => {
                    let scale = slices.get(idx).map(|s| s.time_scale.max(0.05)).unwrap_or(1.0);
                    let delta_src = (t - press_out) * scale;
                    let dur = st.project.media.duration_ms;
                    st.edit(|pr| {
                        if let Some(s) = pr.scene.slices.get_mut(idx) {
                            if left {
                                s.source_start_ms = (orig_source + delta_src).clamp(0.0, s.source_end_ms - MIN_LEN_MS);
                            } else {
                                s.source_end_ms = (orig_source + delta_src).clamp(s.source_start_ms + MIN_LEN_MS, dur);
                            }
                        }
                    });
                }
                Drag::ZoomMove { idx, grab_src, orig_start, orig_end } => {
                    let scale = scale_at(&slices, orig_start);
                    let dur = st.project.media.duration_ms;
                    let delta = (t - grab_src) * scale;
                    let len = orig_end - orig_start;
                    st.edit(|pr| {
                        if let Some(z) = pr.scene.zooms.get_mut(idx) {
                            let s = (orig_start + delta).clamp(0.0, (dur - len).max(0.0));
                            z.source_start_ms = s;
                            z.source_end_ms = s + len;
                        }
                    });
                }
                Drag::ZoomEdge { idx, left } => {
                    let t_src = screencraft_core::timeline::output_to_source(&slices, t.clamp(0.0, total)).map(|x| x.0);
                    if let Some(ts) = t_src {
                        let dur = st.project.media.duration_ms;
                        st.edit(|pr| {
                            if let Some(z) = pr.scene.zooms.get_mut(idx) {
                                if left {
                                    z.source_start_ms = ts.clamp(0.0, z.source_end_ms - MIN_LEN_MS);
                                } else {
                                    z.source_end_ms = ts.clamp(z.source_start_ms + MIN_LEN_MS, dur);
                                }
                            }
                        });
                    }
                }
            }
        }
    }
    if resp.drag_stopped() {
        st.timeline.drag = None;
    }

    // Cursor feedback on edges
    if let Some(h) = resp.hover_pos() {
        let on_edge = (0..slices.len()).any(|i| {
            let r = slice_rect(i);
            in_lane(h.y, SLICE_Y, SLICE_H) && ((h.x - r.min.x).abs() <= EDGE || (h.x - r.max.x).abs() <= EDGE)
        }) || zooms.iter().any(|z| {
            let r = zoom_rect(z);
            in_lane(h.y, ZOOM_Y, ZOOM_H) && ((h.x - r.min.x).abs() <= EDGE || (h.x - r.max.x).abs() <= EDGE)
        });
        if on_edge {
            ui.ctx().set_cursor_icon(egui::CursorIcon::ResizeHorizontal);
        }
    }
    let _ = output_duration_ms(&slices);
}
