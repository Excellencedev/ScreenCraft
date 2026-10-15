//! Right-hand inspector: settings for the selection and for the whole project.

use super::{EditorState, Selection, Tab};
use crate::theme::{self, section};
use eframe::egui::{self, Color32};
use screencraft_core::project::{ClickEffect, Project, ZoomKind};
use screencraft_render::wallpapers::WALLPAPERS;

fn hex_to_color(h: &str) -> Color32 {
    let c = screencraft_render::wallpapers::parse_color(h);
    Color32::from_rgba_unmultiplied((c[0] * 255.0) as u8, (c[1] * 255.0) as u8, (c[2] * 255.0) as u8, (c[3] * 255.0) as u8)
}

fn color_to_hex(c: Color32) -> String {
    format!("#{:02x}{:02x}{:02x}", c.r(), c.g(), c.b())
}

/// A label of fixed width followed by a control that fills the rest of the row.
fn labelled(ui: &mut egui::Ui, label: &str, add: impl FnOnce(&mut egui::Ui)) {
    ui.horizontal(|ui| {
        let (r, _) = ui.allocate_exact_size(egui::vec2(104.0, 22.0), egui::Sense::hover());
        ui.painter().text(
            r.left_center(),
            egui::Align2::LEFT_CENTER,
            label,
            egui::FontId::proportional(13.5),
            ui.visuals().text_color(),
        );
        ui.spacing_mut().slider_width = (ui.available_width() - 74.0).max(50.0);
        add(ui);
    });
}

fn slider(
    ui: &mut egui::Ui,
    st: &mut EditorState,
    label: &str,
    range: std::ops::RangeInclusive<f64>,
    suffix: &str,
    get: impl Fn(&Project) -> f64,
    set: impl Fn(&mut Project, f64),
) {
    let mut v = get(&st.project);
    labelled(ui, label, |ui| {
        if ui.add(egui::Slider::new(&mut v, range).suffix(suffix).max_decimals(2)).changed() {
            st.edit(|p| set(p, v));
        }
    });
}

fn toggle(ui: &mut egui::Ui, st: &mut EditorState, label: &str, get: impl Fn(&Project) -> bool, set: impl Fn(&mut Project, bool)) {
    let mut v = get(&st.project);
    if ui.checkbox(&mut v, label).changed() {
        st.edit(|p| set(p, v));
    }
}

fn color_row(ui: &mut egui::Ui, st: &mut EditorState, label: &str, get: impl Fn(&Project) -> String, set: impl Fn(&mut Project, String)) {
    let mut c = hex_to_color(&get(&st.project));
    labelled(ui, label, |ui| {
        if ui.color_edit_button_srgba(&mut c).changed() {
            let hex = color_to_hex(c);
            st.edit(|p| set(p, hex));
        }
    });
}

pub fn show(st: &mut EditorState, ui: &mut egui::Ui) {
    let p = theme::palette(ui.ctx());
    ui.add_space(10.0);
    ui.horizontal_wrapped(|ui| {
        ui.spacing_mut().item_spacing.x = 2.0;
        for (t, name) in [
            (Tab::Clip, "Clip"),
            (Tab::Zoom, "Zoom"),
            (Tab::Cursor, "Cursor"),
            (Tab::Style, "Style"),
            (Tab::Output, "Output"),
            (Tab::Audio, "Audio"),
        ] {
            let sel = st.tab == t;
            let b = egui::Button::new(egui::RichText::new(name).color(if sel { p.text } else { p.dim }))
                .fill(if sel { p.hover } else { Color32::TRANSPARENT })
                .stroke(egui::Stroke::NONE);
            if ui.add(b).clicked() {
                st.tab = t;
            }
        }
    });
    ui.add_space(4.0);
    ui.separator();
    ui.add_space(4.0);
    egui::Frame::NONE.inner_margin(egui::Margin::symmetric(14, 4)).show(ui, |ui| {
        ui.set_width(ui.available_width());
        match st.tab {
            Tab::Clip => clip(st, ui),
            Tab::Zoom => zoom(st, ui),
            Tab::Cursor => cursor(st, ui),
            Tab::Style => style(st, ui),
            Tab::Output => output(st, ui),
            Tab::Audio => audio(st, ui),
        }
    });
}

fn hint(ui: &mut egui::Ui, text: &str) {
    let p = theme::palette(ui.ctx());
    ui.label(egui::RichText::new(text).small().color(p.dim));
}

fn clip(st: &mut EditorState, ui: &mut egui::Ui) {
    let Selection::Slice(i) = st.selection else {
        section(ui, "Clip");
        hint(ui, "Select a clip on the timeline to change its speed and volume. Drag its edges to trim, press S to split at the playhead, Delete to cut it out.");
        return;
    };
    if i >= st.project.scene.slices.len() {
        return;
    }
    section(ui, &format!("Clip {}", i + 1));
    let mut speed = st.project.scene.slices[i].time_scale;
    labelled(ui, "Speed", |ui| {
        if ui.add(egui::Slider::new(&mut speed, 0.25..=8.0).logarithmic(true).suffix("×")).changed() {
            st.edit(|p| p.scene.slices[i].time_scale = speed);
        }
    });
    ui.horizontal(|ui| {
        for v in [0.5, 1.0, 1.5, 2.0, 4.0] {
            if ui.small_button(format!("{v}×")).clicked() {
                st.edit(|p| p.scene.slices[i].time_scale = v);
            }
        }
    });
    let mut vol = st.project.scene.slices[i].volume;
    labelled(ui, "Volume", |ui| {
        if ui.add(egui::Slider::new(&mut vol, 0.0..=1.5).custom_formatter(|v, _| format!("{:.0}%", v * 100.0))).changed() {
            st.edit(|p| p.scene.slices[i].volume = vol);
        }
    });
    let mut hide = st.project.scene.slices[i].hide_cursor;
    if ui.checkbox(&mut hide, "Hide cursor in this clip").changed() {
        st.edit(|p| p.scene.slices[i].hide_cursor = hide);
    }
    let s = &st.project.scene.slices[i];
    ui.add_space(6.0);
    hint(ui, &format!("Source {} – {}", theme::fmt_time(s.source_start_ms), theme::fmt_time(s.source_end_ms)));
}

fn zoom(st: &mut EditorState, ui: &mut egui::Ui) {
    section(ui, "Default");
    slider(ui, st, "Zoom level", 1.0..=5.0, "×", |p| p.config.animations.default_zoom, |p, v| p.config.animations.default_zoom = v);
    slider(ui, st, "Ease in", 100.0..=1500.0, " ms", |p| p.config.animations.ease_in_ms, |p, v| p.config.animations.ease_in_ms = v);
    slider(ui, st, "Ease out", 100.0..=1500.0, " ms", |p| p.config.animations.ease_out_ms, |p, v| p.config.animations.ease_out_ms = v);
    slider(ui, st, "Follow smoothing", 50.0..=1200.0, " ms", |p| p.config.animations.follow_smooth_ms, |p, v| p.config.animations.follow_smooth_ms = v);

    let Selection::Zoom(i) = st.selection else {
        ui.add_space(8.0);
        hint(ui, "Select a zoom region on the timeline, or use Auto zoom to create them from your clicks and typing.");
        return;
    };
    if i >= st.project.scene.zooms.len() {
        return;
    }
    section(ui, &format!("Zoom region {}", i + 1));
    let mut z = st.project.scene.zooms[i].zoom;
    labelled(ui, "Level", |ui| {
        if ui.add(egui::Slider::new(&mut z, 1.0..=5.0).suffix("×")).changed() {
            st.edit(|p| p.scene.zooms[i].zoom = z);
        }
    });
    let mut kind = st.project.scene.zooms[i].kind;
    if theme::segmented(ui, &mut kind, &[(ZoomKind::FollowCursor, "Follow cursor"), (ZoomKind::Manual, "Fixed point")]) {
        st.edit(|p| p.scene.zooms[i].kind = kind);
    }
    if kind == ZoomKind::Manual {
        hint(ui, "Click or drag in the preview to place the zoom target.");
        let t = st.project.scene.zooms[i].manual_target_point;
        hint(ui, &format!("Target {:.0}%, {:.0}%", t.x * 100.0, t.y * 100.0));
    }
    let mut instant = st.project.scene.zooms[i].instant;
    if ui.checkbox(&mut instant, "Jump instead of easing").changed() {
        st.edit(|p| p.scene.zooms[i].instant = instant);
    }
    let mut disabled = st.project.scene.zooms[i].is_disabled;
    if ui.checkbox(&mut disabled, "Disabled").changed() {
        st.edit(|p| p.scene.zooms[i].is_disabled = disabled);
    }
    let z = &st.project.scene.zooms[i];
    hint(ui, &format!("Source {} – {}", theme::fmt_time(z.source_start_ms), theme::fmt_time(z.source_end_ms)));
}

fn cursor(st: &mut EditorState, ui: &mut egui::Ui) {
    section(ui, "Pointer");
    toggle(ui, st, "Hide cursor", |p| p.config.cursor.hide, |p, v| p.config.cursor.hide = v);
    slider(ui, st, "Size", 12.0..=96.0, " pt", |p| p.config.cursor.size, |p, v| p.config.cursor.size = v);
    let mut style = st.project.config.cursor.style.clone();
    ui.horizontal(|ui| {
        ui.label("Style");
        egui::ComboBox::from_id_salt("cursor-style")
            .selected_text(match style.as_str() {
                "light" => "Light",
                "accent" => "Accent",
                "dot" => "Dot",
                _ => "Dark",
            })
            .show_ui(ui, |ui| {
                for (v, n) in [("macos", "Dark"), ("light", "Light"), ("accent", "Accent"), ("dot", "Dot")] {
                    ui.selectable_value(&mut style, v.to_string(), n);
                }
            });
    });
    if style != st.project.config.cursor.style {
        st.edit(|p| p.config.cursor.style = style);
    }
    slider(ui, st, "Smoothing", 0.0..=400.0, " ms", |p| p.config.animations.cursor_smooth_ms, |p, v| p.config.animations.cursor_smooth_ms = v);
    toggle(ui, st, "Tilt with movement", |p| p.config.cursor.rotate_with_velocity, |p, v| p.config.cursor.rotate_with_velocity = v);

    section(ui, "Highlight");
    toggle(ui, st, "Highlight the cursor", |p| p.config.cursor.highlight, |p, v| p.config.cursor.highlight = v);
    if st.project.config.cursor.highlight {
        slider(ui, st, "Radius", 12.0..=120.0, " pt", |p| p.config.cursor.highlight_size, |p, v| p.config.cursor.highlight_size = v);
        slider(ui, st, "Opacity", 0.05..=1.0, "", |p| p.config.cursor.highlight_opacity, |p, v| p.config.cursor.highlight_opacity = v);
        color_row(ui, st, "Colour", |p| p.config.cursor.highlight_color.clone(), |p, v| p.config.cursor.highlight_color = v);
    }

    section(ui, "Clicks");
    let mut fx = st.project.config.cursor.click_effect;
    if theme::segmented(ui, &mut fx, &[(ClickEffect::None, "None"), (ClickEffect::Circle, "Circle"), (ClickEffect::Ripple, "Ripple")]) {
        st.edit(|p| p.config.cursor.click_effect = fx);
    }
    if fx != ClickEffect::None {
        color_row(ui, st, "Colour", |p| p.config.cursor.click_effect_color.clone(), |p, v| p.config.cursor.click_effect_color = v);
    }
}

fn style(st: &mut EditorState, ui: &mut egui::Ui) {
    section(ui, "Background");
    let mut kind = st.project.config.styles.background.kind.clone();
    let mut changed = false;
    ui.horizontal(|ui| {
        for (v, n) in [("wallpaper", "Wallpaper"), ("gradient", "Gradient"), ("color", "Solid")] {
            let sel = kind == v;
            if ui.selectable_label(sel, n).clicked() && !sel {
                kind = v.to_string();
                changed = true;
            }
        }
    });
    if changed {
        st.edit(|p| p.config.styles.background.kind = kind.clone());
    }
    match st.project.config.styles.background.kind.as_str() {
        "wallpaper" => {
            let cur = st.project.config.styles.background.wallpaper.clone();
            let p = theme::palette(ui.ctx());
            let mut pick: Option<String> = None;
            ui.horizontal_wrapped(|ui| {
                for w in WALLPAPERS {
                    let c = hex_to_color(w.base[1]);
                    let a = hex_to_color(w.blobs[0].0);
                    let (r, resp) = ui.allocate_exact_size(egui::vec2(38.0, 26.0), egui::Sense::click());
                    ui.painter().rect_filled(r, 6.0, c);
                    ui.painter().circle_filled(r.center() - egui::vec2(6.0, 3.0), 8.0, a.gamma_multiply(0.8));
                    if cur == w.name {
                        ui.painter().rect_stroke(r, 6.0, egui::Stroke::new(2.0, p.accent), egui::StrokeKind::Outside);
                    }
                    if resp.on_hover_text(w.name).clicked() {
                        pick = Some(w.name.to_string());
                    }
                }
            });
            if let Some(n) = pick {
                st.edit(|p| p.config.styles.background.wallpaper = n);
            }
            let mut drift = st.project.config.styles.background.motion != "none";
            if ui.checkbox(&mut drift, "Slow drift").changed() {
                st.edit(|p| p.config.styles.background.motion = if drift { "drift".into() } else { "none".into() });
            }
        }
        "gradient" => {
            color_row(ui, st, "From", |p| p.config.styles.background.gradient_from.clone(), |p, v| p.config.styles.background.gradient_from = v);
            color_row(ui, st, "To", |p| p.config.styles.background.gradient_to.clone(), |p, v| p.config.styles.background.gradient_to = v);
            slider(ui, st, "Angle", 0.0..=360.0, "°", |p| p.config.styles.background.gradient_angle, |p, v| p.config.styles.background.gradient_angle = v);
        }
        _ => {
            color_row(ui, st, "Colour", |p| p.config.styles.background.color.clone(), |p, v| p.config.styles.background.color = v);
        }
    }

    section(ui, "Screen");
    slider(ui, st, "Padding", 0.0..=0.25, "", |p| p.config.styles.padding_ratio, |p, v| p.config.styles.padding_ratio = v);
    slider(ui, st, "Corner radius", 0.0..=60.0, " px", |p| p.config.styles.corner_radius, |p, v| p.config.styles.corner_radius = v);

    section(ui, "Shadow");
    slider(ui, st, "Intensity", 0.0..=1.0, "", |p| p.config.styles.shadow.intensity, |p, v| p.config.styles.shadow.intensity = v);
    slider(ui, st, "Blur", 0.0..=80.0, " px", |p| p.config.styles.shadow.blur, |p, v| p.config.styles.shadow.blur = v);
    slider(ui, st, "Distance", 0.0..=80.0, " px", |p| p.config.styles.shadow.distance, |p, v| p.config.styles.shadow.distance = v);
    toggle(ui, st, "Directional", |p| p.config.styles.shadow.directional, |p, v| p.config.styles.shadow.directional = v);
    if st.project.config.styles.shadow.directional {
        slider(ui, st, "Angle", 0.0..=360.0, "°", |p| p.config.styles.shadow.angle, |p, v| p.config.styles.shadow.angle = v);
    }
}

fn output(st: &mut EditorState, ui: &mut egui::Ui) {
    section(ui, "Frame");
    let mut aspect = st.project.config.output.aspect_ratio.clone();
    ui.horizontal(|ui| {
        ui.label("Aspect ratio");
        egui::ComboBox::from_id_salt("aspect").selected_text(if aspect == "auto" { "Match recording".to_string() } else { aspect.clone() }).show_ui(
            ui,
            |ui| {
                ui.selectable_value(&mut aspect, "auto".to_string(), "Match recording");
                for (n, _) in screencraft_render::ASPECTS {
                    ui.selectable_value(&mut aspect, n.to_string(), *n);
                }
            },
        );
    });
    if aspect != st.project.config.output.aspect_ratio {
        st.edit(|p| p.config.output.aspect_ratio = aspect);
    }
    section(ui, "Crop recording");
    slider(ui, st, "Left", 0.0..=0.45, "", |p| p.config.crop.x, |p, v| {
        let right = p.config.crop.x + p.config.crop.width;
        p.config.crop.x = v;
        p.config.crop.width = (right - v).max(0.1);
    });
    slider(ui, st, "Top", 0.0..=0.45, "", |p| p.config.crop.y, |p, v| {
        let bottom = p.config.crop.y + p.config.crop.height;
        p.config.crop.y = v;
        p.config.crop.height = (bottom - v).max(0.1);
    });
    slider(ui, st, "Right", 0.55..=1.0, "", |p| p.config.crop.x + p.config.crop.width, |p, v| {
        p.config.crop.width = (v - p.config.crop.x).max(0.1);
    });
    slider(ui, st, "Bottom", 0.55..=1.0, "", |p| p.config.crop.y + p.config.crop.height, |p, v| {
        p.config.crop.height = (v - p.config.crop.y).max(0.1);
    });
    if ui.small_button("Reset crop").clicked() {
        st.edit(|p| p.config.crop = screencraft_core::project::Rect::FULL);
    }
}

fn audio(st: &mut EditorState, ui: &mut egui::Ui) {
    let has_sys = st.project.media.has_system_audio;
    let has_mic = st.project.media.has_microphone;
    if !has_sys && !has_mic {
        section(ui, "Audio");
        hint(ui, "This recording has no audio. Turn on system audio or the microphone before recording.");
        return;
    }
    if has_sys {
        section(ui, "System audio");
        toggle(ui, st, "Mute", |p| p.config.audio.mute_system_audio, |p, v| p.config.audio.mute_system_audio = v);
        slider(ui, st, "Volume", 0.0..=2.0, "", |p| p.config.audio.device_volume, |p, v| p.config.audio.device_volume = v);
    }
    if has_mic {
        section(ui, "Microphone");
        toggle(ui, st, "Mute", |p| p.config.audio.mute_microphone, |p, v| p.config.audio.mute_microphone = v);
        slider(ui, st, "Volume", 0.0..=2.0, "", |p| p.config.audio.mic_volume, |p, v| p.config.audio.mic_volume = v);
    }
}
