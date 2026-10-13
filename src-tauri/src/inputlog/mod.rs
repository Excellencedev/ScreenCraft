//! Cursor, click, key and text-caret logging while recording.

#[cfg(target_os = "macos")]
mod macos;
#[cfg(target_os = "macos")]
pub use macos::InputTracker;

#[cfg(windows)]
mod windows;
#[cfg(windows)]
pub use windows::InputTracker;
