//! Audio capture. Microphone and system loopback are read on their own device clocks, stamped
//! against one shared monotonic clock, and re-timed to a gap-free 48 kHz stereo stream per
//! source. The alignment logic (`align`) is pure and has no device dependencies.

pub mod align;
pub mod capture;
pub mod clock;
pub mod decode;
pub mod devices;
pub mod mix;

pub use align::{AlignStats, StreamAligner, OUT_CHANNELS, OUT_RATE};
pub use capture::{AudioBlock, AudioCapture, CaptureConfig, Source};
pub use clock::Clock;
pub use devices::{list_devices, DeviceInfo, DeviceKind};

pub use decode::{decode_file, probe_duration_ms};
