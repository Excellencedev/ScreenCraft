//! Everything the compositor needs for one output frame, derived on the CPU from the project
//! and the render track. Pure and unit-tested; no GPU types in here.

use crate::wallpapers::{parse_color, wallpaper};
use screencraft_core::input::CursorKind;
use screencraft_core::project::{ClickEffect, Project, Rect};
use screencraft_core::track::RenderTrack;

#[derive(Clone, Copy, Debug, PartialEq)]
pub enum BackgroundKind {
    Solid,
    Gradient,
    Wallpaper,
}

#[derive(Clone, Debug, PartialEq)]
pub struct CursorDraw {
    /// Normalised to the full source frame.
    pub pos: [f32; 2],
    pub size_px: f32,
    pub rotation: f32,
    pub alpha: f32,
    pub scale: f32,
    pub kind: u32,
    pub style: u32,
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Ripple {
    pub pos: [f32; 2],
    /// 0..1 through the effect.
    pub progress: f32,
    pub ring: bool,
}

#[derive(Clone, Debug, PartialEq)]
pub struct Scene {
    pub out: (u32, u32),
    pub source: (u32, u32),
    /// Screen rectangle in output px: x, y, w, h.
    pub content: [f32; 4],
    pub radius_px: f32,
    pub shadow_blur_px: f32,
    pub shadow_intensity: f32,
    pub shadow_offset: [f32; 2],
    /// Visible crop of the source, normalised: x, y, w, h.
    pub crop: [f32; 4],
    /// Camera: centre (normalised to the full source) and zoom.
    pub camera: [f32; 3],
    pub bg_kind: BackgroundKind,
    pub bg_angle_rad: f32,
    pub bg_drift: bool,
    pub bg0: [f32; 4],
    pub bg1: [f32; 4],
    pub blobs: [[f32; 3]; 3],
    pub blob_colors: [[f32; 4]; 3],
    pub time_s: f32,
    pub cursor: Option<CursorDraw>,
    pub highlight_radius_px: f32,
    pub highlight_alpha: f32,
    pub highlight_color: [f32; 4],
    pub ripples: Vec<Ripple>,
    pub ripple_color: [f32; 4],
    pub points_to_px: f32,
    pub ripple_line_px: f32,
}

pub const ASPECTS: &[(&str, f64)] =
    &[("16:9", 16.0 / 9.0), ("9:16", 9.0 / 16.0), ("1:1", 1.0), ("4:3", 4.0 / 3.0), ("3:4", 3.0 / 4.0), ("4:5", 4.0 / 5.0), ("21:9", 21.0 / 9.0)];

fn even(n: f64) -> u32 {
    ((n / 2.0).round() as i64 * 2).max(2) as u32
}

/// Aspect ratio of the visible recording after cropping.
pub fn content_aspect(p: &Project) -> f64 {
    let c = p.config.crop;
    (c.width * p.media.width as f64) / (c.height * p.media.height as f64).max(1.0)
}

/// Output pixel size when the shorter side is `resolution` px.
pub fn output_size(p: &Project, resolution: u32) -> (u32, u32) {
    let a = if p.config.output.aspect_ratio == "auto" {
        content_aspect(p)
    } else {
        ASPECTS.iter().find(|(n, _)| *n == p.config.output.aspect_ratio).map(|x| x.1).unwrap_or(16.0 / 9.0)
    };
    let r = resolution as f64;
    if a >= 1.0 {
        (even(r * a), even(r))
    } else {
        (even(r), even(r / a))
    }
}

/// The screen rectangle inside an output of `w` x `h`: padded, aspect-correct, centred.
pub fn content_rect(p: &Project, w: f64, h: f64) -> [f32; 4] {
    let pad = p.config.styles.padding_ratio * w.min(h);
    let (aw, ah) = (w - 2.0 * pad, h - 2.0 * pad);
    let a = content_aspect(p);
    let ch = ah.min(aw / a);
    let cw = ch * a;
    [((w - cw) / 2.0) as f32, ((h - ch) / 2.0) as f32, cw as f32, ch as f32]
}

fn kind_index(k: u8) -> u32 {
    (k as usize).min(CursorKind::ALL.len() - 1) as u32
}

/// Build the scene for output frame `i` of `track`.
pub fn build_scene(p: &Project, track: &RenderTrack, i: usize, out: (u32, u32)) -> Scene {
    let i = i.min(track.count.saturating_sub(1));
    let (w, h) = (out.0 as f64, out.1 as f64);
    let s = (h / 1080.0) as f32;
    let st = &p.config.styles;
    let crop: Rect = p.config.crop;
    let content = content_rect(p, w, h);

    let bg = &st.background;
    let wp = wallpaper(&bg.wallpaper);
    let (bg_kind, bg0, bg1) = match bg.kind.as_str() {
        "color" => (BackgroundKind::Solid, parse_color(&bg.color), parse_color(&bg.color)),
        "gradient" => (BackgroundKind::Gradient, parse_color(&bg.gradient_from), parse_color(&bg.gradient_to)),
        _ => (BackgroundKind::Wallpaper, parse_color(wp.base[0]), parse_color(wp.base[1])),
    };
    let mut blobs = [[0f32; 3]; 3];
    let mut blob_colors = [[0f32; 4]; 3];
    for (k, b) in wp.blobs.iter().enumerate() {
        blobs[k] = [b.1, b.2, b.3];
        blob_colors[k] = parse_color(b.0);
    }

    let t_out_ms = i as f64 * 1000.0 / track.fps.max(1.0);
    let cfg = &p.config.cursor;

    // Points (recording units) to output px, through the camera.
    let rec_h_pts = (crop.height * p.media.bounds.height).max(1.0);
    let zoom = track.zoom.get(i).copied().unwrap_or(1.0);
    let points_to_px = (content[3] as f64 / rec_h_pts * zoom) as f32;

    let cursor = if cfg.hide || track.cursor_alpha.get(i).copied().unwrap_or(0.0) < 0.01 {
        None
    } else {
        Some(CursorDraw {
            pos: [track.mx[i] as f32, track.my[i] as f32],
            size_px: (cfg.size as f32) * points_to_px,
            rotation: track.cursor_rotation.get(i).copied().unwrap_or(0.0) as f32,
            alpha: track.cursor_alpha[i] as f32,
            scale: track.cursor_scale.get(i).copied().unwrap_or(1.0) as f32,
            kind: kind_index(track.cursor_kind.get(i).copied().unwrap_or(0)),
            style: crate::cursors::style_index(&cfg.style),
        })
    };

    let mut ripples = Vec::new();
    if cfg.click_effect != ClickEffect::None {
        for c in &track.clicks {
            let age = t_out_ms - c.t_out_ms;
            if (0.0..=650.0).contains(&age) {
                ripples.push(Ripple { pos: [c.x as f32, c.y as f32], progress: (age / 650.0) as f32, ring: cfg.click_effect == ClickEffect::Ripple });
                if ripples.len() == 4 {
                    break;
                }
            }
        }
    }

    let sh = &st.shadow;
    let ang = (sh.angle as f32).to_radians();
    let dist = sh.distance as f32 * s;
    let shadow_offset = if sh.directional { [ang.cos() * dist * 0.5, ang.sin() * dist * 0.5] } else { [0.0, dist * 0.3] };

    Scene {
        out,
        source: (p.media.width, p.media.height),
        content,
        radius_px: st.corner_radius as f32 * s,
        shadow_blur_px: sh.blur as f32 * 2.0 * s,
        shadow_intensity: sh.intensity as f32,
        shadow_offset,
        crop: [crop.x as f32, crop.y as f32, crop.width as f32, crop.height as f32],
        camera: [track.cx.get(i).copied().unwrap_or(0.5) as f32, track.cy.get(i).copied().unwrap_or(0.5) as f32, zoom as f32],
        bg_kind,
        bg_angle_rad: (bg.gradient_angle as f32).to_radians(),
        bg_drift: bg.kind == "wallpaper" && bg.motion != "none",
        bg0,
        bg1,
        blobs,
        blob_colors,
        time_s: (t_out_ms / 1000.0) as f32,
        cursor,
        highlight_radius_px: if cfg.highlight { (cfg.highlight_size as f32 * points_to_px).max(2.0) } else { 0.0 },
        highlight_alpha: cfg.highlight_opacity as f32,
        highlight_color: parse_color(&cfg.highlight_color),
        ripples,
        ripple_color: parse_color(&cfg.click_effect_color),
        points_to_px,
        ripple_line_px: (3.0 * s * points_to_px / 1.5).max(1.5),
    }
}

/// A scene with nothing in it but the screen, for tests and thumbnails.
pub fn plain_scene(out: (u32, u32), source: (u32, u32)) -> Scene {
    let a = source.0 as f32 / source.1 as f32;
    let (w, h) = (out.0 as f32, out.1 as f32);
    let ch = h.min(w / a);
    let cw = ch * a;
    Scene {
        out,
        source,
        content: [(w - cw) / 2.0, (h - ch) / 2.0, cw, ch],
        radius_px: 0.0,
        shadow_blur_px: 1.0,
        shadow_intensity: 0.0,
        shadow_offset: [0.0, 0.0],
        crop: [0.0, 0.0, 1.0, 1.0],
        camera: [0.5, 0.5, 1.0],
        bg_kind: BackgroundKind::Solid,
        bg_angle_rad: 0.0,
        bg_drift: false,
        bg0: [0.1, 0.1, 0.1, 1.0],
        bg1: [0.1, 0.1, 0.1, 1.0],
        blobs: [[0.0; 3]; 3],
        blob_colors: [[0.0; 4]; 3],
        time_s: 0.0,
        cursor: None,
        highlight_radius_px: 0.0,
        highlight_alpha: 0.0,
        highlight_color: [1.0, 1.0, 1.0, 1.0],
        ripples: Vec::new(),
        ripple_color: [1.0, 1.0, 1.0, 1.0],
        points_to_px: 1.0,
        ripple_line_px: 2.0,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use screencraft_core::project::Media;

    fn project() -> Project {
        Project::from_recording("t", Media { width: 1920, height: 1080, duration_ms: 2000.0, ..Default::default() })
    }

    #[test]
    fn output_sizes_follow_aspect() {
        let mut p = project();
        assert_eq!(output_size(&p, 1080), (1920, 1080));
        p.config.output.aspect_ratio = "9:16".into();
        assert_eq!(output_size(&p, 1080), (1080, 1920));
        p.config.output.aspect_ratio = "1:1".into();
        assert_eq!(output_size(&p, 720), (720, 720));
    }

    #[test]
    fn content_is_centred_and_padded() {
        let p = project();
        let r = content_rect(&p, 1920.0, 1080.0);
        let pad = 0.02 * 1080.0;
        assert!((r[1] - pad as f32).abs() < 0.01);
        assert!((r[0] + r[2] / 2.0 - 960.0).abs() < 0.01);
        assert!((r[2] / r[3] - 16.0 / 9.0).abs() < 1e-3);
        // A portrait output letterboxes a landscape recording.
        let r2 = content_rect(&p, 1080.0, 1920.0);
        assert!(r2[1] > 100.0);
    }
}
