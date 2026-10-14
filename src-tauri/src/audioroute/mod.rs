//! Keeping Bluetooth headphones out of call-quality mode.
//!
//! When anything opens a Bluetooth headset's *microphone*, macOS (and Windows) moves the headset
//! from its high-quality stereo profile (A2DP) to the hands-free profile (HFP): 16 kHz, mono, and
//! the music in your ears and the system audio being captured both turn to telephone quality.
//! OBS, Audacity and every other recorder suffer the same thing; their documented cure is "use a
//! different microphone". This module lets the app do that for you: it lists inputs with their
//! transport (built-in, Bluetooth, USB ...), and can pin the system's default input to the
//! built-in microphone while recording, then put it back.

use serde::Serialize;

#[derive(Serialize, Clone, Debug)]
#[serde(rename_all = "camelCase")]
pub struct InputInfo {
    /// Same string as AVCaptureDevice.uniqueID.
    pub uid: String,
    pub name: String,
    /// "builtin" | "bluetooth" | "usb" | "virtual" | "other"
    pub transport: String,
    pub is_default: bool,
}

#[derive(Serialize, Clone, Debug, Default)]
#[serde(rename_all = "camelCase")]
pub struct Route {
    pub inputs: Vec<InputInfo>,
    pub default_input_uid: Option<String>,
    pub default_input_bluetooth: bool,
    pub default_output_bluetooth: bool,
    pub default_output_name: Option<String>,
    /// UID of the Mac's own microphone, when it has one.
    pub builtin_input_uid: Option<String>,
    /// True while the app has the default input pinned to the built-in microphone.
    pub pinned: bool,
}

#[cfg(target_os = "macos")]
mod macos;
#[cfg(target_os = "macos")]
pub use macos::{pin_builtin_input, restore_input, route};

#[cfg(not(target_os = "macos"))]
pub fn route() -> Route {
    Route::default()
}
#[cfg(not(target_os = "macos"))]
pub fn pin_builtin_input() -> Result<Option<String>, String> {
    Ok(None)
}
#[cfg(not(target_os = "macos"))]
pub fn restore_input() {}
