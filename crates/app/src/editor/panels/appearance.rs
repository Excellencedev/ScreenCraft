use super::*;
use crate::theme::*;
use eframe::egui::{Color32, CornerRadius, Pos2, Rect, Stroke, StrokeKind, Vec2};
use screencraft_render::wallpapers::WALLPAPERS;

const DEVICES: [(&str, &str); 6] =
    [("none", "None"), ("laptop", "Laptop"), ("monitor", "Monitor"), ("tablet", "Tablet"), ("phone", "Phone"), ("browser", "Browser")];
const MOTIONS: [(&str, &str); 5] = [("none", "Still"), ("drift", "Drift"), ("bokeh", "Bokeh"), ("waves", "Waves"), ("dust", "Dust")];

fn swatch_texture(st: &mut EditorState, ctx: &egui::Context, name: &str) -> egui::TextureId {
    if let Some(t) = st.swatches.get(name) {
        return t.id();
    }
    let px = screencraft_render::wallpapers::thumbnail(name, 120, 90);
    let img = egui::ColorImage::from_rgba_unmultiplied([120, 90], &px);
    let tex = ctx.load_texture(format!("wp-{name}"), img, egui::TextureOptions::LINEAR);
    let id = tex.id();
    st.swatches.insert(name.to_string(), tex);
    id
}

pub fn show(st: &mut EditorState, ui: &mut Ui) {
    let ctx = ui.ctx().clone();
    let styles = st.project.config.styles.clone();
    let bg = styles.background.clone();
    kit::h3(ui, "Background", true);
    seg(
        st,
        ui,
        Some("Type"),
        &[("wallpaper".to_string(), "Wallpaper"), ("gradient".to_string(), "Gradient"), ("color".to_string(), "Color"), ("image".to_string(), "Image")],
        bg.kind.clone(),
        |p, v| p.config.styles.background.kind = v,
    );
    match bg.kind.as_str() {
        "wallpaper" => {
            ui.add_space(6.0);
            let ids: Vec<(usize, egui::TextureId)> = (0..WALLPAPERS.len()).map(|i| (i, swatch_texture(st, &ctx, WALLPAPERS[i].name))).collect();
            let mut pick = None;
            let w = ui.available_width();
            let cw = (w - 7.0 * 5.0) / 6.0;
            let rows = WALLPAPERS.len().div_ceil(6);
            let ch = cw / 1.2;
            let (rect, _) = ui.allocate_exact_size(Vec2::new(w, rows as f32 * ch + (rows as f32 - 1.0) * 7.0), egui::Sense::hover());
            for (i, tex) in ids {
                let (r, c) = (i / 6, i % 6);
                let cell = Rect::from_min_size(Pos2::new(rect.min.x + c as f32 * (cw + 7.0), rect.min.y + r as f32 * (ch + 7.0)), Vec2::new(cw, ch));
                let resp = kit::cell_response(ui, cell, ("wp", i));
                ui.painter().image(tex, cell, Rect::from_min_max(Pos2::ZERO, Pos2::new(1.0, 1.0)), Color32::WHITE);
                // 6px corners: mask them with a rounded border in the panel colour
                let on = bg.wallpaper == WALLPAPERS[i].name;
                ui.painter().rect_stroke(cell.expand(1.0), CornerRadius::same(7), Stroke::new(3.0, BG), StrokeKind::Outside);
                if on {
                    ui.painter().rect_stroke(cell, CornerRadius::same(6), Stroke::new(2.0, Color32::WHITE), StrokeKind::Inside);
                }
                if resp.clicked() {
                    pick = Some(WALLPAPERS[i].name.to_string());
                }
            }
            if let Some(n) = pick {
                st.edit(|p| p.config.styles.background.wallpaper = n);
            }
            ui.add_space(4.0);
            if kit::btn_line(ui, "Pick random wallpaper", false).clicked() {
                let n = (std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).map(|d| d.subsec_nanos()).unwrap_or(0) as usize) % WALLPAPERS.len();
                st.edit(|p| p.config.styles.background.wallpaper = WALLPAPERS[n].name.to_string());
            }
            let motions: Vec<(String, &str)> = MOTIONS.iter().map(|(a, b)| (a.to_string(), *b)).collect();
            seg(st, ui, Some("Motion"), &motions, if bg.motion.is_empty() { "none".to_string() } else { bg.motion.clone() }, |p, v| p.config.styles.background.motion = v);
        }
        "gradient" => {
            color(st, ui, "bg-from", "From", &bg.gradient_from, |p, v| p.config.styles.background.gradient_from = v);
            color(st, ui, "bg-to", "To", &bg.gradient_to, |p, v| p.config.styles.background.gradient_to = v);
            slider(st, ui, "bg-angle", "Angle", bg.gradient_angle, (0.0, 360.0, 1.0), deg, |p, v| p.config.styles.background.gradient_angle = v);
        }
        "color" => {
            color(st, ui, "bg-color", "Color", &bg.color, |p, v| p.config.styles.background.color = v);
        }
        _ => {
            if kit::btn_line(ui, "Choose image…", false).clicked() {
                match crate::imgdata::pick_image_data_url(2560) {
                    Ok(Some(url)) => st.edit(|p| p.config.styles.background.image = Some(url)),
                    Ok(None) => {}
                    Err(e) => st.error = Some(e),
                }
            }
        }
    }

    kit::h3(ui, "Device frame", false);
    let devs: Vec<(String, &str)> = DEVICES.iter().map(|(a, b)| (a.to_string(), *b)).collect();
    seg(st, ui, Some("Frame"), &devs, if styles.device.is_empty() { "none".into() } else { styles.device.clone() }, |p, v| p.config.styles.device = v);
    if bg.kind != "color" {
        slider(st, ui, "bg-blur", "Background blur", bg.blur, (0.0, 40.0, 1.0), |v| format!("{}", v.round()), |p, v| p.config.styles.background.blur = v);
    }
    slider(st, ui, "padding", "Padding", styles.padding_ratio, (0.0, 0.3, 0.005), pct, |p, v| p.config.styles.padding_ratio = v);
    slider(st, ui, "corners", "Screen Corners", styles.corner_radius, (0.0, 60.0, 1.0), |v| format!("{}", v.round()), |p, v| p.config.styles.corner_radius = v);
    slider(st, ui, "shadow", "Shadow", styles.shadow.intensity, (0.0, 1.0, 0.01), pct, |p, v| p.config.styles.shadow.intensity = v);
    seg(st, ui, Some("Shadow style"), &[(false, "Classic"), (true, "Directional")], styles.shadow.directional, |p, v| p.config.styles.shadow.directional = v);
    slider(st, ui, "sh-dist", "Shadow distance", styles.shadow.distance, (0.0, 80.0, 1.0), |v| format!("{}", v.round()), |p, v| p.config.styles.shadow.distance = v);
    slider(st, ui, "sh-blur", "Shadow blur", styles.shadow.blur, (0.0, 60.0, 1.0), |v| format!("{}", v.round()), |p, v| p.config.styles.shadow.blur = v);
    slider(st, ui, "sh-angle", "Shadow angle", styles.shadow.angle, (0.0, 360.0, 1.0), deg, |p, v| p.config.styles.shadow.angle = v);

    // watermark: a logo over the whole video
    let wm = st.project.config.watermark.clone();
    kit::h3(ui, "Watermark", false);
    if kit::btn_line(ui, if wm.image.is_empty() { "Choose image…" } else { "Change image…" }, false).clicked() {
        match crate::imgdata::pick_image_data_url(1024) {
            Ok(Some(url)) => st.edit(|p| {
                p.config.watermark.image = url;
                p.config.watermark.enabled = true;
            }),
            Ok(None) => {}
            Err(e) => st.error = Some(e),
        }
    }
    if !wm.image.is_empty() {
        toggle(st, ui, "Show watermark", None, wm.enabled, |p, v| p.config.watermark.enabled = v);
        if let Some((x, y)) = kit::pos_grid(ui, "Position", (wm.x, wm.y)) {
            st.edit(|p| {
                p.config.watermark.x = x;
                p.config.watermark.y = y;
            });
        }
        slider(st, ui, "wm-size", "Size", wm.size, (0.03, 0.6, 0.01), pct, |p, v| p.config.watermark.size = v);
        slider(st, ui, "wm-op", "Opacity", wm.opacity, (0.05, 1.0, 0.01), pct, |p, v| p.config.watermark.opacity = v);
        if kit::btn_line(ui, "Remove watermark", true).clicked() {
            st.edit(|p| p.config.watermark.image.clear());
        }
    }
}
