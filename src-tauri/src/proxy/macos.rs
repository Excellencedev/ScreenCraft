//! Proxy strip decoding through AVFoundation.

#![allow(deprecated)]

use super::{StripMeta, FPS};
use objc2::rc::Retained;
use objc2::runtime::AnyObject;
use objc2::AnyThread;
use objc2_av_foundation::{AVAssetReader, AVAssetReaderTrackOutput, AVURLAsset};
use objc2_core_video::{
    CVPixelBufferGetBaseAddress, CVPixelBufferGetBytesPerRow, CVPixelBufferGetHeight, CVPixelBufferGetWidth,
    CVPixelBufferLockBaseAddress, CVPixelBufferLockFlags, CVPixelBufferUnlockBaseAddress,
};
use objc2_foundation::{NSDictionary, NSNumber, NSString, NSURL};
use std::collections::BTreeMap;
use std::fs::File;
use std::io::{BufWriter, Write};
use std::path::Path;
use std::sync::mpsc;
use std::sync::{Arc, Mutex};

/// Presentation time in seconds with 32-bit timestamp wrap-around undone.
///
/// Browser-recorded fragmented MP4s sometimes carry negative composition offsets stored
/// as unsigned numbers, which AVFoundation reads as 2^32 ticks too late (about 7,158,278 s
/// at a 600 Hz timescale). Everything after that point then looked "in the far future" and
/// the facecam froze. Fold such values back by whole multiples of 2^32 ticks.
pub fn unwrapped_seconds(t: objc2_core_media::CMTime) -> f64 {
    let (v, ts) = (t.value, t.timescale as f64);
    if ts <= 0.0 {
        return unsafe { t.seconds() };
    }
    const WRAP: i64 = 1 << 32;
    let v = if v >= WRAP / 2 { v - WRAP * ((v + WRAP / 2) / WRAP) } else { v };
    v as f64 / ts
}

/// The raw (still wrapped) time to ask AVAssetReader for when a sample was seen at `t`.
pub fn raw_seconds(t: objc2_core_media::CMTime) -> f64 {
    unsafe { t.seconds() }
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

fn encode_jpeg(bgra: Vec<u8>, w: u32, h: u32, quality: u8) -> Vec<u8> {
    crate::jpegio::encode_bgra(bgra, w as usize, h as usize, quality).unwrap_or_default()
}

/// Decode `src` and write the strip. `target_w` is the proxy width in pixels.
pub fn build_strip(
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

        let settings = dict(vec![
            ("PixelFormatType", num_i(1111970369)), // 32BGRA
            ("Width", num_i(w as i64)),
            ("Height", num_i(h as i64)),
        ]);
        // (re)open a reader, optionally starting part-way (used to resume after an early end)
        let open_reader = |start_s: f64| -> Result<(Retained<AVAssetReader>, Retained<AVAssetReaderTrackOutput>), String> {
            let reader = AVAssetReader::initWithAsset_error(AVAssetReader::alloc(), &asset)
                .map_err(|e| e.localizedDescription().to_string())?;
            let out = AVAssetReaderTrackOutput::assetReaderTrackOutputWithTrack_outputSettings(&track, Some(&settings));
            reader.addOutput(&out);
            if start_s > 0.0 {
                let span = if duration.is_finite() && duration > start_s { duration - start_s } else { 86_400.0 };
                reader.setTimeRange(objc2_core_media::CMTimeRange {
                    start: objc2_core_media::CMTime::new((start_s * 1000.0) as i64, 1000),
                    duration: objc2_core_media::CMTime::new(((span + 1.0) * 1000.0) as i64, 1000),
                });
            }
            if !reader.startReading() {
                return Err("could not start reading the video".into());
            }
            Ok((reader, out))
        };
        let (mut reader, mut vout) = open_reader(0.0)?;

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
                            let jpg = encode_jpeg(px, w, h, quality);
                            let _ = tx.send((slot, jpg));
                        }
                        Err(_) => break,
                    }
                })
            })
            .collect();
        drop(res_tx);

        eprintln!("[proxy] {} -> {w}x{h} (source {}x{}, {:.1}s)", src.display(), size.width, size.height, duration);
        let mut sci = BufWriter::new(File::create(out_sci).map_err(|e| e.to_string())?);
        // (start, end) byte range per slot; slots that repeat the previous frame reuse its range
        let mut ranges: Vec<(u64, u64)> = Vec::new();
        let mut pending: BTreeMap<usize, Vec<u8>> = BTreeMap::new();
        let mut dups: std::collections::BTreeSet<usize> = std::collections::BTreeSet::new();
        let mut next_write = 0usize;
        let mut cursor = 0u64;
        let mut flush_ready = |pending: &mut BTreeMap<usize, Vec<u8>>,
                               dups: &mut std::collections::BTreeSet<usize>,
                               next_write: &mut usize,
                               ranges: &mut Vec<(u64, u64)>,
                               cursor: &mut u64|
         -> Result<(), String> {
            loop {
                if dups.remove(next_write) && !ranges.is_empty() {
                    let last = *ranges.last().unwrap();
                    ranges.push(last);
                    *next_write += 1;
                } else if let Some(j) = pending.remove(next_write) {
                    sci.write_all(&j).map_err(|e| e.to_string())?;
                    ranges.push((*cursor, *cursor + j.len() as u64));
                    *cursor += j.len() as u64;
                    *next_write += 1;
                } else {
                    break;
                }
            }
            Ok(())
        };

        let step = 1.0 / FPS;
        let mut slot = 0usize; // next slot to emit (slot k is the frame shown at k / FPS)
        let mut shift = 0.0f64; // timestamp discontinuity compensation
        let mut last_raw = 0.0f64;
        let mut last_file_time = 0.0f64;
        let mut resumes = 0;
        'read: loop {
        while let Some(sample) = vout.copyNextSampleBuffer() {
            let raw_pts = unwrapped_seconds(sample.presentation_time_stamp());
            if !raw_pts.is_finite() {
                continue;
            }
            last_raw = raw_pts;
            last_file_time = raw_seconds(sample.presentation_time_stamp());
            let Some(img) = sample.image_buffer() else { continue };
            let mut pts = raw_pts - shift;
            let want_t = slot as f64 * step;
            if pts - want_t > 2.0 {
                // a long timestamp jump (some recorders emit them): don't pad it, just continue
                shift += pts - want_t;
                pts = want_t;
            }
            if want_t > pts + 0.0005 {
                continue; // not yet time for the next slot
            }
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
            } else if !base.is_null() && pw > 0 && ph > 0 {
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

            // this frame covers every slot up to its timestamp; only the first needs encoding
            let mut first = true;
            while (slot as f64) * step <= pts + 0.0005 {
                if first {
                    job_tx.send((slot, std::mem::take(&mut px))).map_err(|e| e.to_string())?;
                    first = false;
                } else {
                    dups.insert(slot);
                }
                slot += 1;
            }
            while let Ok((sl, jpg)) = res_rx.try_recv() {
                pending.insert(sl, jpg);
            }
            flush_ready(&mut pending, &mut dups, &mut next_write, &mut ranges, &mut cursor)?;
            if duration > 0.0 && duration < 86_400.0 {
                progress((raw_pts / duration).clamp(0.0, 1.0));
            }
        }
        // the reader sometimes ends well before the clip does (damaged / odd timestamps):
        // pick the file up again just past the last frame instead of freezing the strip
        if duration.is_finite() && duration < 86_400.0 && last_raw < duration - 1.5 && resumes < 40 {
            resumes += 1;
            match open_reader(last_file_time + 0.05) {
                Ok((r, o)) => {
                    reader = r;
                    vout = o;
                    continue 'read;
                }
                Err(_) => {}
            }
        }
        break 'read;
        }
        let _ = &reader;
        drop(job_tx);
        for wk in workers {
            let _ = wk.join();
        }
        while let Ok((sl, jpg)) = res_rx.recv() {
            pending.insert(sl, jpg);
        }
        flush_ready(&mut pending, &mut dups, &mut next_write, &mut ranges, &mut cursor)?;
        sci.flush().map_err(|e| e.to_string())?;
        if next_write == 0 || ranges.is_empty() {
            return Err("no frames were decoded".into());
        }

        let mut idx = BufWriter::new(File::create(out_idx).map_err(|e| e.to_string())?);
        for (a, b) in &ranges {
            idx.write_all(&a.to_le_bytes()).map_err(|e| e.to_string())?;
            idx.write_all(&b.to_le_bytes()).map_err(|e| e.to_string())?;
        }
        idx.flush().map_err(|e| e.to_string())?;
        Ok(StripMeta { n: ranges.len(), fps: FPS, w, h })
    }
}
