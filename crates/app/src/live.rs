//! Live inputs of the recorder: the camera picture shown in the bar (and fed to the recorder)
//! and the microphone level meter, started and stopped to match the current selections.

use crate::prefs::Prefs;
use crate::recorder::Phase;
use eframe::egui;
use screencraft_audio::{AudioBlock, AudioCapture, CaptureConfig, Clock};
use screencraft_capture::camera::{CamFrame, CameraHub, CameraStream};
use std::sync::atomic::{AtomicBool, AtomicU32, Ordering};
use std::sync::{Arc, Mutex};

#[derive(Clone, Copy, PartialEq, Eq)]
pub enum Input {
    Camera,
    Mic,
    Sys,
}

struct Started<T> {
    key: Option<String>,
    handle: T,
}

pub struct Live {
    ctx: egui::Context,
    hub: Arc<CameraHub>,
    camera: Option<Started<CameraStream>>,
    camera_starting: Arc<AtomicBool>,
    camera_slot: Arc<Mutex<Option<Result<Started<CameraStream>, String>>>>,
    tex: Option<egui::TextureHandle>,
    last_frame: usize,
    mic: Option<Started<AudioCapture>>,
    mic_starting: Arc<AtomicBool>,
    mic_slot: Arc<Mutex<Option<Result<Started<AudioCapture>, String>>>>,
    mic_level: Arc<AtomicU32>,
    error: Arc<Mutex<Option<String>>>,
}

fn set_peak(slot: &AtomicU32, v: f32) {
    let old = f32::from_bits(slot.load(Ordering::Relaxed));
    let new = if v > old { v } else { old * 0.92 + v * 0.08 };
    slot.store(new.to_bits(), Ordering::Relaxed);
}

impl Live {
    pub fn new(ctx: &egui::Context) -> Self {
        Self {
            ctx: ctx.clone(),
            hub: CameraHub::new(),
            camera: None,
            camera_starting: Arc::new(AtomicBool::new(false)),
            camera_slot: Arc::new(Mutex::new(None)),
            tex: None,
            last_frame: 0,
            mic: None,
            mic_starting: Arc::new(AtomicBool::new(false)),
            mic_slot: Arc::new(Mutex::new(None)),
            mic_level: Arc::new(AtomicU32::new(0)),
            error: Arc::new(Mutex::new(None)),
        }
    }

    pub fn camera_source(&self) -> Arc<CameraHub> {
        self.hub.clone()
    }

    pub fn take_error(&mut self) -> Option<String> {
        self.error.lock().unwrap().take()
    }

    pub fn loading(&self, which: Input, prefs: &Prefs) -> bool {
        match which {
            Input::Camera => prefs.camera && self.camera_starting.load(Ordering::SeqCst),
            Input::Mic => prefs.microphone && self.mic_starting.load(Ordering::SeqCst),
            Input::Sys => false,
        }
    }

    pub fn any_loading(&self, prefs: &Prefs) -> bool {
        self.loading(Input::Camera, prefs) || self.loading(Input::Mic, prefs)
    }

    pub fn levels(&self) -> (f32, f32) {
        (f32::from_bits(self.mic_level.load(Ordering::Relaxed)), 0.0)
    }

    /// Start or stop the camera and microphone meter so they match `prefs` and the phase.
    pub fn sync(&mut self, prefs: &Prefs, phase: Phase) {
        // finished background starts
        if let Some(r) = self.camera_slot.lock().unwrap().take() {
            match r {
                Ok(s) => self.camera = Some(s),
                Err(e) => *self.error.lock().unwrap() = Some(e),
            }
        }
        if let Some(r) = self.mic_slot.lock().unwrap().take() {
            match r {
                Ok(s) => self.mic = Some(s),
                Err(e) => *self.error.lock().unwrap() = Some(e),
            }
        }
        let in_use = matches!(phase, Phase::Idle | Phase::Countdown | Phase::Recording | Phase::Confirm | Phase::Starting);
        // camera
        let want_cam = prefs.camera && in_use;
        let cam_key = prefs.camera_id.clone();
        if let Some(c) = &self.camera {
            if !want_cam || c.key != cam_key {
                if let Some(c) = self.camera.take() {
                    c.handle.stop();
                }
                self.hub.clear();
                self.tex = None;
            }
        }
        if want_cam && self.camera.is_none() && !self.camera_starting.swap(true, Ordering::SeqCst) {
            let (slot, flag, hub, ctx, id) = (self.camera_slot.clone(), self.camera_starting.clone(), self.hub.clone(), self.ctx.clone(), cam_key.clone());
            std::thread::spawn(move || {
                let h2 = hub.clone();
                let ctx2 = ctx.clone();
                let r = screencraft_capture::camera::start(id.as_deref(), move |f: CamFrame| {
                    h2.publish(f);
                    ctx2.request_repaint();
                })
                .map(|handle| Started { key: id, handle });
                *slot.lock().unwrap() = Some(r);
                flag.store(false, Ordering::SeqCst);
                ctx.request_repaint();
            });
        }
        // microphone meter (the recording reads the microphone itself while it runs)
        let want_mic = prefs.microphone && phase == Phase::Idle;
        let mic_key = prefs.microphone_id.clone();
        if let Some(m) = &self.mic {
            if !want_mic || m.key != mic_key {
                if let Some(m) = self.mic.take() {
                    m.handle.stop();
                }
                self.mic_level.store(0, Ordering::Relaxed);
            }
        }
        if want_mic && self.mic.is_none() && !self.mic_starting.swap(true, Ordering::SeqCst) {
            let (slot, flag, level, ctx, id) = (self.mic_slot.clone(), self.mic_starting.clone(), self.mic_level.clone(), self.ctx.clone(), mic_key);
            std::thread::spawn(move || {
                let (tx, rx) = std::sync::mpsc::channel::<AudioBlock>();
                let clock = Clock::start();
                let cfg = CaptureConfig { microphone: id.clone(), use_microphone: true, system: None, use_system: false };
                let r = AudioCapture::start(&cfg, &clock, tx).map(|handle| Started { key: id, handle });
                if r.is_ok() {
                    let ctx2 = ctx.clone();
                    std::thread::spawn(move || {
                        while let Ok(b) = rx.recv() {
                            set_peak(&level, screencraft_audio::mix::peak(&b.data));
                            ctx2.request_repaint_after(std::time::Duration::from_millis(40));
                        }
                    });
                }
                *slot.lock().unwrap() = Some(r);
                flag.store(false, Ordering::SeqCst);
                ctx.request_repaint();
            });
        }
    }

    /// The current camera picture as a small square texture for the bar.
    pub fn camera_texture(&mut self) -> Option<egui::TextureId> {
        let f = self.hub.latest()?;
        let ptr = Arc::as_ptr(&f) as usize;
        if ptr != self.last_frame || self.tex.is_none() {
            self.last_frame = ptr;
            const S: usize = 96;
            // centre crop to a square, then sample down
            let side = f.width.min(f.height) as usize;
            let (ox, oy) = ((f.width as usize - side) / 2, (f.height as usize - side) / 2);
            let mut px = Vec::with_capacity(S * S);
            for y in 0..S {
                let sy = oy + y * side / S;
                for x in 0..S {
                    let sx = ox + x * side / S;
                    let o = (sy * f.width as usize + sx) * 3;
                    px.push(egui::Color32::from_rgb(f.rgb[o], f.rgb[o + 1], f.rgb[o + 2]));
                }
            }
            let img = egui::ColorImage::new([S, S], px);
            match &mut self.tex {
                Some(t) => t.set(img, egui::TextureOptions::LINEAR),
                None => self.tex = Some(self.ctx.load_texture("camera-preview", img, egui::TextureOptions::LINEAR)),
            }
        }
        self.tex.as_ref().map(|t| t.id())
    }
}
