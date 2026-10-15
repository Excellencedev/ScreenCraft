//! `cargo run --release -p screencraft-engine --example export -- <package> <out.mp4|webm> [codec h264|av1|vp9] [short-side] [fps]`
use screencraft_encode::{Container, Effort, HwPreference, VideoCodec};
use screencraft_engine::{export_project, ExportOptions, Package};

fn main() {
    let a: Vec<String> = std::env::args().collect();
    let package = Package::open(&a[1]);
    let out = std::path::PathBuf::from(&a[2]);
    let codec = match a.get(3).map(String::as_str) {
        Some("av1") => VideoCodec::Av1,
        Some("vp9") => VideoCodec::Vp9,
        _ => VideoCodec::H264,
    };
    let container = if out.extension().map(|e| e == "webm").unwrap_or(false) { Container::WebM } else { Container::Mp4 };
    let project = package.load_project().expect("project");
    let opts = ExportOptions {
        container,
        codec,
        resolution: a.get(4).and_then(|s| s.parse().ok()).unwrap_or(1080),
        fps: a.get(5).and_then(|s| s.parse().ok()).unwrap_or(30),
        bitrate_kbps: None,
        effort: Effort::Fast,
        hardware: HwPreference::Auto,
        audio: true,
        threads: 0,
    };
    let t0 = std::time::Instant::now();
    let r = export_project(&package, &project, &opts, &out, |p| {
        eprint!("\r{:3.0}%", p * 100.0);
        true
    })
    .expect("export");
    eprintln!();
    println!(
        "{}x{} {} frames in {:.1}s ({:.1} fps) -> {:.1} MB via {}",
        r.width,
        r.height,
        r.frames,
        t0.elapsed().as_secs_f64(),
        r.frames as f64 / t0.elapsed().as_secs_f64(),
        r.summary.bytes as f64 / 1e6,
        r.summary.encoder
    );
}
