//! `cargo run --release -p screencraft-engine --example record -- <seconds> <dir> [--mic] [--system]`
//! Records the main display into a package, then decodes a few frames back as a check.
use screencraft_capture::Target;
use screencraft_engine::{AudioSource, Package, RecordOptions, Recording, VideoSource};
use screencraft_encode::HwPreference;
use std::sync::Arc;
use std::time::Duration;

fn main() {
    let args: Vec<String> = std::env::args().collect();
    let secs: f64 = args.get(1).and_then(|s| s.parse().ok()).unwrap_or(3.0);
    let dir = std::path::PathBuf::from(args.get(2).expect("output dir"));
    let mic = args.iter().any(|a| a == "--mic");
    let system = args.iter().any(|a| a == "--system");
    let package = Package::open(dir.join("test.screencraft"));
    let _ = std::fs::remove_dir_all(&package.dir);
    let rec = Recording::start(RecordOptions {
        target: Target::Display(None),
        fps: 30,
        max_long_side: Some(1920),
        system_audio: system,
        microphone: mic,
        microphone_id: None,
        output: package.clone(),
        camera: None,
        bitrate_kbps: None,
        hardware: HwPreference::Auto,
    })
    .expect("start");
    println!("capturing {}x{} with {}", rec.info().width, rec.info().height, rec.encoder());
    std::thread::sleep(Duration::from_secs_f64(secs));
    println!("frames so far: {}", rec.stats.frames.load(std::sync::atomic::Ordering::Relaxed));
    let done = rec.stop().expect("stop");
    println!("finished: {:.2}s, {} frames, input events: {}", done.duration_ms / 1000.0, done.frames, {
        let log = package.load_input(&done.project);
        log.moves.len() + log.clicks.len() + log.keys.len()
    });

    let path = package.screen_path(&done.project);
    let mut v = VideoSource::open(&path).expect("open video");
    println!("video {}x{} {} frames {:.1} fps {:.0} ms", v.width, v.height, v.frame_count, v.fps(), v.duration_ms);
    for t in [0.0, v.duration_ms / 2.0, v.duration_ms - 50.0] {
        let f = v.frame_at(t).expect("decode");
        let mean: u64 = f.y.iter().map(|&b| b as u64).sum::<u64>() / f.y.len() as u64;
        println!("  t={t:.0}ms mean luma {mean}");
    }
    let file = Arc::new(screencraft_encode::mp4read::Mp4File::open(&path).unwrap());
    for i in 0..file.tracks.iter().filter(|t| t.kind == screencraft_encode::mp4read::TrackKind::Audio).count() {
        let mut a = AudioSource::open(file.clone(), i).expect("audio");
        let pcm = a.read(0.0, 48_000).unwrap();
        let peak = pcm.iter().fold(0f32, |m, s| m.max(s.abs()));
        println!("audio track {i}: {:.1}s, peak {:.3}", a.duration_ms / 1000.0, peak);
    }
}
