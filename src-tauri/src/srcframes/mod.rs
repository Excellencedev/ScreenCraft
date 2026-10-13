//! Full-resolution source frames for export (`sc://localhost/src/<kind>?t=<ms>&d=<dir>`),
//! decoded natively by the platform reader.

#[cfg(target_os = "macos")]
mod macos;
#[cfg(target_os = "macos")]
pub use macos::submit;

#[cfg(windows)]
mod windows;
#[cfg(windows)]
pub use windows::submit;

pub struct Request {
    pub dir: String,
    pub kind: String,
    pub t_ms: f64,
    pub quality: u8,
    pub reply: Box<dyn FnOnce(Option<Vec<u8>>) + Send>,
}
