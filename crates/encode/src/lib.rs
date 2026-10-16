//! ScreenCraft encoding stack: color conversion, H.264 / AV1 / VP9 video, Opus audio, MP4 and
//! WebM writing and MP4 reading. Everything that produces bytes is sans-I/O: it returns
//! `WriteAt` operations and leaves files and threads to the caller.

pub mod av1;
pub mod codec;
pub mod color;
pub mod error;
pub mod h264;
#[cfg(target_os = "macos")]
pub mod hw_videotoolbox;
#[cfg(target_os = "windows")]
pub mod mf;
pub mod mp4;
pub mod mp4read;
pub mod opus;
pub mod pipeline;
pub mod sink;
pub mod types;
#[cfg(target_os = "macos")]
pub mod vt_decode;
#[cfg(feature = "vp9")]
pub mod vp9;
pub mod webm;

pub use codec::{available_video_codecs, create_video_encoder, has_hardware_encoder, VideoEncoder};
pub use error::{Error, Result};
pub use pipeline::{ExportSettings, ExportSummary, Exporter, ThreadedExporter};
pub use sink::{FileSink, MemorySink, Sink};
pub use types::*;
