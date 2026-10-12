//! Screen capture on a dedicated thread. ScreenCaptureKit writes the movie
//! (video + optional system audio + microphone) straight to disk through
//! `SCRecordingOutput`, while the input tracker logs the cursor alongside.

use crate::inputlog::InputTracker;
use screencapturekit::cg::CGRect;
use screencapturekit::prelude::*;
use screencapturekit::recording_output::{
    RecordingCallbacks, SCRecordingOutput, SCRecordingOutputCodec, SCRecordingOutputConfiguration,
    SCRecordingOutputFileType,
};
use screencraft_core::project::Rect;
use serde::{Deserialize, Serialize};
use std::path::PathBuf;
use std::sync::mpsc::{channel, Receiver, Sender};
use std::sync::{Arc, Condvar, Mutex};
use std::thread;
use std::time::{Duration, Instant};

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct AreaRect {
    pub x: f64,
    pub y: f64,
    pub width: f64,
    pub height: f64,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct StartOpts {
    /// "display" | "window" | "area"
    pub mode: String,
    pub display_id: Option<u32>,
    pub window_id: Option<u32>,
    pub area: Option<AreaRect>,
    #[serde(default)]
    pub system_audio: bool,
    #[serde(default)]
    pub microphone: bool,
    #[serde(default)]
    pub microphone_id: Option<String>,
    #[serde(default = "default_fps")]
    pub fps: u32,
    /// Cap for the long side of the recording in pixels; `None` = native resolution.
    #[serde(default)]
    pub max_long_side: Option<u32>,
    pub output_dir: PathBuf,
}

fn default_fps() -> u32 {
    60
}

#[derive(Clone, Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct StartInfo {
    pub width: u32,
    pub height: u32,
    pub bounds: Rect,
    /// Wall clock (ms since epoch) at which the first frame was recorded.
    pub started_unix_ms: f64,
}

#[derive(Clone, Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct StopInfo {
    pub dir: PathBuf,
    pub duration_ms: f64,
    pub width: u32,
    pub height: u32,
    pub bounds: Rect,
    pub started_unix_ms: f64,
    pub has_system_audio: bool,
    pub has_microphone: bool,
}

enum Cmd {
    Start(StartOpts, Sender<Result<StartInfo, String>>),
    Stop(Sender<Result<StopInfo, String>>),
}

#[derive(Clone)]
pub struct Capture {
    tx: Sender<Cmd>,
}

impl Capture {
    pub fn spawn() -> Self {
        let (tx, rx) = channel();
        thread::Builder::new()
            .name("screencraft-capture".into())
            .spawn(move || run(rx))
            .expect("capture thread");
        Self { tx }
    }

    pub fn start(&self, opts: StartOpts) -> Result<StartInfo, String> {
        let (rtx, rrx) = channel();
        self.tx.send(Cmd::Start(opts, rtx)).map_err(|e| e.to_string())?;
        rrx.recv_timeout(Duration::from_secs(20)).map_err(|e| format!("capture start timed out: {e}"))?
    }

    pub fn stop(&self) -> Result<StopInfo, String> {
        let (rtx, rrx) = channel();
        self.tx.send(Cmd::Stop(rtx)).map_err(|e| e.to_string())?;
        rrx.recv_timeout(Duration::from_secs(30)).map_err(|e| format!("capture stop timed out: {e}"))?
    }
}

struct Session {
    stream: SCStream,
    output: SCRecordingOutput,
    tracker: InputTracker,
    zero: Arc<(Mutex<Option<Instant>>, Condvar)>,
    finished: Arc<(Mutex<bool>, Condvar)>,
    dir: PathBuf,
    bounds: Rect,
    width: u32,
    height: u32,
    started_unix_ms: f64,
    system_audio: bool,
    microphone: bool,
}

fn unix_ms() -> f64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_secs_f64() * 1000.0)
        .unwrap_or(0.0)
}

fn even(v: f64) -> u32 {
    let n = v.round().max(2.0) as u32;
    n - (n % 2)
}

fn run(rx: Receiver<Cmd>) {
    let mut session: Option<Session> = None;
    while let Ok(cmd) = rx.recv() {
        match cmd {
            Cmd::Start(opts, reply) => {
                if session.is_some() {
                    let _ = reply.send(Err("already recording".into()));
                    continue;
                }
                match begin(opts) {
                    Ok(s) => {
                        let info = StartInfo {
                            width: s.width,
                            height: s.height,
                            bounds: s.bounds,
                            started_unix_ms: s.started_unix_ms,
                        };
                        session = Some(s);
                        let _ = reply.send(Ok(info));
                    }
                    Err(e) => {
                        let _ = reply.send(Err(e));
                    }
                }
            }
            Cmd::Stop(reply) => {
                let Some(s) = session.take() else {
                    let _ = reply.send(Err("not recording".into()));
                    continue;
                };
                let _ = reply.send(end(s));
            }
        }
    }
}

fn begin(opts: StartOpts) -> Result<Session, String> {
    let content = SCShareableContent::get().map_err(|e| format!("{e} (is Screen Recording permission granted?)"))?;
    let displays = content.displays();
    let windows = content.windows();
    let me = std::process::id() as i32;
    let own: Vec<&SCWindow> = windows
        .iter()
        .filter(|w| w.owning_application().map(|a| a.process_id() == me).unwrap_or(false))
        .collect();

    // Resolve what to capture: filter + captured rect in global points + source rect.
    let (filter, bounds, source_rect): (SCContentFilter, Rect, Option<CGRect>) = match opts.mode.as_str() {
        "window" => {
            let id = opts.window_id.ok_or("window id missing")?;
            let w = windows.iter().find(|w| w.window_id() == id).ok_or("window not found")?;
            let f = w.frame();
            let filter = SCContentFilter::create().with_window(w).build().map_err(|e| e.to_string())?;
            (
                filter,
                Rect { x: f.origin.x, y: f.origin.y, width: f.size.width, height: f.size.height },
                None,
            )
        }
        mode => {
            let display = match opts.display_id {
                Some(id) => displays.iter().find(|d| d.display_id() == id),
                None => displays.first(),
            }
            .ok_or("display not found")?;
            let df = display.frame();
            let filter = SCContentFilter::create()
                .with_display(display)
                .with_excluding_windows(&own)
                .build()
                .map_err(|e| e.to_string())?;
            if mode == "area" {
                let a = opts.area.clone().ok_or("area missing")?;
                let local = CGRect::new(a.x - df.origin.x, a.y - df.origin.y, a.width, a.height);
                (filter, Rect { x: a.x, y: a.y, width: a.width, height: a.height }, Some(local))
            } else {
                (
                    filter,
                    Rect { x: df.origin.x, y: df.origin.y, width: df.size.width, height: df.size.height },
                    None,
                )
            }
        }
    };

    let native = filter.point_pixel_scale().max(1.0) as f64;
    // Resolution is chosen by the user as a cap on the long side (never upscaled).
    let long_pts = bounds.width.max(bounds.height).max(1.0);
    let scale = match opts.max_long_side {
        Some(m) if m > 0 => native.min(m as f64 / long_pts),
        _ => native,
    };
    let (width, height) = (even(bounds.width * scale), even(bounds.height * scale));

    let mut config = SCStreamConfiguration::new()
        .with_width(width)
        .with_height(height)
        .with_fps(opts.fps.clamp(10, 120))
        .with_shows_cursor(false)
        .with_queue_depth(8);
    if let Some(r) = source_rect {
        config = config.with_source_rect(r);
    }
    if opts.system_audio {
        config = config.with_captures_audio(true).with_excludes_current_process_audio(true);
    }
    if opts.microphone {
        config = config.with_captures_microphone(true).map_err(|e| e.to_string())?;
        if let Some(id) = &opts.microphone_id {
            config = config.with_microphone_capture_device_id(id).map_err(|e| e.to_string())?;
        }
    }

    std::fs::create_dir_all(&opts.output_dir).map_err(|e| e.to_string())?;
    let movie = opts.output_dir.join("screen.mp4");
    let _ = std::fs::remove_file(&movie);
    let rec_config = SCRecordingOutputConfiguration::new()
        .map_err(|e| e.to_string())?
        .with_output_url(&movie)
        .map_err(|e| e.to_string())?
        .with_video_codec(SCRecordingOutputCodec::H264)
        .with_output_file_type(SCRecordingOutputFileType::MP4);

    let zero = Arc::new((Mutex::new(None::<Instant>), Condvar::new()));
    let finished = Arc::new((Mutex::new(false), Condvar::new()));
    let (z, f) = (zero.clone(), finished.clone());
    let started_ms = Arc::new(Mutex::new(0.0f64));
    let sm = started_ms.clone();
    let callbacks = RecordingCallbacks::new()
        .on_start(move || {
            *sm.lock().unwrap() = unix_ms();
            *z.0.lock().unwrap() = Some(Instant::now());
            z.1.notify_all();
        })
        .on_finish(move || {
            *f.0.lock().unwrap() = true;
            f.1.notify_all();
        })
        .on_fail(|e| eprintln!("[screencraft] recording failed: {e}"));
    let output = SCRecordingOutput::new_with_delegate(&rec_config, callbacks)
        .ok_or("could not create recording output (macOS 15+ required)")?;

    let stream = SCStream::new(&filter, &config).map_err(|e| e.to_string())?;
    stream.add_recording_output(&output).map_err(|e| e.to_string())?;

    let tracker = InputTracker::start();
    stream.start_capture().map_err(|e| e.to_string())?;

    // Wait for the first frame so cursor times line up with the video.
    {
        let (m, cv) = &*zero;
        let guard = m.lock().unwrap();
        let (guard, _) = cv.wait_timeout_while(guard, Duration::from_secs(5), |z| z.is_none()).unwrap();
        if guard.is_none() {
            drop(guard);
            let _ = stream.stop_capture();
            return Err("screen capture did not start (check Screen Recording permission)".into());
        }
    }
    let started_unix_ms = *started_ms.lock().unwrap();

    Ok(Session {
        stream,
        output,
        tracker,
        zero,
        finished,
        dir: opts.output_dir,
        bounds,
        width,
        height,
        started_unix_ms,
        system_audio: opts.system_audio,
        microphone: opts.microphone,
    })
}

fn end(s: Session) -> Result<StopInfo, String> {
    let zero = s.zero.0.lock().unwrap().unwrap_or_else(Instant::now);
    let elapsed_ms = zero.elapsed().as_secs_f64() * 1000.0;
    let _ = s.stream.stop_capture();
    let _ = s.stream.remove_recording_output(&s.output);
    {
        let (m, cv) = &*s.finished;
        let g = m.lock().unwrap();
        let _ = cv.wait_timeout_while(g, Duration::from_secs(3), |done| !*done).unwrap();
    }
    let d = s.output.recorded_duration();
    let mut duration_ms = if d.timescale > 0 { d.value as f64 / d.timescale as f64 * 1000.0 } else { 0.0 };
    if !(duration_ms.is_finite()) || duration_ms <= 0.0 {
        duration_ms = elapsed_ms;
    }
    let log = s.tracker.finish(zero, s.bounds);
    let json = serde_json::to_vec(&log).map_err(|e| e.to_string())?;
    std::fs::write(s.dir.join("input.json"), json).map_err(|e| e.to_string())?;
    Ok(StopInfo {
        dir: s.dir,
        duration_ms,
        width: s.width,
        height: s.height,
        bounds: s.bounds,
        started_unix_ms: s.started_unix_ms,
        has_system_audio: s.system_audio,
        has_microphone: s.microphone,
    })
}
