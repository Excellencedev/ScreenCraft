//! Source frames through Media Foundation. One worker thread owns the readers; export
//! asks for frames in increasing time order, so each request normally just pulls the
//! next decoded frame, and a backwards or far-forward request seeks.

use super::Request;
use crate::mf::{Frame, Reader};
use std::collections::{HashMap, VecDeque};
use std::sync::mpsc::{channel, Sender};
use std::sync::{Arc, OnceLock};

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
    reader: Reader,
    /// The most recent decoded frame with pts <= the last request, and the one after it.
    cur: Option<Arc<Frame>>,
    next: Option<Arc<Frame>>,
    /// Frames just passed, so requests that arrive slightly out of order are still served.
    recent: VecDeque<Arc<Frame>>,
    eof: bool,
    /// Time of the last forced reseek after the reader ran dry early.
    last_retry: f64,
    /// Always restart from 0 (camera files whose timestamps can't be addressed reliably).
    from_start: bool,
}

/// Full-resolution frames are large, so fewer are kept than on macOS.
const RECENT: usize = 4;

impl Session {
    fn open(path: &str) -> Option<Session> {
        let reader = Reader::open(path, None).map_err(|e| eprintln!("[srcframes] {path}: {e}")).ok()?;
        Some(Session { reader, cur: None, next: None, recent: Default::default(), eof: false, last_retry: -10.0, from_start: false })
    }

    fn reopen(&mut self, start_s: f64) -> bool {
        let start_s = if self.from_start { 0.0 } else { start_s };
        if !self.reader.seek(start_s) {
            return false;
        }
        self.cur = None;
        self.next = None;
        self.recent.clear();
        self.eof = false;
        true
    }

    fn pull(&mut self) -> Option<Arc<Frame>> {
        self.reader.next().map(Arc::new)
    }

    /// The decoded frame shown at `t` seconds (the latest one with pts <= t).
    fn frame_at(&mut self, t: f64) -> Option<Arc<Frame>> {
        // slightly out of order: serve it from the frames we just passed
        if self.cur.as_ref().map(|c| t + 1e-4 < c.pts).unwrap_or(false) {
            if let Some(i) = self.recent.iter().rposition(|f| f.pts <= t + 1e-4) {
                return Some(self.recent[i].clone());
            }
        }
        // a request before what we already passed, or far ahead: restart there
        let behind = self.cur.as_ref().map(|c| t + 1e-4 < c.pts).unwrap_or(false);
        let far = self.cur.as_ref().map(|c| t - c.pts > 4.0).unwrap_or(false);
        if behind || far {
            self.reopen((t - 0.25).max(0.0));
        }
        // The reader can end early (a damaged or oddly timestamped file): a frame that
        // stopped well before `t` would freeze the picture, so seek to `t` and carry on.
        // If nothing more can be read, keep showing the last good frame.
        let stalled = self.eof && self.cur.as_ref().map(|c| t - c.pts > 0.5).unwrap_or(false) && t - self.last_retry > 0.5;
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
                Some(n) if n.pts <= t + 1e-4 => {
                    if let Some(prev) = self.cur.take() {
                        self.recent.push_back(prev);
                        if self.recent.len() > RECENT {
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
        self.cur.clone()
    }
}

/// At most this many frames are JPEG-encoding at once (decode keeps running meanwhile).
const ENCODERS: usize = 6;
static INFLIGHT: (std::sync::Mutex<usize>, std::sync::Condvar) = (std::sync::Mutex::new(0), std::sync::Condvar::new());

fn worker(rx: std::sync::mpsc::Receiver<Request>) {
    crate::mf::init_thread();
    let mut sessions: HashMap<String, Session> = HashMap::new();
    let (mut n, mut t_dec) = (0u32, 0f64);
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
                let mut sess = Session::open(&path)?;
                sess.from_start = req.kind == "camera";
                sessions.insert(key.clone(), sess);
            }
            let frame = sessions.get_mut(&key)?.frame_at(req.t_ms / 1000.0);
            t_dec += t0.elapsed().as_secs_f64();
            frame
        })();
        n += 1;
        if n % 100 == 0 {
            eprintln!("[srcframes] {} frames: decode {:.1} ms (avg)", n, t_dec * 10.0);
            t_dec = 0.0;
        }
        match result {
            Some(frame) => {
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
                    let out = crate::jpegio::encode_bgra(frame.bgra.clone(), frame.w, frame.h, q);
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

#[cfg(test)]
mod tests {
    use super::*;

    /// Walks `SC_TEST_VIDEO` like an export would (plus a jump back) and checks timing.
    #[test]
    fn frames_in_order_and_after_seek() {
        let Ok(video) = std::env::var("SC_TEST_VIDEO") else {
            eprintln!("SC_TEST_VIDEO not set, skipping");
            return;
        };
        crate::mf::init_thread();
        let mut s = Session::open(&video).expect("open");
        let fps = 30.0;
        let t = std::time::Instant::now();
        let mut last = -1.0;
        for i in 0..90 {
            let want = i as f64 / fps;
            let f = s.frame_at(want).expect("frame");
            assert!(f.pts <= want + 1e-3 && f.pts > want - 0.1, "t={want} got pts {}", f.pts);
            assert!(f.pts >= last);
            last = f.pts;
        }
        let per = t.elapsed().as_secs_f64() / 90.0 * 1000.0;
        // jump back, then far forward
        for want in [0.5, 1.0, s.reader.duration - 0.5] {
            let f = s.frame_at(want).expect("frame after seek");
            assert!(f.pts <= want + 1e-3 && f.pts > want - 0.1, "t={want} got pts {}", f.pts);
        }
        let f = s.frame_at(1.0).unwrap();
        let t = std::time::Instant::now();
        let jpg = crate::jpegio::encode_bgra(f.bgra.clone(), f.w, f.h, 90).expect("jpeg");
        eprintln!("{}x{}: decode {per:.1} ms/frame, jpeg {:.1} ms ({} KB)", f.w, f.h, t.elapsed().as_secs_f64() * 1000.0, jpg.len() / 1024);
        if let Ok(out) = std::env::var("SC_TEST_OUT") {
            std::fs::write(format!("{out}/src-1s.jpg"), &jpg).unwrap();
        }
    }
}
