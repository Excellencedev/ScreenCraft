use super::*;
use crate::theme::*;
use crate::editor::{ops, Sel};
use screencraft_core::project::{CameraConfig, Point};

#[derive(Clone)]
enum Target {
    Whole,
    Layout(String),
}

fn edit_cam(st: &mut EditorState, target: &Target, key: &str, f: impl FnOnce(&mut CameraConfig)) {
    let t = target.clone();
    st.edit_keyed(Some(key), |p| match t {
        Target::Whole => f(&mut p.config.camera),
        Target::Layout(id) => {
            if let Some(s) = p.scene.camera_layouts.iter_mut().find(|l| l.id == id).and_then(|l| l.settings.as_mut()) {
                f(s)
            }
        }
    });
}

/// The bubble's settings: the project's own, or the private copy a layout block keeps.
fn bubble_controls(st: &mut EditorState, ui: &mut Ui, c: &CameraConfig, target: &Target, tag: &str) {
    let k = |n: &str| format!("{tag}{n}");
    let (t, mut v) = (target.clone(), c.clone());
    let _ = &mut v;
    macro_rules! tog {
        ($label:expr, $hint:expr, $get:expr, $set:expr) => {{
            let mut b = $get;
            if kit::toggle_row(ui, $label, $hint, &mut b) {
                edit_cam(st, &t, &k($label), |c| $set(c, b));
            }
        }};
    }
    macro_rules! sld {
        ($key:expr, $label:expr, $get:expr, $range:expr, $fmt:expr, $set:expr) => {{
            let mut x = $get;
            if kit::slider(ui, $label, &mut x, $range.0, $range.1, $range.2, $fmt) {
                edit_cam(st, &t, &k($key), |c| $set(c, x));
            }
        }};
    }
    tog!("Hide camera", None, c.hide, |c: &mut CameraConfig, b| c.hide = b);
    tog!("Mirror", None, c.mirror, |c: &mut CameraConfig, b| c.mirror = b);
    sld!("size", "Size", c.size_ratio, (0.1, 0.6, 0.01), pct, |c: &mut CameraConfig, x| c.size_ratio = x);
    sld!("round", "Roundness", c.corner_radius_ratio, (0.0, 0.5, 0.01), |v| format!("{}%", (v * 200.0).round()), |c: &mut CameraConfig, x| c.corner_radius_ratio = x);
    sld!("zs", "Scale while zoomed", c.zoomed_scale, (0.4, 1.0, 0.01), pct, |c: &mut CameraConfig, x| c.zoomed_scale = x);
    let mut ar = c.aspect_ratio;
    let shapes: [(f64, &str); 4] = [(1.0, "Square"), (4.0 / 3.0, "4:3"), (16.0 / 9.0, "16:9"), (3.0 / 4.0, "3:4")];
    // pick the nearest so a float round trip never loses the highlight
    let cur = shapes.iter().map(|s| s.0).min_by(|a, b| (a - ar).abs().partial_cmp(&(b - ar).abs()).unwrap()).unwrap();
    let mut cur_i = shapes.iter().position(|s| s.0 == cur).unwrap();
    let opts: Vec<(usize, &str)> = shapes.iter().enumerate().map(|(i, s)| (i, s.1)).collect();
    if kit::segmented(ui, Some("Shape"), &opts, &mut cur_i) {
        ar = shapes[cur_i].0;
        edit_cam(st, &t, &k("shape"), |c| c.aspect_ratio = ar);
    }
    if let Some((x, y)) = kit::pos_grid(ui, "Position", (c.position.x, c.position.y)) {
        edit_cam(st, &t, &k("pos"), |c| c.position = Point { x, y });
    }
    tog!("Flush to the frame edge", Some("Ignore the padding and sit right on the edge of the video."), c.edge_flush, |c: &mut CameraConfig, b| c.edge_flush = b);
    if !c.edge_flush {
        sld!("pad", "Padding", c.padding_ratio, (0.0, 2.0, 0.05), |v| format!("{}", (v * 100.0).round() / 100.0), |c: &mut CameraConfig, x| c.padding_ratio = x);
    }
    tog!("Shadow", None, c.shadow, |c: &mut CameraConfig, b| c.shadow = b);
    tog!("Border", None, c.border, |c: &mut CameraConfig, b| c.border = b);
    if c.border {
        let mut hex = if c.border_color.is_empty() { "#ffffff".to_string() } else { c.border_color.clone() };
        if kit::color_row(ui, "Border color", &mut hex) {
            edit_cam(st, &t, &k("bcol"), |c| c.border_color = hex);
        }
    }
}

pub fn show(st: &mut EditorState, ui: &mut Ui) {
    kit::h3(ui, "Camera", true);
    if st.project.media.camera.is_none() {
        note(ui, "This recording has no camera track. Enable a camera in the recorder toolbar before recording.");
        return;
    }
    let base = st.project.config.camera.clone();
    let blocks = st.project.scene.camera_layouts.clone();
    let lay = if let Sel::Layout(id) = &st.sel { blocks.iter().find(|l| &l.id == id).cloned() } else { None };

    let add_tiles = |st: &mut EditorState, ui: &mut Ui, title: &str, intro: &str| {
        kit::h3(ui, title, false);
        note(ui, intro);
        ui.add_space(6.0);
        let kinds = ["bubble", "side-right", "side-left", "camera", "large", "hidden"];
        let mut pick = None;
        kit::grid(ui, 2, kinds.len(), 6.0, 34.0, |ui, i, cell| {
            let resp = kit::cell_response(ui, cell, ("addlay", i));
            let hov = resp.hovered();
            ui.painter().rect_filled(cell, eframe::egui::CornerRadius::same(8), if hov { RAISE } else { PANEL_2 });
            ui.painter().rect_stroke(cell, eframe::egui::CornerRadius::same(8), eframe::egui::Stroke::new(1.0, if hov { ACCENT_2 } else { LINE_2 }), eframe::egui::StrokeKind::Inside);
            ui.painter().text(cell.center(), eframe::egui::Align2::CENTER_CENTER, ops::layout_name(kinds[i]), kit::font(11.0), if hov { eframe::egui::Color32::WHITE } else { eframe::egui::Color32::from_rgb(0xcd, 0xd6, 0xe8) });
            if resp.clicked() {
                pick = Some(kinds[i]);
            }
        });
        if let Some(k) = pick {
            st.add_layout(k);
        }
    };

    if let Some(l) = lay {
        // every block keeps its own settings; a block that has none yet starts from the project's
        let settings = match l.settings.clone() {
            Some(s) => s,
            None => {
                let id = l.id.clone();
                let b = base.clone();
                if let Some(x) = st.project.scene.camera_layouts.iter_mut().find(|x| x.id == id) {
                    x.settings = Some(b.clone());
                }
                st.touch();
                b
            }
        };
        kit::h3(ui, "Camera block", false);
        let names: Vec<(String, &str)> = ops::LAYOUT_NAMES.iter().map(|(a, b)| (a.to_string(), *b)).collect();
        let id = l.id.clone();
        let mut cur = l.layout.clone();
        if kit::segmented(ui, Some("Layout"), &names, &mut cur) {
            st.edit(|p| {
                if let Some(x) = p.scene.camera_layouts.iter_mut().find(|x| x.id == id) {
                    x.layout = cur;
                }
            });
        }
        kit::hint(ui, "Drag the purple block on the Camera track to move it, its ends to trim. The camera eases between blocks.");
        kit::h3(ui, "This block's camera", false);
        bubble_controls(st, ui, &settings, &Target::Layout(l.id.clone()), &format!("blk-{}-", l.id));
        if kit::btn_line(ui, "Remove this block", true).clicked() {
            st.delete_selected();
        }
        return;
    }
    if !blocks.is_empty() {
        note(ui, "Select a purple camera block to edit.");
        add_tiles(st, ui, "Add a block", "Adds a block at the playhead on the Camera track.");
        return;
    }
    // no blocks: these are the settings for the whole video
    kit::h3(ui, "Whole video", false);
    bubble_controls(st, ui, &base, &Target::Whole, "cam-");
    add_tiles(
        st,
        ui,
        "Change it over time",
        "Show the camera differently for part of the video. Adds a block at the playhead on the Camera track; every block gets its own settings.",
    );
}
