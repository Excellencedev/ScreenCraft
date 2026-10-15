use cpal::traits::{DeviceTrait, HostTrait};

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum DeviceKind {
    Microphone,
    /// What the computer is playing: WASAPI loopback or a PulseAudio/PipeWire monitor.
    /// (macOS system audio comes from ScreenCaptureKit instead.)
    SystemLoopback,
}

#[derive(Clone, Debug)]
pub struct DeviceInfo {
    /// Stable id to pass back to `AudioCapture::start`.
    pub id: String,
    pub name: String,
    pub kind: DeviceKind,
    pub is_default: bool,
}

pub fn list_devices() -> Vec<DeviceInfo> {
    let host = cpal::default_host();
    let mut out = Vec::new();
    let default_in = host.default_input_device().and_then(|d| d.id().ok()).map(|i| i.to_string());
    let default_out = host.default_output_device().and_then(|d| d.id().ok()).map(|i| i.to_string());
    if let Ok(devs) = host.input_devices() {
        for d in devs {
            let (Ok(id), Ok(desc)) = (d.id(), d.description()) else { continue };
            let id = id.to_string();
            let name = desc.name().to_string();
            let monitor = name.to_lowercase().contains("monitor");
            out.push(DeviceInfo {
                is_default: default_in.as_deref() == Some(&id),
                kind: if monitor { DeviceKind::SystemLoopback } else { DeviceKind::Microphone },
                id,
                name,
            });
        }
    }
    #[cfg(windows)]
    if let Ok(devs) = host.output_devices() {
        for d in devs {
            let (Ok(id), Ok(desc)) = (d.id(), d.description()) else { continue };
            let id = id.to_string();
            out.push(DeviceInfo {
                is_default: default_out.as_deref() == Some(&id),
                kind: DeviceKind::SystemLoopback,
                name: format!("{} (system audio)", desc.name()),
                id,
            });
        }
    }
    let _ = default_out;
    out
}
