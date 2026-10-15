//! Home screen: capture setup, the recording session controller, and the library.

use crate::prefs::Prefs;
use crate::theme::{self, fmt_time, section};
use eframe::egui;
use screencraft_audio::{list_devices, DeviceKind};
use screencraft_capture::{DisplayInfo, Permissions, Sources, Target, WindowInfo};
use screencraft_core::project::Rect;
use screencraft_encode::HwPreference;
use screencraft_engine::package::list_packages;
use screencraft_engine::{Finished, LiveStats, Package, RecordOptions, Recording};
use std::sync::mpsc::{channel, Receiver, Sender};
use std::sync::Arc;
use std::time::Instant;

#[derive(Clone, Copy, PartialEq, Eq)]
pub enum Mode {
    Screen,
    Window,
    Area,
}

enum Cmd {
    Start(RecordOptions),
    Stop,
}

pub enum Evt {
    Started { stats: Arc<LiveStats>, width: u32, height: u32, encoder: String },
    StartFailed(String),
    Stopped(Result<Finished, String>),
}

/// Owns the `Recording` on its own thread (audio streams are not `Send`).
pub struct Controller {
    tx: Sender<Cmd>,
    pub rx: Receiver<Evt>,
}

impl Controller {
    pub fn spawn(ctx: egui::Context) -> Self {
        let (tx, cmd_rx) = channel::<Cmd>();
        let (evt_tx, rx) = channel::<Evt>();
        std::thread::Builder::new()
            .name("screencraft-controller".into())
            .spawn(move || {
                let mut current: Option<Recording> = None;
                while let Ok(cmd) = cmd_rx.recv() {
                    match cmd {
                        Cmd::Start(opts) => match Recording::start(opts) {
                            Ok(rec) => {
                                let info = rec.info().clone();
                                let _ = evt_tx.send(Evt::Started {
                                    stats: rec.stats.clone(),
                                    width: info.width,
                                    height: info.height,
                                    encoder: rec.encoder().to_string(),
                                });
                                current = Some(rec);
                            }
                            Err(e) => {
                                let _ = evt_tx.send(Evt::StartFailed(e));
                            }
                        },
                        Cmd::Stop => {
                            if let Some(rec) = current.take() {
                                let _ = evt_tx.send(Evt::Stopped(rec.stop()));
                            }
                        }
                    }
                    ctx.request_repaint();
                }
            })
            .expect("controller thread");
        Self { tx, rx }
    }

    pub fn start(&self, o: RecordOptions) {
        let _ = self.tx.send(Cmd::Start(o));
    }

    pub fn stop(&self) {
        let _ = self.tx.send(Cmd::Stop);
    }
}

#[derive(Clone, Copy, PartialEq, Eq)]
pub enum Phase {
    Idle,
    Starting,
    Recording,
    Finishing,
}

enum Probe {
    Sources(Result<Sources, String>),
    Mics(Vec<screencraft_audio::DeviceInfo>),
}

pub struct Home {
    pub mode: Mode,
    pub sources: Option<Result<Sources, String>>,
    sources_at: Option<Instant>,
    pub display: Option<u32>,
    pub window: Option<u32>,
    /// Area as fractions of the chosen display.
    pub area: [f32; 4],
    pub permissions: Permissions,
    perms_at: Option<Instant>,
    pub phase: Phase,
    pub stats: Option<Arc<LiveStats>>,
    pub started_at: Option<Instant>,
    pub encoder: String,
    pub size: (u32, u32),
    pub error: Option<String>,
    mics: Vec<screencraft_audio::DeviceInfo>,
    mics_loaded: bool,
    /// Results of the slow platform queries, which run off the UI thread (they can block on
    /// permission prompts).
    probe_rx: Receiver<Probe>,
    probe_tx: Sender<Probe>,
    sources_busy: bool,
    mics_busy: bool,
    ctx: egui::Context,
    library: Vec<(Package, u64)>,
    library_at: Option<Instant>,
    pub controller: Controller,
    pub finished: Option<Finished>,
}

pub enum Action {
    None,
    Open(Package),
    StartRecording,
    StopRecording,
}

impl Home {
    pub fn new(ctx: &egui::Context) -> Self {
        let (probe_tx, probe_rx) = channel();
        Self {
            mics_loaded: false,
            probe_rx,
            probe_tx,
            sources_busy: false,
            mics_busy: false,
            ctx: ctx.clone(),
            mode: Mode::Screen,
            sources: None,
            sources_at: None,
            display: None,
            window: None,
            area: [0.15, 0.15, 0.7, 0.7],
            permissions: Permissions::default(),
            perms_at: None,
            phase: Phase::Idle,
            stats: None,
            started_at: None,
            encoder: String::new(),
            size: (0, 0),
            error: None,
            mics: Vec::new(),
            library: Vec::new(),
            library_at: None,
            controller: Controller::spawn(ctx.clone()),
            finished: None,
        }
    }

    pub fn refresh(&mut self, prefs: &Prefs) {
        let stale = |t: &Option<Instant>, s: u64| t.map(|t| t.elapsed().as_secs() >= s).unwrap_or(true);
        while let Ok(p) = self.probe_rx.try_recv() {
            match p {
                Probe::Sources(r) => {
                    self.sources = Some(r);
                    self.sources_busy = false;
                }
                Probe::Mics(m) => {
                    self.mics = m;
                    self.mics_busy = false;
                    self.mics_loaded = true;
                }
            }
        }
        if stale(&self.perms_at, 2) {
            self.permissions = screencraft_capture::permissions();
            self.perms_at = Some(Instant::now());
        }
        if !self.sources_busy
            && stale(&self.sources_at, 4)
            && self.phase == Phase::Idle
            && (self.permissions.screen || self.sources.is_none())
        {
            self.sources_busy = true;
            self.sources_at = Some(Instant::now());
            let (tx, ctx) = (self.probe_tx.clone(), self.ctx.clone());
            std::thread::spawn(move || {
                let _ = tx.send(Probe::Sources(screencraft_capture::list_sources()));
                ctx.request_repaint();
            });
        }
        // Only look at audio inputs once the microphone is wanted: enumerating them can
        // trigger the system's microphone permission prompt.
        if prefs.microphone && !self.mics_loaded && !self.mics_busy {
            self.mics_busy = true;
            let (tx, ctx) = (self.probe_tx.clone(), self.ctx.clone());
            std::thread::spawn(move || {
                let m = list_devices().into_iter().filter(|d| d.kind == DeviceKind::Microphone).collect();
                let _ = tx.send(Probe::Mics(m));
                ctx.request_repaint();
            });
        }
        if stale(&self.library_at, 4) && self.phase == Phase::Idle {
            self.library = list_packages(&prefs.recordings_dir);
            self.library_at = Some(Instant::now());
        }
    }

    pub fn invalidate_library(&mut self) {
        self.library_at = None;
    }

    /// Everything needed to start, from the current selections.
    pub fn build_options(&self, prefs: &Prefs) -> Result<RecordOptions, String> {
        let stamp = timestamp();
        let package = Package::create_in(&prefs.recordings_dir, &stamp).map_err(|e| format!("cannot create the recording folder: {e}"))?;
        let displays: &[DisplayInfo] = match &self.sources {
            Some(Ok(s)) => &s.displays,
            _ => &[],
        };
        let display_id = self.display.or_else(|| displays.iter().find(|d| d.is_main).map(|d| d.id)).or_else(|| displays.first().map(|d| d.id));
        let target = match self.mode {
            Mode::Screen => Target::Display(display_id),
            Mode::Window => Target::Window(self.window.ok_or("choose a window to record")?),
            Mode::Area => {
                let d = displays.iter().find(|d| Some(d.id) == display_id).or(displays.first());
                let (dx, dy, dw, dh) = d.map(|d| (d.x, d.y, d.width, d.height)).unwrap_or((0.0, 0.0, 1920.0, 1080.0));
                let a = self.area;
                Target::Area {
                    display: display_id,
                    rect: Rect {
                        x: dx + a[0] as f64 * dw,
                        y: dy + a[1] as f64 * dh,
                        width: (a[2] as f64 * dw).max(64.0),
                        height: (a[3] as f64 * dh).max(64.0),
                    },
                }
            }
        };
        Ok(RecordOptions {
            target,
            fps: prefs.fps,
            max_long_side: if prefs.max_long_side == 0 { None } else { Some(prefs.max_long_side) },
            system_audio: prefs.system_audio,
            microphone: prefs.microphone,
            microphone_id: prefs.microphone_id.clone(),
            output: package,
            bitrate_kbps: None,
            hardware: HwPreference::Auto,
        })
    }

    pub fn elapsed_ms(&self) -> f64 {
        self.started_at.map(|t| t.elapsed().as_secs_f64() * 1000.0).unwrap_or(0.0)
    }

    pub fn show(&mut self, ui: &mut egui::Ui, prefs: &mut Prefs) -> Action {
        let p = theme::palette(ui.ctx());
        let mut action = Action::None;
        self.refresh(prefs);

        egui::Panel::left("record-panel").exact_size(380.0).show(ui, |ui| {
            egui::ScrollArea::vertical().show(ui, |ui| {
                egui::Frame::NONE.inner_margin(egui::Margin::same(22)).show(ui, |ui| {
                    ui.set_width(ui.available_width());
                    ui.label(egui::RichText::new("ScreenCraft").size(22.0).strong());
                    ui.label(egui::RichText::new("Record your screen, then shape it.").color(p.dim));
                    ui.add_space(14.0);
                    if let Some(a) = self.record_card(ui, prefs) {
                        action = a;
                    }
                });
            });
        });

        egui::CentralPanel::default().frame(egui::Frame::NONE.fill(p.bg)).show(ui, |ui| {
            egui::Frame::NONE.inner_margin(egui::Margin::same(26)).show(ui, |ui| {
                ui.set_width(ui.available_width());
                ui.horizontal(|ui| {
                    ui.label(egui::RichText::new("Recordings").size(16.0).strong());
                    ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                        if ui.button("Open…").clicked() {
                            if let Some(dir) = rfd::FileDialog::new().set_title("Open a .screencraft recording").pick_folder() {
                                if dir.join("project.json").exists() {
                                    action = Action::Open(Package::open(dir));
                                } else {
                                    self.error = Some("That folder is not a ScreenCraft recording (no project.json).".into());
                                }
                            }
                        }
                    });
                });
                ui.add_space(10.0);
                if self.library.is_empty() {
                    ui.add_space(40.0);
                    ui.vertical_centered(|ui| {
                        ui.label(egui::RichText::new("No recordings yet").size(15.0));
                        ui.label(egui::RichText::new("Press Record to make your first one.").color(p.dim));
                    });
                }
                egui::ScrollArea::vertical().show(ui, |ui| {
                    let lib = self.library.clone();
                    for (pkg, _modified) in lib {
                        let name = pkg.dir.file_stem().map(|s| s.to_string_lossy().into_owned()).unwrap_or_default();
                        let meta = pkg
                            .load_project()
                            .ok()
                            .map(|pr| (pr.media.duration_ms, pr.media.width, pr.media.height, pr.media.has_microphone || pr.media.has_system_audio));
                        let (r, resp) = ui.allocate_exact_size(egui::vec2(ui.available_width(), 58.0), egui::Sense::click());
                        let fill = if resp.hovered() { p.hover } else { p.raised };
                        ui.painter().rect_filled(r, 10.0, fill);
                        ui.painter().rect_stroke(r, 10.0, egui::Stroke::new(1.0, p.line), egui::StrokeKind::Inside);
                        ui.painter().text(r.min + egui::vec2(16.0, 12.0), egui::Align2::LEFT_TOP, name, egui::FontId::proportional(14.0), p.text);
                        if let Some((d, w, h, audio)) = meta {
                            ui.painter().text(
                                r.min + egui::vec2(16.0, 33.0),
                                egui::Align2::LEFT_TOP,
                                format!("{}  ·  {}×{}{}", fmt_time(d).split('.').next().unwrap_or(""), w, h, if audio { "  ·  audio" } else { "" }),
                                egui::FontId::proportional(12.0),
                                p.dim,
                            );
                        }
                        if resp.clicked() {
                            action = Action::Open(pkg.clone());
                        }
                        ui.add_space(6.0);
                    }
                });
            });
        });
        action
    }

    fn record_card(&mut self, ui: &mut egui::Ui, prefs: &mut Prefs) -> Option<Action> {
        let p = theme::palette(ui.ctx());
        let mut action = None;

        if !self.permissions.screen {
            egui::Frame::NONE.fill(p.raised).stroke(egui::Stroke::new(1.0, p.line)).corner_radius(10).inner_margin(egui::Margin::same(12)).show(ui, |ui| {
                ui.label(egui::RichText::new("Screen recording permission needed").strong());
                ui.label(egui::RichText::new("ScreenCraft needs access to record the screen. After granting it, restart the app.").small().color(p.dim));
                if ui.button("Grant access").clicked() {
                    screencraft_capture::request_screen_access();
                }
            });
            ui.add_space(10.0);
        }

        section(ui, "Source");
        let mut mode = self.mode;
        #[cfg(target_os = "linux")]
        {
            let _ = &mut mode;
            ui.label(egui::RichText::new("The system dialog lets you pick a screen or window when you press Record.").small().color(p.dim));
        }
        #[cfg(not(target_os = "linux"))]
        theme::segmented(ui, &mut mode, &[(Mode::Screen, "Screen"), (Mode::Window, "Window"), (Mode::Area, "Area")]);
        self.mode = mode;

        let sources = match &self.sources {
            Some(Ok(s)) => Some(s.clone()),
            Some(Err(e)) => {
                ui.label(egui::RichText::new(e).small().color(p.danger));
                None
            }
            None => None,
        };
        #[cfg(not(target_os = "linux"))]
        if let Some(src) = &sources {
            match self.mode {
                Mode::Screen | Mode::Area => {
                    let cur = self.display.or_else(|| src.displays.iter().find(|d| d.is_main).map(|d| d.id));
                    let label = src.displays.iter().find(|d| Some(d.id) == cur).map(|d| format!("{}  ({}×{})", d.name, d.width as u32, d.height as u32)).unwrap_or_else(|| "No display".into());
                    egui::ComboBox::from_id_salt("display").selected_text(label).width(ui.available_width()).show_ui(ui, |ui| {
                        for d in &src.displays {
                            ui.selectable_value(&mut self.display, Some(d.id), format!("{}  ({}×{})", d.name, d.width as u32, d.height as u32));
                        }
                    });
                    if self.mode == Mode::Area {
                        area_editor(ui, &mut self.area);
                    }
                }
                Mode::Window => {
                    let label = src
                        .windows
                        .iter()
                        .find(|w| Some(w.id) == self.window)
                        .map(window_label)
                        .unwrap_or_else(|| "Choose a window".into());
                    egui::ComboBox::from_id_salt("window").selected_text(label).width(ui.available_width()).show_ui(ui, |ui| {
                        for w in &src.windows {
                            ui.selectable_value(&mut self.window, Some(w.id), window_label(w));
                        }
                    });
                }
            }
        }

        section(ui, "Quality");
        theme::segmented(ui, &mut prefs.fps, &[(30, "30 fps"), (60, "60 fps")]);
        ui.add_space(2.0);
        theme::segmented(ui, &mut prefs.max_long_side, &[(1920, "1080p"), (2560, "1440p"), (3840, "4K"), (0, "Native")]);

        section(ui, "Audio");
        ui.checkbox(&mut prefs.system_audio, "System audio");
        ui.checkbox(&mut prefs.microphone, "Microphone");
        if prefs.microphone {
            let label = prefs
                .microphone_id
                .as_ref()
                .and_then(|id| self.mics.iter().find(|m| &m.id == id))
                .map(|m| m.name.clone())
                .unwrap_or_else(|| "Default microphone".into());
            egui::ComboBox::from_id_salt("mic").selected_text(label).width(ui.available_width()).show_ui(ui, |ui| {
                ui.selectable_value(&mut prefs.microphone_id, None, "Default microphone");
                for m in &self.mics {
                    ui.selectable_value(&mut prefs.microphone_id, Some(m.id.clone()), &m.name);
                }
            });
        }

        ui.add_space(16.0);
        if let Some(e) = &self.error {
            ui.label(egui::RichText::new(e).small().color(p.danger));
            ui.add_space(6.0);
        }
        let label = match self.phase {
            Phase::Idle => "Record",
            Phase::Starting => "Starting…",
            Phase::Recording => "Stop",
            Phase::Finishing => "Saving…",
        };
        let size = egui::vec2(ui.available_width(), 44.0);
        let enabled = matches!(self.phase, Phase::Idle | Phase::Recording);
        ui.add_enabled_ui(enabled, |ui| {
            let btn = egui::Button::new(egui::RichText::new(label).strong().size(15.0).color(egui::Color32::WHITE))
                .fill(p.danger)
                .corner_radius(10)
                .min_size(size);
            if ui.add(btn).clicked() {
                action = Some(if self.phase == Phase::Recording { Action::StopRecording } else { Action::StartRecording });
            }
        });
        ui.add_space(6.0);
        ui.label(egui::RichText::new(format!("Global shortcut: {}+Shift+R", if cfg!(target_os = "macos") { "⌘" } else { "Ctrl" })).small().color(p.dim));
        ui.add_space(14.0);
        section(ui, "Save to");
        ui.horizontal(|ui| {
            ui.label(egui::RichText::new(prefs.recordings_dir.display().to_string()).small().color(p.dim));
        });
        if ui.small_button("Change…").clicked() {
            if let Some(d) = rfd::FileDialog::new().set_directory(&prefs.recordings_dir).pick_folder() {
                prefs.recordings_dir = d;
                self.library_at = None;
            }
        }
        action
    }
}

fn window_label(w: &WindowInfo) -> String {
    if w.title.is_empty() {
        w.app.clone()
    } else {
        format!("{} — {}", w.app, w.title)
    }
}

/// Four sliders and a small map of the display with the area drawn on it.
fn area_editor(ui: &mut egui::Ui, a: &mut [f32; 4]) {
    let p = theme::palette(ui.ctx());
    let (r, _) = ui.allocate_exact_size(egui::vec2(ui.available_width(), 120.0), egui::Sense::hover());
    ui.painter().rect_filled(r, 6.0, p.raised);
    ui.painter().rect_stroke(r, 6.0, egui::Stroke::new(1.0, p.line), egui::StrokeKind::Inside);
    let inner = r.shrink(10.0);
    let sel = egui::Rect::from_min_size(
        inner.min + egui::vec2(a[0] * inner.width(), a[1] * inner.height()),
        egui::vec2(a[2] * inner.width(), a[3] * inner.height()),
    );
    ui.painter().rect_filled(sel, 3.0, p.accent.gamma_multiply(0.35));
    ui.painter().rect_stroke(sel, 3.0, egui::Stroke::new(1.5, p.accent), egui::StrokeKind::Inside);
    ui.horizontal(|ui| {
        ui.label("Left");
        ui.add(egui::Slider::new(&mut a[0], 0.0..=0.9).show_value(false));
        ui.label("Top");
        ui.add(egui::Slider::new(&mut a[1], 0.0..=0.9).show_value(false));
    });
    ui.horizontal(|ui| {
        ui.label("Width");
        ui.add(egui::Slider::new(&mut a[2], 0.1..=1.0).show_value(false));
        ui.label("Height");
        ui.add(egui::Slider::new(&mut a[3], 0.1..=1.0).show_value(false));
    });
    a[2] = a[2].min(1.0 - a[0]);
    a[3] = a[3].min(1.0 - a[1]);
}

fn timestamp() -> String {
    // Local-time-ish stamp without a date crate: seconds since epoch broken into UTC fields.
    let secs = std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).map(|d| d.as_secs()).unwrap_or(0) as i64;
    let days = secs.div_euclid(86_400);
    let rem = secs.rem_euclid(86_400);
    // civil-from-days (Howard Hinnant)
    let z = days + 719_468;
    let era = z.div_euclid(146_097);
    let doe = z.rem_euclid(146_097);
    let yoe = (doe - doe / 1460 + doe / 36_524 - doe / 146_096) / 365;
    let y = yoe + era * 400;
    let doy = doe - (365 * yoe + yoe / 4 - yoe / 100);
    let mp = (5 * doy + 2) / 153;
    let d = doy - (153 * mp + 2) / 5 + 1;
    let m = if mp < 10 { mp + 3 } else { mp - 9 };
    let y = if m <= 2 { y + 1 } else { y };
    format!("{y:04}-{m:02}-{d:02} {:02}.{:02}.{:02}", rem / 3600, (rem % 3600) / 60, rem % 60)
}
