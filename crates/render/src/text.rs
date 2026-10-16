//! Text for the CPU overlays: platform fonts, outlines turned into paths, word wrapping.

use std::sync::OnceLock;
use tiny_skia::{FillRule, LineCap, LineJoin, Path, PathBuilder, Pixmap, Stroke, Transform};
use skrifa::instance::{LocationRef, Size};
use skrifa::outline::{DrawSettings, OutlinePen};
use skrifa::{FontRef, GlyphId, MetadataProvider};

struct Fonts {
    /// (font bytes, face index, variable?)
    regular: Option<(Vec<u8>, u32)>,
    bold: Option<(Vec<u8>, u32)>,
}

fn read(path: &str) -> Option<Vec<u8>> {
    std::fs::read(path).ok()
}

fn load() -> Fonts {
    let pick = |paths: &[&str]| paths.iter().find_map(|p| read(p).map(|b| (b, 0u32)));
    if cfg!(target_os = "macos") {
        let f = pick(&["/System/Library/Fonts/SFNS.ttf", "/System/Library/Fonts/Helvetica.ttc"]);
        Fonts { regular: f.clone(), bold: f }
    } else if cfg!(target_os = "windows") {
        Fonts { regular: pick(&["C:\\Windows\\Fonts\\segoeui.ttf", "C:\\Windows\\Fonts\\arial.ttf"]), bold: pick(&["C:\\Windows\\Fonts\\segoeuib.ttf", "C:\\Windows\\Fonts\\arialbd.ttf"]) }
    } else {
        Fonts {
            regular: pick(&["/usr/share/fonts/truetype/dejavu/DejaVuSans.ttf", "/usr/share/fonts/TTF/DejaVuSans.ttf", "/usr/share/fonts/noto/NotoSans-Regular.ttf", "/usr/share/fonts/truetype/noto/NotoSans-Regular.ttf"]),
            bold: pick(&["/usr/share/fonts/truetype/dejavu/DejaVuSans-Bold.ttf", "/usr/share/fonts/TTF/DejaVuSans-Bold.ttf", "/usr/share/fonts/noto/NotoSans-Bold.ttf", "/usr/share/fonts/truetype/noto/NotoSans-Bold.ttf"]),
        }
    }
}

fn fonts() -> &'static Fonts {
    static F: OnceLock<Fonts> = OnceLock::new();
    F.get_or_init(load)
}

pub struct Font<'a> {
    font: FontRef<'a>,
    location: skrifa::instance::Location,
}

impl Font<'static> {
    /// The UI font at CSS-style `weight` (100..900). None when the machine has no usable font.
    pub fn at_weight(weight: f32) -> Option<Font<'static>> {
        let f = fonts();
        let src = if weight >= 600.0 { f.bold.as_ref().or(f.regular.as_ref()) } else { f.regular.as_ref() }?;
        let font = FontRef::from_index(&src.0, src.1).ok()?;
        let location = font.axes().location([("wght", weight.clamp(100.0, 900.0))]);
        Some(Font { font, location })
    }
}

struct Pen<'p> {
    pb: &'p mut PathBuilder,
    ox: f32,
    oy: f32,
}

impl OutlinePen for Pen<'_> {
    fn move_to(&mut self, x: f32, y: f32) {
        self.pb.move_to(self.ox + x, self.oy - y);
    }
    fn line_to(&mut self, x: f32, y: f32) {
        self.pb.line_to(self.ox + x, self.oy - y);
    }
    fn quad_to(&mut self, x1: f32, y1: f32, x: f32, y: f32) {
        self.pb.quad_to(self.ox + x1, self.oy - y1, self.ox + x, self.oy - y);
    }
    fn curve_to(&mut self, x1: f32, y1: f32, x2: f32, y2: f32, x: f32, y: f32) {
        self.pb.cubic_to(self.ox + x1, self.oy - y1, self.ox + x2, self.oy - y2, self.ox + x, self.oy - y);
    }
    fn close(&mut self) {
        self.pb.close();
    }
}

impl Font<'_> {
    fn gid(&self, ch: char) -> Option<GlyphId> {
        self.font.charmap().map(ch).or_else(|| self.font.charmap().map('?'))
    }

    pub fn advance(&self, ch: char, size: f32) -> f32 {
        let loc = LocationRef::from(&self.location);
        self.gid(ch).and_then(|g| self.font.glyph_metrics(Size::new(size), loc).advance_width(g)).unwrap_or(size * 0.5)
    }

    pub fn width(&self, text: &str, size: f32) -> f32 {
        text.chars().map(|c| self.advance(c, size)).sum()
    }

    /// The text as one path with its left edge at `x` and the baseline at `y`.
    pub fn path(&self, text: &str, x: f32, baseline: f32, size: f32) -> Option<Path> {
        let mut pb = PathBuilder::new();
        let mut pen = x;
        let outlines = self.font.outline_glyphs();
        let loc = LocationRef::from(&self.location);
        for ch in text.chars() {
            if let Some(g) = self.gid(ch).and_then(|g| outlines.get(g)) {
                let mut p = Pen { pb: &mut pb, ox: pen, oy: baseline };
                let _ = g.draw(DrawSettings::unhinted(Size::new(size), loc), &mut p);
            }
            pen += self.advance(ch, size);
        }
        pb.finish()
    }

    /// Ascent / descent in px for `size`.
    pub fn metrics(&self, size: f32) -> (f32, f32) {
        let m = self.font.metrics(Size::new(size), LocationRef::from(&self.location));
        (m.ascent, -m.descent)
    }

    /// Break `text` into lines no wider than `max_w`.
    pub fn wrap(&self, text: &str, size: f32, max_w: f32) -> Vec<String> {
        let mut out = Vec::new();
        for para in text.split('\n') {
            let mut line = String::new();
            // split keeping whitespace runs, like /(\s+)/
            let mut tokens: Vec<String> = Vec::new();
            let mut cur = String::new();
            let mut ws = None;
            for ch in para.chars() {
                let is_ws = ch.is_whitespace();
                if ws == Some(is_ws) || ws.is_none() {
                    cur.push(ch);
                } else {
                    tokens.push(std::mem::take(&mut cur));
                    cur.push(ch);
                }
                ws = Some(is_ws);
            }
            if !cur.is_empty() {
                tokens.push(cur);
            }
            for word in tokens {
                let test = format!("{line}{word}");
                if !line.is_empty() && self.width(&test, size) > max_w {
                    out.push(line.trim_end().to_string());
                    line = word.trim_start().to_string();
                } else {
                    line = test;
                }
            }
            out.push(line.trim_end().to_string());
        }
        out
    }
}

/// Draw `path` filled, optionally with a stroke underneath (a text outline).
pub fn fill_text_path(px: &mut Pixmap, path: &Path, fill: [f32; 4], outline: Option<([f32; 4], f32)>, t: Transform) {
    if let Some((c, w)) = outline {
        let mut s = Stroke::default();
        s.width = w;
        s.line_join = LineJoin::Round;
        s.line_cap = LineCap::Round;
        s.miter_limit = 2.0;
        px.stroke_path(path, &crate::skia::paint(c), &s, t, None);
    }
    px.fill_path(path, &crate::skia::paint(fill), FillRule::Winding, t, None);
}
