//! `cargo run --release -p screencraft-engine --example bench_decode -- <package>`
use screencraft_engine::{Package, VideoSource};

fn main() {
    let package = Package::open(std::env::args().nth(1).expect("package"));
    let project = package.load_project().unwrap();
    let mut v = VideoSource::open(&package.screen_path(&project)).unwrap();
    let t0 = std::time::Instant::now();
    let n = v.frame_count.min(180);
    for i in 0..n {
        v.frame(i).unwrap();
    }
    let s = t0.elapsed().as_secs_f64();
    println!("{}x{}: decoded {n} frames in {s:.2}s = {:.0} fps", v.width, v.height, n as f64 / s);
}
