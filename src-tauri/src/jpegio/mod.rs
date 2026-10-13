//! Native JPEG encoding of BGRA frames (ImageIO on macOS, WIC on Windows).

#[cfg(target_os = "macos")]
mod macos;
#[cfg(target_os = "macos")]
pub use macos::encode_bgra;

#[cfg(windows)]
mod windows;
#[cfg(windows)]
pub use windows::{encode_bgra, encode_bgra_max_w};
