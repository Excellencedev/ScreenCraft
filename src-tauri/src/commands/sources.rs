use crate::state::CmdResult;
use serde::Serialize;

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct DisplayInfo {
    id: u32,
    name: String,
    x: f64,
    y: f64,
    width: f64,
    height: f64,
    is_main: bool,
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct WindowInfo {
    id: u32,
    title: String,
    app: String,
    x: f64,
    y: f64,
    width: f64,
    height: f64,
}

#[derive(Serialize)]
pub struct Sources {
    displays: Vec<DisplayInfo>,
    windows: Vec<WindowInfo>,
}

#[cfg(target_os = "macos")]
#[tauri::command]
pub fn list_sources() -> CmdResult<Sources> {
    use screencapturekit::prelude::*;
    let content = SCShareableContent::get().map_err(|e| format!("{e} (Screen Recording permission needed)"))?;
    let me = std::process::id() as i32;
    let displays = content
        .displays()
        .iter()
        .enumerate()
        .map(|(i, d)| {
            let f = d.frame();
            let is_main = f.origin.x == 0.0 && f.origin.y == 0.0;
            DisplayInfo {
                id: d.display_id(),
                name: if is_main { "Main Display".into() } else { format!("Display {}", i + 1) },
                x: f.origin.x,
                y: f.origin.y,
                width: f.size.width,
                height: f.size.height,
                is_main,
            }
        })
        .collect();
    let windows = content
        .windows()
        .iter()
        .filter(|w| w.is_on_screen() && w.window_layer() == 0)
        .filter_map(|w| {
            let app = w.owning_application()?;
            if app.process_id() == me {
                return None;
            }
            let f = w.frame();
            if f.size.width < 120.0 || f.size.height < 80.0 {
                return None;
            }
            Some(WindowInfo {
                id: w.window_id(),
                title: w.title().unwrap_or_default(),
                app: app.application_name(),
                x: f.origin.x,
                y: f.origin.y,
                width: f.size.width,
                height: f.size.height,
            })
        })
        .collect();
    Ok(Sources { displays, windows })
}

/// Displays and windows from `monitors.rs`, in logical pixels. Window ids are HWNDs.
#[cfg(windows)]
#[tauri::command]
pub fn list_sources(app: tauri::AppHandle) -> CmdResult<Sources> {
    let displays = crate::windows::display_frames(&app)?
        .into_iter()
        .enumerate()
        .map(|(i, d)| DisplayInfo {
            id: d.id,
            name: if d.is_main { "Main Display".into() } else { format!("Display {}", i + 1) },
            x: d.x,
            y: d.y,
            width: d.width,
            height: d.height,
            is_main: d.is_main,
        })
        .collect();
    let windows = crate::monitors::windows()
        .into_iter()
        .filter_map(|w| {
            let d = crate::monitors::display_of_window(w.hwnd)?;
            let (x, y) = d.to_logical(w.phys.0 as f64, w.phys.1 as f64);
            let (width, height) = (w.phys.2 as f64 / d.scale, w.phys.3 as f64 / d.scale);
            if width < 120.0 || height < 80.0 {
                return None;
            }
            Some(WindowInfo { id: w.hwnd.0 as usize as u32, title: w.title, app: w.app, x, y, width, height })
        })
        .collect();
    Ok(Sources { displays, windows })
}
