//! Export dialog and the background job behind it.

use super::EditorState;
use crate::theme;
use eframe::egui;
use screencraft_encode::{available_video_codecs, has_hardware_encoder, Container, Effort, HwPreference, VideoCodec};
use screencraft_engine::{export_project, ExportOptions, ExportReport};
use std::path::PathBuf;
use std::sync::atomic::{AtomicBool, AtomicU32, Ordering};
use std::sync::{Arc, Mutex};

struct Job {
    progress: Arc<AtomicU32>,
    cancel: Arc<AtomicBool>,
    result: Arc<Mutex<Option<Result<ExportReport, String>>>>,
    out: PathBuf,
}

pub struct ExportState {
    pub open: bool,
    container: Container,
    codec: VideoCodec,
    resolution: u32,
    fps: u32,
    quality: Quality,
    hardware: bool,
    audio: bool,
    job: Option<Job>,
    finished: Option<Result<(PathBuf, ExportReport), String>>,
}

#[derive(Clone, Copy, PartialEq, Eq)]
enum Quality {
    Small,
    Balanced,
    High,
}

impl Default for ExportState {
    fn default() -> Self {
        Self {
            open: false,
            container: Container::Mp4,
            codec: VideoCodec::H264,
            resolution: 1080,
            fps: 60,
            quality: Quality::Balanced,
            hardware: true,
            audio: true,
            job: None,
            finished: None,
        }
    }
}

impl ExportState {
    pub fn poll(&mut self, ctx: &egui::Context) {
        if let Some(job) = &self.job {
            let done = job.result.lock().unwrap().take();
            if let Some(r) = done {
                let out = job.out.clone();
                self.finished = Some(r.map(|rep| (out, rep)));
                self.job = None;
            } else {
                ctx.request_repaint_after(std::time::Duration::from_millis(100));
            }
        }
    }

    pub fn busy(&self) -> bool {
        self.job.is_some()
    }
}

fn bitrate(q: Quality, w: u32, h: u32, fps: u32) -> u32 {
    let base = screencraft_encode::default_bitrate_kbps(w, h, fps) as f64;
    let k = match q {
        Quality::Small => 0.5,
        Quality::Balanced => 1.0,
        Quality::High => 1.8,
    };
    (base * k).round() as u32
}

pub fn show(st: &mut EditorState, ctx: &egui::Context) {
    if !st.export.open && st.export.job.is_none() && st.export.finished.is_none() {
        return;
    }
    let p = theme::palette(ctx);
    let mut open = st.export.open || st.export.job.is_some() || st.export.finished.is_some();
    let mut start: Option<PathBuf> = None;
    let mut cancel = false;
    let mut close = false;
    egui::Window::new("Export")
        .open(&mut open)
        .collapsible(false)
        .resizable(false)
        .default_width(380.0)
        .anchor(egui::Align2::CENTER_CENTER, egui::Vec2::ZERO)
        .show(ctx, |ui| {
            ui.set_width(360.0);
            if let Some(job) = &st.export.job {
                let v = job.progress.load(Ordering::Relaxed) as f32 / 1000.0;
                ui.label("Rendering…");
                ui.add(egui::ProgressBar::new(v).show_percentage());
                ui.label(egui::RichText::new(job.out.display().to_string()).small().color(p.dim));
                if ui.button("Cancel").clicked() {
                    cancel = true;
                }
                return;
            }
            if let Some(fin) = &st.export.finished {
                match fin {
                    Ok((path, rep)) => {
                        ui.label(egui::RichText::new("Export finished").strong());
                        ui.label(format!(
                            "{}×{}, {} frames, {:.1} MB, encoded with {}",
                            rep.width,
                            rep.height,
                            rep.frames,
                            rep.summary.bytes as f64 / 1e6,
                            rep.summary.encoder
                        ));
                        ui.label(egui::RichText::new(path.display().to_string()).small().color(p.dim));
                        ui.horizontal(|ui| {
                            if ui.button("Show in folder").clicked() {
                                reveal(path);
                            }
                            if theme::primary_button(ui, "Done").clicked() {
                                close = true;
                            }
                        });
                    }
                    Err(e) => {
                        ui.label(egui::RichText::new("Export failed").color(p.danger).strong());
                        ui.label(e);
                        if ui.button("Close").clicked() {
                            close = true;
                        }
                    }
                }
                return;
            }

            let ex = &mut st.export;
            theme::section(ui, "Format");
            let mut fmt = (ex.container, ex.codec);
            let combos: Vec<((Container, VideoCodec), &str)> = {
                let have = available_video_codecs();
                let mut v = vec![((Container::Mp4, VideoCodec::H264), "MP4 · H.264")];
                if have.contains(&VideoCodec::Av1) {
                    v.push(((Container::Mp4, VideoCodec::Av1), "MP4 · AV1"));
                    v.push(((Container::WebM, VideoCodec::Av1), "WebM · AV1"));
                }
                if have.contains(&VideoCodec::Vp9) {
                    v.push(((Container::Mp4, VideoCodec::Vp9), "MP4 · VP9"));
                    v.push(((Container::WebM, VideoCodec::Vp9), "WebM · VP9"));
                }
                v
            };
            egui::ComboBox::from_id_salt("export-format")
                .selected_text(combos.iter().find(|(k, _)| *k == fmt).map(|x| x.1).unwrap_or("MP4 · H.264"))
                .show_ui(ui, |ui| {
                    for (k, n) in &combos {
                        ui.selectable_value(&mut fmt, *k, *n);
                    }
                });
            ex.container = fmt.0;
            ex.codec = fmt.1;

            theme::section(ui, "Size");
            theme::segmented(
                ui,
                &mut ex.resolution,
                &[(720, "720p"), (1080, "1080p"), (1440, "1440p"), (2160, "4K")],
            );
            theme::section(ui, "Frame rate");
            theme::segmented(ui, &mut ex.fps, &[(24, "24"), (30, "30"), (60, "60")]);
            theme::section(ui, "Quality");
            theme::segmented(ui, &mut ex.quality, &[(Quality::Small, "Smaller"), (Quality::Balanced, "Balanced"), (Quality::High, "Best")]);
            ui.add_space(6.0);
            if has_hardware_encoder(ex.codec) {
                ui.checkbox(&mut ex.hardware, "Use the hardware encoder");
            }
            ui.checkbox(&mut ex.audio, "Include audio");
            ui.add_space(8.0);
            ui.horizontal(|ui| {
                if theme::primary_button(ui, "Export…").clicked() {
                    let dir = st.package.dir.parent().map(|d| d.to_path_buf());
                    let mut dlg = rfd::FileDialog::new()
                        .set_file_name(format!("{}.{}", st.project.name, ex.container.extension()))
                        .add_filter(ex.container.extension(), &[ex.container.extension()]);
                    if let Some(d) = dir {
                        dlg = dlg.set_directory(d);
                    }
                    if let Some(path) = dlg.save_file() {
                        start = Some(path);
                    }
                }
                if ui.button("Cancel").clicked() {
                    close = true;
                }
            });
        });

    if let Some(path) = start {
        let ex = &mut st.export;
        let (w, h) = screencraft_render::output_size(&st.project, ex.resolution);
        let opts = ExportOptions {
            container: ex.container,
            codec: ex.codec,
            resolution: ex.resolution,
            fps: ex.fps,
            bitrate_kbps: Some(bitrate(ex.quality, w, h, ex.fps)),
            effort: match ex.quality {
                Quality::Small => Effort::Best,
                Quality::Balanced => Effort::Balanced,
                Quality::High => Effort::Balanced,
            },
            hardware: if ex.hardware { HwPreference::Auto } else { HwPreference::Software },
            audio: ex.audio,
            threads: 0,
        };
        st.save_now();
        let (package, project) = (st.package.clone(), st.project.clone());
        let progress = Arc::new(AtomicU32::new(0));
        let cancel_flag = Arc::new(AtomicBool::new(false));
        let result = Arc::new(Mutex::new(None));
        let (pg, cf, rs, out) = (progress.clone(), cancel_flag.clone(), result.clone(), path.clone());
        let ctx2 = ctx.clone();
        std::thread::spawn(move || {
            let r = export_project(&package, &project, &opts, &out, |f| {
                pg.store((f * 1000.0) as u32, Ordering::Relaxed);
                ctx2.request_repaint();
                !cf.load(Ordering::Relaxed)
            });
            *rs.lock().unwrap() = Some(r);
            ctx2.request_repaint();
        });
        st.export.job = Some(Job { progress, cancel: cancel_flag, result, out: path });
        st.export.open = false;
    }
    if cancel {
        if let Some(j) = &st.export.job {
            j.cancel.store(true, Ordering::Relaxed);
        }
    }
    if close || !open {
        if st.export.job.is_none() {
            st.export.open = false;
            st.export.finished = None;
        } else if !open {
            // Closing the window mid-export means cancel.
            if let Some(j) = &st.export.job {
                j.cancel.store(true, Ordering::Relaxed);
            }
        }
    }
}

fn reveal(path: &std::path::Path) {
    #[cfg(target_os = "macos")]
    let _ = std::process::Command::new("open").arg("-R").arg(path).spawn();
    #[cfg(windows)]
    let _ = std::process::Command::new("explorer").arg(format!("/select,{}", path.display())).spawn();
    #[cfg(target_os = "linux")]
    let _ = std::process::Command::new("xdg-open").arg(path.parent().unwrap_or(path)).spawn();
}
