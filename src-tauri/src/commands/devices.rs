//! Audio device listing and the live level meter.

use crate::state::{AppState, CmdResult};
use objc2_av_foundation::AVCaptureDevice;
use objc2_foundation::NSString;
use serde::Serialize;
use tauri::{AppHandle, State};

#[derive(Serialize)]
pub struct Device {
    id: String,
    name: String,
}

fn devices(media: &str) -> Vec<Device> {
    #[allow(deprecated)]
    let list = unsafe { AVCaptureDevice::devicesWithMediaType(&NSString::from_str(media)) };
    list.iter()
        .map(|d| Device { id: unsafe { d.uniqueID() }.to_string(), name: unsafe { d.localizedName() }.to_string() })
        .collect()
}

#[tauri::command]
pub fn list_audio_inputs() -> Vec<Device> {
    devices("soun")
}

#[tauri::command]
pub fn list_cameras() -> Vec<Device> {
    devices("vide")
}

#[tauri::command]
pub fn meter_start(app: AppHandle, state: State<AppState>) -> CmdResult<()> {
    state.meter.start(app)
}

#[tauri::command]
pub fn meter_stop(state: State<AppState>) {
    state.meter.stop();
}
