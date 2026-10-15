//! WebM (Matroska subset) writer for VP9 / AV1 + Opus. Clusters are buffered and written with
//! known sizes; Cues, the segment size and the duration are patched at the end.

use crate::types::*;
use crate::{Error, Result};

fn id_bytes(id: u32) -> Vec<u8> {
    let b = id.to_be_bytes();
    let skip = b.iter().position(|&x| x != 0).unwrap_or(3);
    b[skip..].to_vec()
}

fn size_vint(n: u64) -> Vec<u8> {
    for len in 1..=8u32 {
        if n < (1u64 << (7 * len)) - 1 {
            let mut v = (n | (1u64 << (7 * len))).to_be_bytes().to_vec();
            return v.split_off(8 - len as usize);
        }
    }
    unreachable!("size too large")
}

fn el(id: u32, payload: &[u8]) -> Vec<u8> {
    let mut v = id_bytes(id);
    v.extend(size_vint(payload.len() as u64));
    v.extend_from_slice(payload);
    v
}

fn uint(id: u32, v: u64) -> Vec<u8> {
    let b = v.to_be_bytes();
    let skip = b.iter().position(|&x| x != 0).unwrap_or(7);
    el(id, &b[skip..])
}

fn string(id: u32, s: &str) -> Vec<u8> {
    el(id, s.as_bytes())
}

const SEGMENT_ID: u32 = 0x1853_8067;
const CLUSTER_MAX_US: i64 = 5_000_000;

pub struct WebmMuxer {
    pos: u64,
    /// Offset of the first byte after the segment header (positions are relative to it).
    seg_data: u64,
    seg_size_at: u64,
    seekhead_at: u64,
    info_pos: u64,
    tracks_pos: u64,
    duration_at: u64,
    cluster: Vec<u8>,
    cluster_start_us: Option<i64>,
    cluster_file_pos: u64,
    cues: Vec<(i64, u64)>,
    video_track: u64,
    audio_track: Option<u64>,
    last_pts_us: i64,
    finished: bool,
}

impl WebmMuxer {
    pub fn start(video: VideoTrackInfo, audio: Vec<AudioTrackInfo>) -> Result<(Self, Vec<WriteAt>)> {
        if audio.len() > 1 {
            return Err(Error::Config("WebM export carries a single audio track; mix the tracks first".into()));
        }
        let audio = audio.into_iter().next();
        if !matches!(video.codec, VideoCodec::Av1 | VideoCodec::Vp9) {
            return Err(Error::Config("WebM carries VP9 or AV1 (use MP4 for H.264)".into()));
        }
        let mut out = Vec::new();

        let mut ebml = Vec::new();
        ebml.extend(uint(0x4286, 1));
        ebml.extend(uint(0x42F7, 1));
        ebml.extend(uint(0x42F2, 4));
        ebml.extend(uint(0x42F3, 8));
        ebml.extend(string(0x4282, "webm"));
        ebml.extend(uint(0x4287, 4));
        ebml.extend(uint(0x4285, 2));
        out.extend(el(0x1A45_DFA3, &ebml));

        out.extend(id_bytes(SEGMENT_ID));
        let seg_size_at = out.len() as u64;
        out.extend_from_slice(&[0x01, 0xFF, 0xFF, 0xFF, 0xFF, 0xFF, 0xFF, 0xFF]);
        let seg_data = out.len() as u64;

        // SeekHead has a fixed size so it can be rewritten once the Cues position is known.
        let seekhead_at = out.len() as u64;
        let seekhead_len = Self::seekhead(0, 0, 0).len();
        out.extend(vec![0u8; seekhead_len]);

        let info_pos = out.len() as u64 - seg_data;
        let mut info = Vec::new();
        info.extend(uint(0x2A_D7B1, 1_000_000));
        info.extend(string(0x4D80, "ScreenCraft"));
        info.extend(string(0x5741, "ScreenCraft"));
        info.extend(id_bytes(0x4489));
        info.extend(size_vint(8));
        info.extend_from_slice(&0f64.to_be_bytes());
        let info_el = el(0x1549_A966, &info);
        let duration_at = out.len() as u64 + (info_el.len() as u64 - 8);
        out.extend(info_el);

        let tracks_pos = out.len() as u64 - seg_data;
        let mut tracks = Vec::new();
        let codec_id = if video.codec == VideoCodec::Av1 { "V_AV1" } else { "V_VP9" };
        let mut v = Vec::new();
        v.extend(uint(0xD7, 1));
        v.extend(uint(0x73C5, 1));
        v.extend(uint(0x83, 1));
        v.extend(uint(0x9C, 0));
        v.extend(string(0x86, codec_id));
        if video.codec == VideoCodec::Av1 && !video.extradata.is_empty() {
            v.extend(el(0x63A2, &video.extradata));
        }
        let mut vid = Vec::new();
        vid.extend(uint(0xB0, video.width as u64));
        vid.extend(uint(0xBA, video.height as u64));
        v.extend(el(0xE0, &vid));
        tracks.extend(el(0xAE, &v));
        let mut audio_track = None;
        if let Some(a) = &audio {
            let mut t = Vec::new();
            t.extend(uint(0xD7, 2));
            t.extend(uint(0x73C5, 2));
            t.extend(uint(0x83, 2));
            t.extend(uint(0x9C, 0));
            t.extend(string(0x86, "A_OPUS"));
            let mut head = b"OpusHead".to_vec();
            head.push(1);
            head.push(a.channels);
            head.extend_from_slice(&a.pre_skip.to_le_bytes());
            head.extend_from_slice(&a.sample_rate.to_le_bytes());
            head.extend_from_slice(&0i16.to_le_bytes());
            head.push(0);
            t.extend(el(0x63A2, &head));
            t.extend(uint(0x56AA, a.pre_skip as u64 * 1_000_000_000 / 48_000));
            t.extend(uint(0x56BB, 80_000_000));
            let mut au = Vec::new();
            au.extend(el(0xB5, &(a.sample_rate as f64).to_be_bytes()));
            au.extend(uint(0x9F, a.channels as u64));
            t.extend(el(0xE1, &au));
            tracks.extend(el(0xAE, &t));
            audio_track = Some(2);
        }
        out.extend(el(0x1654_AE6B, &tracks));

        let pos = out.len() as u64;
        let m = Self {
            pos,
            seg_data,
            seg_size_at,
            seekhead_at,
            info_pos,
            tracks_pos,
            duration_at,
            cluster: Vec::new(),
            cluster_start_us: None,
            cluster_file_pos: 0,
            cues: Vec::new(),
            video_track: 1,
            audio_track,
            last_pts_us: 0,
            finished: false,
        };
        Ok((m, vec![WriteAt { offset: 0, data: out }]))
    }

    /// Positions are written as fixed 4-byte integers so the element never changes size.
    fn seekhead(info: u64, tracks: u64, cues: u64) -> Vec<u8> {
        let mut body = Vec::new();
        for (id, pos) in [(0x1549_A966u32, info), (0x1654_AE6B, tracks), (0x1C53_BB6B, cues)] {
            let mut s = Vec::new();
            s.extend(el(0x53AB, &id_bytes(id)));
            s.extend(el(0x53AC, &(pos as u32).to_be_bytes()));
            body.extend(el(0x4DBB, &s));
        }
        el(0x114D_9B74, &body)
    }

    fn flush_cluster(&mut self, ops: &mut Vec<WriteAt>) {
        let Some(start) = self.cluster_start_us else { return };
        let mut payload = uint(0xE7, (start / 1000) as u64);
        payload.extend_from_slice(&self.cluster);
        let data = el(0x1F43_B675, &payload);
        self.cluster_file_pos = self.pos;
        let len = data.len() as u64;
        ops.push(WriteAt { offset: self.pos, data });
        self.pos += len;
        self.cluster.clear();
        self.cluster_start_us = None;
    }

    fn block(&mut self, track: u64, pkt: &Packet, is_video: bool) -> Result<Vec<WriteAt>> {
        if self.finished {
            return Err(Error::State("webm already finished".into()));
        }
        let mut ops = Vec::new();
        let start_new = match self.cluster_start_us {
            None => true,
            Some(s) => pkt.pts_us - s >= CLUSTER_MAX_US || (is_video && pkt.key && pkt.pts_us - s >= 1_000_000),
        };
        if start_new {
            self.flush_cluster(&mut ops);
            self.cluster_start_us = Some(pkt.pts_us);
        }
        let start = self.cluster_start_us.unwrap();
        let rel = ((pkt.pts_us - start) / 1000) as i64;
        let mut b = size_vint(track);
        b.extend_from_slice(&(rel as i16).to_be_bytes());
        b.push(if pkt.key { 0x80 } else { 0 });
        b.extend_from_slice(&pkt.data);
        if is_video && pkt.key {
            // Cluster position is fixed once flushed; remember the future one.
            let pending_pos = self.pos;
            self.cues.push((pkt.pts_us / 1000, pending_pos - self.seg_data));
        }
        self.cluster.extend(el(0xA3, &b));
        self.last_pts_us = self.last_pts_us.max(pkt.pts_us);
        Ok(ops)
    }

    pub fn write_video(&mut self, pkt: &Packet) -> Result<Vec<WriteAt>> {
        self.block(self.video_track, pkt, true)
    }

    pub fn write_audio(&mut self, _track: usize, pkt: &Packet) -> Result<Vec<WriteAt>> {
        let t = self.audio_track.ok_or_else(|| Error::State("no audio track".into()))?;
        self.block(t, pkt, false)
    }

    pub fn finish(&mut self, duration_us: i64) -> Result<Vec<WriteAt>> {
        if self.finished {
            return Err(Error::State("webm already finished".into()));
        }
        let mut ops = Vec::new();
        self.flush_cluster(&mut ops);
        self.finished = true;

        let cues_pos = self.pos - self.seg_data;
        let mut cues = Vec::new();
        for &(t_ms, cluster_pos) in &self.cues {
            let mut tp = uint(0xF7, self.video_track);
            tp.extend(uint(0xF1, cluster_pos));
            let mut cp = uint(0xB3, t_ms as u64);
            cp.extend(el(0xB7, &tp));
            cues.extend(el(0xBB, &cp));
        }
        let cues = el(0x1C53_BB6B, &cues);
        let cues_len = cues.len() as u64;
        ops.push(WriteAt { offset: self.pos, data: cues });
        self.pos += cues_len;

        let seg_len = self.pos - self.seg_data;
        let mut size = (seg_len | (1u64 << 56)).to_be_bytes().to_vec();
        size[0] = 0x01;
        ops.push(WriteAt { offset: self.seg_size_at, data: size });
        ops.push(WriteAt { offset: self.seekhead_at, data: Self::seekhead(self.info_pos, self.tracks_pos, cues_pos) });
        let dur_ms = (duration_us.max(self.last_pts_us) as f64) / 1000.0;
        ops.push(WriteAt { offset: self.duration_at, data: dur_ms.to_be_bytes().to_vec() });
        Ok(ops)
    }
}
