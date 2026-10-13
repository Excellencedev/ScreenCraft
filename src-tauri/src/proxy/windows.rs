//! Proxy strip decoding on Windows. Not implemented yet (Media Foundation later).

use super::StripMeta;
use std::path::Path;

pub fn build_strip(
    _src: &Path,
    _out_sci: &Path,
    _out_idx: &Path,
    _target_w: u32,
    _quality: u8,
    _progress: &mut dyn FnMut(f64),
) -> Result<StripMeta, String> {
    Err("Decoding video is not available on Windows yet".into())
}
