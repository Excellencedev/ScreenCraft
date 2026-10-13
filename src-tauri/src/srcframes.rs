//! Full-resolution source frames for export (`sc://localhost/src/<kind>?t=<ms>&d=<dir>`).
//!
//! One worker thread owns the AVFoundation readers (they are not thread safe). Export
//! asks for frames in increasing time order, so each request normally just pulls the
//! next decoded sample; a backwards or far-forward request reopens the reader at the
//! requested time. Nothing here involves a web-view <video> element.

#![allow(deprecated)]

use objc2::rc::Retained;
use objc2::runtime::AnyObject;
use objc2::AnyThread;
use objc2_av_foundation::{AVAssetReader, AVAssetReaderTrackOutput, AVURLAsset};
use objc2_core_media::{CMSampleBuffer, CMTime, CMTimeRange};
use objc2_core_video::{
    CVPixelBufferGetBaseAddress, CVPixelBufferGetBytesPerRow, CVPixelBufferGetHeight, CVPixelBufferGetWidth,
    CVPixelBufferLockBaseAddress, CVPixelBufferLockFlags, CVPixelBufferUnlockBaseAddress,
};
use objc2_foundation::{NSDictionary, NSNumber, NSString, NSURL};
use std::collections::HashMap;
use std::sync::mpsc::{channel, Sender};
use std::sync::OnceLock;

pub struct Request {
    pub dir: String,
    pub kind: String,
    pub t_ms: f64,
    pub quality: u8,
    /// Scale the frame down to this width first (0 = native). Export asks for no more
    /// resolution than the output needs, which makes JPEG encode/decode much cheaper.
    pub max_w: u32,
    pub reply: Box<dyn FnOnce(Option<Vec<u8>>) + Send>,
}

static TX: OnceLock<Sender<Request>> = OnceLock::new();

pub fn submit(req: Request) {
    let tx = TX.get_or_init(|| {
        let (tx, rx) = channel::<Request>();
        std::thread::Builder::new()
            .name("screencraft-srcframes".into())
            .spawn(move || worker(rx))
            .expect("spawn srcframes worker");
        tx
    });
    if let Err(e) = tx.send(req) {
        (e.0.reply)(None);
    }
}

struct Session {
    asset: Retained<AVURLAsset>,
    reader: Retained<AVAssetReader>,
    output: Retained<AVAssetReaderTrackOutput>,
    duration: f64,
    /// The most recent decoded sample with pts <= the last request, and the one after it.
    cur: Option<(f64, Retained<CMSampleBuffer>)>,
    next: Option<(f64, Retained<CMSampleBuffer>)>,
    /// Frames just passed, so requests that arrive slightly out of order are still served.
    recent: std::collections::VecDeque<(f64, Retained<CMSampleBuffer>)>,
    eof: bool,
    /// Time of the last forced reopen after the reader ran dry early.
    last_retry: f64,
    /// Always restart from 0 (small files whose timestamps can't be addressed by range).
    from_start: bool,
}

fn obj<T: objc2::Message>(r: Retained<T>) -> Retained<AnyObject> {
    unsafe { Retained::cast_unchecked(r) }
}

fn open_at(asset: &Retained<AVURLAsset>, start_s: f64, duration: f64) -> Option<(Retained<AVAssetReader>, Retained<AVAssetReaderTrackOutput>)> {
    unsafe {
        let vtype = NSString::from_str("vide");
        let track = asset.tracksWithMediaType(&vtype).firstObject()?;
        let reader = AVAssetReader::initWithAsset_error(AVAssetReader::alloc(), asset).ok()?;
        let keys = [NSString::from_str("PixelFormatType")];
        let key_refs: Vec<&NSString> = keys.iter().map(|k| &**k).collect();
        let vals = [obj(NSNumber::new_i64(1111970369))]; // 32BGRA, native size
        let settings: Retained<NSDictionary<NSString, AnyObject>> = NSDictionary::from_retained_objects(&key_refs, &vals);
        let output = AVAssetReaderTrackOutput::assetReaderTrackOutputWithTrack_outputSettings(&track, Some(&settings));
        reader.addOutput(&output);
        let start = CMTime::new((start_s.max(0.0) * 1000.0) as i64, 1000);
        let dur = CMTime::new((((duration - start_s).max(0.1)) * 1000.0) as i64 + 1000, 1000);
        reader.setTimeRange(CMTimeRange { start, duration: dur });
        if !reader.startReading() {
            return None;
        }
        Some((reader, output))
    }
}

impl Session {
    fn open(path: &str, start_s: f64) -> Option<Session> {
        unsafe {
            let url = NSURL::fileURLWithPath(&NSString::from_str(path));
            let asset = AVURLAsset::URLAssetWithURL_options(&url, None);
            let duration = asset.duration().seconds();
            let (reader, output) = open_at(&asset, start_s, duration)?;
            Some(Session { asset, reader, output, duration, cur: None, next: None, recent: Default::default(), eof: false, last_retry: -10.0, from_start: false })
        }
    }

    fn reopen(&mut self, start_s: f64) -> bool {
        // a time range is expressed in the file's raw timestamps, which can be offset by a
        // 32-bit wrap for some recordings: reading from the start is always correct
        let start_s = if self.from_start { 0.0 } else { start_s };
        match open_at(&self.asset, start_s, self.duration) {
            Some((r, o)) => {
                self.reader = r;
                self.output = o;
                self.cur = None;
                self.next = None;
                self.recent.clear();
                self.eof = false;
                true
            }
            None => false,
        }
    }

    fn pull(&mut self) -> Option<(f64, Retained<CMSampleBuffer>)> {
        unsafe {
            loop {
                let s = self.output.copyNextSampleBuffer()?;
                let pts = crate::proxy::unwrapped_seconds(s.presentation_time_stamp());
                if s.image_buffer().is_some() {
                    return Some((pts, s));
                }
            }
        }
    }

    /// The decoded frame shown at `t` seconds (the latest one with pts <= t).
    fn frame_at(&mut self, t: f64) -> Option<&Retained<CMSampleBuffer>> {
        // slightly out of order: serve it from the frames we just passed
        if self.cur.as_ref().map(|c| t + 1e-4 < c.0).unwrap_or(false) {
            if let Some(i) = self.recent.iter().rposition(|(p, _)| *p <= t + 1e-4) {
                return Some(&self.recent[i].1);
            }
        }
        // a request before what we already passed, or far ahead: restart there
        let behind = self.cur.as_ref().map(|c| t + 1e-4 < c.0).unwrap_or(false);
        let far = self.cur.as_ref().map(|c| t - c.0 > 4.0).unwrap_or(false);
        if behind || far {
            self.reopen((t - 0.25).max(0.0));
        }
        // The reader can end early (a damaged or oddly timestamped file): a frame that
        // stopped well before `t` would freeze the picture, so reopen at `t` and carry on.
        // If nothing more can be read, keep showing the last good frame.
        let stalled = self.eof && self.cur.as_ref().map(|c| t - c.0 > 0.5).unwrap_or(false) && t - self.last_retry > 0.5;
        if stalled {
            self.last_retry = t;
            let held = self.cur.take();
            if self.reopen((t - 0.1).max(0.0)) {
                self.next = self.pull();
                if self.next.is_none() {
                    self.eof = true;
                }
            }
            if self.next.is_none() && self.cur.is_none() {
                self.cur = held;
                self.eof = true;
            }
        }
        if self.cur.is_none() && self.next.is_none() && !self.eof {
            self.next = self.pull();
            if self.next.is_none() {
                self.eof = true;
            }
        }
        loop {
            match &self.next {
                Some((pts, _)) if *pts <= t + 1e-4 => {
                    if let Some(prev) = self.cur.take() {
                        self.recent.push_back(prev);
                        if self.recent.len() > 12 {
                            self.recent.pop_front();
                        }
                    }
                    self.cur = self.next.take();
                    self.next = self.pull();
                    if self.next.is_none() {
                        self.eof = true;
                        break;
                    }
                }
                _ => break,
            }
        }
        if self.cur.is_none() {
            // t is before the first frame: show the first one
            self.cur = self.next.take();
            self.next = self.pull();
        }
        self.cur.as_ref().map(|c| &c.1)
    }
}

#[repr(C)]
struct VImageBuffer {
    data: *mut std::ffi::c_void,
    height: usize,
    width: usize,
    row_bytes: usize,
}

#[link(name = "Accelerate", kind = "framework")]
extern "C" {
    fn vImageScale_ARGB8888(
        src: *const VImageBuffer,
        dest: *const VImageBuffer,
        temp: *mut std::ffi::c_void,
        flags: u32,
    ) -> isize;
}

/// Copy the sample's pixels out so encoding can happen on another thread, scaling to
/// `max_w` pixels wide (Lanczos, via vImage) when that is clearly smaller than native.
fn pack(sample: &CMSampleBuffer, max_w: usize) -> Option<(Vec<u8>, usize, usize)> {
    unsafe {
        let img = sample.image_buffer()?;
        CVPixelBufferLockBaseAddress(&img, CVPixelBufferLockFlags::ReadOnly);
        let (w, h) = (CVPixelBufferGetWidth(&img), CVPixelBufferGetHeight(&img));
        let stride = CVPixelBufferGetBytesPerRow(&img);
        let base = CVPixelBufferGetBaseAddress(&img) as *const u8;
        let mut out = None;
        if !base.is_null() && w > 0 && h > 0 {
            if max_w >= 64 && (max_w as f64) < w as f64 * 0.9 {
                let tw = max_w & !1;
                let th = (((h as f64 * tw as f64 / w as f64).round() as usize) & !1).max(2);
                let mut scaled = vec![0u8; tw * th * 4];
                let src = VImageBuffer { data: base as *mut _, height: h, width: w, row_bytes: stride };
                let dst = VImageBuffer { data: scaled.as_mut_ptr() as *mut _, height: th, width: tw, row_bytes: tw * 4 };
                // kvImageHighQualityResampling = 32
                if vImageScale_ARGB8888(&src, &dst, std::ptr::null_mut(), 32) == 0 {
                    out = Some((scaled, tw, th));
                }
            }
            if out.is_none() {
                let mut packed = vec![0u8; w * h * 4];
                for row in 0..h {
                    let s = std::slice::from_raw_parts(base.add(row * stride), w * 4);
                    packed[row * w * 4..(row + 1) * w * 4].copy_from_slice(s);
                }
                out = Some((packed, w, h));
            }
        }
        CVPixelBufferUnlockBaseAddress(&img, CVPixelBufferLockFlags::ReadOnly);
        out
    }
}

fn jpeg(packed: Vec<u8>, w: usize, h: usize, quality: u8) -> Option<Vec<u8>> {
    crate::jpegio::encode_bgra(packed, w, h, quality)
}

/// At most this many frames are JPEG-encoding at once (decode keeps running meanwhile).
const ENCODERS: usize = 6;
static INFLIGHT: (std::sync::Mutex<usize>, std::sync::Condvar) = (std::sync::Mutex::new(0), std::sync::Condvar::new());

fn worker(rx: std::sync::mpsc::Receiver<Request>) {
    let mut sessions: HashMap<String, Session> = HashMap::new();
    let (mut n, mut t_dec, mut t_pack) = (0u32, 0f64, 0f64);
    while let Ok(req) = rx.recv() {
        let t0 = std::time::Instant::now();
        let file = match req.kind.as_str() {
            "screen" => "screen",
            "camera" => "camera",
            _ => "",
        };
        let result = (|| {
            if file.is_empty() {
                return None;
            }
            let path = project_file(&req.dir, file)?;
            let key = format!("{}|{}", req.dir, req.kind);
            if !sessions.contains_key(&key) {
                let mut sess = Session::open(&path, 0.0)?;
                sess.from_start = req.kind == "camera";
                sessions.insert(key.clone(), sess);
            }
            let sess = sessions.get_mut(&key)?;
            let frame = sess.frame_at(req.t_ms / 1000.0)?;
            t_dec += t0.elapsed().as_secs_f64();
            let tp = std::time::Instant::now();
            let r = pack(frame, req.max_w as usize);
            t_pack += tp.elapsed().as_secs_f64();
            r
        })();
        n += 1;
        if n % 100 == 0 {
            eprintln!("[srcframes] {} frames: decode {:.1} ms, pack {:.1} ms (avg)", n, t_dec * 10.0, t_pack * 10.0);
            t_dec = 0.0;
            t_pack = 0.0;
        }
        match result {
            Some((px, w, h)) => {
                // encode on a helper thread so the next frame can be decoded right away
                {
                    let mut n = INFLIGHT.0.lock().unwrap();
                    while *n >= ENCODERS {
                        n = INFLIGHT.1.wait(n).unwrap();
                    }
                    *n += 1;
                }
                let q = req.quality;
                let reply = req.reply;
                std::thread::spawn(move || {
                    let out = jpeg(px, w, h, q);
                    {
                        let mut n = INFLIGHT.0.lock().unwrap();
                        *n -= 1;
                        INFLIGHT.1.notify_one();
                    }
                    reply(out);
                });
            }
            None => (req.reply)(None),
        }
    }
}

/// Resolve "screen"/"camera" to the file named in project.json.
fn project_file(dir: &str, which: &str) -> Option<String> {
    let bytes = std::fs::read(std::path::Path::new(dir).join("project.json")).ok()?;
    let v: serde_json::Value = serde_json::from_slice(&bytes).ok()?;
    let name = v["media"][which].as_str()?;
    Some(std::path::Path::new(dir).join(name).to_string_lossy().into_owned())
}
