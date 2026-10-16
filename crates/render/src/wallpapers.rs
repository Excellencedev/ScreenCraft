//! Procedural wallpapers: a base gradient and three soft colour blobs. No image assets ship.

pub struct Wallpaper {
    pub name: &'static str,
    pub base: [&'static str; 2],
    /// (colour, x, y, radius as a fraction of the longer side)
    pub blobs: [(&'static str, f32, f32, f32); 3],
}

pub const WALLPAPERS: &[Wallpaper] = &[
    Wallpaper { name: "sunset", base: ["#2b1055", "#7597de"], blobs: [("#ff7e5f", 0.2, 0.85, 0.7), ("#feb47b", 0.75, 0.8, 0.6), ("#ff4e8a", 0.5, 0.2, 0.5)] },
    Wallpaper { name: "aurora", base: ["#0b1026", "#1d1b4b"], blobs: [("#00e0b0", 0.2, 0.25, 0.55), ("#7a5cff", 0.8, 0.3, 0.6), ("#ff6ad5", 0.55, 0.9, 0.5)] },
    Wallpaper { name: "ocean", base: ["#021b3a", "#0a4a8a"], blobs: [("#00c6ff", 0.15, 0.2, 0.6), ("#0072ff", 0.85, 0.7, 0.7), ("#4be1ec", 0.5, 0.95, 0.4)] },
    Wallpaper { name: "forest", base: ["#07201a", "#1c4a38"], blobs: [("#6fe3a0", 0.25, 0.3, 0.5), ("#2f9e6b", 0.8, 0.75, 0.7), ("#c9f27a", 0.6, 0.1, 0.35)] },
    Wallpaper { name: "midnight", base: ["#05060d", "#15172e"], blobs: [("#3a3fbf", 0.2, 0.8, 0.6), ("#8a2be2", 0.85, 0.2, 0.5), ("#1f6feb", 0.6, 0.6, 0.4)] },
    Wallpaper { name: "candy", base: ["#ff9a9e", "#fad0c4"], blobs: [("#a18cd1", 0.2, 0.2, 0.7), ("#fbc2eb", 0.8, 0.8, 0.7), ("#ffd1ff", 0.6, 0.3, 0.5)] },
    Wallpaper { name: "dune", base: ["#4a2c1a", "#c98f5a"], blobs: [("#ffd29d", 0.8, 0.2, 0.6), ("#8c4a2f", 0.2, 0.8, 0.7), ("#ffb86b", 0.55, 0.6, 0.4)] },
    Wallpaper { name: "slate", base: ["#1b1f2a", "#3a4254"], blobs: [("#6b7a99", 0.2, 0.3, 0.6), ("#2a3140", 0.8, 0.8, 0.7), ("#9fb0d0", 0.7, 0.15, 0.35)] },
    Wallpaper { name: "lavender", base: ["#2d1b69", "#b49cff"], blobs: [("#f093fb", 0.8, 0.25, 0.6), ("#4facfe", 0.2, 0.75, 0.6), ("#ffffff", 0.55, 0.5, 0.25)] },
    Wallpaper { name: "ember", base: ["#1a0505", "#6a1b0a"], blobs: [("#ff512f", 0.25, 0.75, 0.7), ("#f09819", 0.8, 0.35, 0.55), ("#dd2476", 0.5, 0.1, 0.5)] },
    Wallpaper { name: "mint", base: ["#0f3d3e", "#7fe0c3"], blobs: [("#c1ffd7", 0.75, 0.2, 0.55), ("#00b09b", 0.2, 0.8, 0.7), ("#96c93d", 0.6, 0.6, 0.35)] },
    Wallpaper { name: "rose", base: ["#3a0f2a", "#e8668f"], blobs: [("#ffb3c6", 0.8, 0.3, 0.6), ("#8e2de2", 0.15, 0.7, 0.6), ("#ff758f", 0.5, 0.95, 0.5)] },
];

pub fn wallpaper(name: &str) -> &'static Wallpaper {
    WALLPAPERS.iter().find(|w| w.name == name).unwrap_or(&WALLPAPERS[1])
}

/// "#rgb", "#rrggbb" or "#rrggbbaa" -> linear-ish sRGB floats in 0..1 (no gamma conversion:
/// compositing happens in the same space the web editor used).
pub fn parse_color(s: &str) -> [f32; 4] {
    let h = s.trim().trim_start_matches('#');
    let byte = |i: usize| u8::from_str_radix(h.get(i..i + 2).unwrap_or("00"), 16).unwrap_or(0) as f32 / 255.0;
    match h.len() {
        3 => {
            let n = |i: usize| u8::from_str_radix(&h[i..i + 1].repeat(2), 16).unwrap_or(0) as f32 / 255.0;
            [n(0), n(1), n(2), 1.0]
        }
        6 => [byte(0), byte(2), byte(4), 1.0],
        8 => [byte(0), byte(2), byte(4), byte(6)],
        _ => [0.0, 0.0, 0.0, 1.0],
    }
}

/// A small RGBA preview of wallpaper `name` (what the picker shows), drawn on the CPU the same way
/// the web editor drew its swatches: a diagonal base gradient plus three soft radial blobs.
pub fn thumbnail(name: &str, w: usize, h: usize) -> Vec<u8> {
    let wp = wallpaper(name);
    let (c0, c1) = (parse_color(wp.base[0]), parse_color(wp.base[1]));
    let blobs: Vec<([f32; 4], f32, f32, f32)> = wp.blobs.iter().map(|b| (parse_color(b.0), b.1, b.2, b.3)).collect();
    let (wf, hf) = (w as f32, h as f32);
    let m = wf.max(hf);
    let mut out = vec![0u8; w * h * 4];
    for y in 0..h {
        for x in 0..w {
            let (px, py) = (x as f32 + 0.5, y as f32 + 0.5);
            let t = ((px * wf + py * hf) / (wf * wf + hf * hf)).clamp(0.0, 1.0);
            let mut c = [0f32; 3];
            for k in 0..3 {
                c[k] = c0[k] + (c1[k] - c0[k]) * t;
            }
            for (bc, bx, by, br) in &blobs {
                let d = ((px - bx * wf).powi(2) + (py - by * hf).powi(2)).sqrt() / (br * m);
                let a = (1.0 - d).clamp(0.0, 1.0) * (230.0 / 255.0);
                for k in 0..3 {
                    c[k] = c[k] * (1.0 - a) + bc[k] * a;
                }
            }
            let o = (y * w + x) * 4;
            out[o] = (c[0] * 255.0) as u8;
            out[o + 1] = (c[1] * 255.0) as u8;
            out[o + 2] = (c[2] * 255.0) as u8;
            out[o + 3] = 255;
        }
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn colors_parse() {
        assert_eq!(parse_color("#ff0000"), [1.0, 0.0, 0.0, 1.0]);
        assert_eq!(parse_color("#0f0"), [0.0, 1.0, 0.0, 1.0]);
        assert!((parse_color("#00000080")[3] - 0.5).abs() < 0.01);
        assert_eq!(wallpaper("nope").name, "aurora");
    }
}
