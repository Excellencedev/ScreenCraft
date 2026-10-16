//! GIF export.

use screencraft_core::project::Project;
use std::path::Path;

use crate::Package;

pub struct GifReport {
    pub bytes: u64,
    pub width: u32,
    pub height: u32,
}

pub fn export_gif(_package: &Package, _project: &Project, _resolution: u32, _out: &Path, _progress: impl FnMut(f32) -> bool) -> Result<GifReport, String> {
    Err("GIF export is not built into this version yet.".into())
}
