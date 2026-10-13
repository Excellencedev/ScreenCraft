//! Screen recording on Windows: Windows.Graphics.Capture frames are encoded to
//! `screen.mp4` (H.264 through Media Foundation, see `writer.rs`), with system audio
//! from a WASAPI loopback as an AAC track, while the input tracker logs the cursor.
//!
//! Time zero is the first captured frame. Frames, audio packets and the "now" used to
//! fill gaps all use the performance counter in 100 ns units.

use super::{StartInfo, StartOpts, StopInfo};
use crate::inputlog::InputTracker;
use crate::monitors::{self, Display};
use crate::wasapi::{qpc_now, Loopback};
use ::windows::Win32::Foundation::{HWND, RECT};
use screencraft_core::project::Rect;
use std::path::PathBuf;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Condvar, Mutex};
use std::thread::JoinHandle;
use std::time::{Duration, Instant};

mod wgc;
mod writer;

use writer::Writer;

/// Audio is written up to this far behind "now" when no packets arrive (silence),
/// leaving room for late loopback packets.
const AUDIO_LAG_HNS: i64 = 2_000_000;
/// While the screen does not change, the last frame is repeated this often.
const IDLE_REPEAT_HNS: i64 = 5_000_000;

#[derive(Clone, Default)]
pub struct Capture {
    session: Arc<Mutex<Option<Session>>>,
}

struct Shared {
    writer: Option<Writer>,
    /// Capture time of the first frame.
    t0: Option<i64>,
    /// Wall clock and `Instant` of the first frame.
    first: Option<(f64, Instant)>,
    last_written: i64,
    min_gap: i64,
    crop: Option<RECT>,
    /// Audio sample frames written so far (the audio clock).
    audio_pos: u64,
    error: Option<String>,
}

struct Session {
    source: wgc::Source,
    shared: Arc<(Mutex<Shared>, Condvar)>,
    pump: Option<(JoinHandle<()>, Arc<AtomicBool>)>,
    tracker: InputTracker,
    dir: PathBuf,
    bounds: Rect,
    width: u32,
    height: u32,
    system_audio: bool,
}

fn unix_ms() -> f64 {
    std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).map(|d| d.as_secs_f64() * 1000.0).unwrap_or(0.0)
}

fn even(v: f64) -> u32 {
    let n = v.round().max(2.0) as u32;
    n - (n % 2)
}

impl Capture {
    pub fn spawn() -> Self {
        Self::default()
    }

    pub fn start(&self, opts: StartOpts) -> Result<StartInfo, String> {
        let mut guard = self.session.lock().unwrap();
        if guard.is_some() {
            return Err("already recording".into());
        }
        let s = begin(opts)?;
        let first = s.shared.0.lock().unwrap().first;
        let info = StartInfo {
            width: s.width,
            height: s.height,
            bounds: s.bounds,
            started_unix_ms: first.map(|f| f.0).unwrap_or_else(unix_ms),
        };
        *guard = Some(s);
        Ok(info)
    }

    pub fn stop(&self) -> Result<StopInfo, String> {
        let s = self.session.lock().unwrap().take().ok_or("not recording")?;
        end(s)
    }
}

/// What to capture and how: target, display (for coordinates), crop in the
/// target's pixels, and the recorded area in logical coordinates.
fn resolve(opts: &StartOpts) -> Result<(wgc::Target, Display, Option<RECT>, Rect, (f64, f64)), String> {
    match opts.mode.as_str() {
        "window" => {
            let id = opts.window_id.ok_or("window id missing")?;
            let hwnd = HWND(id as usize as *mut std::ffi::c_void);
            let d = monitors::display_of_window(hwnd).ok_or("window not found")?;
            let (x, y, w, h) = monitors::window_frame(hwnd).ok_or("window not found")?;
            let (lx, ly) = d.to_logical(x as f64, y as f64);
            let bounds = Rect { x: lx, y: ly, width: w as f64 / d.scale, height: h as f64 / d.scale };
            Ok((wgc::Target::Window(hwnd), d, None, bounds, (w as f64, h as f64)))
        }
        mode => {
            let d = monitors::display(opts.display_id).ok_or("display not found")?;
            let (lx, ly, lw, lh) = d.logical();
            if mode == "area" {
                let a = opts.area.clone().ok_or("area missing")?;
                let (px, py) = d.to_physical(a.x, a.y);
                let (ox, oy) = (px - d.phys.0 as f64, py - d.phys.1 as f64);
                let (pw, ph) = (a.width * d.scale, a.height * d.scale);
                let crop = RECT {
                    left: ox.round() as i32,
                    top: oy.round() as i32,
                    right: (ox + pw).round() as i32,
                    bottom: (oy + ph).round() as i32,
                };
                let bounds = Rect { x: a.x, y: a.y, width: a.width, height: a.height };
                Ok((wgc::Target::Display(d.handle), d, Some(crop), bounds, (pw, ph)))
            } else {
                let bounds = Rect { x: lx, y: ly, width: lw, height: lh };
                Ok((wgc::Target::Display(d.handle), d, None, bounds, (d.phys.2 as f64, d.phys.3 as f64)))
            }
        }
    }
}

fn begin(opts: StartOpts) -> Result<Session, String> {
    let (target, display, crop, bounds, (pw, ph)) = resolve(&opts)?;
    // Resolution is a cap on the long side in logical pixels (never upscaled).
    let long = bounds.width.max(bounds.height).max(1.0);
    let k = match opts.max_long_side {
        Some(m) if m > 0 => (m as f64 / long).min(pw / bounds.width.max(1.0)),
        _ => pw / bounds.width.max(1.0),
    };
    let (width, height) = (even(bounds.width * k).min(even(pw)), even(bounds.height * k).min(even(ph)));
    let fps = opts.fps.clamp(10, 120);

    std::fs::create_dir_all(&opts.output_dir).map_err(|e| e.to_string())?;
    let movie = opts.output_dir.join("screen.mp4");
    let loopback = if opts.system_audio { Some(Loopback::open()?) } else { None };
    let device = wgc::create_device()?;
    let writer = Writer::new(&device, &movie, width, height, fps, loopback.as_ref().map(|l| out_rate(l.rate)))?;

    let shared = Arc::new((
        Mutex::new(Shared {
            writer: Some(writer),
            t0: None,
            first: None,
            last_written: i64::MIN,
            min_gap: 10_000_000 / fps as i64 - 20_000,
            crop,
            audio_pos: 0,
            error: None,
        }),
        Condvar::new(),
    ));
    let tracker = InputTracker::start(display);
    let sh = shared.clone();
    let source = wgc::Source::start(&device, target, move |frame| {
        let (m, cv) = &*sh;
        let mut s = m.lock().unwrap();
        if s.error.is_some() {
            return;
        }
        let t0 = match s.t0 {
            Some(t) => t,
            None => {
                let behind = Duration::from_nanos(((qpc_now() - frame.time).max(0) * 100) as u64);
                s.first = Some((unix_ms() - behind.as_secs_f64() * 1000.0, Instant::now() - behind));
                s.t0 = Some(frame.time);
                cv.notify_all();
                frame.time
            }
        };
        // the display may refresh faster than the chosen frame rate
        if frame.time.saturating_sub(s.last_written) < s.min_gap {
            return;
        }
        s.last_written = frame.time;
        let crop = s.crop;
        if let Some(w) = s.writer.as_mut() {
            if let Err(e) = w.frame(frame.texture, frame.size, crop, frame.time - t0) {
                eprintln!("[capture] {e}");
                s.error = Some(e);
            }
        }
    });
    let source = match source {
        Ok(s) => s,
        Err(e) => {
            let _ = tracker.finish(Instant::now(), bounds);
            return Err(e);
        }
    };

    // Wait for the first frame so cursor times line up with the video.
    {
        let (m, cv) = &*shared;
        let g = m.lock().unwrap();
        let (g, _) = cv.wait_timeout_while(g, Duration::from_secs(5), |s| s.t0.is_none()).unwrap();
        if g.t0.is_none() {
            drop(g);
            source.stop();
            let _ = tracker.finish(Instant::now(), bounds);
            return Err("screen capture did not start".into());
        }
    }

    let stop = Arc::new(AtomicBool::new(false));
    let (sh, st) = (shared.clone(), stop.clone());
    let pump = std::thread::Builder::new()
        .name("screencraft-capture-pump".into())
        .spawn(move || pump(sh, st, loopback))
        .map_err(|e| e.to_string())?;

    Ok(Session {
        source,
        shared,
        pump: Some((pump, stop)),
        tracker,
        dir: opts.output_dir,
        bounds,
        width,
        height,
        system_audio: opts.system_audio,
    })
}

/// The AAC encoder takes 44.1 or 48 kHz; anything else is resampled to 48 kHz.
fn out_rate(device_rate: u32) -> u32 {
    if device_rate == 44_100 { 44_100 } else { 48_000 }
}

/// Stereo f32 at `from` Hz -> 16-bit stereo at `to` Hz (linear interpolation).
fn to_pcm16(stereo: &[f32], from: u32, to: u32) -> Vec<i16> {
    let q = |v: f32| (v.clamp(-1.0, 1.0) * 32767.0) as i16;
    if from == to {
        return stereo.iter().map(|v| q(*v)).collect();
    }
    let n_in = stereo.len() / 2;
    let n_out = (n_in as u64 * to as u64 / from as u64) as usize;
    let mut out = Vec::with_capacity(n_out * 2);
    for i in 0..n_out {
        let p = i as f64 * from as f64 / to as f64;
        let (a, f) = (p.floor() as usize, (p - p.floor()) as f32);
        let b = (a + 1).min(n_in.saturating_sub(1));
        for c in 0..2 {
            out.push(q(stereo[a * 2 + c] * (1.0 - f) + stereo[b * 2 + c] * f));
        }
    }
    out
}

/// Write audio so the track reaches `upto` sample frames: `pcm` (16-bit stereo) placed
/// at `at` frames, with silence before it if there is a gap and any overlap with what
/// was already written dropped.
fn place_audio(s: &mut Shared, rate: u32, pcm: &[i16], at: i64) {
    let Some(w) = s.writer.as_mut() else { return };
    let mut pos = s.audio_pos as i64;
    let hns = |frames: i64| frames * 10_000_000 / rate as i64;
    if at > pos {
        let mut gap = at - pos;
        while gap > 0 {
            let n = gap.min(rate as i64 / 10);
            let _ = w.audio(&vec![0i16; n as usize * 2], hns(pos));
            pos += n;
            gap -= n;
        }
    }
    let skip = (pos - at).max(0) as usize;
    if skip * 2 < pcm.len() {
        let rest = &pcm[skip * 2..];
        let _ = w.audio(rest, hns(pos));
        pos += (rest.len() / 2) as i64;
    }
    s.audio_pos = pos as u64;
}

fn pump(shared: Arc<(Mutex<Shared>, Condvar)>, stop: Arc<AtomicBool>, mut loopback: Option<Loopback>) {
    crate::mf::init_thread();
    let rate_in = loopback.as_ref().map(|l| l.rate).unwrap_or(48_000);
    let rate = out_rate(rate_in);
    while !stop.load(Ordering::SeqCst) {
        if let Some(lb) = loopback.as_mut() {
            let mut packets: Vec<(Vec<i16>, i64)> = Vec::new();
            let _ = lb.read(|stereo, qpc| packets.push((to_pcm16(stereo, rate_in, rate), qpc)));
            let mut s = shared.0.lock().unwrap();
            if let Some(t0) = s.t0 {
                for (pcm, qpc) in packets {
                    let at = (qpc - t0) as i128 * rate as i128 / 10_000_000;
                    place_audio(&mut s, rate, &pcm, at as i64);
                }
                // nothing is playing: keep the track going with silence
                let upto = (qpc_now() - t0 - AUDIO_LAG_HNS) as i128 * rate as i128 / 10_000_000;
                if upto > s.audio_pos as i128 {
                    place_audio(&mut s, rate, &[], upto as i64);
                }
            }
        }
        {
            let mut s = shared.0.lock().unwrap();
            if let (Some(t0), Some(w)) = (s.t0, s.writer.as_mut()) {
                let now = qpc_now() - t0;
                if w.last_frame_time().map(|t| now - t > IDLE_REPEAT_HNS).unwrap_or(false) {
                    let _ = w.repeat(now);
                }
            }
        }
        std::thread::sleep(Duration::from_millis(10));
    }
    // final audio up to the end of the recording
    if let Some(lb) = loopback.as_mut() {
        let mut packets: Vec<(Vec<i16>, i64)> = Vec::new();
        let _ = lb.read(|stereo, qpc| packets.push((to_pcm16(stereo, rate_in, rate), qpc)));
        let mut s = shared.0.lock().unwrap();
        if let Some(t0) = s.t0 {
            for (pcm, qpc) in packets {
                let at = (qpc - t0) as i128 * rate as i128 / 10_000_000;
                place_audio(&mut s, rate, &pcm, at as i64);
            }
            let upto = (qpc_now() - t0) as i128 * rate as i128 / 10_000_000;
            place_audio(&mut s, rate, &[], upto as i64);
        }
    }
}

fn end(mut s: Session) -> Result<StopInfo, String> {
    let stop_qpc = qpc_now();
    s.source.stop();
    if let Some((h, stop)) = s.pump.take() {
        stop.store(true, Ordering::SeqCst);
        let _ = h.join();
    }
    let (writer, t0, first, error) = {
        let mut g = s.shared.0.lock().unwrap();
        (g.writer.take(), g.t0, g.first, g.error.take())
    };
    let t0 = t0.unwrap_or(stop_qpc);
    let end_hns = (stop_qpc - t0).max(0);
    if let Some(mut w) = writer {
        // hold the last picture until the moment recording stopped
        let _ = w.repeat(end_hns);
        w.finish()?;
    }
    if let Some(e) = error {
        return Err(format!("recording failed: {e}"));
    }
    let (started_unix_ms, zero) = first.unwrap_or_else(|| (unix_ms(), Instant::now()));
    let log = s.tracker.finish(zero, s.bounds);
    let json = serde_json::to_vec(&log).map_err(|e| e.to_string())?;
    std::fs::write(s.dir.join("input.json"), json).map_err(|e| e.to_string())?;
    Ok(StopInfo {
        dir: s.dir,
        duration_ms: end_hns as f64 / 10_000.0,
        width: s.width,
        height: s.height,
        bounds: s.bounds,
        started_unix_ms,
        has_system_audio: s.system_audio,
        has_microphone: false,
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::capture::AreaRect;

    /// Records the primary display for `SC_TEST_SECONDS` (default 3) into
    /// `SC_TEST_RECORD`. `SC_TEST_MODE=area` records a 640x360 area at (100, 100),
    /// `SC_TEST_MODE=window` the window whose title contains `SC_TEST_WINDOW`.
    #[test]
    fn record_display() {
        let Ok(dir) = std::env::var("SC_TEST_RECORD") else {
            eprintln!("SC_TEST_RECORD not set, skipping");
            return;
        };
        let secs: f64 = std::env::var("SC_TEST_SECONDS").ok().and_then(|s| s.parse().ok()).unwrap_or(3.0);
        let mode = std::env::var("SC_TEST_MODE").unwrap_or_else(|_| "display".into());
        let area = mode == "area";
        // the app is per-monitor DPI aware (Tauri's manifest); the test binary is not
        unsafe {
            let _ = ::windows::Win32::UI::HiDpi::SetProcessDpiAwarenessContext(
                ::windows::Win32::UI::HiDpi::DPI_AWARENESS_CONTEXT_PER_MONITOR_AWARE_V2,
            );
        }
        let d = monitors::display(None).unwrap();
        let (lx, ly, _, _) = d.logical();
        let cap = Capture::spawn();
        let t = Instant::now();
        let info = cap
            .start(StartOpts {
                mode: mode.clone(),
                display_id: None,
                // SC_TEST_MODE=window records the window titled SC_TEST_WINDOW
                window_id: (mode == "window").then(|| {
                    let title = std::env::var("SC_TEST_WINDOW").unwrap_or_default();
                    let w = monitors::windows().into_iter().find(|w| w.title.contains(&title)).expect("window");
                    eprintln!("window: {} ({}) {:?}", w.title, w.app, w.phys);
                    w.hwnd.0 as usize as u32
                }),
                area: area.then(|| AreaRect { x: lx + 100.0, y: ly + 100.0, width: 640.0, height: 360.0 }),
                system_audio: std::env::var("SC_TEST_AUDIO").ok().as_deref() != Some("0"),
                microphone: false,
                microphone_id: None,
                fps: 60,
                max_long_side: Some(1920),
                output_dir: dir.into(),
            })
            .unwrap();
        eprintln!("started in {:.0} ms: {info:?}", t.elapsed().as_secs_f64() * 1000.0);
        std::thread::sleep(Duration::from_secs_f64(secs));
        let stop = cap.stop().unwrap();
        eprintln!("stopped: {stop:?}");
        assert!((stop.duration_ms - secs * 1000.0).abs() < 400.0, "duration {}", stop.duration_ms);
    }
}
