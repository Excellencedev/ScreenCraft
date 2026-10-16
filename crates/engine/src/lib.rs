//! ScreenCraft engine: everything between the platform APIs and the editor UI that is not
//! pixels-on-screen. A `Recording` drives capture, audio, input logging and encoding into a
//! `.screencraft` package; `Package` reads and writes that folder; `MediaSource` decodes it
//! back for the editor and the exporter.

pub mod audio_mix;
pub mod compress;
pub mod export;
pub mod gif;
pub mod package;
pub mod recorder;
pub mod source;

pub use gif::{export_gif, GifReport};
pub use export::{build_soundtrack, export_project, ExportOptions, ExportReport};
pub use package::Package;
pub use recorder::{Finished, LiveStats, RecordOptions, Recording};
pub use source::{AudioSource, FrameTable, VideoSource};
