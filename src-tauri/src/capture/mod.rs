//! Screen capture: the options and results shared by every platform, plus the
//! platform recorder (`Capture`).

use screencraft_core::project::Rect;
use serde::{Deserialize, Serialize};
use std::path::PathBuf;

#[cfg(target_os = "macos")]
mod macos;
#[cfg(target_os = "macos")]
pub use macos::Capture;

#[cfg(windows)]
mod win;
#[cfg(windows)]
pub use win::Capture;

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct AreaRect {
    pub x: f64,
    pub y: f64,
    pub width: f64,
    pub height: f64,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct StartOpts {
    /// "display" | "window" | "area"
    pub mode: String,
    pub display_id: Option<u32>,
    pub window_id: Option<u32>,
    pub area: Option<AreaRect>,
    #[serde(default)]
    pub system_audio: bool,
    #[serde(default)]
    pub microphone: bool,
    #[serde(default)]
    pub microphone_id: Option<String>,
    #[serde(default = "default_fps")]
    pub fps: u32,
    /// Cap for the long side of the recording in pixels; `None` = native resolution.
    #[serde(default)]
    pub max_long_side: Option<u32>,
    pub output_dir: PathBuf,
}

fn default_fps() -> u32 {
    60
}

#[derive(Clone, Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct StartInfo {
    pub width: u32,
    pub height: u32,
    pub bounds: Rect,
    /// Wall clock (ms since epoch) at which the first frame was recorded.
    pub started_unix_ms: f64,
}

#[derive(Clone, Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct StopInfo {
    pub dir: PathBuf,
    pub duration_ms: f64,
    pub width: u32,
    pub height: u32,
    pub bounds: Rect,
    pub started_unix_ms: f64,
    pub has_system_audio: bool,
    pub has_microphone: bool,
}
