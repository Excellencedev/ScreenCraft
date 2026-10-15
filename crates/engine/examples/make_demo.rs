//! `cargo run --release -p screencraft-engine --example make_demo -- <dir>`
//! Writes a synthetic recording package (a made-up app UI, a cursor path with clicks, a tone)
//! so the editor and exporter can be tried without a display or permissions.
use screencraft_core::input::{Button, ClickEvent, CursorKind, InputLog, KeyEvent, MoveEvent};
use screencraft_core::project::{Media, Project, Rect};
use screencraft_encode::color::{to_i420, PixelOrder};
use screencraft_encode::{AudioConfig, Container, Effort, ExportSettings, Exporter, FileSink, HwPreference, Sink, VideoCodec, VideoConfig};
use screencraft_engine::Package;

const W: u32 = 1280;
const H: u32 = 720;
const FPS: u32 = 30;
const SECS: u32 = 12;

fn cursor_at(t: f64) -> (f64, f64) {
    // Wander between a few targets, easing between them.
    let pts = [(200.0, 160.0), (640.0, 300.0), (1000.0, 210.0), (330.0, 560.0), (900.0, 560.0), (640.0, 360.0)];
    let seg = (t / 2.0).floor() as usize % pts.len();
    let a = pts[seg];
    let b = pts[(seg + 1) % pts.len()];
    let k = (t / 2.0).fract();
    let e = k * k * (3.0 - 2.0 * k);
    (a.0 + (b.0 - a.0) * e, a.1 + (b.1 - a.1) * e)
}

fn main() {
    let dir = std::path::PathBuf::from(std::env::args().nth(1).expect("output dir"));
    let package = Package::open(dir.join("Demo.screencraft"));
    let _ = std::fs::remove_dir_all(&package.dir);
    std::fs::create_dir_all(&package.dir).unwrap();

    let mut video = VideoConfig::new(VideoCodec::H264, W, H, FPS);
    video.bitrate_kbps = 8000;
    video.effort = Effort::Fast;
    video.hardware = HwPreference::Software;
    video.keyint = 15;
    let mut ex = Exporter::new(ExportSettings { container: Container::Mp4, video, audio: vec![AudioConfig::default()] }).unwrap();
    let mut sink = FileSink::create(&package.dir.join("screen.mp4")).unwrap();

    let mut rgba = vec![0u8; (W * H * 4) as usize];
    let samples_per_frame = (48_000 / FPS) as usize;
    for f in 0..FPS * SECS {
        let t = f as f64 / FPS as f64;
        for y in 0..H {
            for x in 0..W {
                let o = ((y * W + x) * 4) as usize;
                let c: [u8; 3] = if y < 44 {
                    [236, 236, 240]
                } else if x < 220 {
                    [244, 245, 248]
                } else {
                    // card grid
                    let (cx, cy) = ((x - 220) % 340, (y - 44) % 230);
                    if cx > 30 && cx < 310 && cy > 30 && cy < 200 {
                        let hue = (((x - 220) / 340 + (y - 44) / 230) * 40) as u8;
                        [255 - hue / 4, 255 - hue / 8, 255]
                    } else {
                        [250, 250, 252]
                    }
                };
                rgba[o..o + 3].copy_from_slice(&c);
                rgba[o + 3] = 255;
            }
        }
        // a progress bar that fills, so time is visible in the picture
        let fill = (t / SECS as f64 * 600.0) as u32;
        for y in 100..114 {
            for x in 260..260 + fill {
                let o = ((y * W + x) * 4) as usize;
                rgba[o..o + 3].copy_from_slice(&[88, 108, 240]);
            }
        }
        ex.push_video(&to_i420(&rgba, W, H, (W * 4) as usize, PixelOrder::Rgba, 4), (f as u64 * 1_000_000 / FPS as u64) as i64).unwrap();
        let pcm: Vec<f32> = (0..samples_per_frame)
            .flat_map(|k| {
                let n = f as usize * samples_per_frame + k;
                let s = (n as f32 * 330.0 * std::f32::consts::TAU / 48_000.0).sin() * 0.15 * (1.0 + (t as f32 * 2.0).sin()) * 0.5;
                [s, s]
            })
            .collect();
        ex.push_audio(0, &pcm).unwrap();
        while let Some(op) = ex.poll() {
            sink.apply(&op).unwrap();
        }
    }
    let summary = ex.finish().unwrap();
    while let Some(op) = ex.poll() {
        sink.apply(&op).unwrap();
    }

    let mut log = InputLog { bounds: Rect { x: 0.0, y: 0.0, width: W as f64, height: H as f64 }, ..Default::default() };
    let mut t = 0.0;
    while t < SECS as f64 * 1000.0 {
        let (x, y) = cursor_at(t / 1000.0);
        log.moves.push(MoveEvent { t, x, y, cursor: CursorKind::Arrow });
        t += 16.0;
    }
    for ct in [1900.0, 3900.0, 5900.0, 7900.0, 9900.0] {
        let (x, y) = cursor_at(ct / 1000.0);
        log.clicks.push(ClickEvent { t: ct, x, y, button: Button::Left, down: true });
        log.clicks.push(ClickEvent { t: ct + 90.0, x, y, button: Button::Left, down: false });
    }
    for (i, kt) in [6000.0, 6120.0, 6240.0, 6360.0, 6500.0].iter().enumerate() {
        log.keys.push(KeyEvent { t: *kt, key: "0".into(), down: true, mods: 0, label: String::new() });
        let _ = i;
    }
    std::fs::write(package.dir.join("input.json"), serde_json::to_vec(&log).unwrap()).unwrap();

    let media = Media {
        duration_ms: summary.duration_us as f64 / 1000.0,
        width: W,
        height: H,
        bounds: Rect { x: 0.0, y: 0.0, width: W as f64, height: H as f64 },
        has_system_audio: true,
        system_track: Some(0),
        ..Default::default()
    };
    let mut project = Project::from_recording("Demo", media);
    project.config.cursor.click_effect = screencraft_core::project::ClickEffect::Ripple;
    project.config.styles.padding_ratio = 0.06;
    project.config.styles.corner_radius = 22.0;
    package.save_project(&project).unwrap();
    println!("wrote {}", package.dir.display());
}
