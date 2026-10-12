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

#[tauri::command]
pub fn remove_file(path: String) -> CmdResult<()> {
    fs::remove_file(&path).map_err(|e| e.to_string())
}

/// Where the export save sheet opens: ~/Movies when it exists, otherwise ~/Documents.
#[tauri::command]
pub fn default_export_dir() -> String {
    let home = std::env::var("HOME").unwrap_or_else(|_| ".".into());
    let movies = std::path::Path::new(&home).join("Movies");
    if movies.is_dir() {
        movies.to_string_lossy().into()
    } else {
        format!("{home}/Documents")
    }
}

/// Automation hook: when SCREENCRAFT_TEST_EXPORT_PATH is set the editor exports there
/// without showing the save sheet (used to test the export pipeline end to end).
#[tauri::command]
pub fn test_export_path() -> Option<String> {
    std::env::var("SCREENCRAFT_TEST_EXPORT_PATH").ok().filter(|p| !p.is_empty())
}
