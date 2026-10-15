//! `cargo run --release -p screencraft-encode --example make_samples -- <dir>`
//! Writes one short file per codec/container pair so external tools can check them.
use screencraft_encode::*;

fn main() {
    let dir = std::path::PathBuf::from(std::env::args().nth(1).expect("output dir"));
    std::fs::create_dir_all(&dir).unwrap();
    let (w, h, fps, frames) = (640u32, 360u32, 30u32, 90u32);
    let combos: Vec<(VideoCodec, Container, HwPreference)> = vec![
        (VideoCodec::H264, Container::Mp4, HwPreference::Software),
        (VideoCodec::H264, Container::Mp4, HwPreference::Hardware),
        (VideoCodec::Av1, Container::Mp4, HwPreference::Software),
        (VideoCodec::Av1, Container::WebM, HwPreference::Software),
        (VideoCodec::Vp9, Container::WebM, HwPreference::Software),
        (VideoCodec::Vp9, Container::Mp4, HwPreference::Software),
    ];
    for (codec, container, hw) in combos {
        let mut video = VideoConfig::new(codec, w, h, fps);
        video.hardware = hw;
        video.effort = Effort::Fast;
        let name = format!("{}-{:?}-{:?}.{}", codec.name().replace('.', ""), container, hw, container.extension());
        let path = dir.join(name);
        let n_audio = if container == Container::WebM { 1 } else { 2 };
        let ex = match ThreadedExporter::spawn(
            ExportSettings { container, video, audio: vec![AudioConfig::default(); n_audio] },
            Box::new(FileSink::create(&path).unwrap()),
        ) {
            Ok(e) => e,
            Err(e) => {
                println!("skip {codec:?}/{container:?}/{hw:?}: {e}");
                std::fs::remove_file(&path).ok();
                continue;
            }
        };
        let per = 48_000 / fps as usize;
        for i in 0..frames {
            let mut f = I420::new(w, h);
            for y in 0..h as usize {
                for x in 0..w as usize {
                    f.y[y * w as usize + x] = (40 + ((x + i as usize * 4) % 180) + (y % 20)) as u8;
                }
            }
            let t = i as f32 / fps as f32;
            f.u.iter_mut().for_each(|v| *v = (128.0 + 60.0 * (t * 2.0).sin()) as u8);
            ex.push_video(std::sync::Arc::new(f), i as i64 * 1_000_000 / fps as i64).unwrap();
            let pcm: Vec<f32> = (0..per)
                .flat_map(|k| {
                    let s = ((i as usize * per + k) as f32 * 440.0 * std::f32::consts::TAU / 48_000.0).sin() * 0.25;
                    [s, s]
                })
                .collect();
            for t in 0..n_audio {
                ex.push_audio(t, pcm.clone()).unwrap();
            }
        }
        let s = ex.finish().unwrap();
        println!("{} -> {} ({} bytes, {})", codec.name(), path.display(), s.bytes, s.encoder);
    }
}
