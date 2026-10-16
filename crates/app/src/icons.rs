//! The 0.3 line icons (24x24, 1.8 stroke, round caps), drawn from the same SVG paths.

use eframe::egui::{self, Color32, Rect, Vec2};

/// Inner markup of each icon, as in the original UI.
pub fn markup(name: &str) -> &'static str {
    match name {
        "back" => r#"<path d="M15 5l-7 7 7 7"/>"#,
        "folder" => r#"<path d="M3 7a2 2 0 012-2h4l2 2h8a2 2 0 012 2v8a2 2 0 01-2 2H5a2 2 0 01-2-2z"/>"#,
        "undo" => r#"<path d="M9 14L4 9l5-5"/><path d="M4 9h10a6 6 0 010 12h-3"/>"#,
        "redo" => r#"<path d="M15 14l5-5-5-5"/><path d="M20 9H10a6 6 0 000 12h3"/>"#,
        "info" => r#"<path d="M21 12a9 9 0 11-18 0 9 9 0 0118 0zM12 11v5M12 8v.01"/>"#,
        "file" => r#"<path d="M7 3h7l5 5v13H7zM14 3v5h5"/>"#,
        "sparkle" => r#"<path d="M12 3l1.8 4.6L18 9l-4.2 1.4L12 15l-1.8-4.6L6 9l4.2-1.4z"/>"#,
        "chev" => r#"<path d="M6 9l6 6 6-6"/>"#,
        "export" => r#"<path d="M12 16V4M7 9l5-5 5 5M5 20h14"/>"#,
        "start" => r#"<path d="M6 5v14M18 6l-9 6 9 6z"/>"#,
        "end" => r#"<path d="M18 5v14M6 6l9 6-9 6z"/>"#,
        "play" => r#"<path d="M7 4l13 8-13 8z" fill="currentColor" stroke="none"/>"#,
        "pause" => r#"<path d="M7 4h4v16H7zM13 4h4v16h-4z" fill="currentColor" stroke="none"/>"#,
        "zoom-plus" => r#"<circle cx="11" cy="11" r="6"/><path d="M21 21l-5-5M11 8v6M8 11h6"/>"#,
        "zoom-minus" => r#"<circle cx="11" cy="11" r="6"/><path d="M21 21l-5-5M8 11h6"/>"#,
        "split" => r#"<circle cx="6" cy="6" r="3"/><circle cx="6" cy="18" r="3"/><path d="M8.5 8l11 9M8.5 16l11-9"/>"#,
        "duplicate" => r#"<rect x="9" y="9" width="11" height="11" rx="2"/><path d="M5 15V6a2 2 0 012-2h9"/>"#,
        "trash" => r#"<path d="M4 7h16M10 11v6M14 11v6M6 7l1 13h10l1-13M9 7V4h6v3"/>"#,
        "bug" => r#"<path d="M8 8h8a3 3 0 013 3v3a7 7 0 01-14 0v-3a3 3 0 013-3zM9 8V6.500a3 3 0 016 0V8M12 11v9M5 13H2M19 13h3M6 8L4 6M18 8l2-2M6.500 19L4.500 21M17.500 19l2 2"/>"#,
        "x" => r#"<path fill="currentColor" stroke="none" d="M18.244 2.25h3.308l-7.227 8.26 8.502 11.24H16.17l-5.214-6.817L4.99 21.75H1.68l7.73-8.835L1.254 2.25H8.08l4.713 6.231zm-1.161 17.52h1.833L7.084 4.126H5.117z"/>"#,
        "github" => r#"<path fill="currentColor" stroke="none" d="M12 .5a11.5 11.5 0 0 0-3.64 22.41c.58.1.79-.25.79-.56v-2c-3.2.7-3.87-1.37-3.87-1.37-.52-1.33-1.28-1.69-1.28-1.69-1.04-.71.08-.7.08-.7 1.16.08 1.77 1.19 1.77 1.19 1.03 1.76 2.7 1.25 3.36.96.1-.75.4-1.25.73-1.54-2.55-.29-5.24-1.28-5.24-5.69 0-1.26.45-2.28 1.18-3.09-.12-.29-.51-1.46.11-3.05 0 0 .97-.31 3.17 1.18a11 11 0 0 1 5.77 0c2.2-1.49 3.17-1.18 3.17-1.18.62 1.59.23 2.76.11 3.05.74.81 1.18 1.83 1.18 3.09 0 4.42-2.69 5.39-5.25 5.68.41.36.78 1.06.78 2.14v3.17c0 .31.21.67.8.56A11.5 11.5 0 0 0 12 .5Z"/>"#,
        // side rail
        "appearance" => r#"<path d="M4 5h16v14H4zM4 15l5-5 4 4 3-3 4 4"/>"#,
        "cursor" => r#"<path d="M5 3l14 8-6 2-2 6z"/>"#,
        "camera" => r#"<rect x="3" y="7" width="13" height="10" rx="2"/><path d="M16 11l5-3v8l-5-3z"/>"#,
        "zoom" => r#"<circle cx="11" cy="11" r="6"/><path d="M21 21l-5-5M11 8v6M8 11h6"/>"#,
        "audio" => r#"<path d="M4 10v4h4l5 4V6L8 10z"/><path d="M16 9a4 4 0 010 6"/>"#,
        "crop" => r#"<path d="M6 2v14a2 2 0 002 2h14M2 6h14a2 2 0 012 2v14"/>"#,
        "annotate" => r#"<path d="M5 6h14M12 6v13M9 19h6"/>"#,
        "captions" => r#"<rect x="3" y="5" width="18" height="14" rx="2.5"/><path d="M7 12h3M12 12h5M7 15.5h6"/>"#,
        "pace" => r#"<path d="M13 3L5 14h6l-1 7 8-11h-6z"/>"#,
        // annotation tiles
        "a-text" => r#"<path d="M5 6h14M12 6v13M9 19h6"/>"#,
        "a-arrow" => r#"<path d="M5 19L19 5M10 5h9v9"/>"#,
        "a-box" => r#"<rect x="4" y="6" width="16" height="12" rx="2"/>"#,
        "a-ellipse" => r#"<ellipse cx="12" cy="12" rx="8" ry="6"/>"#,
        "a-highlight" => r#"<path d="M4 15l9-9 5 5-9 9H4zM12 18h8"/>"#,
        "a-blur" => r#"<circle cx="12" cy="12" r="3"/><circle cx="12" cy="12" r="7" stroke-dasharray="2 3"/>"#,
        "a-pixelate" => r#"<path d="M4 4h6v6H4zM14 4h6v6h-6zM4 14h6v6H4zM14 14h6v6h-6z"/>"#,
        "a-spotlight" => r#"<path d="M4 4h16v16H4z"/><circle cx="12" cy="12" r="4"/>"#,
        "check" => r#"<path d="M5 12l5 5 9-10"/>"#,
        "mic" => r#"<rect x="9" y="3" width="6" height="11" rx="3"/><path d="M5 11a7 7 0 0014 0M12 18v3"/>"#,
        "monitor" => r#"<rect x="3" y="4" width="18" height="12" rx="2"/><path d="M8 20h8M12 16v4"/>"#,
        "window" => r#"<rect x="3" y="5" width="18" height="14" rx="2"/><path d="M3 9h18"/>"#,
        "area" => r#"<path d="M4 8V5a1 1 0 011-1h3M16 4h3a1 1 0 011 1v3M20 16v3a1 1 0 01-1 1h-3M8 20H5a1 1 0 01-1-1v-3"/>"#,
        "stop" => r#"<rect x="6" y="6" width="12" height="12" rx="2" fill="currentColor" stroke="none"/>"#,
        "close" => r#"<path d="M6 6l12 12M18 6L6 18"/>"#,
        "plus" => r#"<path d="M12 5v14M5 12h14"/>"#,
        "music" => r#"<path d="M9 18V6l10-2v12"/><circle cx="6.500" cy="18" r="2.500"/><circle cx="16.500" cy="16" r="2.500"/>"#,
        "chart" => r#"<path d="M4 18V9M10 18V5M16 18v-7M22 18H2"/>"#,
        "download" => r#"<path d="M12 4v12M7 11l5 5 5-5M5 20h14"/>"#,
        "gear" => r#"<circle cx="12" cy="12" r="3"/><path d="M19.4 15a1.7 1.7 0 00.3 1.9l.1.1a2 2 0 11-2.8 2.8l-.1-.1a1.7 1.7 0 00-1.9-.3 1.7 1.7 0 00-1 1.5V21a2 2 0 11-4 0v-.1a1.7 1.7 0 00-1.1-1.5 1.7 1.7 0 00-1.9.3l-.1.1a2 2 0 11-2.8-2.8l.1-.1a1.7 1.7 0 00.3-1.9 1.7 1.7 0 00-1.5-1H3a2 2 0 110-4h.1a1.7 1.7 0 001.5-1.1 1.7 1.7 0 00-.3-1.9l-.1-.1a2 2 0 112.8-2.8l.1.1a1.7 1.7 0 001.9.3h.1a1.7 1.7 0 001-1.5V3a2 2 0 114 0v.1a1.7 1.7 0 001 1.5 1.7 1.7 0 001.9-.3l.1-.1a2 2 0 112.8 2.8l-.1.1a1.7 1.7 0 00-.3 1.9v.1a1.7 1.7 0 001.5 1H21a2 2 0 110 4h-.1a1.7 1.7 0 00-1.5 1z"/>"#,
        _ => "",
    }
}

fn hex(c: Color32) -> String {
    format!("#{:02x}{:02x}{:02x}", c.r(), c.g(), c.b())
}

/// Paint icon `name` centred in `rect` with colour `color`.
pub fn paint(ui: &egui::Ui, name: &str, rect: Rect, color: Color32) {
    let col = hex(color);
    let svg = format!(
        r#"<svg xmlns="http://www.w3.org/2000/svg" viewBox="0 0 24 24" fill="none" stroke="{col}" stroke-width="1.8" stroke-linecap="round" stroke-linejoin="round" color="{col}">{}</svg>"#,
        markup(name).replace("currentColor", &col)
    );
    let uri = format!("bytes://icon-{name}-{}.svg", col.trim_start_matches('#'));
    let img = egui::Image::from_bytes(uri, svg.into_bytes()).fit_to_exact_size(rect.size()).tint(Color32::from_white_alpha(color.a()));
    img.paint_at(ui, rect);
}

pub fn paint_sized(ui: &egui::Ui, name: &str, center: egui::Pos2, size: f32, color: Color32) {
    paint(ui, name, Rect::from_center_size(center, Vec2::splat(size)), color);
}
