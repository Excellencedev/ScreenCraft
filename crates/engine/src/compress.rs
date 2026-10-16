//! Re-encode a finished MP4 to make it smaller (the compressor): decode with the operating
//! system's H.264 decoder, optionally scale and drop frames, encode again at a lower bitrate.
//! Opus audio is carried over; other audio codecs are not decoded and are left out.

use screencraft_encode::mp4read::{parse_avcc, Mp4File, TrackKind};
use screencraft_encode::opus::OpusDecoder;
use screencraft_encode::{AudioConfig, Container, Effort, ExportSettings, FileSink, HwPreference, ThreadedExporter, VideoCodec, VideoConfig, I420};
use std::path::PathBuf;
use std::sync::Arc;

pub struct CompressOptions {
    pub input: PathBuf,
    pub output: PathBuf,
    /// Reserved: HEVC output is not built in yet, H.264 is used.
    pub hevc: bool,
    /// 5..100, as in the 0.3 compressor.
    pub quality: f32,
    pub height: Option<u32>,
    pub fps: Option<f32>,
    pub audio_kbps: Option<u32>,
}

/// Box-filter scale of an I420 picture.
fn scale(f: &I420, w: u32, h: u32) -> I420 {
    if (f.width, f.height) == (w, h) {
        return f.clone();
    }
    let mut out = I420::new(w, h);
    let plane = |src: &[u8], sw: usize, sh: usize, dst: &mut [u8], dw: usize, dh: usize| {
        for y in 0..dh {
            let (y0, y1) = (y * sh / dh, ((y + 1) * sh / dh).max(y * sh / dh + 1).min(sh));
            for x in 0..dw {
                let (x0, x1) = (x * sw / dw, ((x + 1) * sw / dw).max(x * sw / dw + 1).min(sw));
                let mut sum = 0u32;
                for yy in y0..y1 {
                    for xx in x0..x1 {
                        sum += src[yy * sw + xx] as u32;
                    }
                }
                dst[y * dw + x] = (sum / ((y1 - y0) * (x1 - x0)) as u32) as u8;
            }
        }
    };
    let (sw, sh, dw, dh) = (f.width as usize, f.height as usize, w as usize, h as usize);
    plane(&f.y, sw, sh, &mut out.y, dw, dh);
    plane(&f.u, sw / 2, sh / 2, &mut out.u, dw / 2, dh / 2);
    plane(&f.v, sw / 2, sh / 2, &mut out.v, dw / 2, dh / 2);
    out
}

/// Returns the size of the written file in bytes.
pub fn compress_video(opts: &CompressOptions, mut progress: impl FnMut(f32) -> bool) -> Result<u64, String> {
    let file = Arc::new(Mp4File::open(&opts.input).map_err(|e| e.to_string())?);
    let v = file.video().ok_or("the file has no video")?;
    if &v.codec != b"avc1" {
        return Err(format!("this build compresses H.264 videos ('{}' is not supported yet)", v.codec_str()));
    }
    let (params_len, params) = parse_avcc(&v.config).map_err(|e| e.to_string())?;
    let mut dec = screencraft_encode::h264::H264Decoder::new(&params, params_len).map_err(|e| e.to_string())?;
    let (sw, sh) = (v.width, v.height);
    let samples = v.samples.clone();
    let src_fps = if samples.len() > 1 { (samples.len() as f64 - 1.0) * 1e6 / (samples.last().unwrap().pts_us - samples[0].pts_us).max(1) as f64 } else { 30.0 };
    let fps = opts.fps.map(|f| f as f64).unwrap_or(src_fps).min(src_fps).max(1.0);
    let k = opts.height.filter(|h| *h < sh).map(|h| h as f64 / sh as f64).unwrap_or(1.0);
    let (w, h) = (((sw as f64 * k) as u32) & !1, ((sh as f64 * k) as u32) & !1);
    let (w, h) = (w.max(16), h.max(16));

    let mut video = VideoConfig::new(VideoCodec::H264, w, h, fps.round() as u32);
    let base = screencraft_encode::default_bitrate_kbps(w, h, fps.round() as u32) as f64;
    video.bitrate_kbps = ((base * (opts.quality as f64 / 60.0).powf(1.6)) as u32).max(150);
    video.effort = Effort::Balanced;
    video.hardware = HwPreference::Auto;

    // Opus audio, when there is any
    let audio_track = file.tracks.iter().find(|t| t.kind == TrackKind::Audio && &t.codec == b"Opus").cloned();
    let sink = FileSink::create(&opts.output).map_err(|e| format!("cannot create {}: {e}", opts.output.display()))?;
    let settings = ExportSettings { container: Container::Mp4, video, audio: if audio_track.is_some() { vec![AudioConfig::default()] } else { vec![] } };
    let exporter = ThreadedExporter::spawn(settings, Box::new(sink)).map_err(|e| e.to_string())?;
    let fail = |e: String| {
        let _ = std::fs::remove_file(&opts.output);
        e
    };

    let min_gap_us = (1_000_000.0 / fps) as i64 - 2000;
    let mut last_pts = i64::MIN / 2;
    let total = samples.len().max(1);
    let t0 = samples.first().map(|s| s.pts_us).unwrap_or(0);
    for (i, s) in samples.iter().enumerate() {
        let raw = file.read_sample(s).map_err(|e| fail(e.to_string()))?;
        let pic = dec.decode(&raw).map_err(|e| fail(e.to_string()))?;
        if let Some(p) = pic {
            if s.pts_us - last_pts >= min_gap_us {
                last_pts = s.pts_us;
                exporter.push_video(Arc::new(scale(&p, w, h)), s.pts_us - t0).map_err(|e| fail(e.to_string()))?;
            }
        }
        if i % 8 == 0 && !progress(i as f32 / total as f32 * 0.9) {
            drop(exporter);
            let _ = std::fs::remove_file(&opts.output);
            return Err("cancelled".into());
        }
    }
    if let Some(a) = audio_track {
        let ch = a.channels.clamp(1, 2) as usize;
        let mut od = OpusDecoder::new(ch as u8).map_err(|e| fail(e.to_string()))?;
        let pre_skip = if a.config.len() >= 4 { u16::from_be_bytes([a.config[2], a.config[3]]) as usize } else { 312 };
        let mut skip = pre_skip * ch;
        for s in &a.samples {
            let raw = file.read_sample(s).map_err(|e| fail(e.to_string()))?;
            let pcm = od.decode(&raw).map_err(|e| fail(e.to_string()))?;
            let mut stereo: Vec<f32> = if ch == 1 { pcm.iter().flat_map(|x| [*x, *x]).collect() } else { pcm };
            let drop_n = skip.min(stereo.len());
            stereo.drain(..drop_n);
            skip -= drop_n.min(skip);
            if !stereo.is_empty() {
                exporter.push_audio(0, stereo).map_err(|e| fail(e.to_string()))?;
            }
        }
    }
    exporter.finish().map_err(|e| fail(e.to_string()))?;
    progress(1.0);
    Ok(std::fs::metadata(&opts.output).map(|m| m.len()).unwrap_or(0))
}
