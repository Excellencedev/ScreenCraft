//! Camera capture through the operating system's camera API (AVFoundation, Media Foundation or
//! Video4Linux), used for the live preview in the recorder and for recording the camera bubble.

use nokhwa::pixel_format::RgbFormat;
use nokhwa::utils::{ApiBackend, CameraIndex, RequestedFormat, RequestedFormatType, Resolution};
use nokhwa::Camera;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;
use std::time::Instant;

/// Fans one camera stream out to the live preview and (while recording) the recorder.
#[derive(Default)]
pub struct CameraHub {
    subs: std::sync::Mutex<Vec<std::sync::mpsc::SyncSender<Arc<CamFrame>>>>,
    latest: std::sync::Mutex<Option<Arc<CamFrame>>>,
}

impl CameraHub {
    pub fn new() -> Arc<Self> {
        Arc::new(Self::default())
    }

    pub fn publish(&self, f: CamFrame) {
        let f = Arc::new(f);
        *self.latest.lock().unwrap() = Some(f.clone());
        // a slow subscriber drops frames rather than stalling the camera
        self.subs.lock().unwrap().retain(|s| !matches!(s.try_send(f.clone()), Err(std::sync::mpsc::TrySendError::Disconnected(_))));
    }

    pub fn latest(&self) -> Option<Arc<CamFrame>> {
        self.latest.lock().unwrap().clone()
    }

    pub fn clear(&self) {
        *self.latest.lock().unwrap() = None;
    }

    pub fn subscribe(&self, capacity: usize) -> std::sync::mpsc::Receiver<Arc<CamFrame>> {
        let (tx, rx) = std::sync::mpsc::sync_channel(capacity);
        self.subs.lock().unwrap().push(tx);
        rx
    }
}

pub struct CamFrame {
    pub width: u32,
    pub height: u32,
    /// Packed RGB8.
    pub rgb: Vec<u8>,
    pub at: Instant,
}

/// Cameras as (id, name). The id is the device index as a string.
pub fn list() -> Vec<(String, String)> {
    #[cfg(target_os = "macos")]
    request_access_blocking();
    nokhwa::query(ApiBackend::Auto)
        .map(|v| v.into_iter().map(|i| (i.index().as_index().map(|n| n.to_string()).unwrap_or_default(), i.human_name())).collect())
        .unwrap_or_default()
}

#[cfg(target_os = "macos")]
fn request_access_blocking() {
    let (tx, rx) = std::sync::mpsc::channel();
    nokhwa::nokhwa_initialize(move |ok| {
        let _ = tx.send(ok);
    });
    let _ = rx.recv_timeout(std::time::Duration::from_secs(30));
}

pub struct CameraStream {
    stop: Arc<AtomicBool>,
    join: Option<std::thread::JoinHandle<()>>,
}

impl CameraStream {
    pub fn stop(mut self) {
        self.stop.store(true, Ordering::SeqCst);
        if let Some(j) = self.join.take() {
            let _ = j.join();
        }
    }
}

impl Drop for CameraStream {
    fn drop(&mut self) {
        self.stop.store(true, Ordering::SeqCst);
        if let Some(j) = self.join.take() {
            let _ = j.join();
        }
    }
}

/// Open camera `id` (None = the first one) and call `on_frame` for every frame on a worker thread.
/// Returns once the stream is running or the reason it could not.
pub fn start(id: Option<&str>, mut on_frame: impl FnMut(CamFrame) + Send + 'static) -> Result<CameraStream, String> {
    let index: u32 = id.and_then(|s| s.parse().ok()).unwrap_or(0);
    let stop = Arc::new(AtomicBool::new(false));
    let (ready_tx, ready_rx) = std::sync::mpsc::channel::<Result<(), String>>();
    let stop2 = stop.clone();
    let join = std::thread::Builder::new()
        .name("screencraft-camera".into())
        .spawn(move || {
            // Not every camera offers the exact format, so try a few ways of asking.
            let tries = [
                RequestedFormatType::HighestResolution(Resolution::new(1280, 720)),
                RequestedFormatType::AbsoluteHighestFrameRate,
                RequestedFormatType::None,
            ];
            let mut opened = None;
            let mut last = String::new();
            for t in tries {
                match Camera::new(CameraIndex::Index(index), RequestedFormat::new::<RgbFormat>(t)) {
                    Ok(c) => {
                        opened = Some(c);
                        break;
                    }
                    Err(e) => last = e.to_string(),
                }
            }
            let mut cam = match opened {
                Some(c) => c,
                None => {
                    let _ = ready_tx.send(Err(format!("could not open the camera: {last}")));
                    return;
                }
            };
            if let Err(e) = cam.open_stream() {
                let _ = ready_tx.send(Err(format!("could not start the camera: {e}")));
                return;
            }
            let _ = ready_tx.send(Ok(()));
            while !stop2.load(Ordering::SeqCst) {
                match cam.frame() {
                    Ok(buf) => {
                        let at = Instant::now();
                        if let Ok(img) = buf.decode_image::<RgbFormat>() {
                            on_frame(CamFrame { width: img.width(), height: img.height(), rgb: img.into_raw(), at });
                        }
                    }
                    Err(_) => std::thread::sleep(std::time::Duration::from_millis(5)),
                }
            }
            let _ = cam.stop_stream();
        })
        .map_err(|e| e.to_string())?;
    match ready_rx.recv_timeout(std::time::Duration::from_secs(20)) {
        Ok(Ok(())) => Ok(CameraStream { stop, join: Some(join) }),
        Ok(Err(e)) => Err(e),
        Err(_) => Err("the camera did not start in time".into()),
    }
}
