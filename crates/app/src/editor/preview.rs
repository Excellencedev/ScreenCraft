//! Background frame decoding for the editor preview. The UI asks for the source times it needs
//! (screen, camera, and the other clip's pictures during a dissolve); the worker decodes the
//! newest request and sends the pictures back.

use eframe::egui;
use screencraft_engine::{Frames, MultiSource, Package};
use screencraft_core::project::Project;
use std::sync::mpsc::{channel, Receiver, Sender};

#[derive(Clone, Copy, PartialEq, Debug)]
pub struct Want {
    pub t_src: f64,
    pub other_t: Option<f64>,
    /// Identifies the request (frame indexes), echoed back with the pictures.
    pub key: (usize, Option<usize>, Option<usize>),
}

pub struct Reply {
    pub key: (usize, Option<usize>, Option<usize>),
    pub frames: Frames,
}

pub struct PreviewWorker {
    tx: Sender<Option<Want>>,
    rx: Receiver<Result<Reply, String>>,
    pub requested: Option<(usize, Option<usize>, Option<usize>)>,
    pub has_camera: bool,
}

impl PreviewWorker {
    pub fn spawn(package: Package, project: Project, ctx: egui::Context) -> Self {
        let (tx, req_rx) = channel::<Option<Want>>();
        let (reply_tx, rx) = channel::<Result<Reply, String>>();
        let (cam_tx, cam_rx) = channel::<bool>();
        std::thread::Builder::new()
            .name("screencraft-preview".into())
            .spawn(move || {
                let mut source = match MultiSource::open(&package, &project) {
                    Ok(s) => s,
                    Err(e) => {
                        let _ = cam_tx.send(false);
                        let _ = reply_tx.send(Err(e));
                        ctx.request_repaint();
                        return;
                    }
                };
                let _ = cam_tx.send(source.has_camera());
                while let Ok(first) = req_rx.recv() {
                    let Some(mut want) = first else { return };
                    // Only the newest request matters while scrubbing.
                    while let Ok(next) = req_rx.try_recv() {
                        match next {
                            Some(w) => want = w,
                            None => return,
                        }
                    }
                    let r = source.fetch(want.t_src, want.other_t).map(|frames| Reply { key: want.key, frames });
                    if reply_tx.send(r).is_err() {
                        return;
                    }
                    ctx.request_repaint();
                }
            })
            .expect("preview thread");
        let has_camera = cam_rx.recv().unwrap_or(false);
        Self { tx, rx, requested: None, has_camera }
    }

    pub fn request(&mut self, want: Want) {
        if self.requested != Some(want.key) {
            self.requested = Some(want.key);
            let _ = self.tx.send(Some(want));
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
