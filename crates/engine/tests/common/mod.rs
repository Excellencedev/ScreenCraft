//! Shared test helpers: a capture backend that needs no display.

use screencraft_audio::{AudioBlock, Clock, Source};
use screencraft_capture::{CaptureRequest, Capturer, Event, Frame, StartInfo};
use screencraft_core::project::Rect;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::mpsc::SyncSender;
use std::sync::Arc;
use std::time::Duration;

pub struct Synthetic {
    pub stop: Arc<AtomicBool>,
    pub thread: Option<std::thread::JoinHandle<()>>,
}

impl Capturer for Synthetic {
    fn start(&mut self, req: &CaptureRequest, clock: Clock, tx: SyncSender<Event>) -> Result<StartInfo, String> {
        let (w, h) = (320u32, 180u32);
        let stop = self.stop.clone();
        let fps = req.fps;
        self.thread = Some(std::thread::spawn(move || {
            let mut i = 0u64;
            let mut audio_t = 0u64;
            while !stop.load(Ordering::SeqCst) {
                // A bright bar moving left to right; it makes frame order checkable.
                let mut bgra = vec![30u8; (w * h * 4) as usize];
                let x0 = ((i * 4) % (w as u64 - 20)) as usize;
                for y in 0..h as usize {
                    for x in x0..x0 + 20 {
                        let o = (y * w as usize + x) * 4;
                        bgra[o..o + 4].copy_from_slice(&[240, 240, 240, 255]);
                    }
                }
                // Skip every 5th frame like a capture API would on an idle screen.
                if i % 5 != 4 {
                    let _ = tx.send(Event::Frame(Frame { width: w, height: h, bgra, pts_ns: clock.now_ns() }));
                }
                // 20 ms of a 440 Hz tone per video tick pair.
                let n = 960usize;
                let data: Vec<f32> = (0..n)
                    .flat_map(|k| {
                        let s = ((audio_t + k as u64) as f32 * 440.0 * std::f32::consts::TAU / 48_000.0).sin() * 0.5;
                        [s, s]
                    })
                    .collect();
                let _ = tx.send(Event::SystemAudio(AudioBlock {
                    source: Source::System,
                    pts_ns: clock.now_ns(),
                    sample_rate: 48_000,
                    channels: 2,
                    data,
                }));
                audio_t += n as u64;
                i += 1;
                std::thread::sleep(Duration::from_millis(1000 / fps as u64));
            }
        }));
        Ok(StartInfo {
            width: w,
            height: h,
            bounds: Rect { x: 0.0, y: 0.0, width: w as f64, height: h as f64 },
            display_id: None,
            provides_system_audio: true,
        })
    }

    fn stop(&mut self) {
        self.stop.store(true, Ordering::SeqCst);
        if let Some(t) = self.thread.take() {
            let _ = t.join();
        }
    }
}

