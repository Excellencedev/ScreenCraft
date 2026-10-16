//! `cargo run -p screencraft-engine --release --example tools -- gif <package> <out.gif>`
//! `... -- compress <in.mp4> <out.mp4>` `... -- export <package> <out.mp4>`
use screencraft_engine::*;
fn main() {
    let a: Vec<String> = std::env::args().collect();
    match a.get(1).map(|s| s.as_str()) {
        Some("gif") => {
            let p = Package::open(&a[2]);
            let proj = p.load_project().unwrap();
            let r = export_gif(&p, &proj, 540, std::path::Path::new(&a[3]), |_| true).unwrap();
            println!("gif {}x{} {} bytes", r.width, r.height, r.bytes);
        }
        Some("export") => {
            let p = Package::open(&a[2]);
            let proj = p.load_project().unwrap();
            let r = export_project(&p, &proj, &ExportOptions { resolution: 720, fps: 30, ..Default::default() }, std::path::Path::new(&a[3]), |_| true).unwrap();
            println!("export {}x{} {} frames {} bytes {}", r.width, r.height, r.frames, r.summary.bytes, r.summary.encoder);
        }
        Some("compress") => {
            let n = compress::compress_video(&compress::CompressOptions { input: a[2].clone().into(), output: a[3].clone().into(), hevc: false, quality: 45.0, height: Some(360), fps: None, audio_kbps: None }, |_| true).unwrap();
            println!("compressed {n} bytes");
        }
        _ => eprintln!("usage"),
    }
}
