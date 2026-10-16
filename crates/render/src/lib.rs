//! ScreenCraft compositor. `scene` turns a project + render track into a `Scene` for one
//! output frame; `gpu::Compositor` draws it with wgpu into a texture that the editor shows and
//! the exporter reads back.

pub mod assets;
pub mod cursors;
pub mod devices;
pub mod layout;
pub mod overlay;
pub mod skia;
pub mod text;
pub mod gpu;
pub mod scene;
pub mod wallpapers;

pub use gpu::{Compositor, Slot};
pub use scene::{build_scene, CameraDraw, content_aspect, content_rect, output_size, plain_scene, BackgroundKind, CursorDraw, Ripple, Scene, ASPECTS};
