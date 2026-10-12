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
    eof: bool,
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
            Some(Session { asset, reader, output, duration, cur: None, next: None, eof: false })
        }
    }

    fn reopen(&mut self, start_s: f64) -> bool {
        match open_at(&self.asset, start_s, self.duration) {
            Some((r, o)) => {
                self.reader = r;
                self.output = o;
                self.cur = None;
                self.next = None;
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
                let pts = s.presentation_time_stamp().seconds();
                if s.image_buffer().is_some() {
                    return Some((pts, s));
                }
            }
        }
    }

    /// The decoded frame shown at `t` seconds (the latest one with pts <= t).
    fn frame_at(&mut self, t: f64) -> Option<&Retained<CMSampleBuffer>> {
        // a request before what we already passed, or far ahead: restart there
        let behind = self.cur.as_ref().map(|c| t + 1e-4 < c.0).unwrap_or(false);
        let far = self.cur.as_ref().map(|c| t - c.0 > 4.0).unwrap_or(false);
        if behind || far {
            self.reopen((t - 0.25).max(0.0));
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

fn to_jpeg(sample: &CMSampleBuffer, quality: u8) -> Option<Vec<u8>> {
    unsafe {
        let img = sample.image_buffer()?;
        CVPixelBufferLockBaseAddress(&img, CVPixelBufferLockFlags::ReadOnly);
        let (w, h) = (CVPixelBufferGetWidth(&img), CVPixelBufferGetHeight(&img));
        let stride = CVPixelBufferGetBytesPerRow(&img);
        let base = CVPixelBufferGetBaseAddress(&img) as *const u8;
        let mut out = None;
        if !base.is_null() && w > 0 && h > 0 {
            let mut packed = vec![0u8; w * h * 4];
            for row in 0..h {
                let s = std::slice::from_raw_parts(base.add(row * stride), w * 4);
                packed[row * w * 4..(row + 1) * w * 4].copy_from_slice(s);
            }
            let mut buf = Vec::with_capacity(w * h / 6);
            let mut enc = jpeg_encoder::Encoder::new(&mut buf, quality);
            enc.set_sampling_factor(jpeg_encoder::SamplingFactor::F_2_2);
            if enc.encode(&packed, w as u16, h as u16, jpeg_encoder::ColorType::Bgra).is_ok() {
                out = Some(buf);
            }
        }
        CVPixelBufferUnlockBaseAddress(&img, CVPixelBufferLockFlags::ReadOnly);
        out
    }
}

fn worker(rx: std::sync::mpsc::Receiver<Request>) {
    let mut sessions: HashMap<String, Session> = HashMap::new();
    while let Ok(req) = rx.recv() {
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
                sessions.insert(key.clone(), Session::open(&path, (req.t_ms / 1000.0 - 0.25).max(0.0))?);
            }
            let sess = sessions.get_mut(&key)?;
            let frame = sess.frame_at(req.t_ms / 1000.0)?;
            to_jpeg(frame, req.quality)
        })();
        (req.reply)(result);
    }
}

/// Resolve "screen"/"camera" to the file named in project.json.
fn project_file(dir: &str, which: &str) -> Option<String> {
    let bytes = std::fs::read(std::path::Path::new(dir).join("project.json")).ok()?;
    let v: serde_json::Value = serde_json::from_slice(&bytes).ok()?;
    let name = v["media"][which].as_str()?;
    Some(std::path::Path::new(dir).join(name).to_string_lossy().into_owned())
}
