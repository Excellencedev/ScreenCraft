//! Application shell: home screen, editor, the compact recording bar and global shortcut.

use crate::editor::EditorState;
use crate::home::{Action, Evt, Home, Phase};
use crate::prefs::Prefs;
use crate::theme::{self, fmt_time};
use eframe::egui;
use global_hotkey::hotkey::{Code, HotKey, Modifiers};
use global_hotkey::{GlobalHotKeyEvent, GlobalHotKeyManager, HotKeyState};

enum Screen {
    Home,
    Editor(Box<EditorState>),
}

pub struct App {
    screen: Screen,
    home: Home,
    prefs: Prefs,
    render_state: eframe::egui_wgpu::RenderState,
    _hotkeys: Option<GlobalHotKeyManager>,
    hotkey_id: Option<u32>,
    bar_mode: bool,
    /// Developer hooks: open a package, seek and save a screenshot, then quit.
    shot: Option<Shot>,
    /// `SCREENCRAFT_AUTORECORD=<seconds>`: record the main display, stop, report, quit.
    autorecord: Option<f64>,
    autorecord_started: bool,
    /// Native window handle (Windows), to keep the recording bar out of the capture.
    native_window: Option<isize>,
}

struct Shot {
    path: std::path::PathBuf,
    after_frames: u32,
    frame: u32,
    requested: bool,
}

impl App {
    pub fn new(cc: &eframe::CreationContext<'_>) -> Self {
        theme::install(&cc.egui_ctx);
        let render_state = cc.wgpu_render_state.clone().expect("the wgpu renderer is required");
        Self::build(&cc.egui_ctx, render_state, true)
    }

    /// Without a window or global shortcuts: for the screenshot tool and tests.
    pub fn new_headless(ctx: &egui::Context, render_state: eframe::egui_wgpu::RenderState) -> Self {
        Self::build(ctx, render_state, false)
    }

    pub fn open_package(&mut self, ctx: &egui::Context, dir: std::path::PathBuf) {
        match EditorState::open(screencraft_engine::Package::open(dir), ctx, self.render_state.clone()) {
            Ok(ed) => self.screen = Screen::Editor(Box::new(ed)),
            Err(e) => self.home.error = Some(e),
        }
    }

    /// Show the compact recording bar as if a recording were running (screenshots only).
    pub fn dev_bar(&mut self) {
        self.home.phase = Phase::Recording;
        self.home.started_at = Some(std::time::Instant::now());
        self.bar_mode = true;
    }

    pub fn dev_seek(&mut self, ms: f64) {
        if let Screen::Editor(ed) = &mut self.screen {
            ed.seek(ms);
        }
    }

    pub fn dev_select(&mut self, what: Option<&str>) {
        if let Screen::Editor(ed) = &mut self.screen {
            ed.apply_dev_selection(what);
        }
    }

    fn build(ctx: &egui::Context, render_state: eframe::egui_wgpu::RenderState, hotkeys: bool) -> Self {
        let (mgr, id) = match if hotkeys { GlobalHotKeyManager::new() } else { Err(global_hotkey::Error::AlreadyRegistered(HotKey::new(None, Code::KeyA))) } {
            Ok(m) => {
                let mods = if cfg!(target_os = "macos") { Modifiers::SUPER | Modifiers::SHIFT } else { Modifiers::CONTROL | Modifiers::SHIFT };
                let hk = HotKey::new(Some(mods), Code::KeyR);
                let id = hk.id();
                let ok = m.register(hk).is_ok();
                (Some(m), ok.then_some(id))
            }
            Err(_) => (None, None),
        };
        let mut app = Self {
            screen: Screen::Home,
            home: Home::new(ctx),
            prefs: Prefs::load(),
            render_state,
            _hotkeys: mgr,
            hotkey_id: id,
            bar_mode: false,
            autorecord: std::env::var("SCREENCRAFT_AUTORECORD").ok().and_then(|v| v.parse().ok()),
            autorecord_started: false,
            native_window: None,
            shot: std::env::var_os("SCREENCRAFT_SHOT").map(|p| Shot {
                path: p.into(),
                after_frames: std::env::var("SCREENCRAFT_SHOT_FRAMES").ok().and_then(|v| v.parse().ok()).unwrap_or(40),
                frame: 0,
                requested: false,
            }),
        };
        if let Some(dir) = std::env::var_os("SCREENCRAFT_OPEN") {
            match EditorState::open(screencraft_engine::Package::open(dir), ctx, app.render_state.clone()) {
                Ok(mut ed) => {
                    if let Ok(ms) = std::env::var("SCREENCRAFT_SEEK").unwrap_or_default().parse::<f64>() {
                        ed.seek(ms);
                    }
                    ed.apply_dev_selection(std::env::var("SCREENCRAFT_SELECT").ok().as_deref());
                    app.screen = Screen::Editor(Box::new(ed));
                }
                Err(e) => eprintln!("[screencraft] cannot open {e}"),
            }
        }
        app
    }

    /// Save a screenshot of the window to a PNG once things have settled (developer hook).
    fn dev_screenshot(&mut self, ctx: &egui::Context) {
        let Some(shot) = self.shot.as_mut() else { return };
        shot.frame += 1;
        ctx.request_repaint();
        if !shot.requested && shot.frame >= shot.after_frames {
            shot.requested = true;
            ctx.send_viewport_cmd(egui::ViewportCommand::Screenshot(egui::UserData::default()));
        }
        let image = ctx.input(|i| {
            i.events.iter().find_map(|e| match e {
                egui::Event::Screenshot { image, .. } => Some(image.clone()),
                _ => None,
            })
        });
        if let Some(img) = image {
            let (w, h) = (img.width() as u32, img.height() as u32);
            let mut raw = Vec::with_capacity((w * h * 4) as usize);
            for c in &img.pixels {
                raw.extend_from_slice(&c.to_array());
            }
            if let Ok(f) = std::fs::File::create(&shot.path) {
                let mut enc = png::Encoder::new(std::io::BufWriter::new(f), w, h);
                enc.set_color(png::ColorType::Rgba);
                enc.set_depth(png::BitDepth::Eight);
                if let Ok(mut wr) = enc.write_header() {
                    let _ = wr.write_image_data(&raw);
                }
            }
            ctx.send_viewport_cmd(egui::ViewportCommand::Close);
        }
    }

    fn start_recording(&mut self, ctx: &egui::Context) {
        if self.home.phase != Phase::Idle {
            return;
        }
        self.home.error = None;
        match self.home.build_options(&self.prefs) {
            Ok(opts) => {
                self.prefs.save();
                self.home.phase = Phase::Starting;
                self.home.controller.start(opts);
            }
            Err(e) => self.home.error = Some(e),
        }
        ctx.request_repaint();
    }

    fn stop_recording(&mut self, ctx: &egui::Context) {
        if self.home.phase == Phase::Recording {
            self.home.phase = Phase::Finishing;
            self.home.controller.stop();
        }
        ctx.request_repaint();
    }

    fn enter_bar(&mut self, ctx: &egui::Context) {
        self.bar_mode = true;
        if let Some(h) = self.native_window {
            screencraft_capture::set_excluded_from_capture(h, true);
        }
        ctx.send_viewport_cmd(egui::ViewportCommand::Decorations(false));
        ctx.send_viewport_cmd(egui::ViewportCommand::WindowLevel(egui::WindowLevel::AlwaysOnTop));
        ctx.send_viewport_cmd(egui::ViewportCommand::InnerSize(egui::vec2(340.0, 64.0)));
        ctx.send_viewport_cmd(egui::ViewportCommand::OuterPosition(egui::pos2(40.0, 40.0)));
    }

    fn leave_bar(&mut self, ctx: &egui::Context) {
        self.bar_mode = false;
        if let Some(h) = self.native_window {
            screencraft_capture::set_excluded_from_capture(h, false);
        }
        ctx.send_viewport_cmd(egui::ViewportCommand::WindowLevel(egui::WindowLevel::Normal));
        ctx.send_viewport_cmd(egui::ViewportCommand::Decorations(true));
        ctx.send_viewport_cmd(egui::ViewportCommand::InnerSize(egui::vec2(1280.0, 800.0)));
    }

    fn pump_events(&mut self, ctx: &egui::Context) {
        while let Ok(evt) = self.home.controller.rx.try_recv() {
            match evt {
                Evt::Started { stats, width, height, encoder } => {
                    self.home.stats = Some(stats);
                    self.home.started_at = Some(std::time::Instant::now());
                    self.home.size = (width, height);
                    self.home.encoder = encoder;
                    self.home.phase = Phase::Recording;
                    self.enter_bar(ctx);
                }
                Evt::StartFailed(e) => {
                    self.home.phase = Phase::Idle;
                    self.home.error = Some(e);
                }
                Evt::Stopped(r) => {
                    self.home.phase = Phase::Idle;
                    self.home.stats = None;
                    self.home.started_at = None;
                    self.leave_bar(ctx);
                    self.home.invalidate_library();
                    if self.autorecord.is_some() {
                        match &r {
                            Ok(f) => eprintln!(
                                "[autorecord] finished: {} frames, {:.2}s, encoder {}, package {}",
                                f.frames,
                                f.duration_ms / 1000.0,
                                f.encoder,
                                f.package.dir.display()
                            ),
                            Err(e) => eprintln!("[autorecord] failed: {e}"),
                        }
                        ctx.send_viewport_cmd(egui::ViewportCommand::Close);
                    }
                    match r {
                        Ok(f) => match EditorState::open(f.package.clone(), ctx, self.render_state.clone()) {
                            Ok(ed) => self.screen = Screen::Editor(Box::new(ed)),
                            Err(e) => self.home.error = Some(e),
                        },
                        Err(e) => self.home.error = Some(e),
                    }
                }
            }
        }
        // Global shortcut.
        if let Some(id) = self.hotkey_id {
            while let Ok(ev) = GlobalHotKeyEvent::receiver().try_recv() {
                if ev.id == id && ev.state == HotKeyState::Pressed {
                    match self.home.phase {
                        Phase::Idle => self.start_recording(ctx),
                        Phase::Recording => self.stop_recording(ctx),
                        _ => {}
                    }
                }
            }
        }
    }

    fn recording_bar(&mut self, ui: &mut egui::Ui) {
        let p = theme::palette(ui.ctx());
        let (mic, sys) = self.home.stats.as_ref().map(|s| (s.mic_peak(), s.system_peak())).unwrap_or((0.0, 0.0));
        egui::CentralPanel::default().frame(egui::Frame::NONE.fill(p.raised).inner_margin(egui::Margin::symmetric(16, 10))).show(ui, |ui| {
            ui.horizontal_centered(|ui| {
                let t = ui.input(|i| i.time);
                let on = (t * 1.6).fract() < 0.7;
                let (r, _) = ui.allocate_exact_size(egui::vec2(12.0, 12.0), egui::Sense::hover());
                ui.painter().circle_filled(r.center(), 5.0, if on { p.danger } else { p.danger.gamma_multiply(0.35) });
                ui.label(egui::RichText::new(fmt_time(self.home.elapsed_ms())).monospace().size(16.0));
                meter(ui, "SYS", sys, self.prefs.system_audio);
                meter(ui, "MIC", mic, self.prefs.microphone);
                ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                    if theme::danger_button(ui, "Stop").clicked() {
                        self.stop_recording(ui.ctx());
                    }
                });
            });
        });
        ui.ctx().request_repaint_after(std::time::Duration::from_millis(50));
    }
}

fn meter(ui: &mut egui::Ui, label: &str, level: f32, active: bool) {
    let p = theme::palette(ui.ctx());
    ui.label(egui::RichText::new(label).small().color(p.dim));
    let (r, _) = ui.allocate_exact_size(egui::vec2(34.0, 6.0), egui::Sense::hover());
    ui.painter().rect_filled(r, 3.0, p.line);
    if active {
        let w = (level.sqrt() * r.width()).clamp(0.0, r.width());
        ui.painter().rect_filled(egui::Rect::from_min_size(r.min, egui::vec2(w, r.height())), 3.0, p.accent);
    }
}

impl eframe::App for App {
    fn logic(&mut self, ctx: &egui::Context, frame: &mut eframe::Frame) {
        #[cfg(windows)]
        if self.native_window.is_none() {
            use raw_window_handle::{HasWindowHandle, RawWindowHandle};
            if let Ok(h) = frame.window_handle() {
                if let RawWindowHandle::Win32(w) = h.as_raw() {
                    self.native_window = Some(w.hwnd.get());
                }
            }
        }
        #[cfg(not(windows))]
        let _ = frame;
        self.pump_events(ctx);
    }

    fn ui(&mut self, ui: &mut egui::Ui, _frame: &mut eframe::Frame) {
        self.draw(ui);
    }

    fn on_exit(&mut self) {
        if let Screen::Editor(ed) = &mut self.screen {
            ed.save_now();
        }
        self.prefs.save();
    }
}

impl App {
    /// One frame of the whole application UI.
    pub fn draw(&mut self, ui: &mut egui::Ui) {
        let ctx = ui.ctx().clone();
        self.dev_screenshot(&ctx);
        if let Some(secs) = self.autorecord {
            match self.home.phase {
                Phase::Idle if !self.autorecord_started => {
                    self.autorecord_started = true;
                    self.start_recording(&ctx);
                    if let Some(e) = &self.home.error {
                        eprintln!("[autorecord] could not start: {e}");
                        ctx.send_viewport_cmd(egui::ViewportCommand::Close);
                    }
                }
                Phase::Recording if self.home.elapsed_ms() >= secs * 1000.0 => self.stop_recording(&ctx),
                _ => {}
            }
            if self.home.error.is_some() && self.home.phase == Phase::Idle && self.autorecord_started {
                eprintln!("[autorecord] error: {}", self.home.error.clone().unwrap_or_default());
                ctx.send_viewport_cmd(egui::ViewportCommand::Close);
            }
            ctx.request_repaint_after(std::time::Duration::from_millis(100));
        }
        if self.bar_mode {
            self.recording_bar(ui);
            return;
        }
        let mut open_pkg = None;
        let mut action = Action::None;
        let mut back = false;
        match &mut self.screen {
            Screen::Home => {
                action = self.home.show(ui, &mut self.prefs);
                if let Action::Open(pkg) = &action {
                    open_pkg = Some(pkg.clone());
                }
            }
            Screen::Editor(ed) => {
                back = ed.ui(ui);
            }
        }
        match action {
            Action::StartRecording => self.start_recording(ui.ctx()),
            Action::StopRecording => self.stop_recording(ui.ctx()),
            _ => {}
        }
        if let Some(pkg) = open_pkg {
            match EditorState::open(pkg, ui.ctx(), self.render_state.clone()) {
                Ok(ed) => self.screen = Screen::Editor(Box::new(ed)),
                Err(e) => self.home.error = Some(e),
            }
        }
        if back {
            if let Screen::Editor(ed) = &mut self.screen {
                ed.pause();
                ed.save_now();
            }
            self.screen = Screen::Home;
            self.home.invalidate_library();
        }
    }
}
