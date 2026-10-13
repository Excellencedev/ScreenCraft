//! Live system-audio level meter on Windows: a WASAPI loopback feeds the peak value
//! the toolbar shows before you record.

use crate::wasapi::Loopback;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex};
use std::thread;
use std::time::Duration;
use tauri::{AppHandle, Emitter};

#[derive(Default)]
pub struct Meter {
    running: Mutex<Option<Arc<AtomicBool>>>,
}

impl Meter {
    pub fn start(&self, app: AppHandle) -> Result<(), String> {
        let mut guard = self.running.lock().unwrap();
        if guard.is_some() {
            return Ok(());
        }
        let stop = Arc::new(AtomicBool::new(false));
        let s = stop.clone();
        let (tx, rx) = std::sync::mpsc::channel();
        thread::Builder::new()
            .name("screencraft-meter".into())
            .spawn(move || {
                let mut lb = match Loopback::open() {
                    Ok(lb) => {
                        let _ = tx.send(Ok(()));
                        lb
                    }
                    Err(e) => {
                        let _ = tx.send(Err(e));
                        return;
                    }
                };
                while !s.load(Ordering::Relaxed) {
                    let mut peak = 0f32;
                    let _ = lb.read(|pcm, _| {
                        for v in pcm {
                            peak = peak.max(v.abs());
                        }
                    });
                    let _ = app.emit("system-level", peak);
                    thread::sleep(Duration::from_millis(60));
                }
            })
            .map_err(|e| e.to_string())?;
        rx.recv().map_err(|e| e.to_string())??;
        *guard = Some(stop);
        Ok(())
    }

    pub fn stop(&self) {
        if let Some(s) = self.running.lock().unwrap().take() {
            s.store(true, Ordering::Relaxed);
        }
    }
}
