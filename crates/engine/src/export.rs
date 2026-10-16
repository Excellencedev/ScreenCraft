//! Render a project to a video file: decode -> compose on the GPU -> encode.

use crate::audio_mix::{Lane, Soundtrack};
use crate::package::Package;
use crate::source::AudioSource;
use screencraft_core::project::Project;
use screencraft_core::track::compute_track;
use screencraft_encode::color::{default_threads, to_i420, PixelOrder};
use screencraft_encode::mp4read::Mp4File;
use screencraft_encode::{
    AudioConfig, Container, Effort, ExportSettings, ExportSummary, FileSink, HwPreference, ThreadedExporter, VideoCodec,
    VideoConfig, I420,
};
use screencraft_render::{build_scene, output_size, Compositor, Slot};
use std::path::Path;
use std::sync::mpsc::sync_channel;
use std::sync::Arc;

#[derive(Clone, Debug)]
pub struct ExportOptions {
    pub container: Container,
    pub codec: VideoCodec,
    /// Length of the shorter output side in pixels.
    pub resolution: u32,
    pub fps: u32,
    pub bitrate_kbps: Option<u32>,
    pub effort: Effort,
    pub hardware: HwPreference,
    pub audio: bool,
    /// Encoder threads; 0 = automatic. The output bytes do not depend on this.
    pub threads: usize,
}

impl Default for ExportOptions {
    fn default() -> Self {
        Self {
            container: Container::Mp4,
            codec: VideoCodec::H264,
            resolution: 1080,
            fps: 60,
            bitrate_kbps: None,
            effort: Effort::Balanced,
            hardware: HwPreference::Auto,
            audio: true,
            threads: 0,
        }
    }
}

#[derive(Clone, Debug)]
pub struct ExportReport {
    pub summary: ExportSummary,
    pub width: u32,
    pub height: u32,
    pub frames: usize,
}

/// The edited soundtrack of a project: every audio track of `screen.mp4` through the cuts,
/// speeds and volumes. `None` when the recording has no audio (or it is all muted).
pub fn build_soundtrack(package: &Package, project: &Project) -> Result<Option<Soundtrack>, String> {
    if !(project.media.has_system_audio || project.media.has_microphone) {
        return Ok(None);
    }
    let screen_path = package.screen_path(project);
    let file = Arc::new(Mp4File::open(&screen_path).map_err(|e| e.to_string())?);
    let a = &project.config.audio;
    let mut lanes = Vec::new();
    if let (Some(t), false) = (project.media.system_track, a.mute_system_audio) {
        if let Ok(src) = AudioSource::open(file.clone(), t as usize) {
            lanes.push(Lane { source: Box::new(src), gain: a.device_volume as f32 });
        }
    }
    if let (Some(t), false) = (project.media.mic_track, a.mute_microphone) {
        if let Ok(src) = AudioSource::open(file.clone(), t as usize) {
            lanes.push(Lane { source: Box::new(src), gain: a.mic_volume as f32 });
        }
    }
    Ok(if lanes.is_empty() { None } else { Some(Soundtrack::new(project, lanes)) })
}

fn even_down(n: u32) -> u32 {
    n & !1
}

/// Render `project` (stored in `package`) to `out`. `progress` gets 0..1 and returns `false`
/// to cancel. A cancelled export removes the partial file and returns an error.
pub fn export_project(
    package: &Package,
    project: &Project,
    opts: &ExportOptions,
    out: &Path,
    mut progress: impl FnMut(f32) -> bool,
) -> Result<ExportReport, String> {
    if !opts.container.supports(opts.codec) {
        return Err(format!("{} can't be stored in .{}", opts.codec.name(), opts.container.extension()));
    }
    let fps = opts.fps.clamp(1, 240);
    let log = package.load_input(project);
    let track = compute_track(project, &log, fps as f64);
    if track.count == 0 {
        return Err("nothing to export: the timeline is empty".into());
    }
    let (w, h) = output_size(project, opts.resolution.max(144));
    let (w, h) = (even_down(w), even_down(h));

    // Decode on its own thread; the decoders are not Send, so they are created over there.
    let src_idx: Vec<(f64, Option<f64>)> = (0..track.count)
        .map(|i| (track.t_src[i], track.other_alpha.get(i).filter(|a| **a > 0.003).map(|_| track.other_t_src[i])))
        .collect();
    let (ftx, frx) = sync_channel::<Result<crate::source::Frames, String>>(4);
    let (dp, dproj) = (package.clone(), project.clone());
    let decoder = std::thread::Builder::new()
        .name("screencraft-export-decode".into())
        .spawn(move || {
            let mut m = match crate::source::MultiSource::open(&dp, &dproj) {
                Ok(m) => m,
                Err(e) => {
                    let _ = ftx.send(Err(e));
                    return;
                }
            };
            for (t, other) in src_idx {
                if ftx.send(m.fetch(t, other)).is_err() {
                    return;
                }
            }
        })
        .map_err(|e| e.to_string())?;

    let mut soundtrack = if opts.audio { build_soundtrack(package, project)? } else { None };

    let mut video = VideoConfig::new(opts.codec, w, h, fps);
    if let Some(b) = opts.bitrate_kbps {
        video.bitrate_kbps = b;
    }
    video.effort = opts.effort;
    video.hardware = opts.hardware;
    video.threads = opts.threads;
    let settings = ExportSettings {
        container: opts.container,
        video,
        audio: if soundtrack.is_some() { vec![AudioConfig::default()] } else { vec![] },
    };
    let sink = FileSink::create(out).map_err(|e| format!("cannot create {}: {e}", out.display()))?;
    let exporter = ThreadedExporter::spawn(settings, Box::new(sink)).map_err(|e| e.to_string())?;

    let mut gpu = Compositor::headless()?;
    let threads = default_threads();
    let mut cancelled = false;
    let mut failure: Option<String> = None;
    // GPU read-back and colour conversion overlap: this thread renders and reads frames, a
    // second one converts them to I420 and feeds the encoder, in order.
    std::thread::scope(|scope| {
        let (rtx, rrx) = sync_channel::<(usize, Vec<u8>)>(3);
        let exporter = &exporter;
        let mut soundtrack = soundtrack.take();
        let conv = scope.spawn(move || -> Result<(), String> {
            let mut audio_done = 0u64;
            for (i, rgba) in rrx {
                let yuv = to_i420(&rgba, w, h, w as usize * 4, PixelOrder::Rgba, threads);
                let pts_us = (i as u64 * 1_000_000 / fps as u64) as i64;
                exporter.push_video(Arc::new(yuv), pts_us).map_err(|e| e.to_string())?;
                if let Some(st) = soundtrack.as_mut() {
                    let end = (i as u64 + 1) * 48_000 / fps as u64;
                    let n = (end - audio_done) as usize;
                    let pcm = st.render(audio_done, n);
                    audio_done = end;
                    exporter.push_audio(0, pcm).map_err(|e| e.to_string())?;
                }
            }
            Ok(())
        });
        let mut produce = || -> Result<(), String> {
            for i in 0..track.count {
                let frames = frx.recv().map_err(|_| "decoder stopped".to_string())??;
                gpu.upload(&frames.screen);
                for (slot, f) in [(Slot::Screen2, &frames.screen2), (Slot::Camera, &frames.camera), (Slot::Camera2, &frames.camera2)] {
                    match f {
                        Some(f) => gpu.upload_slot(slot, f),
                        None => gpu.clear_slot(slot),
                    }
                }
                let scene = build_scene(project, &track, i, (w, h));
                gpu.render(&scene);
                let rgba = gpu.read_rgba()?;
                if rtx.send((i, rgba)).is_err() {
                    break; // the converter stopped; its error is collected below
                }
                if i % 4 == 0 && !progress(i as f32 / track.count as f32) {
                    cancelled = true;
                    break;
                }
            }
            Ok(())
        };
        let produced = produce();
        drop(rtx);
        let converted = conv.join().unwrap_or_else(|_| Err("encoder thread panicked".into()));
        failure = produced.err().or(converted.err());
    });
    if let Some(e) = failure {
        drop(frx);
        let _ = decoder.join();
        drop(exporter);
        let _ = std::fs::remove_file(out);
        return Err(e);
    }
    drop(frx);
    let _ = decoder.join();
    if cancelled {
        drop(exporter);
        let _ = std::fs::remove_file(out);
        return Err("export cancelled".into());
    }
    let summary = exporter.finish().map_err(|e| e.to_string())?;
    progress(1.0);
    Ok(ExportReport { summary, width: w, height: h, frames: track.count })
}
