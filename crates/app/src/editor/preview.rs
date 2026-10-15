//! Background frame decoding for the editor preview. The UI asks for a source frame index; the
//! worker decodes the newest request and sends the picture back.

use eframe::egui;
use screencraft_encode::I420;
use screencraft_engine::VideoSource;
use std::path::PathBuf;
use std::sync::mpsc::{channel, Receiver, Sender};
use std::sync::Arc;

pub struct Reply {
    pub index: usize,
    pub frame: Arc<I420>,
}

pub struct PreviewWorker {
    tx: Sender<Option<usize>>,
    rx: Receiver<Result<Reply, String>>,
    pub requested: Option<usize>,
}

impl PreviewWorker {
    pub fn spawn(path: PathBuf, ctx: egui::Context) -> Self {
        let (tx, req_rx) = channel::<Option<usize>>();
        let (reply_tx, rx) = channel::<Result<Reply, String>>();
        std::thread::Builder::new()
            .name("screencraft-preview".into())
            .spawn(move || {
                let mut source = match VideoSource::open(&path) {
                    Ok(s) => s,
                    Err(e) => {
                        let _ = reply_tx.send(Err(e));
                        ctx.request_repaint();
                        return;
                    }
                };
                while let Ok(first) = req_rx.recv() {
                    let Some(mut idx) = first else { return };
                    // Only the newest request matters while scrubbing.
                    while let Ok(next) = req_rx.try_recv() {
                        match next {
                            Some(i) => idx = i,
                            None => return,
                        }
                    }
                    let r = source.frame(idx).map(|f| Reply { index: idx, frame: f });
                    if reply_tx.send(r).is_err() {
                        return;
                    }
                    ctx.request_repaint();
                }
            })
            .expect("preview thread");
        Self { tx, rx, requested: None }
    }

    pub fn request(&mut self, index: usize) {
        if self.requested != Some(index) {
            self.requested = Some(index);
            let _ = self.tx.send(Some(index));
        }
    }

    /// Newest finished frame, if any arrived since the last call.
    pub fn poll(&mut self) -> Option<Result<Reply, String>> {
        let mut last = None;
        while let Ok(r) = self.rx.try_recv() {
            last = Some(r);
        }
        last
    }
}

impl Drop for PreviewWorker {
    fn drop(&mut self) {
        let _ = self.tx.send(None);
    }
}
