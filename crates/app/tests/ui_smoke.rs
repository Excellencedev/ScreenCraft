//! Renders the real UI headlessly and checks it draws something sensible.

use screencraft::{app::App, headless::Headless};
use screencraft_core::project::{Media, Project, Rect};
use screencraft_encode::color::{to_i420, PixelOrder};
use screencraft_encode::{
    Container, Effort, ExportSettings, Exporter, FileSink, HwPreference, Sink, VideoCodec, VideoConfig,
};
use screencraft_engine::Package;

fn headless() -> Option<Headless> {
    match Headless::new([1280, 800], 1.0) {
        Ok(h) => Some(h),
        Err(e) => {
            eprintln!("skipping UI test: {e}");
            None
        }
    }
}

fn distinct_colours(px: &[u8]) -> usize {
    let mut set = std::collections::HashSet::new();
    for p in px.chunks_exact(4).step_by(37) {
        set.insert([p[0] >> 3, p[1] >> 3, p[2] >> 3]);
    }
    set.len()
}

fn make_package(dir: &std::path::Path) -> Package {
    let package = Package::open(dir.join("ui.screencraft"));
    std::fs::create_dir_all(&package.dir).unwrap();
    let (w, h, fps, frames) = (320u32, 180u32, 30u32, 30u32);
    let mut video = VideoConfig::new(VideoCodec::H264, w, h, fps);
    video.hardware = HwPreference::Software;
    video.effort = Effort::Fast;
    let mut ex = Exporter::new(ExportSettings { container: Container::Mp4, video, audio: vec![] }).unwrap();
    let mut sink = FileSink::create(&package.dir.join("screen.mp4")).unwrap();
    for f in 0..frames {
        let mut px = vec![255u8; (w * h * 4) as usize];
        for y in 0..h {
            for x in 0..w {
                let o = ((y * w + x) * 4) as usize;
                px[o] = (x * 255 / w) as u8;
                px[o + 1] = (y * 255 / h) as u8;
                px[o + 2] = (f * 8) as u8;
            }
        }
        ex.push_video(&to_i420(&px, w, h, (w * 4) as usize, PixelOrder::Rgba, 1), (f as u64 * 1_000_000 / fps as u64) as i64)
            .unwrap();
        while let Some(op) = ex.poll() {
            sink.apply(&op).unwrap();
        }
    }
    let s = ex.finish().unwrap();
    while let Some(op) = ex.poll() {
        sink.apply(&op).unwrap();
    }
    let media = Media {
        duration_ms: s.duration_us as f64 / 1000.0,
        width: w,
        height: h,
        bounds: Rect { x: 0.0, y: 0.0, width: w as f64, height: h as f64 },
        ..Default::default()
    };
    package.save_project(&Project::from_recording("UI test", media)).unwrap();
    std::fs::write(package.dir.join("input.json"), b"{}").unwrap();
    package
}

#[test]
fn home_screen_renders() {
    let Some(mut head) = headless() else { return };
    let mut app = App::new_headless(&head.ctx, head.state.clone());
    let px = head.frame(|ui| app.draw(ui));
    assert_eq!(px.len(), 1280 * 800 * 4);
    assert!(distinct_colours(&px) > 5, "home screen looks blank");
}

#[test]
fn editor_renders_the_recording_through_the_compositor() {
    let Some(mut head) = headless() else { return };
    let dir = std::env::temp_dir().join(format!("sc-ui-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(&dir).unwrap();
    let package = make_package(&dir);

    let mut app = App::new_headless(&head.ctx, head.state.clone());
    app.open_package(&head.ctx, package.dir.clone());
    app.dev_seek(500.0);
    // Give the decode worker time to deliver, drawing frames meanwhile.
    let mut px = Vec::new();
    for _ in 0..40 {
        px = head.frame(|ui| app.draw(ui));
        std::thread::sleep(std::time::Duration::from_millis(25));
    }
    // The preview shows a colourful gradient, so many distinct colours appear.
    assert!(distinct_colours(&px) > 40, "preview did not render ({} colours)", distinct_colours(&px));
    let _ = std::fs::remove_dir_all(&dir);
}
