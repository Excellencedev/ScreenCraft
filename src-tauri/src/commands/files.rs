use crate::state::{save_settings, AppState, CmdResult};
use serde::Serialize;
use std::fs;
use tauri::State;

#[tauri::command]
pub fn reveal_in_finder(path: String) {
    let _ = std::process::Command::new("open").arg("-R").arg(path).spawn();
}

#[tauri::command]
pub fn copy_file(src: String, dst: String) -> CmdResult<u64> {
    fs::copy(&src, &dst).map_err(|e| e.to_string())
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct SettingsView {
    projects_dir: String,
}

#[tauri::command]
pub fn get_settings(state: State<AppState>) -> SettingsView {
    SettingsView { projects_dir: state.projects_root().to_string_lossy().into() }
}

#[tauri::command]
pub fn set_projects_dir(state: State<AppState>, path: Option<String>) -> CmdResult<String> {
    {
        let mut s = state.settings.lock().unwrap();
        s.projects_dir = path.filter(|p| !p.is_empty());
        save_settings(&s);
    }
    let root = state.projects_root();
    fs::create_dir_all(&root).map_err(|e| e.to_string())?;
    Ok(root.to_string_lossy().into())
}
