use super::*;
use crate::editor::{ops, Sel};
use screencraft_core::project::Transition;

/// Remembered across panel redraws, like the module-level `opt` of the 0.3 panel.
pub struct PaceOpts {
    pub min_idle: f64,
    pub speed: f64,
    pub keep_speech: bool,
    pub ramp: f64,
    pub hold: f64,
    pub status: String,
}

impl Default for PaceOpts {
    fn default() -> Self {
        Self { min_idle: 2.5, speed: 4.0, keep_speech: true, ramp: 0.4, hold: 1.5, status: String::new() }
    }
}

const KINDS: [(&str, &str); 7] = [("none", "None"), ("dissolve", "Dissolve"), ("fade", "Fade"), ("flash", "Flash"), ("blur", "Blur"), ("slide", "Slide"), ("zoom", "Zoom")];

pub fn show(st: &mut EditorState, ui: &mut Ui) {
    kit::h3(ui, "Speed up idle parts", true);
    note(ui, "Finds stretches with no clicks, typing or cursor movement (and no talking) and fast-forwards them.");
    let mut o = std::mem::take(&mut st.panel.pace);
    kit::slider(ui, "Idle for at least", &mut o.min_idle, 1.0, 10.0, 0.5, |v| format!("{v:.1} s"));
    let speeds = [(2.0f64.to_bits(), "2×"), (4.0f64.to_bits(), "4×"), (8.0f64.to_bits(), "8×")];
    let mut sb = o.speed.to_bits();
    if kit::segmented(ui, Some("Speed"), &speeds, &mut sb) {
        o.speed = f64::from_bits(sb);
    }
    let has_mic = st.project.media.mic_track.is_some() || st.project.media.mic.is_some();
    if has_mic {
        kit::toggle_row(ui, "Keep the parts where I'm talking", None, &mut o.keep_speech);
    }
    if kit::btn_line(ui, "Find and speed up", false).clicked() {
        let segs = st.find_idle(o.min_idle * 1000.0, o.keep_speech && has_mic);
        if segs.is_empty() {
            o.status = "No idle stretches that long. Try a shorter minimum.".into();
        } else {
            let before = st.duration_ms();
            let speed = o.speed;
            let mut ids: Vec<String> = (0..segs.len() * 3 + 4).map(|_| st.new_id("slice")).collect();
            let mut count = 0;
            let ramp_ms = o.ramp * 1000.0;
            st.edit(|p| {
                let mut next = || ids.pop().unwrap_or_else(|| "slice-x".into());
                count = ops::apply_idle(&mut p.scene.slices, &segs, speed, &mut next);
                let _ = ramp_ms;
            });
            let eased = if o.ramp > 0.0 { st.ease_all(ramp_ms) } else { 0 };
            let saved = (before - st.duration_ms()).max(0.0);
            o.status = format!("Sped up {count} stretch{}{}. Saved {:.1} s.", if count == 1 { "" } else { "es" }, if eased > 0 { ", eased in and out" } else { "" }, saved / 1000.0);
            st.select(Sel::None);
        }
    }
    if !o.status.is_empty() {
        kit::hint(ui, &o.status);
    }

    kit::h3(ui, "Speed ramps", false);
    note(ui, "Turns each abrupt change of speed between clips into a smooth ease.");
    kit::slider(ui, "Ease length", &mut o.ramp, 0.2, 1.5, 0.1, |v| format!("{v:.1} s"));
    if kit::btn_line(ui, "Ease all speed changes", false).clicked() {
        let n = st.ease_all(o.ramp * 1000.0);
        o.status = if n > 0 { format!("Eased {n} speed change{}.", if n == 1 { "" } else { "s" }) } else { "No speed changes to ease (or the clips are too short).".into() };
    }

    kit::h3(ui, "Freeze frame", false);
    kit::slider(ui, "Hold for", &mut o.hold, 0.5, 5.0, 0.5, |v| format!("{v:.1} s"));
    if kit::btn_line(ui, "Freeze at playhead", false).clicked() {
        st.freeze_at_playhead(o.hold * 1000.0);
    }

    kit::h3(ui, "Transition", false);
    let slices = st.project.scene.slices.clone();
    let idx = if let Sel::Slice(id) = &st.sel { slices.iter().position(|s| &s.id == id) } else { None };
    st.panel.pace = o;
    let Some(idx) = idx.filter(|&i| i > 0) else {
        note(ui, "Click the + on the timeline at a cut between two clips to give it a transition.");
        return;
    };
    let s = slices[idx].clone();
    kit::hint(ui, &format!("At the cut before “{}”.", if s.freeze { "freeze".to_string() } else { format!("clip {}", idx + 1) }));
    let opts: Vec<(String, &str)> = KINDS.iter().map(|(a, b)| (a.to_string(), *b)).collect();
    let cur = s.transition.as_ref().map(|t| t.kind.clone()).unwrap_or_else(|| "none".into());
    let id = s.id.clone();
    let dur = s.transition.as_ref().map(|t| t.duration_ms).unwrap_or(500.0);
    seg(st, ui, Some("Style"), &opts, cur, |p, v| {
        if let Some(x) = p.scene.slices.iter_mut().find(|x| x.id == id) {
            x.transition = if v == "none" { None } else { Some(Transition { kind: v, duration_ms: dur }) };
        }
    });
    if let Some(t) = &s.transition {
        let id = s.id.clone();
        slider(st, ui, &format!("trans-len-{id}"), "Length", t.duration_ms / 1000.0, (0.2, 1.5, 0.1), |v| format!("{v:.1} s"), |p, v| {
            if let Some(tr) = p.scene.slices.iter_mut().find(|x| x.id == id).and_then(|x| x.transition.as_mut()) {
                tr.duration_ms = (v * 1000.0).round();
            }
        });
        kit::hint(ui, "Dissolve blends the two clips into each other. Fade dips to black, Flash to white, Blur softens the cut, Slide pushes the picture away and Zoom punches in.");
    }
}
