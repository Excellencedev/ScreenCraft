use serde::Serialize;
use tauri::AppHandle;

#[link(name = "CoreGraphics", kind = "framework")]
extern "C" {
    fn CGPreflightScreenCaptureAccess() -> bool;
    fn CGRequestScreenCaptureAccess() -> bool;
}

fn av_status(media: &str) -> String {
    use objc2_av_foundation::{AVAuthorizationStatus, AVCaptureDevice};
    use objc2_foundation::NSString;
    let st = unsafe { AVCaptureDevice::authorizationStatusForMediaType(&NSString::from_str(media)) };
    match st {
        AVAuthorizationStatus::Authorized => "granted",
        AVAuthorizationStatus::Denied => "denied",
        AVAuthorizationStatus::Restricted => "restricted",
        _ => "notDetermined",
    }
    .into()
}

pub fn screen_granted() -> bool {
    unsafe { CGPreflightScreenCaptureAccess() }
}

#[derive(Serialize)]
pub struct PermissionStatus {
    screen: bool,
    camera: String,
    microphone: String,
    accessibility: bool,
}

#[tauri::command]
pub fn permission_status() -> PermissionStatus {
    PermissionStatus {
        screen: screen_granted(),
        camera: av_status("vide"),
        microphone: av_status("soun"),
        accessibility: crate::ax::trusted(),
    }
}

/// Triggers the system prompt the first time; afterwards the user must flip the
/// switch in System Settings.
#[tauri::command]
pub fn request_screen_access() -> bool {
    unsafe { CGRequestScreenCaptureAccess() }
}

#[tauri::command]
pub fn open_privacy_pane(kind: String) {
    let pane = match kind.as_str() {
        "camera" => "Privacy_Camera",
        "microphone" => "Privacy_Microphone",
        "accessibility" => "Privacy_Accessibility",
        _ => "Privacy_ScreenCapture",
    };
    let _ = std::process::Command::new("open")
        .arg(format!("x-apple.systempreferences:com.apple.preference.security?{pane}"))
        .spawn();
}

#[tauri::command]
pub fn restart_app(app: AppHandle) {
    app.restart();
}
