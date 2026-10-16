//! ScreenCraft's design tokens, exactly as in the 0.3 web UI: deep-ink surfaces, a calm blue
//! accent, rounded type and tight corners. The app is dark only, like 0.3.

use eframe::egui::{self, Color32, CornerRadius, FontData, FontDefinitions, FontFamily, FontId, Stroke, TextStyle, Vec2};
use std::sync::Arc;

const fn rgb(c: u32) -> Color32 {
    Color32::from_rgb((c >> 16) as u8, (c >> 8) as u8, c as u8)
}

pub const BG: Color32 = rgb(0x090d14);
pub const PANEL: Color32 = rgb(0x0f1520);
pub const PANEL_2: Color32 = rgb(0x141c2a);
pub const RAISE: Color32 = rgb(0x182132);
pub const LINE: Color32 = rgb(0x1d2738);
pub const LINE_2: Color32 = rgb(0x2a374d);
pub const TEXT: Color32 = rgb(0xe5eaf4);
pub const MUTED: Color32 = rgb(0x8693aa);
pub const ACCENT: Color32 = rgb(0x3b82c4);
pub const ACCENT_2: Color32 = rgb(0x6aaee8);
pub const DANGER: Color32 = rgb(0xff5566);
pub const OK: Color32 = rgb(0x3ddc97);
pub const CLIP: Color32 = rgb(0xd9895e);
/// Secondary label colour used all over the inspector (#c4cde0).
pub const LABEL: Color32 = rgb(0xc4cde0);
pub const ICON: Color32 = rgb(0xa9b4c9);
pub const RADIUS: u8 = 7;

/// Timeline block colours.
pub const BLOCK_LAYOUT: Color32 = rgb(0x7c5cd6);
pub const BLOCK_NOTE: Color32 = rgb(0xc9792a);
pub const BLOCK_NOTE_SHAPE: Color32 = rgb(0xb8483c);
pub const BLOCK_NOTE_BLUR: Color32 = rgb(0x5d6b82);
pub const BLOCK_NOTE_SPOT: Color32 = rgb(0x8a6d12);
pub const BLOCK_CAPTION: Color32 = rgb(0x1f8f87);
pub const BLOCK_MUSIC: Color32 = rgb(0x2f9a72);

pub fn mix(a: Color32, b: Color32, t: f32) -> Color32 {
    let l = |x: u8, y: u8| (x as f32 + (y as f32 - x as f32) * t).round() as u8;
    Color32::from_rgba_unmultiplied(l(a.r(), b.r()), l(a.g(), b.g()), l(a.b(), b.b()), l(a.a(), b.a()))
}

pub fn alpha(c: Color32, a: f32) -> Color32 {
    Color32::from_rgba_unmultiplied(c.r(), c.g(), c.b(), (a.clamp(0.0, 1.0) * 255.0).round() as u8)
}

pub fn parse_hex(s: &str) -> Color32 {
    let t = s.trim().trim_start_matches('#');
    let v = |i: usize| u8::from_str_radix(t.get(i..i + 2).unwrap_or("00"), 16).unwrap_or(0);
    match t.len() {
        6 => Color32::from_rgb(v(0), v(2), v(4)),
        8 => Color32::from_rgba_unmultiplied(v(0), v(2), v(4), v(6)),
        3 => {
            let d = |i: usize| u8::from_str_radix(&t[i..i + 1], 16).unwrap_or(0) * 17;
            Color32::from_rgb(d(0), d(1), d(2))
        }
        _ => Color32::BLACK,
    }
}

pub fn to_hex(c: Color32) -> String {
    format!("#{:02x}{:02x}{:02x}", c.r(), c.g(), c.b())
}

pub fn to_hex_alpha(c: Color32) -> String {
    format!("#{:02x}{:02x}{:02x}{:02x}", c.r(), c.g(), c.b(), c.a())
}

fn load_ui_font() -> Option<Vec<u8>> {
    let candidates: &[&str] = if cfg!(target_os = "macos") {
        // `ui-rounded`: the same face the web UI asked for.
        &["/System/Library/Fonts/SFNSRounded.ttf", "/System/Library/Fonts/SFNS.ttf", "/System/Library/Fonts/Helvetica.ttc"]
    } else if cfg!(target_os = "windows") {
        &["C:\\Windows\\Fonts\\segoeui.ttf", "C:\\Windows\\Fonts\\arial.ttf"]
    } else {
        &[
            "/usr/share/fonts/truetype/inter/Inter-Regular.ttf",
            "/usr/share/fonts/opentype/inter/Inter-Regular.otf",
            "/usr/share/fonts/truetype/ubuntu/Ubuntu-R.ttf",
            "/usr/share/fonts/TTF/DejaVuSans.ttf",
        ]
    };
    candidates.iter().find_map(|p| std::fs::read(p).ok())
}

fn install_fonts(ctx: &egui::Context) {
    let mut fonts = FontDefinitions::default();
    if let Some(bytes) = load_ui_font() {
        fonts.font_data.insert("ui".into(), Arc::new(FontData::from_owned(bytes)));
        fonts.families.entry(FontFamily::Proportional).or_default().insert(0, "ui".into());
    }
    ctx.set_fonts(fonts);
}

fn visuals() -> egui::Visuals {
    let mut v = egui::Visuals::dark();
    v.override_text_color = Some(TEXT);
    v.panel_fill = PANEL;
    v.window_fill = PANEL;
    v.extreme_bg_color = BG;
    v.faint_bg_color = PANEL_2;
    v.code_bg_color = BG;
    v.window_stroke = Stroke::new(1.0, LINE_2);
    v.window_corner_radius = CornerRadius::same(12);
    v.menu_corner_radius = CornerRadius::same(10);
    v.popup_shadow = egui::Shadow { offset: [0, 14], blur: 44, spread: 0, color: Color32::from_black_alpha(140) };
    v.window_shadow = egui::Shadow { offset: [0, 24], blur: 70, spread: 0, color: Color32::from_black_alpha(140) };
    v.hyperlink_color = ACCENT_2;
    v.selection.bg_fill = alpha(ACCENT, 0.5);
    v.selection.stroke = Stroke::new(1.0, ACCENT_2);
    let r = CornerRadius::same(RADIUS);
    let w = &mut v.widgets;
    w.noninteractive.bg_fill = PANEL;
    w.noninteractive.weak_bg_fill = PANEL;
    w.noninteractive.bg_stroke = Stroke::new(1.0, LINE);
    w.noninteractive.fg_stroke = Stroke::new(1.0, TEXT);
    w.noninteractive.corner_radius = r;
    w.inactive.bg_fill = PANEL_2;
    w.inactive.weak_bg_fill = PANEL_2;
    w.inactive.bg_stroke = Stroke::new(1.0, LINE);
    w.inactive.fg_stroke = Stroke::new(1.0, TEXT);
    w.inactive.corner_radius = r;
    w.hovered.bg_fill = RAISE;
    w.hovered.weak_bg_fill = RAISE;
    w.hovered.bg_stroke = Stroke::new(1.0, LINE_2);
    w.hovered.fg_stroke = Stroke::new(1.0, TEXT);
    w.hovered.corner_radius = r;
    w.active.bg_fill = RAISE;
    w.active.weak_bg_fill = RAISE;
    w.active.bg_stroke = Stroke::new(1.0, ACCENT_2);
    w.active.fg_stroke = Stroke::new(1.0, TEXT);
    w.active.corner_radius = r;
    w.open = w.hovered;
    v.text_cursor.stroke = Stroke::new(1.5, ACCENT_2);
    v
}

pub fn install(ctx: &egui::Context) {
    egui_extras::install_image_loaders(ctx);
    install_fonts(ctx);
    ctx.set_theme(egui::ThemePreference::Dark);
    ctx.set_visuals_of(egui::Theme::Dark, visuals());
    ctx.set_visuals_of(egui::Theme::Light, visuals());
    ctx.all_styles_mut(|style| {
        style.spacing.item_spacing = Vec2::new(8.0, 6.0);
        style.spacing.button_padding = Vec2::new(11.0, 6.0);
        style.spacing.interact_size.y = 28.0;
        style.spacing.slider_width = 150.0;
        style.spacing.window_margin = egui::Margin::same(14);
        style.spacing.scroll = egui::style::ScrollStyle::solid();
        style.spacing.scroll.bar_width = 8.0;
        style.spacing.scroll.floating = false;
        style.text_styles = [
            (TextStyle::Heading, FontId::new(16.0, FontFamily::Proportional)),
            (TextStyle::Body, FontId::new(13.0, FontFamily::Proportional)),
            (TextStyle::Button, FontId::new(13.0, FontFamily::Proportional)),
            (TextStyle::Small, FontId::new(11.5, FontFamily::Proportional)),
            (TextStyle::Monospace, FontId::new(12.5, FontFamily::Monospace)),
        ]
        .into();
    });
}

pub fn fmt_time(ms: f64) -> String {
    let ms = ms.max(0.0);
    let total = (ms / 1000.0).floor() as u64;
    let (m, s) = (total / 60, total % 60);
    let t = ((ms % 1000.0) / 100.0).floor() as u64;
    format!("{m}:{s:02}.{t}")
}

// ---- temporary compatibility for the library screen, replaced by the recorder bar ----------
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
}

pub fn palette(_ctx: &egui::Context) -> Palette {
    Palette { bg: BG, panel: PANEL, raised: RAISE, hover: LINE, line: LINE, text: TEXT, dim: MUTED, accent: ACCENT, accent_text: Color32::WHITE, danger: DANGER }
}

pub fn section(ui: &mut egui::Ui, text: &str) {
    ui.add_space(6.0);
    ui.label(egui::RichText::new(text.to_uppercase()).size(10.5).color(MUTED).strong());
}

pub fn primary_button(ui: &mut egui::Ui, text: &str) -> egui::Response {
    ui.add(egui::Button::new(egui::RichText::new(text).color(Color32::WHITE).strong()).fill(ACCENT).stroke(Stroke::NONE).corner_radius(8))
}

pub fn danger_button(ui: &mut egui::Ui, text: &str) -> egui::Response {
    ui.add(egui::Button::new(egui::RichText::new(text).color(Color32::WHITE).strong()).fill(DANGER).stroke(Stroke::NONE).corner_radius(8))
}

pub fn segmented<T: PartialEq + Copy>(ui: &mut egui::Ui, value: &mut T, options: &[(T, &str)]) -> bool {
    let mut copy = *value;
    let r = crate::kit::segmented(ui, None, options, &mut copy);
    *value = copy;
    r
}
