//! The editor: preview canvas, transport, timeline, inspector and export.

mod export_ui;
mod inspector;
mod playback;
mod preview;
mod timeline;

use crate::theme::{self, fmt_time};
use eframe::egui::{self, Color32, Rect, Vec2};
use screencraft_core::input::InputLog;
use screencraft_core::project::Project;
use screencraft_core::timeline::{output_duration_ms, output_to_source};
use screencraft_core::track::{compute_track, RenderTrack};
use screencraft_engine::{build_soundtrack, FrameTable, Package};
use screencraft_render::{build_scene, output_size, Compositor};
use std::sync::Arc;
use std::time::{Duration, Instant};

pub use export_ui::ExportState;
use playback::AudioPlayback;
use preview::PreviewWorker;

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum Selection {
    None,
    Slice(usize),
    Zoom(usize),
}

#[derive(Clone, Copy, PartialEq, Eq)]
pub enum Tab {
    Clip,
    Zoom,
    Cursor,
    Style,
    Output,
    Audio,
}

/// Preview frame rate of the render track (editing); exports use their own.
const TRACK_FPS: f64 = 60.0;

pub struct EditorState {
    pub package: Package,
    pub project: Project,
    pub input: InputLog,
    track: RenderTrack,
    track_dirty: bool,
    undo: Vec<Project>,
    redo: Vec<Project>,
    last_undo_push: Instant,
    pub selection: Selection,
    pub tab: Tab,
    /// Playhead in output (edited) time.
    pub playhead_ms: f64,
    playing: bool,
    play_anchor: Option<(Instant, f64)>,
    table: FrameTable,
    worker: PreviewWorker,
    uploaded_index: Option<usize>,
    compositor: Compositor,
    render_state: eframe::egui_wgpu::RenderState,
    tex: Option<(egui::TextureId, (u32, u32))>,
    audio: Option<AudioPlayback>,
    pub timeline: timeline::View,
    pub waveform: Option<Arc<Vec<f32>>>,
    wave_rx: Option<std::sync::mpsc::Receiver<Vec<f32>>>,
    pub export: ExportState,
    dirty_since: Option<Instant>,
    pub error: Option<String>,
    id_counter: u64,
    /// Last rendered content rect in screen space (for the zoom-target picker).
    preview_content: Option<Rect>,
}

impl EditorState {
    pub fn open(package: Package, ctx: &egui::Context, render_state: eframe::egui_wgpu::RenderState) -> Result<Self, String> {
        let mut project = package.load_project()?;
        let input = package.load_input(&project);
        let screen = package.screen_path(&project);
        let table = FrameTable::open(&screen)?;
        if project.scene.slices.is_empty() {
            project.scene.slices.push(screencraft_core::project::Slice {
                id: "slice-0".into(),
                source_start_ms: 0.0,
                source_end_ms: project.media.duration_ms,
                ..Default::default()
            });
        }
        let worker = PreviewWorker::spawn(screen.clone(), ctx.clone());
        let compositor = Compositor::new(&render_state.device, &render_state.queue);

        // Waveform peaks, computed once in the background.
        let (wtx, wrx) = std::sync::mpsc::channel();
        {
            let (path, ctx) = (screen.clone(), ctx.clone());
            let track = project.media.system_track.or(project.media.mic_track);
            std::thread::spawn(move || {
                if let Some(t) = track {
                    if let Ok(peaks) = waveform_peaks(&path, t as usize) {
                        let _ = wtx.send(peaks);
                        ctx.request_repaint();
                    }
                }
            });
        }
        let mut s = Self {
            package,
            project,
            input,
            track: RenderTrack::default(),
            track_dirty: true,
            undo: Vec::new(),
            redo: Vec::new(),
            last_undo_push: Instant::now() - Duration::from_secs(10),
            selection: Selection::None,
            tab: Tab::Style,
            playhead_ms: 0.0,
            playing: false,
            play_anchor: None,
            table,
            worker,
            uploaded_index: None,
            compositor,
            render_state,
            tex: None,
            audio: None,
            timeline: timeline::View::default(),
            waveform: None,
            wave_rx: Some(wrx),
            export: ExportState::default(),
            dirty_since: None,
            error: None,
            id_counter: 0,
            preview_content: None,
        };
        s.timeline.fit_pending = true;
        Ok(s)
    }

    /// Developer hook: `zoom:N`, `slice:N` or a tab name from the environment.
    pub fn apply_dev_selection(&mut self, what: Option<&str>) {
        let Some(w) = what else { return };
        if w == "auto-zoom" {
            self.auto_zoom();
        } else if let Some(n) = w.strip_prefix("zoom:").and_then(|n| n.parse().ok()) {
            self.selection = Selection::Zoom(n);
            self.tab = Tab::Zoom;
        } else if let Some(n) = w.strip_prefix("slice:").and_then(|n| n.parse().ok()) {
            self.selection = Selection::Slice(n);
            self.tab = Tab::Clip;
        } else {
            self.tab = match w {
                "cursor" => Tab::Cursor,
                "output" => Tab::Output,
                "audio" => Tab::Audio,
                "zoom" => Tab::Zoom,
                "clip" => Tab::Clip,
                _ => Tab::Style,
            };
        }
    }

    /// Replace the zoom regions with ones generated from clicks and typing.
    pub fn auto_zoom(&mut self) {
        let opts = screencraft_core::zoom::AutoZoomOptions::for_ease(
            self.project.config.animations.default_zoom,
            self.project.config.animations.ease_in_ms,
        );
        let mut regions = screencraft_core::zoom::generate(&self.input, self.project.media.duration_ms, &opts);
        for r in regions.iter_mut() {
            r.id = self.new_id("zoom");
        }
        self.edit(|pr| pr.scene.zooms = regions);
        self.selection = Selection::None;
    }

    pub fn duration_ms(&self) -> f64 {
        output_duration_ms(&self.project.scene.slices)
    }

    pub fn new_id(&mut self, prefix: &str) -> String {
        self.id_counter += 1;
        let t = std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).map(|d| d.as_millis()).unwrap_or(0);
        format!("{prefix}-{t:x}-{}", self.id_counter)
    }

    fn ensure_track(&mut self) {
        if self.track_dirty {
            self.track = compute_track(&self.project, &self.input, TRACK_FPS);
            self.track_dirty = false;
        }
    }

    pub fn track(&mut self) -> &RenderTrack {
        self.ensure_track();
        &self.track
    }

    /// Mark the project edited by the UI: recompute the track and schedule a save.
    fn changed(&mut self) {
        self.track_dirty = true;
        self.dirty_since = Some(Instant::now());
        if self.playing {
            // Audio is rendered ahead from a snapshot of the project; restart it.
            self.restart_audio();
        }
    }

    /// Run `edit` on the project. History is pushed once per burst of edits.
    pub fn edit(&mut self, edit: impl FnOnce(&mut Project)) {
        let before = self.project.clone();
        edit(&mut self.project);
        if self.project != before {
            if self.last_undo_push.elapsed() > Duration::from_millis(450) {
                self.undo.push(before);
                if self.undo.len() > 200 {
                    self.undo.remove(0);
                }
            }
            self.last_undo_push = Instant::now();
            self.redo.clear();
            self.changed();
            let d = self.duration_ms();
            self.playhead_ms = self.playhead_ms.min(d);
        }
    }

    pub fn undo(&mut self) {
        if let Some(p) = self.undo.pop() {
            self.redo.push(std::mem::replace(&mut self.project, p));
            self.selection = Selection::None;
            self.changed();
        }
    }

    pub fn redo(&mut self) {
        if let Some(p) = self.redo.pop() {
            self.undo.push(std::mem::replace(&mut self.project, p));
            self.selection = Selection::None;
            self.changed();
        }
    }

    pub fn save_now(&mut self) {
        if let Err(e) = self.package.save_project(&self.project) {
            self.error = Some(e);
        }
        self.dirty_since = None;
    }

    fn restart_audio(&mut self) {
        if let Some(a) = self.audio.as_mut() {
            a.stop();
            if self.playing {
                if let Ok(Some(st)) = build_soundtrack(&self.package, &self.project) {
                    a.play(st, self.playhead_ms);
                }
            }
        }
    }

    pub fn play(&mut self) {
        if self.duration_ms() <= 0.0 {
            return;
        }
        if self.playhead_ms >= self.duration_ms() - 1.0 {
            self.playhead_ms = 0.0;
        }
        self.playing = true;
        self.play_anchor = Some((Instant::now(), self.playhead_ms));
        if self.audio.is_none() {
            self.audio = AudioPlayback::new();
        }
        self.restart_audio();
    }

    pub fn pause(&mut self) {
        self.playing = false;
        self.play_anchor = None;
        if let Some(a) = self.audio.as_mut() {
            a.stop();
        }
    }

    pub fn is_playing(&self) -> bool {
        self.playing
    }

    pub fn seek(&mut self, t_ms: f64) {
        self.playhead_ms = t_ms.clamp(0.0, self.duration_ms());
        if self.playing {
            self.play_anchor = Some((Instant::now(), self.playhead_ms));
            self.restart_audio();
        }
    }

    /// Source time currently shown at the playhead.
    pub fn playhead_source_ms(&self) -> f64 {
        output_to_source(&self.project.scene.slices, self.playhead_ms).map(|x| x.0).unwrap_or(0.0)
    }

    fn frame_index(&mut self) -> usize {
        self.ensure_track();
        let n = self.track.count.max(1);
        ((self.playhead_ms * TRACK_FPS / 1000.0).round() as usize).min(n - 1)
    }

    /// Per-frame housekeeping: clock, decode requests, autosave, async results.
    pub fn tick(&mut self, ctx: &egui::Context) {
        if self.playing {
            if let Some((t0, p0)) = self.play_anchor {
                let t = p0 + t0.elapsed().as_secs_f64() * 1000.0;
                let d = self.duration_ms();
                if t >= d {
                    self.playhead_ms = d;
                    self.pause();
                } else {
                    self.playhead_ms = t;
                }
            }
            ctx.request_repaint();
        }
        if let Some(rx) = &self.wave_rx {
            if let Ok(p) = rx.try_recv() {
                self.waveform = Some(Arc::new(p));
                self.wave_rx = None;
            }
        }
        if let Some(t) = self.dirty_since {
            if t.elapsed() > Duration::from_millis(900) {
                self.save_now();
            }
        }
        self.export.poll(ctx);
    }

    /// Draw the preview into `rect`.
    fn show_preview(&mut self, ui: &mut egui::Ui, rect: Rect) {
        self.ensure_track();
        let p = theme::palette(ui.ctx());
        ui.painter().rect_filled(rect, 0.0, p.bg);
        if self.track.count == 0 {
            return;
        }
        let i = self.frame_index();
        let t_src = self.track.t_src[i];
        let want = self.table.index_at(t_src);
        self.worker.request(want);
        if let Some(r) = self.worker.poll() {
            match r {
                Ok(r) => {
                    // Upload whatever arrived, even if the playhead moved on: it is the freshest.
                    self.compositor.upload(&r.frame);
                    self.uploaded_index = Some(r.index);
                }
                Err(e) => self.error = Some(e),
            }
        }
        if self.uploaded_index != Some(want) {
            ui.ctx().request_repaint_after(Duration::from_millis(8));
        }

        // Fit the output aspect into the rect, and render at the physical size (capped).
        let (ow, oh) = output_size(&self.project, 1080);
        let aspect = ow as f32 / oh as f32;
        let avail = rect.shrink(16.0);
        let mut size = Vec2::new(avail.width(), avail.width() / aspect);
        if size.y > avail.height() {
            size = Vec2::new(avail.height() * aspect, avail.height());
        }
        let shown = Rect::from_center_size(avail.center(), size);
        let ppp = ui.ctx().pixels_per_point();
        let scale = (ppp).min(2.0);
        let (mut w, mut h) = ((size.x * scale).round() as u32, (size.y * scale).round() as u32);
        if w.max(h) > 2560 {
            let k = 2560.0 / w.max(h) as f32;
            w = (w as f32 * k) as u32;
            h = (h as f32 * k) as u32;
        }
        let (w, h) = ((w & !1).max(64), (h & !1).max(64));
        let scene = build_scene(&self.project, &self.track, i, (w, h));
        self.preview_content = Some(Rect::from_min_size(
            shown.min + Vec2::new(scene.content[0] / w as f32 * shown.width(), scene.content[1] / h as f32 * shown.height()),
            Vec2::new(scene.content[2] / w as f32 * shown.width(), scene.content[3] / h as f32 * shown.height()),
        ));
        self.compositor.render(&scene);
        let view = self.compositor.target_view().unwrap();
        let mut renderer = self.render_state.renderer.write();
        let id = match self.tex {
            Some((id, sz)) if sz == (w, h) => id,
            Some((id, _)) => {
                renderer.update_egui_texture_from_wgpu_texture(&self.render_state.device, view, eframe::wgpu::FilterMode::Linear, id);
                self.tex = Some((id, (w, h)));
                id
            }
            None => {
                let id = renderer.register_native_texture(&self.render_state.device, view, eframe::wgpu::FilterMode::Linear);
                self.tex = Some((id, (w, h)));
                id
            }
        };
        drop(renderer);
        ui.painter().image(id, shown, Rect::from_min_max(egui::pos2(0.0, 0.0), egui::pos2(1.0, 1.0)), Color32::WHITE);

        // Manual zoom target picker.
        if let Selection::Zoom(zi) = self.selection {
            let manual = self.project.scene.zooms.get(zi).map(|z| z.kind == screencraft_core::project::ZoomKind::Manual).unwrap_or(false);
            if manual {
                if let Some(content) = self.preview_content {
                    let resp = ui.interact(content, ui.id().with("zoom-target"), egui::Sense::click_and_drag());
                    if resp.dragged() || resp.clicked() {
                        if let Some(pos) = resp.interact_pointer_pos() {
                            let local = ((pos - content.min) / content.size()).clamp(Vec2::ZERO, Vec2::splat(1.0));
                            let c = &self.project.config.crop;
                            let (cx, cy, z) = (self.track.cx[i], self.track.cy[i], self.track.zoom[i]);
                            let sx = cx + (local.x as f64 - 0.5) * c.width / z;
                            let sy = cy + (local.y as f64 - 0.5) * c.height / z;
                            self.edit(|pr| {
                                if let Some(zr) = pr.scene.zooms.get_mut(zi) {
                                    zr.manual_target_point.x = sx.clamp(0.0, 1.0);
                                    zr.manual_target_point.y = sy.clamp(0.0, 1.0);
                                }
                            });
                        }
                    }
                    if let Some(z) = self.project.scene.zooms.get(zi) {
                        let c = &self.project.config.crop;
                        let (cx, cy, zz) = (self.track.cx[i], self.track.cy[i], self.track.zoom[i]);
                        let lx = (z.manual_target_point.x - cx) * zz / c.width + 0.5;
                        let ly = (z.manual_target_point.y - cy) * zz / c.height + 0.5;
                        if (0.0..=1.0).contains(&lx) && (0.0..=1.0).contains(&ly) {
                            let pt = content.min + Vec2::new(lx as f32 * content.width(), ly as f32 * content.height());
                            ui.painter().circle_stroke(pt, 11.0, egui::Stroke::new(2.0, p.accent));
                            ui.painter().circle_filled(pt, 3.0, p.accent);
                        }
                    }
                }
            }
        }
    }

    /// The editor's whole UI. Returns `true` when the user asked to go back to the home screen.
    pub fn ui(&mut self, ui: &mut egui::Ui) -> bool {
        let ctx = ui.ctx().clone();
        let p = theme::palette(&ctx);
        let mut back = false;
        self.tick(&ctx);
        self.handle_keys(&ctx);

        egui::Panel::top("editor-top").exact_size(48.0).show(ui, |ui| {
            ui.horizontal_centered(|ui| {
                ui.add_space(8.0);
                if ui.button("‹  Library").clicked() {
                    back = true;
                }
                ui.add_space(8.0);
                ui.label(egui::RichText::new(&self.project.name).strong());
                ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                    ui.add_space(8.0);
                    if theme::primary_button(ui, "Export").clicked() {
                        self.export.open = true;
                    }
                    ui.add_enabled_ui(!self.redo.is_empty(), |ui| {
                        if ui.button("Redo").clicked() {
                            self.redo();
                        }
                    });
                    ui.add_enabled_ui(!self.undo.is_empty(), |ui| {
                        if ui.button("Undo").clicked() {
                            self.undo();
                        }
                    });
                });
            });
        });

        egui::Panel::right("inspector").exact_size(320.0).show(ui, |ui| {
            egui::ScrollArea::vertical().show(ui, |ui| {
                inspector::show(self, ui);
            });
        });

        egui::Panel::bottom("timeline").exact_size(230.0).show(ui, |ui| {
            timeline::show(self, ui);
        });

        egui::CentralPanel::default().frame(egui::Frame::NONE.fill(p.bg)).show(ui, |ui| {
            let full = ui.available_rect_before_wrap();
            let bar_h = 52.0;
            let canvas = Rect::from_min_max(full.min, egui::pos2(full.max.x, full.max.y - bar_h));
            self.show_preview(ui, canvas);
            let bar = Rect::from_min_max(egui::pos2(full.min.x, full.max.y - bar_h), full.max);
            ui.scope_builder(egui::UiBuilder::new().max_rect(bar), |ui| {
                ui.horizontal_centered(|ui| {
                    ui.add_space(16.0);
                    let label = if self.playing { "Pause" } else { "Play" };
                    if theme::primary_button(ui, label).clicked() {
                        if self.playing {
                            self.pause();
                        } else {
                            self.play();
                        }
                    }
                    ui.label(
                        egui::RichText::new(format!("{} / {}", fmt_time(self.playhead_ms), fmt_time(self.duration_ms())))
                            .monospace()
                            .color(p.dim),
                    );
                });
            });
        });

        if let Some(e) = self.error.clone() {
            let mut open = true;
            egui::Window::new("Something went wrong").open(&mut open).collapsible(false).show(&ctx, |ui| {
                ui.label(e);
            });
            if !open {
                self.error = None;
            }
        }
        export_ui::show(self, &ctx);
        back
    }

    fn handle_keys(&mut self, ctx: &egui::Context) {
        if ctx.egui_wants_keyboard_input() {
            return;
        }
        let (space, split, delete, undo, redo, left, right, home, end) = ctx.input(|i| {
            let cmd = i.modifiers.command;
            (
                i.key_pressed(egui::Key::Space),
                i.key_pressed(egui::Key::S) && !cmd,
                i.key_pressed(egui::Key::Delete) || i.key_pressed(egui::Key::Backspace),
                cmd && !i.modifiers.shift && i.key_pressed(egui::Key::Z),
                cmd && i.modifiers.shift && i.key_pressed(egui::Key::Z),
                i.key_pressed(egui::Key::ArrowLeft),
                i.key_pressed(egui::Key::ArrowRight),
                i.key_pressed(egui::Key::Home),
                i.key_pressed(egui::Key::End),
            )
        });
        if space {
            if self.playing {
                self.pause();
            } else {
                self.play();
            }
        }
        if undo {
            self.undo();
        }
        if redo {
            self.redo();
        }
        if split {
            self.split_at_playhead();
        }
        if delete {
            self.delete_selection();
        }
        let step = 1000.0 / TRACK_FPS;
        if left {
            self.seek(self.playhead_ms - step);
        }
        if right {
            self.seek(self.playhead_ms + step);
        }
        if home {
            self.seek(0.0);
        }
        if end {
            self.seek(self.duration_ms());
        }
    }

    pub fn split_at_playhead(&mut self) {
        let t = self.playhead_source_ms();
        let id = self.new_id("slice");
        let mut done = false;
        self.edit(|p| {
            done = screencraft_core::timeline::split_at(&mut p.scene.slices, t, id);
        });
        if done {
            self.selection = Selection::None;
        }
    }

    pub fn delete_selection(&mut self) {
        match self.selection {
            Selection::Slice(i) => {
                if self.project.scene.slices.len() > 1 {
                    self.edit(|p| {
                        if i < p.scene.slices.len() {
                            p.scene.slices.remove(i);
                        }
                    });
                    self.selection = Selection::None;
                }
            }
            Selection::Zoom(i) => {
                self.edit(|p| {
                    if i < p.scene.zooms.len() {
                        p.scene.zooms.remove(i);
                    }
                });
                self.selection = Selection::None;
            }
            Selection::None => {}
        }
    }
}

/// One peak value per 20 ms packet of audio track `track`, 0..1.
fn waveform_peaks(path: &std::path::Path, track: usize) -> Result<Vec<f32>, String> {
    use screencraft_encode::mp4read::Mp4File;
    use screencraft_encode::opus::OpusDecoder;
    let file = Mp4File::open(path).map_err(|e| e.to_string())?;
    let t = file
        .tracks
        .iter()
        .filter(|t| t.kind == screencraft_encode::mp4read::TrackKind::Audio)
        .nth(track)
        .ok_or("no audio track")?;
    let mut dec = OpusDecoder::new(t.channels.clamp(1, 2) as u8).map_err(|e| e.to_string())?;
    let mut peaks = Vec::with_capacity(t.samples.len());
    for s in &t.samples {
        let raw = file.read_sample(s).map_err(|e| e.to_string())?;
        let pcm = dec.decode(&raw).map_err(|e| e.to_string())?;
        peaks.push(pcm.iter().fold(0f32, |m, v| m.max(v.abs())).min(1.0));
    }
    Ok(peaks)
}
