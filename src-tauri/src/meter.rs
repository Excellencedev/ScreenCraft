//! Live system-audio level meter. A tiny audio-only SCStream feeds a peak
//! value that the UI polls via events, so the toolbar can show that audio is
//! really being captured before you record.

use screencapturekit::prelude::*;
use std::sync::atomic::{AtomicBool, AtomicU32, Ordering};
use std::sync::{Arc, Mutex};
use std::thread;
use std::time::Duration;
use tauri::{AppHandle, Emitter};

struct Handler {
    peak: Arc<AtomicU32>,
}

impl SCStreamOutputTrait for Handler {
    fn did_output_sample_buffer(&self, sample: CMSampleBuffer, kind: SCStreamOutputType) {
        if !matches!(kind, SCStreamOutputType::Audio) {
            return;
        }
        let Ok(list) = sample.audio_buffer_list() else { return };
        let mut peak = 0.0f32;
        for i in 0..list.num_buffers() {
            if let Some(buf) = list.buffer(i) {
                let bytes = buf.data();
                for c in bytes.chunks_exact(4) {
                    let v = f32::from_le_bytes([c[0], c[1], c[2], c[3]]).abs();
                    if v.is_finite() && v > peak {
                        peak = v;
                    }
                }
            }
        }
        // keep the largest value seen since the UI last read it
        let bits = peak.to_bits();
        let _ = self
            .peak
            .fetch_update(Ordering::Relaxed, Ordering::Relaxed, |old| (f32::from_bits(old) < peak).then_some(bits));
    }
}

struct Running {
    stop: Arc<AtomicBool>,
}

#[derive(Default)]
pub struct Meter {
    running: Mutex<Option<Running>>,
}

impl Meter {
    pub fn start(&self, app: AppHandle) -> Result<(), String> {
        let mut guard = self.running.lock().unwrap();
        if guard.is_some() {
            return Ok(());
        }
        let content = SCShareableContent::get().map_err(|e| e.to_string())?;
        let display = content.displays().into_iter().next().ok_or("no display")?;
        let filter = SCContentFilter::create()
            .with_display(&display)
            .with_excluding_windows(&[])
            .build()
            .map_err(|e| e.to_string())?;
        let config = SCStreamConfiguration::new()
            .with_width(64)
            .with_height(36)
            .with_fps(1)
            .with_captures_audio(true)
            .with_sample_rate(48000)
            .with_channel_count(2)
            .with_excludes_current_process_audio(true);
        let peak = Arc::new(AtomicU32::new(0));
        let mut stream = SCStream::new(&filter, &config).map_err(|e| e.to_string())?;
        stream
            .add_output_handler(Handler { peak: peak.clone() }, SCStreamOutputType::Audio)
            .map_err(|e| e.to_string())?;
        stream.start_capture().map_err(|e| e.to_string())?;

        let stop = Arc::new(AtomicBool::new(false));
        let s = stop.clone();
        thread::Builder::new()
            .name("screencraft-meter".into())
            .spawn(move || {
                while !s.load(Ordering::Relaxed) {
                    let v = f32::from_bits(peak.swap(0, Ordering::Relaxed));
                    let _ = app.emit("system-level", v);
                    thread::sleep(Duration::from_millis(60));
                }
                let _ = stream.stop_capture();
            })
            .map_err(|e| e.to_string())?;
        *guard = Some(Running { stop });
        Ok(())
    }

    pub fn stop(&self) {
        if let Some(r) = self.running.lock().unwrap().take() {
            r.stop.store(true, Ordering::Relaxed);
        }
    }
}
