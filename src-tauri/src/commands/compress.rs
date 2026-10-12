use crate::compress;
use crate::state::CmdResult;
use std::fs;
use tauri::{AppHandle, Emitter};

#[tauri::command]
pub fn probe_video(path: String) -> CmdResult<compress::Probe> {
    compress::probe(&path)
}

#[tauri::command]
pub async fn compress_video(app: AppHandle, opts: compress::CompressOpts) -> CmdResult<u64> {
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
pub fn cancel_compress() {
    compress::CANCEL.store(true, std::sync::atomic::Ordering::SeqCst);
}

#[tauri::command]
pub fn estimate_bitrate(width: f64, height: f64, fps: f64, quality: f64, hevc: bool) -> f64 {
    compress::target_bitrate(width, height, fps, quality, hevc)
}
