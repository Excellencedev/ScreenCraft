use super::*;
use crate::theme::*;
use eframe::egui::{Color32, CornerRadius, Pos2, Rect, Sense, Shape, Stroke, StrokeKind, Vec2};
use screencraft_core::project::ClickEffect;

const ARROW: [(f32, f32); 7] = [(0.0, 0.0), (0.0, 0.86), (0.19, 0.69), (0.31, 0.97), (0.43, 0.92), (0.31, 0.64), (0.56, 0.64)];

/// The cursor-style picker: four tiles, each showing the cursor on the editor's dark swatch.
fn style_picker(st: &mut EditorState, ui: &mut Ui) {
    ui.add_space(6.0);
    let (rect, _) = ui.allocate_exact_size(Vec2::new(ui.available_width(), 16.0), Sense::hover());
    ui.painter().text(rect.left_center(), egui::Align2::LEFT_CENTER, "Cursor style", kit::font(13.0), LABEL);
    ui.add_space(1.0);
    let cur = st.project.config.cursor.style.clone();
    let styles = [("macos", "Dark"), ("light", "Light"), ("accent", "Accent"), ("dot", "Dot")];
    let tile = Vec2::new(72.0, 48.0);
    let (rect, _) = ui.allocate_exact_size(Vec2::new(tile.x * 4.0, tile.y), Sense::hover());
    let mut pick = None;
    for (i, (id, name)) in styles.iter().enumerate() {
        let r = Rect::from_min_size(Pos2::new(rect.min.x + i as f32 * tile.x, rect.min.y), tile);
        let resp = ui.interact(r, ui.id().with(("cs", i)), Sense::click());
        let on = cur == *id;
        let cr = CornerRadius { nw: if i == 0 { 7 } else { 0 }, sw: if i == 0 { 7 } else { 0 }, ne: if i == 3 { 7 } else { 0 }, se: if i == 3 { 7 } else { 0 } };
        ui.painter().rect_filled(r.shrink(1.0), cr, Color32::from_rgb(0x2a, 0x2c, 0x3d));
        draw_arrow(ui, Pos2::new(r.min.x + 28.0, r.min.y + 8.0), 30.0, id);
        if on {
            ui.painter().rect_filled(r.shrink(1.0), cr, alpha(ACCENT, 0.35));
        }
        if i < 3 {
            ui.painter().line_segment([Pos2::new(r.max.x - 0.5, r.min.y), Pos2::new(r.max.x - 0.5, r.max.y)], Stroke::new(1.0, LINE_2));
        }
        if resp.on_hover_text(*name).on_hover_cursor(egui::CursorIcon::PointingHand).clicked() {
            pick = Some(id.to_string());
        }
    }
    ui.painter().rect_stroke(rect, CornerRadius::same(8), Stroke::new(1.0, LINE_2), StrokeKind::Inside);
    if let Some(s) = pick {
        st.edit(|p| p.config.cursor.style = s);
    }
    ui.add_space(6.0);
}

fn draw_arrow(ui: &Ui, at: Pos2, h: f32, style: &str) {
    if style == "dot" {
        ui.painter().circle_filled(at, h * 0.28, Color32::from_white_alpha(230));
        ui.painter().circle_stroke(at, h * 0.28, Stroke::new(1.0, Color32::from_black_alpha(128)));
        return;
    }
    let (fill, stroke) = match style {
        "light" => (Color32::WHITE, Color32::from_rgb(0x11, 0x11, 0x11)),
        "accent" => (Color32::from_rgb(0x3b, 0x82, 0xc4), Color32::WHITE),
        _ => (Color32::from_rgb(0x11, 0x11, 0x11), Color32::WHITE),
    };
    let pts: Vec<Pos2> = ARROW.iter().map(|(x, y)| Pos2::new(at.x + x * h, at.y + y * h)).collect();
    ui.painter().add(Shape::convex_polygon(pts.clone(), fill, Stroke::NONE));
    let mut closed = pts.clone();
    closed.push(pts[0]);
    ui.painter().add(Shape::line(closed, Stroke::new((h * 0.065).max(1.0), stroke)));
}

pub fn show(st: &mut EditorState, ui: &mut Ui) {
    let c = st.project.config.cursor.clone();
    let a = st.project.config.animations.clone();
    kit::h3(ui, "Cursor", true);
    slider(st, ui, "cur-size", "Size", c.size, (10.0, 80.0, 1.0), |v| format!("{} pt", v.round()), |p, v| p.config.cursor.size = v);
    style_picker(st, ui);
    toggle(st, ui, "Hide cursor if not moving", None, c.hide_not_moving_after_ms.is_some(), |p, v| p.config.cursor.hide_not_moving_after_ms = if v { Some(2000.0) } else { None });
    toggle(st, ui, "Hide cursor", None, c.hide, |p, v| p.config.cursor.hide = v);
    seg(st, ui, Some("Click effect"), &[(ClickEffect::None, "None"), (ClickEffect::Circle, "Circle"), (ClickEffect::Ripple, "Ripple")], c.click_effect, |p, v| p.config.cursor.click_effect = v);
    if c.click_effect != ClickEffect::None {
        color(st, ui, "click-color", "Effect color", &c.click_effect_color, |p, v| p.config.cursor.click_effect_color = v);
    }

    kit::h3(ui, "Camera motion", false);
    seg(st, ui, Some("Follow"), &[("center".to_string(), "Keep centered"), ("edge".to_string(), "Only near edges")], a.follow.clone(), |p, v| p.config.animations.follow = v);
    kit::hint(ui, "Zoomed views keep the cursor in the middle of the screen. While you type, the camera moves to the text caret (grant Accessibility in Permissions).");
    slider(st, ui, "follow-smooth", "Camera smoothness", a.follow_smooth_ms / 1000.0, (0.1, 1.2, 0.05), sec2, |p, v| p.config.animations.follow_smooth_ms = v * 1000.0);
    slider(st, ui, "typing-smooth", "Typing follow", a.typing_smooth_ms / 1000.0, (0.05, 0.8, 0.05), sec2, |p, v| p.config.animations.typing_smooth_ms = v * 1000.0);
    slider(st, ui, "cursor-smooth", "Cursor smoothing", a.cursor_smooth_ms / 1000.0, (0.0, 0.5, 0.01), sec2, |p, v| p.config.animations.cursor_smooth_ms = v * 1000.0);
    slider(st, ui, "ease-in-all", "Zoom ease in", a.ease_in_ms / 1000.0, (0.0, 1.5, 0.05), sec2, |p, v| p.config.animations.ease_in_ms = v * 1000.0);
    slider(st, ui, "ease-out-all", "Zoom ease out", a.ease_out_ms / 1000.0, (0.0, 1.5, 0.05), sec2, |p, v| p.config.animations.ease_out_ms = v * 1000.0);
}
