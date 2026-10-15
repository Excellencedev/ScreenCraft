//! Visual language: quiet neutral surfaces, hairline borders, one accent, generous spacing.
//! Follows the operating system's light / dark setting.

use eframe::egui::{self, Color32, CornerRadius, FontFamily, FontId, Stroke, TextStyle, Vec2};

#[derive(Clone, Copy)]
pub struct Palette {
    pub bg: Color32,
    pub panel: Color32,
    pub raised: Color32,
    pub hover: Color32,
    pub line: Color32,
    pub text: Color32,
    pub dim: Color32,
    pub accent: Color32,
    pub accent_text: Color32,
    pub danger: Color32,
    pub slice: Color32,
    pub slice_sel: Color32,
    pub zoom: Color32,
    pub zoom_sel: Color32,
}

pub const DARK: Palette = Palette {
    bg: Color32::from_rgb(14, 14, 16),
    panel: Color32::from_rgb(20, 20, 23),
    raised: Color32::from_rgb(29, 29, 33),
    hover: Color32::from_rgb(40, 40, 46),
    line: Color32::from_rgb(44, 44, 50),
    text: Color32::from_rgb(232, 232, 236),
    dim: Color32::from_rgb(138, 138, 148),
    accent: Color32::from_rgb(124, 140, 255),
    accent_text: Color32::from_rgb(10, 10, 20),
    danger: Color32::from_rgb(240, 82, 82),
    slice: Color32::from_rgb(52, 56, 78),
    slice_sel: Color32::from_rgb(78, 88, 150),
    zoom: Color32::from_rgb(40, 84, 110),
    zoom_sel: Color32::from_rgb(52, 130, 170),
};

pub const LIGHT: Palette = Palette {
    bg: Color32::from_rgb(244, 244, 246),
    panel: Color32::from_rgb(250, 250, 251),
    raised: Color32::from_rgb(255, 255, 255),
    hover: Color32::from_rgb(235, 235, 240),
    line: Color32::from_rgb(222, 222, 228),
    text: Color32::from_rgb(24, 24, 28),
    dim: Color32::from_rgb(110, 110, 122),
    accent: Color32::from_rgb(72, 88, 232),
    accent_text: Color32::WHITE,
    danger: Color32::from_rgb(214, 48, 48),
    slice: Color32::from_rgb(196, 202, 232),
    slice_sel: Color32::from_rgb(132, 146, 230),
    zoom: Color32::from_rgb(170, 208, 226),
    zoom_sel: Color32::from_rgb(92, 168, 208),
};

pub fn palette(ctx: &egui::Context) -> Palette {
    if ctx.global_style().visuals.dark_mode {
        DARK
    } else {
        LIGHT
    }
}

fn visuals(p: &Palette, dark: bool) -> egui::Visuals {
    let mut v = if dark { egui::Visuals::dark() } else { egui::Visuals::light() };
    v.override_text_color = Some(p.text);
    v.panel_fill = p.panel;
    v.window_fill = p.raised;
    v.extreme_bg_color = p.bg;
    v.faint_bg_color = p.raised;
    v.window_stroke = Stroke::new(1.0, p.line);
    v.window_corner_radius = CornerRadius::same(10);
    v.menu_corner_radius = CornerRadius::same(8);
    v.hyperlink_color = p.accent;
    v.selection.bg_fill = p.accent.gamma_multiply(0.45);
    v.selection.stroke = Stroke::new(1.0, p.accent);
    let r = CornerRadius::same(7);
    let w = &mut v.widgets;
    w.noninteractive.bg_fill = p.panel;
    w.noninteractive.weak_bg_fill = p.panel;
    w.noninteractive.bg_stroke = Stroke::new(1.0, p.line);
    w.noninteractive.fg_stroke = Stroke::new(1.0, p.text);
    w.noninteractive.corner_radius = r;
    // bg_fill paints slider rails and checkbox boxes, weak_bg_fill paints buttons.
    w.inactive.bg_fill = p.line;
    w.inactive.weak_bg_fill = p.raised;
    w.inactive.bg_stroke = Stroke::new(1.0, p.line);
    w.inactive.fg_stroke = Stroke::new(1.0, p.text);
    w.inactive.corner_radius = r;
    w.hovered.bg_fill = p.hover;
    w.hovered.weak_bg_fill = p.hover;
    w.hovered.bg_stroke = Stroke::new(1.0, p.dim);
    w.hovered.fg_stroke = Stroke::new(1.0, p.text);
    w.hovered.corner_radius = r;
    w.active.bg_fill = p.hover;
    w.active.weak_bg_fill = p.hover;
    w.active.bg_stroke = Stroke::new(1.0, p.accent);
    w.active.fg_stroke = Stroke::new(1.0, p.text);
    w.active.corner_radius = r;
    w.open = w.hovered;
    v
}

pub fn install(ctx: &egui::Context) {
    // Follow the system theme live.
    ctx.set_theme(egui::ThemePreference::System);
    ctx.set_visuals_of(egui::Theme::Dark, visuals(&DARK, true));
    ctx.set_visuals_of(egui::Theme::Light, visuals(&LIGHT, false));
    ctx.all_styles_mut(|style| {
        style.spacing.item_spacing = Vec2::new(8.0, 8.0);
        style.spacing.button_padding = Vec2::new(12.0, 6.0);
        style.spacing.interact_size.y = 28.0;
        style.spacing.slider_width = 150.0;
        style.spacing.window_margin = egui::Margin::same(14);
        style.text_styles = [
            (TextStyle::Heading, FontId::new(18.0, FontFamily::Proportional)),
            (TextStyle::Body, FontId::new(13.5, FontFamily::Proportional)),
            (TextStyle::Button, FontId::new(13.5, FontFamily::Proportional)),
            (TextStyle::Small, FontId::new(11.5, FontFamily::Proportional)),
            (TextStyle::Monospace, FontId::new(12.5, FontFamily::Monospace)),
        ]
        .into();
    });
}

/// A small, dim, upper-case section label.
pub fn section(ui: &mut egui::Ui, text: &str) {
    let p = palette(ui.ctx());
    ui.add_space(6.0);
    ui.label(egui::RichText::new(text.to_uppercase()).size(10.5).color(p.dim).strong());
}

/// A filled accent button.
pub fn primary_button(ui: &mut egui::Ui, text: &str) -> egui::Response {
    let p = palette(ui.ctx());
    ui.add(
        egui::Button::new(egui::RichText::new(text).color(p.accent_text).strong())
            .fill(p.accent)
            .stroke(Stroke::NONE)
            .corner_radius(8),
    )
}

pub fn danger_button(ui: &mut egui::Ui, text: &str) -> egui::Response {
    let p = palette(ui.ctx());
    ui.add(
        egui::Button::new(egui::RichText::new(text).color(Color32::WHITE).strong())
            .fill(p.danger)
            .stroke(Stroke::NONE)
            .corner_radius(8),
    )
}

/// Segmented control. Returns true when the value changed.
pub fn segmented<T: PartialEq + Copy>(ui: &mut egui::Ui, value: &mut T, options: &[(T, &str)]) -> bool {
    let p = palette(ui.ctx());
    let mut changed = false;
    ui.horizontal(|ui| {
        ui.spacing_mut().item_spacing.x = 2.0;
        for (v, label) in options {
            let selected = *value == *v;
            let btn = egui::Button::new(egui::RichText::new(*label).color(if selected { p.text } else { p.dim }))
                .fill(if selected { p.hover } else { Color32::TRANSPARENT })
                .stroke(Stroke::NONE)
                .corner_radius(6);
            if ui.add(btn).clicked() && !selected {
                *value = *v;
                changed = true;
            }
        }
    });
    changed
}

pub fn fmt_time(ms: f64) -> String {
    let ms = ms.max(0.0);
    let total = (ms / 1000.0).floor() as u64;
    let (m, s) = (total / 60, total % 60);
    let cs = ((ms % 1000.0) / 10.0).floor() as u64;
    format!("{m}:{s:02}.{cs:02}")
}
