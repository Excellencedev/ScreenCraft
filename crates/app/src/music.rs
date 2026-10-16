//! Music files: copy into the project and measure (decoding arrives with the audio milestone).

use screencraft_engine::Package;
use std::path::Path;

/// Copy `src` into the package; returns (file name, display name, duration ms).
pub fn import(package: &Package, src: &Path) -> Result<(String, String, f64), String> {
    let ext = src.extension().and_then(|e| e.to_str()).unwrap_or("mp3").to_lowercase();
    let stamp = std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).map(|d| d.as_millis()).unwrap_or(0);
    let file = format!("music-{stamp}.{ext}");
    std::fs::copy(src, package.dir.join(&file)).map_err(|e| format!("Could not copy that file: {e}"))?;
    let name = src.file_stem().and_then(|s| s.to_str()).unwrap_or("Music").to_string();
    let dur = screencraft_audio::probe_duration_ms(&package.dir.join(&file)).ok_or_else(|| "That audio file couldn't be read.".to_string())?;
    Ok((file, name, dur))
}
