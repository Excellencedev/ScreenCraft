//! On-device captions with whisper.cpp (filled in by the captions milestone).

use std::sync::mpsc::Sender;

pub struct Caption {
    pub text: String,
    pub start_ms: f64,
    pub end_ms: f64,
    pub word_starts: Vec<f64>,
}

pub enum CapEvent {
    Progress(String),
    Done(Result<Vec<Caption>, String>),
}

pub struct Request {
    pub package: screencraft_engine::Package,
    pub project: screencraft_core::project::Project,
    pub model: String,
    pub language: String,
}

pub fn spawn(tx: Sender<CapEvent>, _req: Request) {
    let _ = tx.send(CapEvent::Done(Err("Captions are not built into this version yet.".into())));
}
