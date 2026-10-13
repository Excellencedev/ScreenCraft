//! Video compression: options and probe results shared by every platform, plus the
//! platform encoder (`probe`, `compress`).

use serde::{Deserialize, Serialize};
use std::sync::atomic::AtomicBool;

#[cfg(target_os = "macos")]
mod macos;
#[cfg(target_os = "macos")]
pub use macos::{compress, probe};

#[cfg(windows)]
mod windows;
#[cfg(windows)]
pub use windows::{compress, probe};

pub static CANCEL: AtomicBool = AtomicBool::new(false);

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Probe {
    pub width: u32,
    pub height: u32,
    pub fps: f64,
    pub duration_s: f64,
    pub video_kbps: f64,
    pub has_audio: bool,
    pub file_size: u64,
}

#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct CompressOpts {
    pub input: String,
    pub output: String,
    /// "h264" | "hevc"
    pub codec: String,
    /// 0..=100, higher = better quality / bigger file.
    pub quality: f64,
    /// Downscale to this height (never upscales).
    pub height: Option<u32>,
    /// Limit the frame rate (never increases it).
    pub fps: Option<f64>,
    /// "keep" | "aac160" | "aac128" | "aac96" | "aac64" | "none"
    pub audio: String,
    #[serde(default)]
    pub optimize_for_web: bool,
}

/// Target video bitrate from the quality slider (bits per pixel per frame).
pub fn target_bitrate(width: f64, height: f64, fps: f64, quality: f64, hevc: bool) -> f64 {
    let q = (quality / 100.0).clamp(0.0, 1.0);
    let bpp = 0.015 + q * q * 0.2;
    let bpp = if hevc { bpp * 0.62 } else { bpp };
    (width * height * fps * bpp).clamp(250_000.0, 100_000_000.0)
}
