use super::*;
use crate::editor::{ops, Sel};
use crate::theme::*;
use eframe::egui::{Align2, Color32, CornerRadius, Sense, Stroke, StrokeKind, Vec2};

const ADD: [(&str, &str, &str); 8] = [
    ("text", "Text", "a-text"),
    ("arrow", "Arrow", "a-arrow"),
    ("box", "Box", "a-box"),
    ("ellipse", "Circle", "a-ellipse"),
    ("highlight", "Highlight", "a-highlight"),
    ("blur", "Blur", "a-blur"),
    ("pixelate", "Pixelate", "a-pixelate"),
    ("spotlight", "Spotlight", "a-spotlight"),
];

fn hex6(c: &str) -> String {
    if c.len() >= 7 && c.starts_with('#') {
        c[..7].to_string()
    } else {
        "#000000".to_string()
    }
}

fn alpha_of(c: &str) -> f64 {
    if c.len() == 9 && c.starts_with('#') {
        u8::from_str_radix(&c[7..], 16).map(|v| v as f64 / 255.0).unwrap_or(1.0)
    } else {
        1.0
    }
}

fn with_alpha(rgb: &str, a: f64) -> String {
    format!("{}{:02x}", rgb, (a.clamp(0.0, 1.0) * 255.0).round() as u8)
}

pub fn show(st: &mut EditorState, ui: &mut Ui) {
    kit::h3(ui, "Annotate", true);
    note(ui, "Add something at the playhead, then drag it on the preview. Blur and spotlight stay on the same spot of the recording as the camera zooms.");
    ui.add_space(10.0);
    let mut add = None;
    kit::grid(ui, 4, ADD.len(), 6.0, 62.0, |ui, i, cell| {
        let (kind, name, icon) = ADD[i];
        let resp = kit::cell_response(ui, cell, ("add", i));
        let hov = resp.hovered();
        ui.painter().rect_filled(cell, CornerRadius::same(8), if hov { RAISE } else { PANEL_2 });
        ui.painter().rect_stroke(cell, CornerRadius::same(8), Stroke::new(1.0, if hov { ACCENT_2 } else { LINE_2 }), StrokeKind::Inside);
        let fg = if hov { Color32::WHITE } else { Color32::from_rgb(0xcd, 0xd6, 0xe8) };
        crate::icons::paint(ui, icon, eframe::egui::Rect::from_center_size(eframe::egui::Pos2::new(cell.center().x, cell.min.y + 21.0), Vec2::splat(20.0)), fg);
        ui.painter().text(eframe::egui::Pos2::new(cell.center().x, cell.max.y - 13.0), Align2::CENTER_CENTER, name, kit::font(11.0), fg);
        if resp.on_hover_text(format!("Add {} at the playhead", name.to_lowercase())).clicked() {
            add = Some(kind);
        }
    });
    ui.add_space(4.0);
    if let Some(k) = add {
        st.add_annotation(k);
    }

    let list = st.project.scene.annotations.clone();
    let cur = if let Sel::Annot(id) = &st.sel { list.iter().find(|a| &a.id == id).cloned() } else { None };
    if let Some(a) = cur {
        edit_annotation(st, ui, &a);
    } else if !list.is_empty() {
        kit::h3(ui, "On the timeline", false);
        for it in &list {
            let label = if it.kind == "text" { format!("Text · {}", it.text.chars().take(26).collect::<String>()) } else { ops::note_name(&it.kind).to_string() };
            if note_row(ui, &label).clicked() {
                st.select(Sel::Annot(it.id.clone()));
            }
        }
    }

    // shortcut display
    let ks = st.project.config.keystrokes.clone();
    kit::h3(ui, "Shortcut display", false);
    toggle(st, ui, "Show shortcuts", Some("Pops up keys like ⌘C or Esc as you press them. Only recordings made after this feature was added carry shortcut data."), ks.enabled, |p, v| p.config.keystrokes.enabled = v);
    if ks.enabled {
        let pos: Vec<(String, &str)> = vec![("bottom-center".into(), "Bottom"), ("bottom-left".into(), "Left"), ("bottom-right".into(), "Right"), ("top-center".into(), "Top")];
        seg(st, ui, Some("Position"), &pos, ks.position.clone(), |p, v| p.config.keystrokes.position = v);
        slider(st, ui, "ks-size", "Size", ks.size, (16.0, 64.0, 1.0), px, |p, v| p.config.keystrokes.size = v);
        slider(st, ui, "ks-hold", "Stays on screen", ks.hold_ms, (600.0, 3000.0, 100.0), |v| format!("{:.1} s", v / 1000.0), |p, v| p.config.keystrokes.hold_ms = v);
    }
}

/// `.note-row`: a full-width list button.
pub fn note_row(ui: &mut Ui, text: &str) -> eframe::egui::Response {
    ui.add_space(2.0);
    let (rect, resp) = ui.allocate_exact_size(Vec2::new(ui.available_width(), 32.0), Sense::click());
    ui.painter().rect_filled(rect, CornerRadius::same(6), if resp.hovered() { RAISE } else { PANEL_2 });
    ui.painter().rect_stroke(rect, CornerRadius::same(6), Stroke::new(1.0, LINE), StrokeKind::Inside);
    ui.painter().with_clip_rect(rect.shrink(4.0)).text(eframe::egui::Pos2::new(rect.min.x + 10.0, rect.center().y), Align2::LEFT_CENTER, text, kit::font(12.0), TEXT);
    resp.on_hover_cursor(eframe::egui::CursorIcon::PointingHand)
}

fn edit_annotation(st: &mut EditorState, ui: &mut Ui, a: &screencraft_core::project::Annotation) {
    let k = a.kind.clone();
    let id = a.id.clone();
    kit::h3(ui, ops::note_name(&k), false);
    macro_rules! edit {
        ($key:expr, $f:expr) => {{
            let id2 = id.clone();
            st.edit_keyed(Some(&format!("{}-{}", $key, id)), move |p| {
                if let Some(x) = p.scene.annotations.iter_mut().find(|x| x.id == id2) {
                    $f(x)
                }
            });
        }};
    }
    if k == "text" {
        ui.add_space(6.0);
        kit::muted(ui, "Text");
        let mut text = a.text.clone();
        let r = ui.add(eframe::egui::TextEdit::multiline(&mut text).desired_rows(3).desired_width(f32::INFINITY).margin(eframe::egui::Margin::symmetric(10, 8)));
        if r.changed() {
            edit!("an-text", |x: &mut screencraft_core::project::Annotation| x.text = text.clone());
        }
        let mut fs = a.font_size;
        if kit::slider(ui, "Size", &mut fs, 16.0, 140.0, 1.0, px) {
            edit!("an-fs", |x: &mut screencraft_core::project::Annotation| x.font_size = fs);
        }
        let mut col = hex6(&a.color);
        if kit::color_row(ui, "Text color", &mut col) {
            edit!("an-col", |x: &mut screencraft_core::project::Annotation| x.color = col.clone());
        }
        let has_fill = !a.fill.is_empty() && a.fill != "none";
        let mut f = has_fill;
        if kit::toggle_row(ui, "Background", None, &mut f) {
            edit!("an-fill", |x: &mut screencraft_core::project::Annotation| x.fill = if f { "#000000b3".into() } else { String::new() });
        }
        if has_fill {
            let mut fc = hex6(&a.fill);
            if kit::color_row(ui, "Background color", &mut fc) {
                let al = alpha_of(&a.fill);
                edit!("an-fc", |x: &mut screencraft_core::project::Annotation| x.fill = with_alpha(&fc, al));
            }
            let mut fo = alpha_of(&a.fill);
            if kit::slider(ui, "Background opacity", &mut fo, 0.1, 1.0, 0.05, pct) {
                let rgb = hex6(&a.fill);
                edit!("an-fo", |x: &mut screencraft_core::project::Annotation| x.fill = with_alpha(&rgb, fo));
            }
        }
    } else if matches!(k.as_str(), "box" | "ellipse" | "arrow" | "highlight") {
        let mut col = hex6(&a.color);
        if kit::color_row(ui, "Color", &mut col) {
            edit!("an-col", |x: &mut screencraft_core::project::Annotation| x.color = col.clone());
        }
        if k != "highlight" {
            let mut th = a.thickness;
            if kit::slider(ui, "Thickness", &mut th, 1.0, 24.0, 1.0, px) {
                edit!("an-th", |x: &mut screencraft_core::project::Annotation| x.thickness = th);
            }
        }
    } else if k == "blur" || k == "pixelate" {
        let mut s = a.strength;
        if kit::slider(ui, "Strength", &mut s, 0.1, 1.0, 0.05, pct) {
            edit!("an-st", |x: &mut screencraft_core::project::Annotation| x.strength = s);
        }
    } else if k == "spotlight" {
        let mut s = a.strength;
        if kit::slider(ui, "Dim outside", &mut s, 0.2, 0.95, 0.05, pct) {
            edit!("an-st", |x: &mut screencraft_core::project::Annotation| x.strength = s);
        }
    }
    if matches!(k.as_str(), "text" | "arrow" | "box" | "ellipse" | "highlight") {
        let mut an = a.anim.clone();
        if kit::segmented(ui, Some("Appears"), &[("fade".to_string(), "Fade"), ("pop".to_string(), "Pop"), ("none".to_string(), "Instant")], &mut an) {
            edit!("an-anim", |x: &mut screencraft_core::project::Annotation| x.anim = an.clone());
        }
        let mut sp = a.space.clone();
        if kit::segmented(ui, Some("Stays"), &[("content".to_string(), "On the recording"), ("screen".to_string(), "On the frame")], &mut sp) {
            edit!("an-space", |x: &mut screencraft_core::project::Annotation| x.space = sp.clone());
        }
    }
    let dur = (a.source_end_ms - a.source_start_ms) / 1000.0;
    kit::hint(
        ui,
        &format!("{dur:.1} s long. Drag the block on the Notes track to move it, its ends to trim. Drag on the preview to position{}.", if k == "text" { "" } else { " and resize" }),
    );
    if kit::btn_line(ui, "Delete", true).clicked() {
        st.delete_selected();
    }
}
