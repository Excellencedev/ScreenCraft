use super::projects::write_project;
use crate::capture::{self, StartOpts};
use crate::state::{timestamp, AppState, CmdResult};
use screencraft_core::input::InputLog;
use screencraft_core::project::{Media, Project};
use screencraft_core::zoom::{generate, AutoZoomOptions};
use serde::{Deserialize, Serialize};
use std::fs::{self, File};
use std::io::Write;
use tauri::ipc::{InvokeBody, Request};
use tauri::State;

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct StartArgs {
    mode: String,
    display_id: Option<u32>,
    window_id: Option<u32>,
    area: Option<capture::AreaRect>,
    system_audio: bool,
    microphone: bool,
    microphone_id: Option<String>,
    fps: Option<u32>,
    max_long_side: Option<u32>,
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct StartReply {
    dir: String,
    started_unix_ms: f64,
}

#[tauri::command]
pub fn start_recording(state: State<AppState>, args: StartArgs) -> CmdResult<StartReply> {
    let name = format!("Recording {}", timestamp());
    let dir = state.projects_root().join(format!("{name}.screencraft"));
    let info = state.capture.start(StartOpts {
        mode: args.mode,
        display_id: args.display_id,
        window_id: args.window_id,
        area: args.area,
        system_audio: args.system_audio,
        microphone: args.microphone,
        microphone_id: args.microphone_id,
        fps: args.fps.unwrap_or(60),
        max_long_side: args.max_long_side,
        output_dir: dir.clone(),
    })?;
    *state.recording_dir.lock().unwrap() = Some(dir.clone());
    Ok(StartReply { dir: dir.to_string_lossy().into(), started_unix_ms: info.started_unix_ms })
}

#[tauri::command]
pub fn camera_begin(state: State<AppState>, ext: String) -> CmdResult<()> {
    let dir = state.recording_dir.lock().unwrap().clone().ok_or("not recording")?;
    let file = File::create(dir.join(format!("camera.{ext}"))).map_err(|e| e.to_string())?;
    *state.camera.lock().unwrap() = Some((file, ext));
    Ok(())
}

#[tauri::command]
pub fn camera_chunk(state: State<AppState>, request: Request<'_>) -> CmdResult<()> {
    let InvokeBody::Raw(bytes) = request.body() else { return Err("expected raw body".into()) };
    if let Some((f, _)) = state.camera.lock().unwrap().as_mut() {
        f.write_all(bytes).map_err(|e| e.to_string())?;
    }
    Ok(())
}

#[tauri::command]
pub fn stop_recording(state: State<AppState>, camera_started_unix_ms: Option<f64>) -> CmdResult<String> {
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
    project.scene.zooms = generate(&log, info.duration_ms, &AutoZoomOptions::default());
    write_project(&info.dir, &project)?;
    Ok(info.dir.to_string_lossy().into())
}
