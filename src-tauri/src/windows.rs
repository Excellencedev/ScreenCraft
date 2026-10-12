//! Window creation helpers and window-related commands.

use crate::commands::permissions::all_required_granted;
use screencapturekit::prelude::*;
use tauri::{AppHandle, Manager, WebviewUrl, WebviewWindowBuilder};

/// Native window level / colour tweaks (AppKit) that Tauri doesn't expose.
/// AppKit objects may only be touched from the main thread, so hop there first.
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

/// Keep the toolbar / HUD above the dimmed surround of an area recording.
pub fn raise_above_frame(win: &tauri::WebviewWindow) {
    with_ns_window(win, |w| w.setLevel(25));
}

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
pub fn show_area_frame(app: AppHandle, rect: FrameRect, display_id: Option<u32>) -> Result<(), String> {
    hide_area_frame(app.clone());

    let content = SCShareableContent::get().map_err(|e| e.to_string())?;
    let displays = content.displays();
    let d = displays
        .iter()
        .find(|d| Some(d.display_id()) == display_id)
        .or_else(|| {
            displays.iter().find(|d| {
                let f = d.frame();
                rect.x >= f.origin.x && rect.x < f.origin.x + f.size.width && rect.y >= f.origin.y && rect.y < f.origin.y + f.size.height
            })
        })
        .or_else(|| displays.first())
        .ok_or("display not found")?;
    let f = d.frame();
    let (dx, dy, dw, dh) = (f.origin.x, f.origin.y, f.size.width, f.size.height);
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
    with_ns_window(&w, |n| n.setLevel(26));
    Ok(w)
}

pub fn prewarm_menu_window(app: &AppHandle) {
    let _ = ensure_menu_window(app);
}

/// Stash the items + anchor and tell the menu page to render; the page then calls
/// `menu_show` with its measured size.
#[tauri::command]
pub fn open_menu_window(app: AppHandle, items: serde_json::Value, left: f64) -> Result<(), String> {
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
pub fn menu_show(app: AppHandle, width: f64, height: f64) -> Result<(), String> {
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
