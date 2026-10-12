#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

mod capture;
mod compress;
mod inputlog;

use capture::{Capture, StartOpts};
use screencapturekit::prelude::*;
use screencraft_core::input::InputLog;
use screencraft_core::project::{Media, Project, ZoomRegion};
use screencraft_core::track::{compute_track, RenderTrack};
use screencraft_core::zoom::{generate, AutoZoomOptions};
use serde::{Deserialize, Serialize};
use std::fs::{self, File};
use std::io::Write;
use std::path::{Path, PathBuf};
use std::sync::Mutex;
use tauri::ipc::{InvokeBody, Request};
use tauri::menu::{Menu, MenuItem};
use tauri::tray::TrayIconBuilder;
use tauri::{AppHandle, Emitter, Manager, State, WebviewUrl, WebviewWindowBuilder};
use tauri_plugin_global_shortcut::{Code, GlobalShortcutExt, Modifiers, Shortcut, ShortcutState};

struct AppState {
    capture: Capture,
    recording_dir: Mutex<Option<PathBuf>>,
    camera: Mutex<Option<(File, String)>>,
    export: Mutex<Option<File>>,
    inputs: Mutex<std::collections::HashMap<PathBuf, InputLog>>,
}

type CmdResult<T> = Result<T, String>;

fn projects_root() -> PathBuf {
    let home = std::env::var("HOME").unwrap_or_else(|_| ".".into());
    PathBuf::from(home).join("ScreenCraft Projects")
}

fn timestamp() -> String {
    unsafe {
        let t = libc::time(std::ptr::null_mut());
        let mut tm: libc::tm = std::mem::zeroed();
        libc::localtime_r(&t, &mut tm);
        format!(
            "{:04}-{:02}-{:02} {:02}.{:02}.{:02}",
            tm.tm_year + 1900,
            tm.tm_mon + 1,
            tm.tm_mday,
            tm.tm_hour,
            tm.tm_min,
            tm.tm_sec
        )
    }
}

// ---------------------------------------------------------------- sources

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
struct DisplayInfo {
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
struct WindowInfo {
    id: u32,
    title: String,
    app: String,
    x: f64,
    y: f64,
    width: f64,
    height: f64,
}

#[derive(Serialize)]
struct Sources {
    displays: Vec<DisplayInfo>,
    windows: Vec<WindowInfo>,
}

#[tauri::command]
fn list_sources() -> CmdResult<Sources> {
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
            let title = w.title().unwrap_or_default();
            Some(WindowInfo {
                id: w.window_id(),
                title,
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

// -------------------------------------------------------------- recording

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct StartArgs {
    mode: String,
    display_id: Option<u32>,
    window_id: Option<u32>,
    area: Option<capture::AreaRect>,
    system_audio: bool,
    microphone: bool,
    microphone_id: Option<String>,
    fps: Option<u32>,
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
struct StartReply {
    dir: String,
    started_unix_ms: f64,
}

#[tauri::command]
fn start_recording(state: State<AppState>, args: StartArgs) -> CmdResult<StartReply> {
    let name = format!("Recording {}", timestamp());
    let dir = projects_root().join(format!("{name}.screencraft"));
    let info = state.capture.start(StartOpts {
        mode: args.mode,
        display_id: args.display_id,
        window_id: args.window_id,
        area: args.area,
        system_audio: args.system_audio,
        microphone: args.microphone,
        microphone_id: args.microphone_id,
        fps: args.fps.unwrap_or(60),
        output_dir: dir.clone(),
    })?;
    *state.recording_dir.lock().unwrap() = Some(dir.clone());
    Ok(StartReply { dir: dir.to_string_lossy().into(), started_unix_ms: info.started_unix_ms })
}

#[tauri::command]
fn camera_begin(state: State<AppState>, ext: String) -> CmdResult<()> {
    let dir = state.recording_dir.lock().unwrap().clone().ok_or("not recording")?;
    let file = File::create(dir.join(format!("camera.{ext}"))).map_err(|e| e.to_string())?;
    *state.camera.lock().unwrap() = Some((file, ext));
    Ok(())
}

#[tauri::command]
fn camera_chunk(state: State<AppState>, request: Request<'_>) -> CmdResult<()> {
    let InvokeBody::Raw(bytes) = request.body() else { return Err("expected raw body".into()) };
    if let Some((f, _)) = state.camera.lock().unwrap().as_mut() {
        f.write_all(bytes).map_err(|e| e.to_string())?;
    }
    Ok(())
}

#[tauri::command]
fn stop_recording(state: State<AppState>, camera_started_unix_ms: Option<f64>) -> CmdResult<String> {
    let info = state.capture.stop()?;
    let cam = state.camera.lock().unwrap().take();
    *state.recording_dir.lock().unwrap() = None;

    let log: InputLog = fs::read(info.dir.join("input.json"))
        .ok()
        .and_then(|b| serde_json::from_slice(&b).ok())
        .unwrap_or_default();

    let camera = cam.map(|(f, ext)| {
        drop(f);
        format!("camera.{ext}")
    });
    let camera_offset_ms = match (&camera, camera_started_unix_ms) {
        // camera time that corresponds to screen t=0
        (Some(_), Some(cam_start)) => info.started_unix_ms - cam_start,
        _ => 0.0,
    };
    let media = Media {
        screen: "screen.mp4".into(),
        camera,
        input: "input.json".into(),
        duration_ms: info.duration_ms,
        width: info.width,
        height: info.height,
        bounds: info.bounds,
        camera_offset_ms,
        has_system_audio: info.has_system_audio,
        has_microphone: info.has_microphone,
    };
    let name = info.dir.file_stem().map(|s| s.to_string_lossy().to_string()).unwrap_or_default();
    let mut project = Project::from_recording(&name, media);
    project.id = format!("{:x}", (info.started_unix_ms as u64) ^ 0x5c0ffee);
    project.created_at = timestamp();
    project.scene.zooms = generate(&log.clicks, info.duration_ms, &AutoZoomOptions::default());
    write_project(&info.dir, &project)?;
    Ok(info.dir.to_string_lossy().into())
}

// --------------------------------------------------------------- projects

fn write_project(dir: &Path, project: &Project) -> CmdResult<()> {
    let json = serde_json::to_vec_pretty(project).map_err(|e| e.to_string())?;
    fs::write(dir.join("project.json"), json).map_err(|e| e.to_string())
}

#[derive(Serialize)]
struct Loaded {
    project: Project,
    input: InputLog,
    dir: String,
}

fn read_input(state: &AppState, dir: &Path, rel: &str) -> InputLog {
    let key = dir.to_path_buf();
    if let Some(l) = state.inputs.lock().unwrap().get(&key) {
        return l.clone();
    }
    let log: InputLog = fs::read(dir.join(rel))
        .ok()
        .and_then(|b| serde_json::from_slice(&b).ok())
        .unwrap_or_default();
    state.inputs.lock().unwrap().insert(key, log.clone());
    log
}

#[tauri::command]
fn load_project(state: State<AppState>, dir: String) -> CmdResult<Loaded> {
    let d = PathBuf::from(&dir);
    let bytes = fs::read(d.join("project.json")).map_err(|e| format!("cannot read project: {e}"))?;
    let project: Project = serde_json::from_slice(&bytes).map_err(|e| format!("invalid project: {e}"))?;
    let input = read_input(&state, &d, &project.media.input);
    Ok(Loaded { project, input, dir })
}

#[tauri::command]
fn save_project(dir: String, project: Project) -> CmdResult<()> {
    write_project(Path::new(&dir), &project)
}

#[tauri::command]
fn compute_render_track(state: State<AppState>, dir: String, project: Project, fps: f64) -> CmdResult<RenderTrack> {
    let log = read_input(&state, Path::new(&dir), &project.media.input);
    Ok(compute_track(&project, &log, fps.clamp(1.0, 240.0)))
}

#[tauri::command]
fn auto_zoom(state: State<AppState>, dir: String, project: Project, zoom: f64) -> CmdResult<Vec<ZoomRegion>> {
    let log = read_input(&state, Path::new(&dir), &project.media.input);
    let opts = AutoZoomOptions { zoom, ..Default::default() };
    Ok(generate(&log.clicks, project.media.duration_ms, &opts))
}

#[derive(Serialize)]
struct RecentProject {
    dir: String,
    name: String,
    modified: u64,
}

#[tauri::command]
fn recent_projects() -> Vec<RecentProject> {
    let mut out = Vec::new();
    if let Ok(rd) = fs::read_dir(projects_root()) {
        for e in rd.flatten() {
            let p = e.path();
            if p.extension().map(|x| x == "screencraft").unwrap_or(false) && p.join("project.json").exists() {
                let modified = e
                    .metadata()
                    .and_then(|m| m.modified())
                    .ok()
                    .and_then(|t| t.duration_since(std::time::UNIX_EPOCH).ok())
                    .map(|d| d.as_secs())
                    .unwrap_or(0);
                out.push(RecentProject {
                    dir: p.to_string_lossy().into(),
                    name: p.file_stem().map(|s| s.to_string_lossy().into()).unwrap_or_default(),
                    modified,
                });
            }
        }
    }
    out.sort_by(|a, b| b.modified.cmp(&a.modified));
    out
}

#[tauri::command]
fn projects_dir() -> String {
    projects_root().to_string_lossy().into()
}

// ----------------------------------------------------------------- export

#[tauri::command]
fn export_begin(state: State<AppState>, path: String) -> CmdResult<()> {
    let f = File::create(&path).map_err(|e| e.to_string())?;
    *state.export.lock().unwrap() = Some(f);
    Ok(())
}

#[tauri::command]
fn export_write(state: State<AppState>, request: Request<'_>) -> CmdResult<()> {
    use std::io::{Seek, SeekFrom};
    let InvokeBody::Raw(bytes) = request.body() else { return Err("expected raw body".into()) };
    let position: u64 = request
        .headers()
        .get("x-position")
        .and_then(|v| v.to_str().ok())
        .and_then(|s| s.parse().ok())
        .ok_or("missing position")?;
    if let Some(f) = state.export.lock().unwrap().as_mut() {
        f.seek(SeekFrom::Start(position)).map_err(|e| e.to_string())?;
        f.write_all(bytes).map_err(|e| e.to_string())?;
    }
    Ok(())
}

#[tauri::command]
fn export_end(state: State<AppState>) {
    *state.export.lock().unwrap() = None;
}

// ------------------------------------------------------------ permissions

#[link(name = "CoreGraphics", kind = "framework")]
extern "C" {
    fn CGPreflightScreenCaptureAccess() -> bool;
    fn CGRequestScreenCaptureAccess() -> bool;
}

fn av_status(media: &str) -> String {
    use objc2_av_foundation::{AVAuthorizationStatus, AVCaptureDevice};
    use objc2_foundation::NSString;
    let st = unsafe { AVCaptureDevice::authorizationStatusForMediaType(&NSString::from_str(media)) };
    match st {
        AVAuthorizationStatus::Authorized => "granted",
        AVAuthorizationStatus::Denied => "denied",
        AVAuthorizationStatus::Restricted => "restricted",
        _ => "notDetermined",
    }
    .into()
}

fn screen_granted() -> bool {
    unsafe { CGPreflightScreenCaptureAccess() }
}

#[derive(Serialize)]
struct PermissionStatus {
    screen: bool,
    camera: String,
    microphone: String,
}

#[tauri::command]
fn permission_status() -> PermissionStatus {
    PermissionStatus { screen: screen_granted(), camera: av_status("vide"), microphone: av_status("soun") }
}

/// Triggers the system prompt the first time; afterwards the user must flip the
/// switch in System Settings.
#[tauri::command]
fn request_screen_access() -> bool {
    unsafe { CGRequestScreenCaptureAccess() }
}

#[tauri::command]
fn open_privacy_pane(kind: String) {
    let pane = match kind.as_str() {
        "camera" => "Privacy_Camera",
        "microphone" => "Privacy_Microphone",
        _ => "Privacy_ScreenCapture",
    };
    let _ = std::process::Command::new("open")
        .arg(format!("x-apple.systempreferences:com.apple.preference.security?{pane}"))
        .spawn();
}

#[tauri::command]
fn restart_app(app: AppHandle) {
    app.restart();
}

fn show_permissions_window(app: &AppHandle) -> tauri::Result<()> {
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
        .inner_size(560.0, 560.0)
        .resizable(false)
        .center()
        .build()?;
    Ok(())
}

#[tauri::command]
fn show_permissions(app: AppHandle) -> CmdResult<()> {
    show_permissions_window(&app).map_err(|e| e.to_string())
}

// ------------------------------------------------------------ compression

#[tauri::command]
fn probe_video(path: String) -> CmdResult<compress::Probe> {
    compress::probe(&path)
}

#[tauri::command]
async fn compress_video(app: AppHandle, opts: compress::CompressOpts) -> CmdResult<u64> {
    let out = opts.output.clone();
    tauri::async_runtime::spawn_blocking(move || {
        compress::compress(&opts, |f| {
            let _ = app.emit("compress-progress", f);
        })
    })
    .await
    .map_err(|e| e.to_string())??;
    Ok(fs::metadata(out).map(|m| m.len()).unwrap_or(0))
}

#[tauri::command]
fn cancel_compress() {
    compress::CANCEL.store(true, std::sync::atomic::Ordering::SeqCst);
}

#[tauri::command]
fn estimate_bitrate(width: f64, height: f64, fps: f64, quality: f64, hevc: bool) -> f64 {
    compress::target_bitrate(width, height, fps, quality, hevc)
}

#[tauri::command]
fn show_compressor(app: AppHandle) -> CmdResult<()> {
    if let Some(w) = app.get_webview_window("compress") {
        return w.set_focus().map_err(|e| e.to_string());
    }
    if let Some(r) = app.get_webview_window("recorder") {
        let _ = r.hide();
    }
    WebviewWindowBuilder::new(&app, "compress", WebviewUrl::App("compress.html".into()))
        .title("Compress video")
        .inner_size(760.0, 640.0)
        .min_inner_size(640.0, 560.0)
        .center()
        .build()
        .map_err(|e| e.to_string())?;
    Ok(())
}

// ---------------------------------------------------------------- windows

fn main_display_size() -> (f64, f64) {
    SCShareableContent::get()
        .ok()
        .and_then(|c| c.displays().first().map(|d| (d.frame().size.width, d.frame().size.height)))
        .unwrap_or((1440.0, 900.0))
}

fn show_recorder_window(app: &AppHandle) -> tauri::Result<()> {
    if let Some(w) = app.get_webview_window("recorder") {
        w.show()?;
        w.set_focus()?;
        return Ok(());
    }
    let (sw, sh) = main_display_size();
    let (w, h) = (920.0, 76.0);
    WebviewWindowBuilder::new(app, "recorder", WebviewUrl::App("recorder.html".into()))
        .title("ScreenCraft")
        .inner_size(w, h)
        .position((sw - w) / 2.0, sh - h - 90.0)
        .decorations(false)
        .transparent(true)
        .shadow(false)
        .resizable(false)
        .always_on_top(true)
        .visible_on_all_workspaces(true)
        .build()?;
    Ok(())
}

#[tauri::command]
fn show_recorder(app: AppHandle) -> CmdResult<()> {
    show_recorder_window(&app).map_err(|e| e.to_string())
}

#[tauri::command]
fn open_editor(app: AppHandle, dir: String) -> CmdResult<()> {
    let label = format!("editor-{}", dir.bytes().fold(5381u64, |h, b| h.wrapping_mul(33) ^ b as u64));
    if let Some(w) = app.get_webview_window(&label) {
        w.set_focus().map_err(|e| e.to_string())?;
        return Ok(());
    }
    if let Some(r) = app.get_webview_window("recorder") {
        let _ = r.hide();
    }
    let q = dir.replace('%', "%25").replace(' ', "%20").replace('&', "%26").replace('#', "%23");
    WebviewWindowBuilder::new(&app, label, WebviewUrl::App(format!("editor.html?dir={q}").into()))
        .title("ScreenCraft")
        .inner_size(1240.0, 780.0)
        .min_inner_size(960.0, 640.0)
        .center()
        .build()
        .map_err(|e| e.to_string())?;
    Ok(())
}

#[tauri::command]
fn open_area_picker(app: AppHandle, display_id: Option<u32>) -> CmdResult<()> {
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
        .build()
        .map_err(|e| e.to_string())?;
    Ok(())
}

#[tauri::command]
fn reveal_in_finder(path: String) {
    let _ = std::process::Command::new("open").arg("-R").arg(path).spawn();
}

// ------------------------------------------------------------------- main

fn main() {
    let toggle = Shortcut::new(Some(Modifiers::SUPER | Modifiers::SHIFT), Code::Digit2);
    tauri::Builder::default()
        .plugin(tauri_plugin_dialog::init())
        .plugin(tauri_plugin_opener::init())
        .plugin(
            tauri_plugin_global_shortcut::Builder::new()
                .with_handler(move |app, shortcut, event| {
                    if shortcut == &toggle && event.state() == ShortcutState::Pressed {
                        let _ = app.emit("toggle-record", ());
                    }
                })
                .build(),
        )
        .on_window_event(|window, event| {
            // Bring the recorder back once the last editor window is closed.
            if let tauri::WindowEvent::Destroyed = event {
                let l = window.label();
                if l.starts_with("editor-") || l == "compress" || (l == "permissions" && screen_granted()) {
                    let app = window.app_handle();
                    let any_editor = app
                        .webview_windows()
                        .keys()
                        .any(|k| (k.starts_with("editor-") || k == "compress") && k != window.label());
                    if !any_editor {
                        let _ = show_recorder_window(app);
                    }
                }
            }
        })
        .manage(AppState {
            capture: Capture::spawn(),
            recording_dir: Mutex::new(None),
            camera: Mutex::new(None),
            export: Mutex::new(None),
            inputs: Mutex::new(Default::default()),
        })
        .invoke_handler(tauri::generate_handler![
            list_sources,
            start_recording,
            stop_recording,
            camera_begin,
            camera_chunk,
            load_project,
            save_project,
            compute_render_track,
            auto_zoom,
            recent_projects,
            projects_dir,
            export_begin,
            export_write,
            export_end,
            show_recorder,
            open_editor,
            open_area_picker,
            reveal_in_finder,
            probe_video,
            compress_video,
            cancel_compress,
            estimate_bitrate,
            show_compressor,
            permission_status,
            request_screen_access,
            open_privacy_pane,
            restart_app,
            show_permissions,
        ])
        .setup(move |app| {
            let _ = app.global_shortcut().register(toggle);
            let new_rec = MenuItem::with_id(app, "new", "New Recording", true, Some("CmdOrCtrl+Shift+2"))?;
            let stop = MenuItem::with_id(app, "stop", "Stop Recording", true, None::<&str>)?;
            let compress = MenuItem::with_id(app, "compress", "Compress Video…", true, None::<&str>)?;
            let perms = MenuItem::with_id(app, "perms", "Permissions…", true, None::<&str>)?;
            let quit = MenuItem::with_id(app, "quit", "Quit ScreenCraft", true, Some("CmdOrCtrl+Q"))?;
            let menu = Menu::with_items(app, &[&new_rec, &compress, &perms, &stop, &quit])?;
            TrayIconBuilder::with_id("tray")
                .icon(app.default_window_icon().cloned().ok_or("no icon")?)
                .menu(&menu)
                .on_menu_event(|app, event| match event.id().as_ref() {
                    "new" => {
                        let _ = show_recorder_window(app);
                    }
                    "perms" => {
                        let _ = show_permissions_window(app);
                    }
                    "compress" => {
                        let _ = show_compressor(app.clone());
                    }
                    "stop" => {
                        let _ = app.emit("stop-record", ());
                    }
                    "quit" => app.exit(0),
                    _ => {}
                })
                .build(app)?;
            fs::create_dir_all(projects_root()).ok();
            if screen_granted() {
                show_recorder_window(app.handle())?;
            } else {
                show_permissions_window(app.handle())?;
            }
            Ok(())
        })
        .run(tauri::generate_context!())
        .expect("error while running ScreenCraft");
}
