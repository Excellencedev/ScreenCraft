use screencraft_encode::h264::H264Decoder;
use screencraft_encode::mp4read::{parse_avcc, Mp4File, TrackKind};
use screencraft_encode::*;

const W: u32 = 128;
const H: u32 = 96;

fn frame(i: u32) -> I420 {
    let mut f = I420::new(W, H);
    for y in 0..H as usize {
        for x in 0..W as usize {
            f.y[y * W as usize + x] = (16 + ((x + i as usize * 3) * 200 / W as usize) % 220) as u8;
        }
    }
    for (k, v) in f.u.iter_mut().enumerate() {
        *v = 100 + ((k + i as usize) % 40) as u8;
    }
    f
}

fn tone(frames: u32, fps: u32) -> Vec<f32> {
    let n = (48_000 * frames / fps) as usize;
    (0..n)
        .flat_map(|i| {
            let s = (i as f32 * 440.0 * std::f32::consts::TAU / 48_000.0).sin() * 0.3;
            [s, s]
        })
        .collect()
}

fn run(codec: VideoCodec, container: Container, threads: usize, audio: bool, frames: u32) -> Vec<u8> {
    let fps = 30;
    let mut video = VideoConfig::new(codec, W, H, fps);
    video.threads = threads;
    video.hardware = HwPreference::Software;
    video.effort = Effort::Fast;
    video.bitrate_kbps = 1500;
    let settings = ExportSettings { container, video, audio: if audio { vec![AudioConfig::default()] } else { vec![] } };
    let mut ex = Exporter::new(settings).unwrap();
    let mut sink = MemorySink::default();
    let pcm = tone(frames, fps);
    let per = pcm.len() / frames as usize / 2 * 2;
    for i in 0..frames {
        ex.push_video(&frame(i), i as i64 * 1_000_000 / fps as i64).unwrap();
        if audio {
            ex.push_audio(0, &pcm[i as usize * per..(i as usize + 1) * per]).unwrap();
        }
        while let Some(op) = ex.poll() {
            sink.apply(&op).unwrap();
        }
    }
    let s = ex.finish().unwrap();
    while let Some(op) = ex.poll() {
        sink.apply(&op).unwrap();
    }
    assert_eq!(s.video_frames, frames as u64);
    assert_eq!(s.bytes as usize, sink.bytes.len());
    sink.bytes
}

#[test]
fn h264_mp4_roundtrip_decodes() {
    let bytes = run(VideoCodec::H264, Container::Mp4, 1, true, 30);
    let mp4 = Mp4File::from_source(Box::new(bytes)).unwrap();
    let v = mp4.video().expect("video track");
    assert_eq!(&v.codec, b"avc1");
    assert_eq!((v.width, v.height), (W, H));
    assert_eq!(v.samples.len(), 30);
    assert!(v.samples[0].key);
    assert_eq!(v.samples[1].pts_us, 33_333);
    let a = mp4.audio_tracks().next().expect("audio track");
    assert_eq!(&a.codec, b"Opus");
    assert!(a.samples.len() >= 49, "{}", a.samples.len());
    assert_eq!(a.kind, TrackKind::Audio);

    let (ls, params) = parse_avcc(&v.config).unwrap();
    let mut dec = H264Decoder::new(&params, ls).unwrap();
    let mut got = 0;
    for (i, s) in v.samples.iter().enumerate() {
        let raw = mp4.read_sample(s).unwrap();
        if let Some(pic) = dec.decode(&raw).unwrap() {
            let src = frame(i as u32);
            let err: u64 = pic.y.iter().zip(&src.y).map(|(a, b)| (*a as i64 - *b as i64).unsigned_abs()).sum();
            assert!(err / (W * H) as u64 <= 6, "frame {i}: mean luma error {}", err / (W * H) as u64);
            got += 1;
        }
    }
    assert!(got >= 29, "decoded {got}");
}

#[test]
fn av1_in_both_containers() {
    for c in [Container::Mp4, Container::WebM] {
        let b = run(VideoCodec::Av1, c, 2, true, 12);
        assert!(b.len() > 500);
        if c == Container::Mp4 {
            let m = Mp4File::from_source(Box::new(b)).unwrap();
            assert_eq!(&m.video().unwrap().codec, b"av01");
            assert_eq!(m.video().unwrap().samples.len(), 12);
        } else {
            assert_eq!(&b[..4], &[0x1A, 0x45, 0xDF, 0xA3]);
        }
    }
}

#[test]
fn output_is_deterministic_across_thread_counts() {
    for codec in [VideoCodec::Av1] {
        let a = run(codec, Container::Mp4, 1, true, 10);
        let b = run(codec, Container::Mp4, 4, true, 10);
        let c = run(codec, Container::Mp4, 4, true, 10);
        assert!(a == b && b == c, "{codec:?} export bytes differ between runs");
    }
}

#[test]
fn threaded_exporter_matches_inline() {
    let inline = run(VideoCodec::Av1, Container::Mp4, 1, false, 8);
    let mut video = VideoConfig::new(VideoCodec::Av1, W, H, 30);
    video.threads = 1;
    video.hardware = HwPreference::Software;
    video.effort = Effort::Fast;
    video.bitrate_kbps = 1500;
    let path = std::env::temp_dir().join(format!("sc-threaded-{}.mp4", std::process::id()));
    let ex = ThreadedExporter::spawn(
        ExportSettings { container: Container::Mp4, video, audio: vec![] },
        Box::new(FileSink::create(&path).unwrap()),
    )
    .unwrap();
    for i in 0..8 {
        ex.push_video(std::sync::Arc::new(frame(i)), i as i64 * 1_000_000 / 30).unwrap();
    }
    ex.finish().unwrap();
    let on_disk = std::fs::read(&path).unwrap();
    std::fs::remove_file(&path).ok();
    assert_eq!(on_disk, inline);
}

#[cfg(feature = "vp9")]
#[test]
fn vp9_webm() {
    let b = run(VideoCodec::Vp9, Container::WebM, 2, true, 12);
    assert_eq!(&b[..4], &[0x1A, 0x45, 0xDF, 0xA3]);
    let again = run(VideoCodec::Vp9, Container::WebM, 2, true, 12);
    assert_eq!(b, again);
    let one = run(VideoCodec::Vp9, Container::WebM, 1, true, 12);
    let four = run(VideoCodec::Vp9, Container::WebM, 4, true, 12);
    assert_eq!(one, four, "VP9 output depends on the thread count");
}

fn run_sized(codec: VideoCodec, container: Container, threads: usize, w: u32, h: u32, frames: u32) -> Vec<u8> {
    let mut video = VideoConfig::new(codec, w, h, 30);
    video.threads = threads;
    video.hardware = HwPreference::Software;
    video.effort = Effort::Fast;
    video.bitrate_kbps = 3000;
    let mut ex = Exporter::new(ExportSettings { container, video, audio: vec![] }).unwrap();
    let mut sink = MemorySink::default();
    for i in 0..frames {
        let mut f = I420::new(w, h);
        for y in 0..h as usize {
            for x in 0..w as usize {
                f.y[y * w as usize + x] = (16 + ((x * 3 + y * 5 + i as usize * 7) % 200)) as u8;
            }
        }
        ex.push_video(&f, i as i64 * 33_333).unwrap();
        while let Some(op) = ex.poll() {
            sink.apply(&op).unwrap();
        }
    }
    ex.finish().unwrap();
    while let Some(op) = ex.poll() {
        sink.apply(&op).unwrap();
    }
    sink.bytes
}

/// Frames wide enough for several tiles / slices: the thread count must still not matter.
#[test]
fn wide_frames_are_deterministic_across_thread_counts() {
    let cases: Vec<(VideoCodec, Container)> = vec![
        (VideoCodec::Av1, Container::Mp4),
        (VideoCodec::Av1, Container::WebM),
        #[cfg(feature = "vp9")]
        (VideoCodec::Vp9, Container::WebM),
    ];
    for (codec, container) in cases {
        let a = run_sized(codec, container, 1, 1280, 720, 6);
        let b = run_sized(codec, container, 6, 1280, 720, 6);
        assert_eq!(a, b, "{codec:?} output depends on the thread count");
    }
}
