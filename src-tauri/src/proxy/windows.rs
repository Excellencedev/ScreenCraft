//! Proxy strip decoding through Media Foundation (scaled by MF's video processor)
//! with JPEG encoding through WIC on a few worker threads.

use super::{StripMeta, FPS};
use crate::mf::Reader;
use std::collections::{BTreeMap, BTreeSet};
use std::fs::File;
use std::io::{BufWriter, Write};
use std::path::Path;
use std::sync::mpsc;
use std::sync::{Arc, Mutex};

/// Decode `src` and write the strip. `target_w` is the proxy width in pixels.
pub fn build_strip(
    src: &Path,
    out_sci: &Path,
    out_idx: &Path,
    target_w: u32,
    quality: u8,
    progress: &mut dyn FnMut(f64),
) -> Result<StripMeta, String> {
    let path = src.to_string_lossy().into_owned();
    let mut reader = Reader::open(&path, Some(target_w))?;
    let (w, h, duration) = (reader.w, reader.h, reader.duration);
    eprintln!("[proxy] {} -> {w}x{h} (source {}x{}, {:.1}s)", src.display(), reader.src_w, reader.src_h, duration);

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
                        let jpg = crate::jpegio::encode_bgra(px, w as usize, h as usize, quality).unwrap_or_default();
                        let _ = tx.send((slot, jpg));
                    }
                    Err(_) => break,
                }
            })
        })
        .collect();
    drop(res_tx);

    let mut sci = BufWriter::new(File::create(out_sci).map_err(|e| e.to_string())?);
    // (start, end) byte range per slot; slots that repeat the previous frame reuse its range
    let mut ranges: Vec<(u64, u64)> = Vec::new();
    let mut pending: BTreeMap<usize, Vec<u8>> = BTreeMap::new();
    let mut dups: BTreeSet<usize> = BTreeSet::new();
    let mut next_write = 0usize;
    let mut cursor = 0u64;
    let mut flush_ready = |pending: &mut BTreeMap<usize, Vec<u8>>,
                           dups: &mut BTreeSet<usize>,
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
    let mut resumes = 0;
    'read: loop {
        while let Some(frame) = reader.next() {
            let raw_pts = frame.pts;
            if !raw_pts.is_finite() {
                continue;
            }
            last_raw = raw_pts;
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
            let mut px = if frame.w as u32 == w && frame.h as u32 == h {
                frame.bgra
            } else {
                fit(&frame.bgra, frame.w, frame.h, w as usize, h as usize)
            };

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
        if duration > 0.0 && duration < 86_400.0 && last_raw < duration - 1.5 && resumes < 40 {
            resumes += 1;
            if reader.seek(last_raw + 0.05) {
                continue 'read;
            }
        }
        break 'read;
    }
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

/// Nearest-neighbour fit, for the rare stream whose frame size changes mid-way.
fn fit(src: &[u8], sw: usize, sh: usize, w: usize, h: usize) -> Vec<u8> {
    let mut px = vec![0u8; w * h * 4];
    if sw == 0 || sh == 0 {
        return px;
    }
    for y in 0..h {
        let sy = (y * sh / h).min(sh - 1);
        for x in 0..w {
            let sx = (x * sw / w).min(sw - 1);
            px[(y * w + x) * 4..(y * w + x) * 4 + 4].copy_from_slice(&src[(sy * sw + sx) * 4..(sy * sw + sx) * 4 + 4]);
        }
    }
    px
}

#[cfg(test)]
mod tests {
    /// Builds a strip from `SC_TEST_VIDEO` into a temp dir and checks its frames.
    #[test]
    fn strip_from_video() {
        let Ok(video) = std::env::var("SC_TEST_VIDEO") else {
            eprintln!("SC_TEST_VIDEO not set, skipping");
            return;
        };
        let dir = std::env::temp_dir().join(format!("sc-proxy-test-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        let t = std::time::Instant::now();
        let meta = super::build_strip(
            std::path::Path::new(&video),
            &dir.join("proxy-screen.sci"),
            &dir.join("proxy-screen.idx"),
            1280,
            62,
            &mut |_| {},
        )
        .unwrap();
        eprintln!("strip {meta:?} in {:.2}s", t.elapsed().as_secs_f64());
        assert!(meta.n > 0 && meta.w > 0 && meta.h > 0);
        let dir_s = dir.to_string_lossy().into_owned();
        for n in [0, meta.n / 2, meta.n - 1] {
            let jpg = crate::proxy::frame_bytes(&dir_s, "screen", n).expect("frame");
            assert_eq!(&jpg[..2], &[0xFF, 0xD8], "frame {n} is a JPEG");
            if let Ok(out) = std::env::var("SC_TEST_OUT") {
                std::fs::write(format!("{out}/proxy-{n}.jpg"), &jpg).unwrap();
            }
        }
        crate::proxy::forget(&dir_s);
        let _ = std::fs::remove_dir_all(&dir);
    }
}
