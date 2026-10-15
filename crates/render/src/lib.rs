//! ScreenCraft compositor. `scene` turns a project + render track into a `Scene` for one
//! output frame; `gpu::Compositor` draws it with wgpu into a texture that the editor shows and
//! the exporter reads back.

pub mod cursors;
pub mod gpu;
pub mod scene;
pub mod wallpapers;

pub use gpu::Compositor;
pub use scene::{build_scene, content_aspect, content_rect, output_size, plain_scene, BackgroundKind, CursorDraw, Ripple, Scene, ASPECTS};
