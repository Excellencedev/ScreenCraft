//! Window creation helpers and window-related commands.

use crate::commands::permissions::screen_granted;
use screencapturekit::prelude::*;
use tauri::{AppHandle, Manager, WebviewUrl, WebviewWindowBuilder};

fn main_display_size() -> (f64, f64) {
    SCShareableContent::get()
        .ok()
        .and_then(|c| c.displays().first().map(|d| (d.frame().size.width, d.frame().size.height)))
        .unwrap_or((1440.0, 900.0))
}

pub fn show_recorder_window(app: &AppHandle) -> tauri::Result<()> {
    if let Some(w) = app.get_webview_window("recorder") {
        w.show()?;
        w.set_focus()?;
        return Ok(());
    }
    let (sw, sh) = main_display_size();
    let (w, h) = (1040.0, 76.0);
    WebviewWindowBuilder::new(app, "recorder", WebviewUrl::App("recorder.html".into()))
        .title("ScreenCraft")
        .inner_size(w, h)
        .position((sw - w) / 2.0, sh - h - 90.0)
        .decorations(false)
        .transparent(true)
        .shadow(false)
        .resizable(false)
        .always_on_top(true)
        .accept_first_mouse(true)
        .visible_on_all_workspaces(true)
        .build()?;
    Ok(())
}

pub fn show_permissions_window(app: &AppHandle) -> tauri::Result<()> {
    if let Some(r) = app.get_webview_window("recorder") {
        let _ = r.hide();
    }
    if let Some(w) = app.get_webview_window("permissions") {
        w.show()?;
        w.set_focus()?;
        return Ok(());
    }
    WebviewWindowBuilder::new(app, "permissions", WebviewUrl::App("permissions.html".into()))
        .title("ScreenCraft permissions")
        .inner_size(580.0, 660.0)
        .resizable(false)
        .accept_first_mouse(true)
        .center()
        .build()?;
    Ok(())
}

fn url_escape(s: &str) -> String {
    s.replace('%', "%25").replace(' ', "%20").replace('&', "%26").replace('#', "%23")
}

#[tauri::command]
pub fn show_recorder(app: AppHandle) -> Result<(), String> {
    show_recorder_window(&app).map_err(|e| e.to_string())
}

#[tauri::command]
pub fn show_permissions(app: AppHandle) -> Result<(), String> {
    show_permissions_window(&app).map_err(|e| e.to_string())
}

#[tauri::command]
pub fn show_compressor(app: AppHandle, input: Option<String>) -> Result<(), String> {
    if let Some(w) = app.get_webview_window("compress") {
        let _ = w.close();
    }
    let url = match input {
        Some(p) => format!("compress.html?input={}", url_escape(&p)),
        None => "compress.html".into(),
    };
    WebviewWindowBuilder::new(&app, "compress", WebviewUrl::App(url.into()))
        .title("Compress video")
        .inner_size(780.0, 700.0)
        .min_inner_size(660.0, 600.0)
        .accept_first_mouse(true)
        .center()
        .build()
        .map_err(|e| e.to_string())?;
    Ok(())
}

#[tauri::command]
pub fn open_editor(app: AppHandle, dir: String) -> Result<(), String> {
    let label = format!("editor-{}", dir.bytes().fold(5381u64, |h, b| h.wrapping_mul(33) ^ b as u64));
    if let Some(w) = app.get_webview_window(&label) {
        w.set_focus().map_err(|e| e.to_string())?;
        return Ok(());
    }
    if let Some(r) = app.get_webview_window("recorder") {
        let _ = r.hide();
    }
    WebviewWindowBuilder::new(&app, label, WebviewUrl::App(format!("editor.html?dir={}", url_escape(&dir)).into()))
        .title("ScreenCraft")
        .accept_first_mouse(true)
        .inner_size(1240.0, 780.0)
        .min_inner_size(960.0, 640.0)
        .center()
        .build()
        .map_err(|e| e.to_string())?;
    Ok(())
}

#[tauri::command]
pub fn open_area_picker(app: AppHandle, display_id: Option<u32>) -> Result<(), String> {
    if let Some(w) = app.get_webview_window("area") {
        let _ = w.close();
    }
    let content = SCShareableContent::get().map_err(|e| e.to_string())?;
    let displays = content.displays();
    let d = match display_id {
        Some(id) => displays.iter().find(|d| d.display_id() == id),
        None => displays.first(),
    }
    .ok_or("display not found")?;
    let f = d.frame();
    WebviewWindowBuilder::new(&app, "area", WebviewUrl::App(format!("area.html?x={}&y={}", f.origin.x, f.origin.y).into()))
        .title("Select area")
        .position(f.origin.x, f.origin.y)
        .inner_size(f.size.width, f.size.height)
        .decorations(false)
        .transparent(true)
        .shadow(false)
        .resizable(false)
        .always_on_top(true)
        .skip_taskbar(true)
        .accept_first_mouse(true)
        .build()
        .map_err(|e| e.to_string())?;
    Ok(())
}

/// Show the recorder again once the last editor/compressor/permissions window closes.
pub fn on_window_destroyed(app: &AppHandle, label: &str) {
    let tracked = |l: &str| l.starts_with("editor-") || l == "compress" || l == "permissions";
    if !tracked(label) {
        return;
    }
    if label == "permissions" && !screen_granted() {
        return;
    }
    let others = app.webview_windows().keys().any(|k| tracked(k) && k != label);
    if !others {
        let _ = show_recorder_window(app);
    }
}
