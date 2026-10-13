//! Live system-audio level meter for the recorder toolbar.

#[cfg(target_os = "macos")]
mod macos;
#[cfg(target_os = "macos")]
pub use macos::Meter;

#[cfg(windows)]
mod windows;
#[cfg(windows)]
pub use windows::Meter;
