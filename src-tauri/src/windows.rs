//! Window creation helpers and window-related commands.

use crate::commands::permissions::all_required_granted;
use tauri::{AppHandle, Manager, WebviewUrl, WebviewWindowBuilder};

/// A display's frame in global logical coordinates (points on macOS).
pub struct DisplayFrame {
    pub id: u32,
    pub x: f64,
    pub y: f64,
    pub width: f64,
    pub height: f64,
    pub is_main: bool,
}

#[cfg(target_os = "macos")]
pub fn display_frames(_app: &AppHandle) -> Result<Vec<DisplayFrame>, String> {
    use screencapturekit::prelude::*;
    let content = SCShareableContent::get().map_err(|e| e.to_string())?;
    Ok(content
        .displays()
        .iter()
        .map(|d| {
            let f = d.frame();
            DisplayFrame {
                id: d.display_id(),
                x: f.origin.x,
                y: f.origin.y,
                width: f.size.width,
                height: f.size.height,
                is_main: f.origin.x == 0.0 && f.origin.y == 0.0,
            }
        })
        .collect())
}

/// Monitors in Tauri's order, the primary one first so it is the default. Ids are
/// the position in that list.
#[cfg(windows)]
pub fn display_frames(app: &AppHandle) -> Result<Vec<DisplayFrame>, String> {
    let primary = app.primary_monitor().ok().flatten().map(|m| *m.position());
    let mut monitors = app.available_monitors().map_err(|e| e.to_string())?;
    monitors.sort_by_key(|m| Some(*m.position()) != primary);
    Ok(monitors
        .iter()
        .enumerate()
        .map(|(i, m)| {
            let sf = m.scale_factor();
            let (p, s) = (m.position(), m.size());
            DisplayFrame {
                id: i as u32,
                x: p.x as f64 / sf,
                y: p.y as f64 / sf,
                width: s.width as f64 / sf,
                height: s.height as f64 / sf,
                is_main: Some(*p) == primary,
            }
        })
        .collect())
}

/// Native window level / colour tweaks (AppKit) that Tauri doesn't expose.
/// AppKit objects may only be touched from the main thread, so hop there first.
#[cfg(target_os = "macos")]
fn with_ns_window(win: &tauri::WebviewWindow, f: impl FnOnce(&objc2_app_kit::NSWindow) + Send + 'static) {
    let Ok(ptr) = win.ns_window() else { return };
    if ptr.is_null() {
        return;
    }
    let addr = ptr as usize;
    let _ = win.run_on_main_thread(move || {
        f(unsafe { &*(addr as *const objc2_app_kit::NSWindow) });
    });
}

/// Resize the recorder bar in one native call, keeping its bottom-centre fixed
/// (AppKit's origin is bottom-left, so the bottom edge simply stays put).
#[cfg(target_os = "macos")]
#[tauri::command]
pub fn resize_bar(app: AppHandle, w: f64, h: f64) -> Result<(), String> {
    let win = app.get_webview_window("recorder").ok_or("no recorder window")?;
    let ptr = win.ns_window().map_err(|e| e.to_string())? as usize;
    let (tx, rx) = std::sync::mpsc::channel();
    win.run_on_main_thread(move || {
        let nswin = unsafe { &*(ptr as *const objc2_app_kit::NSWindow) };
        let f = nswin.frame();
        let cx = f.origin.x + f.size.width / 2.0;
        let frame = objc2_foundation::NSRect::new(
            objc2_foundation::NSPoint::new(cx - w / 2.0, f.origin.y),
            objc2_foundation::NSSize::new(w, h),
        );
        nswin.setFrame_display_animate(frame, true, false);
        let _ = tx.send(());
    })
    .map_err(|e| e.to_string())?;
    let _ = rx.recv_timeout(std::time::Duration::from_millis(500));
    Ok(())
}

/// Resize the recorder bar, keeping its bottom-centre fixed.
#[cfg(windows)]
#[tauri::command]
pub fn resize_bar(app: AppHandle, w: f64, h: f64) -> Result<(), String> {
    let win = app.get_webview_window("recorder").ok_or("no recorder window")?;
    let sf = win.scale_factor().map_err(|e| e.to_string())?;
    let pos = win.outer_position().map_err(|e| e.to_string())?.to_logical::<f64>(sf);
    let size = win.outer_size().map_err(|e| e.to_string())?.to_logical::<f64>(sf);
    let (cx, bottom) = (pos.x + size.width / 2.0, pos.y + size.height);
    win.set_size(tauri::LogicalSize::new(w, h)).map_err(|e| e.to_string())?;
    win.set_position(tauri::LogicalPosition::new(cx - w / 2.0, bottom - h)).map_err(|e| e.to_string())?;
    Ok(())
}

/// Keep the toolbar / HUD above the dimmed surround of an area recording.
pub fn raise_above_frame(win: &tauri::WebviewWindow) {
    #[cfg(target_os = "macos")]
    with_ns_window(win, |w| w.setLevel(25));
    // Windows: both are topmost; re-asserting it puts the toolbar above the newer frame
    #[cfg(windows)]
    let _ = win.set_always_on_top(true);
}

fn main_display_size(app: &AppHandle) -> (f64, f64) {
    display_frames(app)
        .ok()
        .and_then(|d| d.first().map(|d| (d.width, d.height)))
        .unwrap_or((1440.0, 900.0))
}

pub fn show_recorder_window(app: &AppHandle) -> tauri::Result<()> {
    if let Some(w) = app.get_webview_window("recorder") {
        w.show()?;
        w.set_focus()?;
        return Ok(());
    }
    let (sw, sh) = main_display_size(app);
    let (w, h) = (1100.0, 76.0);
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
    if let Some(w) = app.get_webview_window("recorder") {
        raise_above_frame(&w);
    }
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
        .inner_size(600.0, 760.0)
        .resizable(false)
        .accept_first_mouse(true)
        .center()
        .build()?;
    Ok(())
}

fn url_escape(s: &str) -> String {
    s.replace('%', "%25").replace(' ', "%20").replace('&', "%26").replace('#', "%23")
}

// Commands that create windows are async: creating a webview from a synchronous
// command (which runs on the main thread) deadlocks on Windows / WebView2.

#[tauri::command]
pub async fn show_recorder(app: AppHandle) -> Result<(), String> {
    show_recorder_window(&app).map_err(|e| e.to_string())
}

#[tauri::command]
pub async fn show_permissions(app: AppHandle) -> Result<(), String> {
    show_permissions_window(&app).map_err(|e| e.to_string())
}

#[tauri::command]
pub async fn show_compressor(app: AppHandle, input: Option<String>) -> Result<(), String> {
    show_compressor_window(&app, input)
}

pub fn show_compressor_window(app: &AppHandle, input: Option<String>) -> Result<(), String> {
    if let Some(w) = app.get_webview_window("compress") {
        let _ = w.close();
    }
    let url = match input {
        Some(p) => format!("compress.html?input={}", url_escape(&p)),
        None => "compress.html".into(),
    };
    WebviewWindowBuilder::new(app, "compress", WebviewUrl::App(url.into()))
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
pub async fn open_editor(app: AppHandle, dir: String) -> Result<(), String> {
    let label = format!("editor-{}", dir.bytes().fold(5381u64, |h, b| h.wrapping_mul(33) ^ b as u64));
    if let Some(w) = app.get_webview_window(&label) {
        w.set_focus().map_err(|e| e.to_string())?;
        return Ok(());
    }
    // open on the screen the recorder bar is on, then zoom to fill it (not fullscreen)
    let mut origin = (60.0, 60.0);
    if let Some(r) = app.get_webview_window("recorder") {
        if let Ok(Some(m)) = r.current_monitor() {
            let sf = m.scale_factor();
            origin = (m.position().x as f64 / sf + 60.0, m.position().y as f64 / sf + 60.0);
        }
        let _ = r.hide();
    }
    let win = WebviewWindowBuilder::new(&app, label, WebviewUrl::App(format!("editor.html?dir={}", url_escape(&dir)).into()))
        .title("ScreenCraft")
        .accept_first_mouse(true)
        .inner_size(1240.0, 780.0)
        .min_inner_size(960.0, 640.0)
        .position(origin.0, origin.1)
        .build()
        .map_err(|e| e.to_string())?;
    let _ = win.maximize();
    Ok(())
}

/// Close the area picker without a selection (the user chose another capture mode).
#[tauri::command]
pub fn close_area_picker(app: AppHandle) {
    if let Some(w) = app.get_webview_window("area") {
        let _ = w.close();
    }
}

#[tauri::command]
pub async fn open_area_picker(app: AppHandle, display_id: Option<u32>) -> Result<(), String> {
    if let Some(w) = app.get_webview_window("area") {
        let _ = w.close();
    }
    let displays = display_frames(&app)?;
    let d = match display_id {
        Some(id) => displays.iter().find(|d| d.id == id),
        None => displays.first(),
    }
    .ok_or("display not found")?;
    WebviewWindowBuilder::new(&app, "area", WebviewUrl::App(format!("area.html?x={}&y={}", d.x, d.y).into()))
        .title("Select area")
        .position(d.x, d.y)
        .inner_size(d.width, d.height)
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
    if QUITTING.load(std::sync::atomic::Ordering::SeqCst) {
        let editors_left = app.webview_windows().keys().any(|k| k.starts_with("editor-") && k != label);
        if !editors_left {
            exit_now(app);
        }
        return;
    }
    let tracked = |l: &str| l.starts_with("editor-") || l == "compress" || l == "permissions";
    if !tracked(label) {
        return;
    }
    if label == "permissions" && !all_required_granted() {
        return;
    }
    let others = app.webview_windows().keys().any(|k| tracked(k) && k != label);
    if !others {
        let _ = show_recorder_window(app);
    }
}

// ---------------------------------------------------------------- area focus frame

#[derive(serde::Deserialize)]
pub struct FrameRect {
    x: f64,
    y: f64,
    width: f64,
    height: f64,
}

/// Slightly dims everything around the chosen recording area so it is obvious
/// what is being captured. Four click-through translucent windows form the
/// surround and one transparent window draws the outline. They are our own
/// windows, so ScreenCaptureKit excludes them from the recording.
#[tauri::command]
pub async fn show_area_frame(app: AppHandle, rect: FrameRect, display_id: Option<u32>) -> Result<(), String> {
    hide_area_frame(app.clone());

    let displays = display_frames(&app)?;
    let d = displays
        .iter()
        .find(|d| Some(d.id) == display_id)
        .or_else(|| {
            displays
                .iter()
                .find(|d| rect.x >= d.x && rect.x < d.x + d.width && rect.y >= d.y && rect.y < d.y + d.height)
        })
        .or_else(|| displays.first())
        .ok_or("display not found")?;
    let (dx, dy, dw, dh) = (d.x, d.y, d.width, d.height);
    let (ax, ay, aw, ah) = (rect.x, rect.y, rect.width, rect.height);

    // One transparent, click-through window covering the display. The page draws
    // the dim with a cut-out hole over the area, so the area itself stays clear.
    let url = format!(
        "frame.html?x={}&y={}&w={}&h={}",
        ax - dx, ay - dy, aw, ah
    );
    let win = WebviewWindowBuilder::new(&app, "frame-border", WebviewUrl::App(url.into()))
        .position(dx, dy)
        .inner_size(dw, dh)
        .decorations(false)
        .transparent(true)
        .shadow(false)
        .resizable(false)
        .always_on_top(true)
        .skip_taskbar(true)
        .focused(false)
        .visible_on_all_workspaces(true)
        .build()
        .map_err(|e| e.to_string())?;
    let _ = win.set_ignore_cursor_events(true);

    // keep the toolbar above the dimmed surround
    if let Some(r) = app.get_webview_window("recorder") {
        raise_above_frame(&r);
    }
    Ok(())
}

#[tauri::command]
pub fn hide_area_frame(app: AppHandle) {
    for label in ["frame-border"] {
        if let Some(w) = app.get_webview_window(label) {
            let _ = w.close();
        }
    }
}

static QUITTING: std::sync::atomic::AtomicBool = std::sync::atomic::AtomicBool::new(false);
/// Set right before we exit on purpose, so the exit handler lets it through.
pub static ALLOW_EXIT: std::sync::atomic::AtomicBool = std::sync::atomic::AtomicBool::new(false);

fn exit_now(app: &AppHandle) {
    ALLOW_EXIT.store(true, std::sync::atomic::Ordering::SeqCst);
    app.exit(0);
}

/// Quit the app, but let open editors ask about unsaved changes first: each editor
/// is asked to close, and the app exits once the last one is gone.
pub fn request_quit(app: &AppHandle) {
    hide_area_frame(app.clone());
    let editors: Vec<_> = app
        .webview_windows()
        .into_iter()
        .filter(|(k, _)| k.starts_with("editor-"))
        .map(|(_, w)| w)
        .collect();
    if editors.is_empty() {
        exit_now(app);
        return;
    }
    QUITTING.store(true, std::sync::atomic::Ordering::SeqCst);
    for w in editors {
        let _ = w.close();
    }
}

/// The X on the recorder ribbon quits the whole app.
#[tauri::command]
pub fn quit_app(app: AppHandle) {
    request_quit(&app);
}

/// An editor declined to close (unsaved changes, user chose Cancel).
#[tauri::command]
pub fn cancel_quit() {
    QUITTING.store(false, std::sync::atomic::Ordering::SeqCst);
}

// ---------------------------------------------------------------- dropdown menu window
// Recorder-bar menus live in their own borderless window above the bar, so the bar
// never has to resize (which flashed) and the menu is always fully opaque and unclipped.

static MENU_STATE: std::sync::Mutex<Option<(serde_json::Value, f64, f64)>> = std::sync::Mutex::new(None);

fn ensure_menu_window(app: &AppHandle) -> tauri::Result<tauri::WebviewWindow> {
    if let Some(w) = app.get_webview_window("menu") {
        return Ok(w);
    }
    let w = WebviewWindowBuilder::new(app, "menu", WebviewUrl::App("menu.html".into()))
        .title("ScreenCraft menu")
        .inner_size(300.0, 100.0)
        .decorations(false)
        .transparent(true)
        .shadow(true)
        .resizable(false)
        .always_on_top(true)
        .skip_taskbar(true)
        .visible(false)
        .focused(false)
        .accept_first_mouse(true)
        .visible_on_all_workspaces(true)
        .build()?;
    #[cfg(target_os = "macos")]
    with_ns_window(&w, |n| n.setLevel(26));
    Ok(w)
}

pub fn prewarm_menu_window(app: &AppHandle) {
    let _ = ensure_menu_window(app);
}

/// Stash the items + anchor and tell the menu page to render; the page then calls
/// `menu_show` with its measured size.
#[tauri::command]
pub async fn open_menu_window(app: AppHandle, items: serde_json::Value, left: f64) -> Result<(), String> {
    use tauri::Emitter;
    let rec = app.get_webview_window("recorder").ok_or("no recorder window")?;
    let scale = rec.scale_factor().map_err(|e| e.to_string())?;
    let pos = rec.outer_position().map_err(|e| e.to_string())?;
    let (px, py) = (pos.x as f64 / scale, pos.y as f64 / scale);
    *MENU_STATE.lock().unwrap() = Some((items.clone(), px + left, py - 6.0));
    let w = ensure_menu_window(&app).map_err(|e| e.to_string())?;
    w.emit("menu-render", items).map_err(|e| e.to_string())
}

/// The menu page asks for what to draw when it (re)loads.
#[tauri::command]
pub fn menu_take_items() -> Option<serde_json::Value> {
    MENU_STATE.lock().unwrap().as_ref().map(|s| s.0.clone())
}

/// Size, place (bottom edge just above the bar) and reveal the menu in one go.
#[tauri::command]
pub async fn menu_show(app: AppHandle, width: f64, height: f64) -> Result<(), String> {
    let Some((_, x, bottom)) = MENU_STATE.lock().unwrap().clone() else { return Ok(()) };
    let w = ensure_menu_window(&app).map_err(|e| e.to_string())?;
    w.set_size(tauri::LogicalSize::new(width, height)).map_err(|e| e.to_string())?;
    w.set_position(tauri::LogicalPosition::new(x, bottom - height)).map_err(|e| e.to_string())?;
    w.show().map_err(|e| e.to_string())?;
    w.set_focus().map_err(|e| e.to_string())?;
    Ok(())
}

#[tauri::command]
pub fn close_menu_window(app: AppHandle) {
    if let Some(w) = app.get_webview_window("menu") {
        let _ = w.hide();
    }
}
