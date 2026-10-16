//! The nine inspector panels. Each mirrors the matching 0.3 panel control for control.

mod annotate;
mod appearance;
mod audio;
mod camera;
mod captions;
mod crop;
mod cursor;
mod pace;
mod zoom;

use super::{EditorState, Tab};
use crate::kit;
use eframe::egui::{self, Ui};
use screencraft_core::project::Project;
use std::time::Instant;

/// Settings the panels remember between redraws (the pace tools, caption generation ...).
pub struct PanelState {
    pub space_since: Option<Instant>,
    pub space_held: bool,
    pub space_was_playing: bool,
    pub pace: pace::PaceOpts,
    pub caps: captions::CapState,
    pub audio: audio::AudioState,
}

impl Default for PanelState {
    fn default() -> Self {
        Self {
            space_since: None,
            space_held: false,
            space_was_playing: false,
            pace: pace::PaceOpts::default(),
            caps: captions::CapState::default(),
            audio: audio::AudioState::default(),
        }
    }
}

impl PanelState {
    pub fn poll(&mut self, ctx: &egui::Context) {
        self.caps.poll(ctx);
    }
}

pub use captions::preset as caption_preset;

pub fn show(st: &mut EditorState, ui: &mut Ui) {
    match st.tab {
        Tab::Appearance => appearance::show(st, ui),
        Tab::Cursor => cursor::show(st, ui),
        Tab::Camera => camera::show(st, ui),
        Tab::Zoom => zoom::show(st, ui),
        Tab::Audio => audio::show(st, ui),
        Tab::Crop => crop::show(st, ui),
        Tab::Annotate => annotate::show(st, ui),
        Tab::Captions => captions::show(st, ui),
        Tab::Pace => pace::show(st, ui),
    }
}

// ---- binding helpers: read a value, show the control, write back through an undoable edit ----

pub fn slider(
    st: &mut EditorState,
    ui: &mut Ui,
    key: &str,
    label: &str,
    cur: f64,
    (min, max, step): (f64, f64, f64),
    fmt: impl Fn(f64) -> String,
    set: impl FnOnce(&mut Project, f64),
) {
    let mut v = cur;
    if kit::slider(ui, label, &mut v, min, max, step, fmt) {
        st.edit_keyed(Some(key), |p| set(p, v));
    }
}

pub fn toggle(st: &mut EditorState, ui: &mut Ui, label: &str, hint: Option<&str>, cur: bool, set: impl FnOnce(&mut Project, bool)) {
    let mut v = cur;
    if kit::toggle_row(ui, label, hint, &mut v) {
        st.edit(|p| set(p, v));
    }
}

pub fn seg<T: PartialEq + Clone>(
    st: &mut EditorState,
    ui: &mut Ui,
    label: Option<&str>,
    opts: &[(T, &str)],
    cur: T,
    set: impl FnOnce(&mut Project, T),
) {
    let mut v = cur;
    if kit::segmented(ui, label, opts, &mut v) {
        st.edit(|p| set(p, v));
    }
}

pub fn color(st: &mut EditorState, ui: &mut Ui, key: &str, label: &str, cur: &str, set: impl FnOnce(&mut Project, String)) {
    let mut v = cur.to_string();
    if kit::color_row(ui, label, &mut v) {
        st.edit_keyed(Some(key), |p| set(p, v));
    }
}

pub fn pct(v: f64) -> String {
    format!("{}%", (v * 100.0).round())
}

pub fn sec2(v: f64) -> String {
    format!("{v:.2} s")
}

pub fn px(v: f64) -> String {
    format!("{} px", v.round())
}

pub fn deg(v: f64) -> String {
    format!("{}°", v.round())
}

/// Wrapped read-only text for panels (`.muted` divs).
pub fn note(ui: &mut Ui, text: &str) {
    kit::muted(ui, text);
}
