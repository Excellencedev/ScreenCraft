//! `cargo run --release -p screencraft-render --example sample_png -- out.png [wallpaper]`
//! Renders one styled frame of a synthetic recording so the look can be checked by eye.
use screencraft_core::input::InputLog;
use screencraft_core::project::{ClickEffect, Media, Project, Rect};
use screencraft_core::track::compute_track;
use screencraft_encode::color::{to_i420, PixelOrder};
use screencraft_render::{build_scene, output_size, Compositor};

fn main() {
    let out_path = std::env::args().nth(1).expect("out.png");
    let wp = std::env::args().nth(2).unwrap_or_else(|| "sunset".into());
    let (sw, sh) = (1920u32, 1080u32);
    // A fake "app": title bar, sidebar, cards.
    let mut px = vec![255u8; (sw * sh * 4) as usize];
    for y in 0..sh {
        for x in 0..sw {
            let o = ((y * sw + x) * 4) as usize;
            let c: [u8; 3] = if y < 48 {
                [235, 235, 238]
            } else if x < 260 {
                [245, 246, 248]
            } else if (x / 300 + y / 220) % 2 == 0 && x > 300 && y > 120 && (x % 300) < 260 && (y % 220) < 180 {
                [255, 255, 255]
            } else {
                [250, 250, 252]
            };
            px[o..o + 3].copy_from_slice(&c);
            px[o + 3] = 255;
        }
    }
    let src = to_i420(&px, sw, sh, (sw * 4) as usize, PixelOrder::Rgba, 4);

    let mut p = Project::from_recording("sample", Media { width: sw, height: sh, duration_ms: 2000.0, bounds: Rect { x: 0.0, y: 0.0, width: 1920.0, height: 1080.0 }, ..Default::default() });
    p.config.styles.background.wallpaper = wp;
    p.config.styles.padding_ratio = 0.06;
    p.config.styles.corner_radius = 22.0;
    p.config.cursor.click_effect = ClickEffect::Ripple;
    p.config.animations.default_zoom = 1.0;
    let mut log = InputLog::default();
    log.bounds = Rect { x: 0.0, y: 0.0, width: 1920.0, height: 1080.0 };
    log.moves.push(screencraft_core::input::MoveEvent { t: 0.0, x: 900.0, y: 500.0, cursor: Default::default() });
    log.moves.push(screencraft_core::input::MoveEvent { t: 1900.0, x: 900.0, y: 500.0, cursor: Default::default() });
    let track = compute_track(&p, &log, 30.0);
    let out = output_size(&p, 1080);
    let mut scene = build_scene(&p, &track, 10, out);
    scene.highlight_radius_px = 0.0;
    let mut c = Compositor::headless().expect("gpu");
    c.upload(&src);
    c.render(&scene);
    let rgba = c.read_rgba().unwrap();
    let mut pm = tiny_skia::Pixmap::new(out.0, out.1).unwrap();
    for (d, s) in pm.data_mut().chunks_exact_mut(4).zip(rgba.chunks_exact(4)) {
        d.copy_from_slice(s); // opaque, so premultiplied == straight
    }
    pm.save_png(&out_path).unwrap();
    println!("wrote {out_path} ({}x{})", out.0, out.1);
}
