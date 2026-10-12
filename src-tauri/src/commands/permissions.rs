use serde::Serialize;
use tauri::AppHandle;

#[link(name = "CoreGraphics", kind = "framework")]
extern "C" {
    fn CGPreflightScreenCaptureAccess() -> bool;
    fn CGRequestScreenCaptureAccess() -> bool;
    fn CGPreflightListenEventAccess() -> bool;
    fn CGRequestListenEventAccess() -> bool;
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

pub fn input_monitoring_granted() -> bool {
    unsafe { CGPreflightListenEventAccess() }
}

/// Everything required before the recorder is usable.
pub fn all_required_granted() -> bool {
    screen_granted() && crate::ax::trusted() && input_monitoring_granted()
}

#[derive(Serialize)]
pub struct PermissionStatus {
    screen: bool,
    camera: String,
    microphone: String,
    accessibility: bool,
    input_monitoring: bool,
    /// Granted *and* actually usable in this process (otherwise a restart is needed).
    screen_ready: bool,
    input_ready: bool,
}

fn tap_ready() -> bool {
    use core_graphics::event::{CGEventTap, CGEventTapLocation, CGEventTapOptions, CGEventTapPlacement, CGEventType, CallbackResult};
    CGEventTap::new(
        CGEventTapLocation::Session,
        CGEventTapPlacement::HeadInsertEventTap,
        CGEventTapOptions::ListenOnly,
        vec![CGEventType::KeyDown],
        |_, _, _| CallbackResult::Keep,
    )
    .is_ok()
}

#[tauri::command]
pub fn permission_status() -> PermissionStatus {
    let screen = screen_granted();
    let input = input_monitoring_granted();
    PermissionStatus {
        screen_ready: screen && screencapturekit::prelude::SCShareableContent::get().is_ok(),
        input_ready: input && tap_ready(),
        screen,
        camera: av_status("vide"),
        microphone: av_status("soun"),
        accessibility: crate::ax::trusted(),
        input_monitoring: input,
    }
}

/// Triggers the system prompt the first time; afterwards the user must flip the
/// switch in System Settings.
#[tauri::command]
pub fn request_screen_access() -> bool {
    unsafe { CGRequestScreenCaptureAccess() }
}

#[tauri::command]
pub fn request_input_monitoring() -> bool {
    unsafe { CGRequestListenEventAccess() }
}

#[tauri::command]
pub fn open_privacy_pane(kind: String) {
    let pane = match kind.as_str() {
        "camera" => "Privacy_Camera",
        "microphone" => "Privacy_Microphone",
        "accessibility" => "Privacy_Accessibility",
        "input" => "Privacy_ListenEvent",
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
