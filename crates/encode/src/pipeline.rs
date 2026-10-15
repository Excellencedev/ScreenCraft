//! The export pipeline as a state machine. Frames and PCM go in, `WriteAt` operations come out;
//! nothing here opens a file or spawns a thread, so identical input always yields identical
//! bytes. `ThreadedExporter` adds a worker thread around it without changing the result.

use crate::codec::{create_video_encoder, VideoEncoder};
use crate::mp4::Mp4Muxer;
use crate::opus::{OpusEncoder, FRAME_SAMPLES};
use crate::sink::Sink;
use crate::types::*;
use crate::webm::WebmMuxer;
use crate::{Error, Result};
use std::collections::VecDeque;
use std::sync::mpsc::{sync_channel, SyncSender};
use std::sync::Arc;
use std::thread::JoinHandle;

#[derive(Clone, Debug)]
pub struct ExportSettings {
    pub container: Container,
    pub video: VideoConfig,
    /// One entry per audio track; empty makes a silent file. WebM allows at most one.
    pub audio: Vec<AudioConfig>,
}

enum Mux {
    Mp4(Mp4Muxer),
    WebM(WebmMuxer),
}

impl Mux {
    fn video(&mut self, p: &Packet) -> Result<Vec<WriteAt>> {
        match self {
            Mux::Mp4(m) => m.write_video(p),
            Mux::WebM(m) => m.write_video(p),
        }
    }
    fn audio(&mut self, track: usize, p: &Packet) -> Result<Vec<WriteAt>> {
        match self {
            Mux::Mp4(m) => m.write_audio(track, p),
            Mux::WebM(m) => m.write_audio(track, p),
        }
    }
    fn finish(&mut self, duration_us: i64) -> Result<Vec<WriteAt>> {
        match self {
            Mux::Mp4(m) => m.finish(),
            Mux::WebM(m) => m.finish(duration_us),
        }
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ExportSummary {
    pub video_frames: u64,
    pub audio_frames: u64,
    pub duration_us: i64,
    pub bytes: u64,
    pub encoder: String,
}

struct AudioLane {
    cfg: AudioConfig,
    opus: OpusEncoder,
    queue: VecDeque<Packet>,
    pcm: Vec<f32>,
    frames: u64,
}

pub struct Exporter {
    settings: ExportSettings,
    video: Box<dyn VideoEncoder>,
    lanes: Vec<AudioLane>,
    mux: Option<Mux>,
    video_q: VecDeque<Packet>,
    video_frames: u64,
    last_video_pts: i64,
    out: VecDeque<WriteAt>,
    bytes: u64,
    finished: bool,
    encoder_name: &'static str,
}

impl Exporter {
    pub fn new(settings: ExportSettings) -> Result<Self> {
        if !settings.container.supports(settings.video.codec) {
            return Err(Error::Config(format!(
                "{} can't be stored in {}",
                settings.video.codec.name(),
                settings.container.extension()
            )));
        }
        let video = create_video_encoder(&settings.video)?;
        let encoder_name = video.name();
        if settings.container == Container::WebM && settings.audio.len() > 1 {
            return Err(Error::Config("WebM export carries a single audio track; mix the tracks first".into()));
        }
        let mut lanes = Vec::new();
        for cfg in &settings.audio {
            lanes.push(AudioLane {
                cfg: *cfg,
                opus: OpusEncoder::new(cfg)?,
                queue: VecDeque::new(),
                pcm: Vec::new(),
                frames: 0,
            });
        }
        Ok(Self {
            settings,
            video,
            lanes,
            mux: None,
            video_q: VecDeque::new(),
            video_frames: 0,
            last_video_pts: 0,
            out: VecDeque::new(),
            bytes: 0,
            finished: false,
            encoder_name,
        })
    }

    pub fn encoder_name(&self) -> &'static str {
        self.encoder_name
    }

    /// One frame. `pts_us` starts at 0 and never decreases.
    pub fn push_video(&mut self, frame: &I420, pts_us: i64) -> Result<()> {
        self.ensure_open()?;
        let pkts = self.video.encode(frame, pts_us)?;
        self.video_frames += 1;
        self.last_video_pts = pts_us;
        self.accept_video(pkts)
    }

    /// Interleaved f32 PCM for audio track `track`, continuous from time zero.
    pub fn push_audio(&mut self, track: usize, pcm: &[f32]) -> Result<()> {
        self.ensure_open()?;
        let lane = self.lanes.get_mut(track).ok_or_else(|| Error::State("no such audio track".into()))?;
        lane.pcm.extend_from_slice(pcm);
        let per = FRAME_SAMPLES * lane.cfg.channels as usize;
        let mut taken = 0;
        while lane.pcm.len() - taken >= per {
            let data = lane.opus.encode(&lane.pcm[taken..taken + per])?;
            lane.queue.push_back(Packet { data, pts_us: lane.frames as i64 * 20_000, key: true });
            lane.frames += 1;
            taken += per;
        }
        lane.pcm.drain(..taken);
        self.pump(false)
    }

    pub fn finish(&mut self) -> Result<ExportSummary> {
        self.ensure_open()?;
        let tail = self.video.flush()?;
        self.accept_video(tail)?;
        for lane in &mut self.lanes {
            let per = FRAME_SAMPLES * lane.cfg.channels as usize;
            if !lane.pcm.is_empty() {
                lane.pcm.resize(per, 0.0);
                let data = lane.opus.encode(&lane.pcm)?;
                lane.queue.push_back(Packet { data, pts_us: lane.frames as i64 * 20_000, key: true });
                lane.frames += 1;
                lane.pcm.clear();
            }
        }
        if self.mux.is_none() {
            return Err(Error::State("no video frames were exported".into()));
        }
        self.pump(true)?;
        let frame_us = self.settings.video.frame_duration_us();
        let duration = self.last_video_pts + frame_us;
        let ops = self.mux.as_mut().unwrap().finish(duration)?;
        for op in ops {
            self.emit(op);
        }
        self.finished = true;
        Ok(ExportSummary {
            video_frames: self.video_frames,
            audio_frames: self.lanes.iter().map(|l| l.frames).max().unwrap_or(0),
            duration_us: duration,
            bytes: self.bytes,
            encoder: self.encoder_name.to_string(),
        })
    }

    /// Next file write, if any is ready.
    pub fn poll(&mut self) -> Option<WriteAt> {
        self.out.pop_front()
    }

    fn ensure_open(&self) -> Result<()> {
        if self.finished {
            Err(Error::State("export already finished".into()))
        } else {
            Ok(())
        }
    }

    fn accept_video(&mut self, pkts: Vec<Packet>) -> Result<()> {
        if self.mux.is_none() && !pkts.is_empty() {
            self.open_mux()?;
        }
        self.video_q.extend(pkts);
        self.pump(false)
    }

    fn open_mux(&mut self) -> Result<()> {
        let v = &self.settings.video;
        let vinfo = VideoTrackInfo {
            codec: v.codec,
            width: v.width,
            height: v.height,
            fps_num: v.fps_num,
            fps_den: v.fps_den,
            extradata: self.video.extradata(),
        };
        let ainfo: Vec<AudioTrackInfo> = self
            .lanes
            .iter()
            .map(|l| AudioTrackInfo { sample_rate: l.cfg.sample_rate, channels: l.cfg.channels, pre_skip: l.opus.pre_skip() })
            .collect();
        let (mux, ops) = match self.settings.container {
            Container::Mp4 => {
                let (m, ops) = Mp4Muxer::start(vinfo, ainfo);
                (Mux::Mp4(m), ops)
            }
            Container::WebM => {
                let (m, ops) = WebmMuxer::start(vinfo, ainfo)?;
                (Mux::WebM(m), ops)
            }
        };
        self.mux = Some(mux);
        for op in ops {
            self.emit(op);
        }
        Ok(())
    }

    fn emit(&mut self, op: WriteAt) {
        self.bytes = self.bytes.max(op.offset + op.data.len() as u64);
        self.out.push_back(op);
    }

    /// Hand packets to the muxer in timestamp order. Without `force`, stop while any stream is
    /// empty because it might still be behind.
    fn pump(&mut self, force: bool) -> Result<()> {
        if self.mux.is_none() {
            return Ok(());
        }
        loop {
            // Earliest head across the video queue and every audio lane.
            let mut best: Option<(i64, Option<usize>)> = self.video_q.front().map(|p| (p.pts_us, None));
            let mut any_empty = self.video_q.is_empty();
            for (i, l) in self.lanes.iter().enumerate() {
                match l.queue.front() {
                    Some(p) => {
                        if best.map(|b| p.pts_us < b.0).unwrap_or(true) {
                            best = Some((p.pts_us, Some(i)));
                        }
                    }
                    None => any_empty = true,
                }
            }
            let Some((_, src)) = best else { break };
            if any_empty && !force {
                break;
            }
            let mux = self.mux.as_mut().unwrap();
            let ops = match src {
                None => mux.video(&self.video_q.pop_front().unwrap())?,
                Some(i) => {
                    let p = self.lanes[i].queue.pop_front().unwrap();
                    mux.audio(i, &p)?
                }
            };
            for op in ops {
                self.emit(op);
            }
        }
        Ok(())
    }
}

enum Msg {
    Video(Arc<I420>, i64),
    Audio(usize, Vec<f32>),
    Finish,
}

/// Runs an `Exporter` on its own thread so rendering and encoding overlap. Input is bounded, so
/// a slow encoder pushes back on the producer. Output bytes are identical to the inline path.
pub struct ThreadedExporter {
    tx: Option<SyncSender<Msg>>,
    join: Option<JoinHandle<Result<ExportSummary>>>,
}

impl ThreadedExporter {
    pub fn spawn(settings: ExportSettings, mut sink: Box<dyn Sink + Send>) -> Result<Self> {
        let mut ex = Exporter::new(settings)?;
        let (tx, rx) = sync_channel::<Msg>(6);
        let join = std::thread::Builder::new()
            .name("screencraft-encode".into())
            .spawn(move || -> Result<ExportSummary> {
                let mut drain = |ex: &mut Exporter| -> Result<()> {
                    while let Some(op) = ex.poll() {
                        sink.apply(&op)?;
                    }
                    Ok(())
                };
                while let Ok(m) = rx.recv() {
                    match m {
                        Msg::Video(f, pts) => ex.push_video(&f, pts)?,
                        Msg::Audio(t, p) => ex.push_audio(t, &p)?,
                        Msg::Finish => {
                            let s = ex.finish()?;
                            drain(&mut ex)?;
                            return Ok(s);
                        }
                    }
                    drain(&mut ex)?;
                }
                Err(Error::State("export abandoned before finish".into()))
            })?;
        Ok(Self { tx: Some(tx), join: Some(join) })
    }

    fn send(&self, m: Msg) -> Result<()> {
        self.tx
            .as_ref()
            .ok_or_else(|| Error::State("export already finished".into()))?
            .send(m)
            .map_err(|_| Error::State("encoder thread stopped (see finish() for the error)".into()))
    }

    pub fn push_video(&self, frame: Arc<I420>, pts_us: i64) -> Result<()> {
        self.send(Msg::Video(frame, pts_us))
    }

    pub fn push_audio(&self, track: usize, pcm: Vec<f32>) -> Result<()> {
        self.send(Msg::Audio(track, pcm))
    }

    pub fn finish(mut self) -> Result<ExportSummary> {
        if let Some(tx) = self.tx.take() {
            let _ = tx.send(Msg::Finish);
        }
        match self.join.take().unwrap().join() {
            Ok(r) => r,
            Err(_) => Err(Error::State("encoder thread panicked".into())),
        }
    }
}

impl Drop for ThreadedExporter {
    fn drop(&mut self) {
        self.tx.take();
        if let Some(j) = self.join.take() {
            let _ = j.join();
        }
    }
}
