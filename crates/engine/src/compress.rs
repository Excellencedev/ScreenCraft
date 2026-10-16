//! Re-encode a finished video to make it smaller (the compressor).

use std::path::PathBuf;

pub struct CompressOptions {
    pub input: PathBuf,
    pub output: PathBuf,
    pub hevc: bool,
    /// 5..100, as in the 0.3 compressor.
    pub quality: f32,
    pub height: Option<u32>,
    pub fps: Option<f32>,
    pub audio_kbps: Option<u32>,
}

/// Returns the size of the written file in bytes.
pub fn compress_video(_opts: &CompressOptions, _progress: impl FnMut(f32) -> bool) -> Result<u64, String> {
    Err("The compressor is not built into this version yet.".into())
}
