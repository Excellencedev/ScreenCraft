//! Video compression on Windows. Not implemented yet.

use super::{CompressOpts, Probe};

pub fn probe(_path: &str) -> Result<Probe, String> {
    Err("Reading video files is not available on Windows yet".into())
}

pub fn compress(_opts: &CompressOpts, _progress: impl FnMut(f64)) -> Result<(), String> {
    Err("Compression is not available on Windows yet".into())
}
