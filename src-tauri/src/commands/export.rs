use crate::state::{AppState, CmdResult};
use std::fs::File;
use std::io::{Seek, SeekFrom, Write};
use tauri::ipc::{InvokeBody, Request};
use tauri::State;

#[tauri::command]
pub fn export_begin(state: State<AppState>, path: String) -> CmdResult<()> {
    let f = File::create(&path).map_err(|e| e.to_string())?;
    *state.export.lock().unwrap() = Some(f);
    Ok(())
}

#[tauri::command]
pub fn export_write(state: State<AppState>, request: Request<'_>) -> CmdResult<()> {
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
pub fn export_end(state: State<AppState>) {
    *state.export.lock().unwrap() = None;
}
