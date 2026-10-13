//! Preview proxy: the recorded screen / camera videos are decoded once, natively, and
//! stored as small JPEG strips that the editor reads frame by frame over the `sc://`
//! scheme. The web view then never plays or seeks a <video> for the preview, which is
//! what used to deadlock WebKit's media player.
//!
//! Files per project (all in the project folder):
//!   proxy-<kind>.sci  concatenated JPEG frames
//!   proxy-<kind>.idx  (n+1) little-endian u64 byte offsets
//!   proxy.json        {"version", "screen":{"n","fps","w","h"}, "camera":{...}}

use serde::Serialize;
use std::fs::File;
use std::path::Path;
use std::sync::{Arc, Mutex};

#[cfg(target_os = "macos")]
mod macos;
#[cfg(target_os = "macos")]
use macos::build_strip;
#[cfg(target_os = "macos")]
pub use macos::unwrapped_seconds;

#[cfg(windows)]
mod windows;
#[cfg(windows)]
use windows::build_strip;

pub const FPS: f64 = 30.0;
const VERSION: u32 = 5;

#[derive(Serialize, Clone, Debug, Default)]
pub struct StripMeta {
    pub n: usize,
    pub fps: f64,
    pub w: u32,
    pub h: u32,
}

#[derive(Serialize, Clone, Debug, Default)]
pub struct ProxyMeta {
    pub version: u32,
    pub screen: Option<StripMeta>,
    pub camera: Option<StripMeta>,
}

fn meta_path(dir: &Path) -> std::path::PathBuf {
    dir.join("proxy.json")
}

fn newer(a: &Path, than: &Path) -> bool {
    match (std::fs::metadata(a).and_then(|m| m.modified()), std::fs::metadata(than).and_then(|m| m.modified())) {
        (Ok(x), Ok(y)) => x >= y,
        _ => false,
    }
}

/// Build (or reuse) the proxy for a project. `progress` gets 0..1 over both strips.
pub fn ensure(
    dir: &Path,
    screen_rel: &str,
    camera_rel: Option<&str>,
    mut progress: impl FnMut(f64),
) -> Result<ProxyMeta, String> {
    let screen_src = dir.join(screen_rel);
    let cam_src = camera_rel.map(|c| dir.join(c));

    // reuse
    if let Ok(bytes) = std::fs::read(meta_path(dir)) {
        if let Ok(v) = serde_json::from_slice::<serde_json::Value>(&bytes) {
            let fresh = v["version"].as_u64() == Some(VERSION as u64)
                && newer(&meta_path(dir), &screen_src)
                && cam_src.as_ref().map(|c| newer(&meta_path(dir), c)).unwrap_or(true)
                && dir.join("proxy-screen.idx").exists();
            if fresh {
                let strip = |k: &str| -> Option<StripMeta> {
                    let s = v.get(k)?;
                    Some(StripMeta {
                        n: s["n"].as_u64()? as usize,
                        fps: s["fps"].as_f64()?,
                        w: s["w"].as_u64()? as u32,
                        h: s["h"].as_u64()? as u32,
                    })
                };
                return Ok(ProxyMeta { version: VERSION, screen: strip("screen"), camera: strip("camera") });
            }
        }
    }

    let two = cam_src.is_some();
    let screen = build_strip(
        &screen_src,
        &dir.join("proxy-screen.sci"),
        &dir.join("proxy-screen.idx"),
        1280,
        62,
        &mut |p| progress(if two { p * 0.7 } else { p }),
    )?;
    let camera = match &cam_src {
        Some(c) => build_strip(c, &dir.join("proxy-camera.sci"), &dir.join("proxy-camera.idx"), 480, 60, &mut |p| {
            progress(0.7 + p * 0.3)
        })
        .ok(),
        None => None,
    };
    let meta = ProxyMeta { version: VERSION, screen: Some(screen), camera };
    std::fs::write(meta_path(dir), serde_json::to_vec(&meta).map_err(|e| e.to_string())?).map_err(|e| e.to_string())?;
    progress(1.0);
    Ok(meta)
}

// ------------------------------------------------------------------ serving

/// Open strips by "<dir>|<kind>" for the `sc://` scheme.
pub struct Strip {
    file: File,
    ranges: Vec<(u64, u64)>,
}

static STRIPS: Mutex<Option<std::collections::HashMap<String, Arc<Strip>>>> = Mutex::new(None);

pub fn frame_bytes(dir: &str, kind: &str, n: usize) -> Option<Vec<u8>> {
    let key = format!("{dir}|{kind}");
    let strip = {
        let mut g = STRIPS.lock().unwrap();
        let map = g.get_or_insert_with(Default::default);
        if let Some(s) = map.get(&key) {
            s.clone()
        } else {
            let base = Path::new(dir);
            let idx = std::fs::read(base.join(format!("proxy-{kind}.idx"))).ok()?;
            let ranges: Vec<(u64, u64)> = idx
                .chunks_exact(16)
                .map(|c| (u64::from_le_bytes(c[..8].try_into().unwrap()), u64::from_le_bytes(c[8..].try_into().unwrap())))
                .collect();
            let file = File::open(base.join(format!("proxy-{kind}.sci"))).ok()?;
            let s = Arc::new(Strip { file, ranges });
            map.insert(key, s.clone());
            s
        }
    };
    let n = n.min(strip.ranges.len().checked_sub(1)?);
    let (a, b) = strip.ranges[n];
    let mut buf = vec![0u8; (b - a) as usize];
    #[cfg(unix)]
    {
        use std::os::unix::fs::FileExt;
        strip.file.read_exact_at(&mut buf, a).ok()?;
    }
    #[cfg(windows)]
    {
        use std::os::windows::fs::FileExt;
        let mut done = 0;
        while done < buf.len() {
            match strip.file.seek_read(&mut buf[done..], a + done as u64).ok()? {
                0 => return None,
                n => done += n,
            }
        }
    }
    Some(buf)
}

/// Forget cached handles for a project (after it is rebuilt).
pub fn forget(dir: &str) {
    if let Some(m) = STRIPS.lock().unwrap().as_mut() {
        m.retain(|k, _| !k.starts_with(&format!("{dir}|")));
    }
}

/// Projects whose proxy is being built right now, with the latest progress. Lets the
/// build that starts when a recording stops be shared with the editor that opens on it.
static BUILDING: Mutex<Option<std::collections::HashMap<String, f64>>> = Mutex::new(None);

fn begin(dir: &str) -> bool {
    let mut g = BUILDING.lock().unwrap();
    let m = g.get_or_insert_with(Default::default);
    if m.contains_key(dir) {
        false
    } else {
        m.insert(dir.to_string(), 0.0);
        true
    }
}

fn set_progress(dir: &str, p: f64) {
    if let Some(m) = BUILDING.lock().unwrap().as_mut() {
        if let Some(v) = m.get_mut(dir) {
            *v = p;
        }
    }
}

fn end(dir: &str) {
    if let Some(m) = BUILDING.lock().unwrap().as_mut() {
        m.remove(dir);
    }
}

fn progress_of(dir: &str) -> Option<f64> {
    BUILDING.lock().unwrap().as_ref().and_then(|m| m.get(dir).copied())
}

fn build_tracked(app: &tauri::AppHandle, dir: &str, screen: &str, camera: Option<&str>) -> Result<ProxyMeta, String> {
    use tauri::Emitter;
    forget(dir);
    let mut last = -1.0f64;
    let r = ensure(Path::new(dir), screen, camera, |p| {
        set_progress(dir, p);
        if p - last >= 0.01 || p >= 1.0 {
            last = p;
            let _ = app.emit("proxy-progress", p);
        }
    });
    forget(dir);
    r
}

/// Kick off the proxy build (and mic levelling) as soon as a recording is saved, so
/// it is usually finished by the time the editor opens.
pub fn start_background(app: tauri::AppHandle, dir: String, screen: String, camera: Option<String>, mic: Option<String>) {
    if !begin(&dir) {
        return;
    }
    std::thread::spawn(move || {
        let r = build_tracked(&app, &dir, &screen, camera.as_deref());
        end(&dir);
        if let Err(e) = r {
            eprintln!("[proxy] background build failed: {e}");
        }
        if let Some(m) = mic {
            let _ = crate::levelmic::level(Path::new(&dir), &m);
        }
    });
}

/// Build in the background and report progress to the UI. If the build is already
/// running (started when the recording stopped), just follow it.
#[tauri::command]
pub async fn ensure_proxy(app: tauri::AppHandle, dir: String, screen: String, camera: Option<String>) -> Result<ProxyMeta, String> {
    use tauri::Emitter;
    tauri::async_runtime::spawn_blocking(move || {
        loop {
            if begin(&dir) {
                let r = build_tracked(&app, &dir, &screen, camera.as_deref());
                end(&dir);
                return r;
            }
            if let Some(p) = progress_of(&dir) {
                let _ = app.emit("proxy-progress", p);
            }
            std::thread::sleep(std::time::Duration::from_millis(120));
        }
    })
    .await
    .map_err(|e| e.to_string())?
}
