//! Records the camera bubble to `camera.mp4` alongside the screen recording.

use screencraft_audio::Clock;
use screencraft_capture::camera::{CamFrame, CameraHub};
use screencraft_encode::color::{default_threads, to_i420, PixelOrder};
use screencraft_encode::{Container, Effort, ExportSettings, FileSink, HwPreference, ThreadedExporter, VideoCodec, VideoConfig};
use std::path::PathBuf;
use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};
use std::sync::Arc;
use std::thread::JoinHandle;
use std::time::Duration;

pub struct CameraRecorder {
    stop: Arc<AtomicBool>,
    /// Clock time (ns) of the first camera frame; u64::MAX until one arrives.
    first_ns: Arc<AtomicU64>,
    join: Option<JoinHandle<Result<(), String>>>,
}

fn rgb_to_rgba(rgb: &[u8]) -> Vec<u8> {
    let mut out = Vec::with_capacity(rgb.len() / 3 * 4);
    for p in rgb.chunks_exact(3) {
        out.extend_from_slice(&[p[0], p[1], p[2], 255]);
    }
    out
}

impl CameraRecorder {
    /// Start recording whatever `hub` delivers. Waits for the first frame so the encoder knows the size.
    pub fn start(hub: Arc<CameraHub>, path: PathBuf, clock: Clock, hw: HwPreference) -> Result<Self, String> {
        let rx = hub.subscribe(8);
        let first: Arc<CamFrame> = match rx.recv_timeout(Duration::from_secs(5)) {
            Ok(f) => f,
            Err(_) => return Err("the camera delivered no pictures".into()),
        };
        let (w, h) = (first.width & !1, first.height & !1);
        let fps = 30;
        let mut video = VideoConfig::new(VideoCodec::H264, w, h, fps);
        video.hardware = hw;
        video.effort = Effort::Fast;
        video.bitrate_kbps = (screencraft_encode::default_bitrate_kbps(w, h, fps)).min(12_000);
        video.keyint = fps;
        let sink = FileSink::create(&path).map_err(|e| e.to_string())?;
        let exporter = ThreadedExporter::spawn(ExportSettings { container: Container::Mp4, video, audio: vec![] }, Box::new(sink)).map_err(|e| e.to_string())?;
        let stop = Arc::new(AtomicBool::new(false));
        let first_ns = Arc::new(AtomicU64::new(u64::MAX));
        let (stop2, first2) = (stop.clone(), first_ns.clone());
        let join = std::thread::Builder::new()
            .name("screencraft-camera-rec".into())
            .spawn(move || -> Result<(), String> {
                let threads = default_threads();
                let mut t0: Option<u64> = None;
                let mut last_pts: i64 = -1;
                let mut handle = |f: Arc<CamFrame>, t0: &mut Option<u64>| -> Result<(), String> {
                    let ns = clock.ns_of(f.at);
                    let base = *t0.get_or_insert_with(|| {
                        first2.store(ns, Ordering::SeqCst);
                        ns
                    });
                    let (fw, fh) = (f.width & !1, f.height & !1);
                    if (fw, fh) != (w, h) {
                        return Ok(());
                    }
                    let rgba = if fw == f.width && fh == f.height {
                        rgb_to_rgba(&f.rgb)
                    } else {
                        let mut v = Vec::with_capacity((fw * fh * 4) as usize);
                        for y in 0..fh as usize {
                            v.extend_from_slice(&rgb_to_rgba(&f.rgb[y * f.width as usize * 3..(y * f.width as usize + fw as usize) * 3]));
                        }
                        v
                    };
                    let yuv = to_i420(&rgba, fw, fh, fw as usize * 4, PixelOrder::Rgba, threads);
                    let pts = ((ns.saturating_sub(base)) / 1000) as i64;
                    let pts = pts.max(last_pts + 1);
                    last_pts = pts;
                    exporter.push_video(Arc::new(yuv), pts).map_err(|e| e.to_string())
                };
                handle(first, &mut t0)?;
                loop {
                    match rx.recv_timeout(Duration::from_millis(50)) {
                        Ok(f) => handle(f, &mut t0)?,
                        Err(std::sync::mpsc::RecvTimeoutError::Timeout) => {
                            if stop2.load(Ordering::SeqCst) {
                                break;
                            }
                        }
                        Err(_) => break,
                    }
                    if stop2.load(Ordering::SeqCst) && rx.try_recv().is_err() {
                        break;
                    }
                }
                exporter.finish().map_err(|e| e.to_string())?;
                Ok(())
            })
            .map_err(|e| e.to_string())?;
        Ok(Self { stop, first_ns, join: Some(join) })
    }

    /// Stop and finish the file. Returns the camera's clock offset in ms (`camera time = source
    /// time + offset`), or None when nothing usable was recorded.
    pub fn finish(mut self, screen_origin_ns: u64) -> Option<f64> {
        self.stop.store(true, Ordering::SeqCst);
        let ok = self.join.take().map(|j| j.join().ok().and_then(|r| r.ok()).is_some()).unwrap_or(false);
        let first = self.first_ns.load(Ordering::SeqCst);
        if !ok || first == u64::MAX {
            return None;
        }
        Some((screen_origin_ns as f64 - first as f64) / 1e6)
    }
}
