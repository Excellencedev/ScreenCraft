//! Preview proxy: the recorded screen / camera videos are decoded once, natively, and
//! stored as small JPEG strips that the editor reads frame by frame over the `sc://`
//! scheme. The web view then never plays or seeks a <video> for the preview, which is
//! what used to deadlock WebKit's media player.
//!
//! Files per project (all in the project folder):
//!   proxy-<kind>.sci  concatenated JPEG frames
//!   proxy-<kind>.idx  (n+1) little-endian u64 byte offsets
//!   proxy.json        {"version", "screen":{"n","fps","w","h"}, "camera":{...}}

#![allow(deprecated)]

use objc2::rc::Retained;
use objc2::runtime::AnyObject;
use objc2::AnyThread;
use objc2_av_foundation::{AVAssetReader, AVAssetReaderTrackOutput, AVURLAsset};
use objc2_core_video::{
    CVPixelBufferGetBaseAddress, CVPixelBufferGetBytesPerRow, CVPixelBufferGetHeight, CVPixelBufferGetWidth,
    CVPixelBufferLockBaseAddress, CVPixelBufferLockFlags, CVPixelBufferUnlockBaseAddress,
};
use objc2_foundation::{NSDictionary, NSNumber, NSString, NSURL};
use serde::Serialize;
use std::collections::BTreeMap;
use std::fs::File;
use std::io::{BufWriter, Write};
use std::path::Path;
use std::sync::mpsc;
use std::sync::{Arc, Mutex};

pub const FPS: f64 = 30.0;
const VERSION: u32 = 2;

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

fn obj<T: objc2::Message>(r: Retained<T>) -> Retained<AnyObject> {
    unsafe { Retained::cast_unchecked(r) }
}
fn num_i(v: i64) -> Retained<AnyObject> {
    obj(NSNumber::new_i64(v))
}
fn dict(pairs: Vec<(&str, Retained<AnyObject>)>) -> Retained<NSDictionary<NSString, AnyObject>> {
    let keys: Vec<Retained<NSString>> = pairs.iter().map(|(k, _)| NSString::from_str(k)).collect();
    let key_refs: Vec<&NSString> = keys.iter().map(|k| &**k).collect();
    let vals: Vec<Retained<AnyObject>> = pairs.into_iter().map(|(_, v)| v).collect();
    NSDictionary::from_retained_objects(&key_refs, &vals)
}

fn encode_jpeg(bgra: &[u8], w: u32, h: u32, quality: u8) -> Vec<u8> {
    let mut out = Vec::with_capacity((w * h / 8) as usize);
    let enc = jpeg_encoder::Encoder::new(&mut out, quality);
    let _ = enc.encode(bgra, w as u16, h as u16, jpeg_encoder::ColorType::Bgra);
    out
}

/// Decode `src` and write the strip. `target_w` is the proxy width in pixels.
fn build_strip(
    src: &Path,
    out_sci: &Path,
    out_idx: &Path,
    target_w: u32,
    quality: u8,
    progress: &mut dyn FnMut(f64),
) -> Result<StripMeta, String> {
    unsafe {
        let url = NSURL::fileURLWithPath(&NSString::from_str(&src.to_string_lossy()));
        let asset = AVURLAsset::URLAssetWithURL_options(&url, None);
        let vtype = NSString::from_str("vide");
        let track = asset.tracksWithMediaType(&vtype).firstObject().ok_or("no video track")?;
        let size = track.naturalSize();
        let duration = asset.duration().seconds();
        if size.width < 2.0 || size.height < 2.0 {
            return Err("empty video".into());
        }
        let w = (target_w as f64).min(size.width).round() as u32 & !1;
        let h = ((w as f64 * size.height / size.width).round() as u32) & !1;

        let reader = AVAssetReader::initWithAsset_error(AVAssetReader::alloc(), &asset)
            .map_err(|e| e.localizedDescription().to_string())?;
        let settings = dict(vec![
            ("PixelFormatType", num_i(1111970369)), // 32BGRA
            ("Width", num_i(w as i64)),
            ("Height", num_i(h as i64)),
        ]);
        let vout = AVAssetReaderTrackOutput::assetReaderTrackOutputWithTrack_outputSettings(&track, Some(&settings));
        reader.addOutput(&vout);
        if !reader.startReading() {
            return Err("could not start reading the video".into());
        }

        // encoder workers: slot -> jpeg
        let (job_tx, job_rx) = mpsc::sync_channel::<(usize, Vec<u8>)>(8);
        let job_rx = Arc::new(Mutex::new(job_rx));
        let (res_tx, res_rx) = mpsc::channel::<(usize, Vec<u8>)>();
        let workers: Vec<_> = (0..3)
            .map(|_| {
                let rx = job_rx.clone();
                let tx = res_tx.clone();
                std::thread::spawn(move || loop {
                    let job = rx.lock().unwrap().recv();
                    match job {
                        Ok((slot, px)) => {
                            let _ = tx.send((slot, encode_jpeg(&px, w, h, quality)));
                        }
                        Err(_) => break,
                    }
                })
            })
            .collect();
        drop(res_tx);

        let mut sci = BufWriter::new(File::create(out_sci).map_err(|e| e.to_string())?);
        let mut offsets: Vec<u64> = vec![0];
        let mut pending: BTreeMap<usize, Vec<u8>> = BTreeMap::new();
        let mut next_write = 0usize;
        let mut cursor = 0u64;
        let mut drain = |block: bool, pending: &mut BTreeMap<usize, Vec<u8>>, next_write: &mut usize| -> Result<(), String> {
            loop {
                let got = if block { res_rx.recv().ok() } else { res_rx.try_recv().ok() };
                match got {
                    Some((slot, jpg)) => {
                        pending.insert(slot, jpg);
                        while let Some(j) = pending.remove(next_write) {
                            sci.write_all(&j).map_err(|e| e.to_string())?;
                            cursor += j.len() as u64;
                            offsets.push(cursor);
                            *next_write += 1;
                        }
                    }
                    None => break,
                }
            }
            Ok(())
        };

        let step = 1.0 / FPS;
        let mut slot = 0usize; // next slot to emit (slot k is the frame shown at k / FPS)
        while let Some(sample) = vout.copyNextSampleBuffer() {
            let pts = sample.presentation_time_stamp().seconds();
            let Some(img) = sample.image_buffer() else { continue };
            CVPixelBufferLockBaseAddress(&img, CVPixelBufferLockFlags::ReadOnly);
            let (pw, ph) = (CVPixelBufferGetWidth(&img), CVPixelBufferGetHeight(&img));
            let stride = CVPixelBufferGetBytesPerRow(&img);
            let base = CVPixelBufferGetBaseAddress(&img) as *const u8;
            let mut px = vec![0u8; (w * h * 4) as usize];
            if !base.is_null() && pw as u32 == w && ph as u32 == h {
                for row in 0..h as usize {
                    let s = std::slice::from_raw_parts(base.add(row * stride), (w * 4) as usize);
                    px[row * (w * 4) as usize..(row + 1) * (w * 4) as usize].copy_from_slice(s);
                }
            } else if !base.is_null() {
                // the reader did not scale: nearest-neighbour fit (slow path, rare)
                for y in 0..h as usize {
                    let sy = (y * ph / h as usize).min(ph - 1);
                    for x in 0..w as usize {
                        let sx = (x * pw / w as usize).min(pw - 1);
                        let s = std::slice::from_raw_parts(base.add(sy * stride + sx * 4), 4);
                        px[(y * w as usize + x) * 4..(y * w as usize + x) * 4 + 4].copy_from_slice(s);
                    }
                }
            }
            CVPixelBufferUnlockBaseAddress(&img, CVPixelBufferLockFlags::ReadOnly);

            // emit this frame for every slot it covers up to its timestamp
            let mut emitted = false;
            while (slot as f64) * step <= pts + 0.0005 {
                // frames arriving later than the slot time stand in for it
                let data = if emitted { px.clone() } else { std::mem::take(&mut px) };
                emitted = true;
                job_tx.send((slot, data)).map_err(|e| e.to_string())?;
                slot += 1;
                drain(false, &mut pending, &mut next_write)?;
            }
            if duration > 0.0 {
                progress((pts / duration).clamp(0.0, 1.0));
            }
        }
        drop(job_tx);
        for w in workers {
            let _ = w.join();
        }
        drain(true, &mut pending, &mut next_write)?;
        sci.flush().map_err(|e| e.to_string())?;
        if next_write == 0 {
            return Err("no frames were decoded".into());
        }

        let mut idx = BufWriter::new(File::create(out_idx).map_err(|e| e.to_string())?);
        for o in &offsets {
            idx.write_all(&o.to_le_bytes()).map_err(|e| e.to_string())?;
        }
        idx.flush().map_err(|e| e.to_string())?;
        Ok(StripMeta { n: next_write, fps: FPS, w, h })
    }
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
    offsets: Vec<u64>,
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
            let offsets: Vec<u64> = idx.chunks_exact(8).map(|c| u64::from_le_bytes(c.try_into().unwrap())).collect();
            let file = File::open(base.join(format!("proxy-{kind}.sci"))).ok()?;
            let s = Arc::new(Strip { file, offsets });
            map.insert(key, s.clone());
            s
        }
    };
    if n + 1 >= strip.offsets.len() {
        // clamp to the last frame
        let last = strip.offsets.len().checked_sub(2)?;
        return frame_bytes_at(&strip, last);
    }
    frame_bytes_at(&strip, n)
}

fn frame_bytes_at(strip: &Strip, n: usize) -> Option<Vec<u8>> {
    use std::os::unix::fs::FileExt;
    let (a, b) = (*strip.offsets.get(n)?, *strip.offsets.get(n + 1)?);
    let mut buf = vec![0u8; (b - a) as usize];
    strip.file.read_exact_at(&mut buf, a).ok()?;
    Some(buf)
}

/// Forget cached handles for a project (after it is rebuilt).
pub fn forget(dir: &str) {
    if let Some(m) = STRIPS.lock().unwrap().as_mut() {
        m.retain(|k, _| !k.starts_with(&format!("{dir}|")));
    }
}

/// Build in the background and report progress to the UI.
#[tauri::command]
pub async fn ensure_proxy(app: tauri::AppHandle, dir: String, screen: String, camera: Option<String>) -> Result<ProxyMeta, String> {
    use tauri::Emitter;
    tauri::async_runtime::spawn_blocking(move || {
        forget(&dir);
        let mut last = -1.0f64;
        let r = ensure(Path::new(&dir), &screen, camera.as_deref(), |p| {
            if p - last >= 0.01 || p >= 1.0 {
                last = p;
                let _ = app.emit("proxy-progress", p);
            }
        });
        forget(&dir);
        r
    })
    .await
    .map_err(|e| e.to_string())?
}
