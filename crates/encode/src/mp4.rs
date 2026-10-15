//! MP4 writer. Append-only: `ftyp`, a 64-bit `mdat` that is patched when the file ends, then
//! `moov`. The writer only describes writes (see `WriteAt`), it never touches a file.

use crate::types::*;
use crate::{Error, Result};

fn bx(typ: &[u8; 4], payload: &[u8]) -> Vec<u8> {
    let mut v = Vec::with_capacity(payload.len() + 8);
    v.extend_from_slice(&((payload.len() + 8) as u32).to_be_bytes());
    v.extend_from_slice(typ);
    v.extend_from_slice(payload);
    v
}

fn fullbox(typ: &[u8; 4], version: u8, flags: u32, payload: &[u8]) -> Vec<u8> {
    let mut p = Vec::with_capacity(payload.len() + 4);
    p.push(version);
    p.extend_from_slice(&flags.to_be_bytes()[1..]);
    p.extend_from_slice(payload);
    bx(typ, &p)
}

fn u32s(vals: &[u32]) -> Vec<u8> {
    vals.iter().flat_map(|v| v.to_be_bytes()).collect()
}

const MATRIX: [u32; 9] = [0x0001_0000, 0, 0, 0, 0x0001_0000, 0, 0, 0, 0x4000_0000];

struct Sample {
    size: u32,
    pts_us: i64,
    key: bool,
}

struct TrackState {
    id: u32,
    timescale: u32,
    samples: Vec<Sample>,
    /// (file offset, sample count) per chunk.
    chunks: Vec<(u64, u32)>,
    default_dur_us: i64,
}

impl TrackState {
    fn new(id: u32, timescale: u32, default_dur_us: i64) -> Self {
        Self { id, timescale, samples: Vec::new(), chunks: Vec::new(), default_dur_us }
    }

    fn ticks(&self, us: i64) -> i64 {
        (us * self.timescale as i64 + 500_000).div_euclid(1_000_000)
    }

    /// dts of each sample in track ticks, plus the end of the last sample.
    fn dts(&self) -> Vec<i64> {
        let mut v: Vec<i64> = self.samples.iter().map(|s| self.ticks(s.pts_us)).collect();
        let last = self.samples.last().map(|s| s.pts_us + self.default_dur_us).unwrap_or(0);
        v.push(self.ticks(last));
        v
    }

    fn duration_ticks(&self) -> i64 {
        self.dts().last().copied().unwrap_or(0)
    }
}

pub struct Mp4Muxer {
    video: VideoTrackInfo,
    audio: Vec<AudioTrackInfo>,
    vt: TrackState,
    at: Vec<TrackState>,
    pos: u64,
    mdat_at: u64,
    /// 0 = video, 1.. = audio track + 1: which track wrote the most recent bytes.
    last: Option<usize>,
    finished: bool,
}

impl Mp4Muxer {
    /// Writes the header. The returned ops must be applied before any sample.
    pub fn start(video: VideoTrackInfo, audio: Vec<AudioTrackInfo>) -> (Self, Vec<WriteAt>) {
        let vdur = 1_000_000i64 * video.fps_den as i64 / video.fps_num.max(1) as i64;
        let vt = TrackState::new(1, 90_000, vdur);
        let at = audio.iter().enumerate().map(|(i, a)| TrackState::new(i as u32 + 2, a.sample_rate, 20_000)).collect();
        let brand: &[u8; 4] = match video.codec {
            VideoCodec::H264 => b"avc1",
            VideoCodec::Av1 => b"av01",
            VideoCodec::Vp9 => b"vp09",
        };
        let mut ftyp = Vec::new();
        ftyp.extend_from_slice(b"isom");
        ftyp.extend_from_slice(&512u32.to_be_bytes());
        for b in [b"isom", b"iso2", brand, b"mp41"] {
            ftyp.extend_from_slice(b);
        }
        let mut head = bx(b"ftyp", &ftyp);
        let mdat_at = head.len() as u64;
        // 64-bit mdat header, size patched in finish().
        head.extend_from_slice(&1u32.to_be_bytes());
        head.extend_from_slice(b"mdat");
        head.extend_from_slice(&0u64.to_be_bytes());
        let pos = head.len() as u64;
        let m = Self { video, audio, vt, at, pos, mdat_at, last: None, finished: false };
        (m, vec![WriteAt { offset: 0, data: head }])
    }

    fn write(&mut self, which: usize, pkt: &Packet) -> Result<Vec<WriteAt>> {
        if self.finished {
            return Err(Error::State("mp4 already finished".into()));
        }
        let new_chunk = self.last != Some(which);
        let off = self.pos;
        let t = if which == 0 {
            &mut self.vt
        } else {
            self.at.get_mut(which - 1).ok_or_else(|| Error::State("no such audio track".into()))?
        };
        if let Some(prev) = t.samples.last() {
            if pkt.pts_us < prev.pts_us {
                return Err(Error::State("timestamps went backwards".into()));
            }
        }
        t.samples.push(Sample { size: pkt.data.len() as u32, pts_us: pkt.pts_us, key: pkt.key });
        if new_chunk || t.chunks.is_empty() {
            t.chunks.push((off, 1));
        } else {
            t.chunks.last_mut().unwrap().1 += 1;
        }
        self.last = Some(which);
        self.pos += pkt.data.len() as u64;
        Ok(vec![WriteAt { offset: off, data: pkt.data.clone() }])
    }

    pub fn write_video(&mut self, pkt: &Packet) -> Result<Vec<WriteAt>> {
        self.write(0, pkt)
    }

    pub fn write_audio(&mut self, track: usize, pkt: &Packet) -> Result<Vec<WriteAt>> {
        self.write(track + 1, pkt)
    }

    pub fn finish(&mut self) -> Result<Vec<WriteAt>> {
        if self.finished {
            return Err(Error::State("mp4 already finished".into()));
        }
        self.finished = true;
        let mdat_size = self.pos - self.mdat_at;
        let mut ops = vec![WriteAt { offset: self.mdat_at + 8, data: mdat_size.to_be_bytes().to_vec() }];
        let moov = self.moov();
        ops.push(WriteAt { offset: self.pos, data: moov });
        Ok(ops)
    }

    fn moov(&self) -> Vec<u8> {
        let vdur_ms = self.vt.duration_ticks() * 1000 / self.vt.timescale as i64;
        let adur_ms = self.at.iter().map(|t| t.duration_ticks() * 1000 / t.timescale as i64).max().unwrap_or(0);
        let movie_ms = vdur_ms.max(adur_ms) as u32;
        let next_id = self.at.len() as u32 + 2;

        let mut mvhd = Vec::new();
        mvhd.extend(u32s(&[0, 0, 1000, movie_ms, 0x0001_0000]));
        mvhd.extend_from_slice(&0x0100u16.to_be_bytes());
        mvhd.extend_from_slice(&[0; 10]);
        mvhd.extend(u32s(&MATRIX));
        mvhd.extend(u32s(&[0; 6]));
        mvhd.extend(u32s(&[next_id]));

        let mut moov = fullbox(b"mvhd", 0, 0, &mvhd);
        moov.extend(self.trak(None));
        for i in 0..self.at.len() {
            moov.extend(self.trak(Some(i)));
        }
        bx(b"moov", &moov)
    }

    /// `None` is the video track, `Some(i)` the i-th audio track.
    fn trak(&self, audio: Option<usize>) -> Vec<u8> {
        let video = audio.is_none();
        let t = match audio {
            None => &self.vt,
            Some(i) => &self.at[i],
        };
        let dur_media = t.duration_ticks() as u32;
        let dur_movie = (t.duration_ticks() * 1000 / t.timescale as i64) as u32;

        let mut tkhd = Vec::new();
        tkhd.extend(u32s(&[0, 0, t.id, 0, dur_movie, 0, 0]));
        tkhd.extend_from_slice(&0u16.to_be_bytes()); // layer
        tkhd.extend_from_slice(&0u16.to_be_bytes()); // alternate group
        tkhd.extend_from_slice(&(if video { 0u16 } else { 0x0100 }).to_be_bytes());
        tkhd.extend_from_slice(&0u16.to_be_bytes());
        tkhd.extend(u32s(&MATRIX));
        if video {
            tkhd.extend(u32s(&[self.video.width << 16, self.video.height << 16]));
        } else {
            tkhd.extend(u32s(&[0, 0]));
        }
        let tkhd = fullbox(b"tkhd", 0, 3, &tkhd);

        let mut mdhd = Vec::new();
        mdhd.extend(u32s(&[0, 0, t.timescale, dur_media]));
        mdhd.extend_from_slice(&0x55C4u16.to_be_bytes());
        mdhd.extend_from_slice(&0u16.to_be_bytes());
        let mdhd = fullbox(b"mdhd", 0, 0, &mdhd);

        let mut hdlr = Vec::new();
        hdlr.extend(u32s(&[0]));
        hdlr.extend_from_slice(if video { b"vide" } else { b"soun" });
        hdlr.extend(u32s(&[0, 0, 0]));
        hdlr.extend_from_slice(if video { b"VideoHandler\0" } else { b"SoundHandler\0" });
        let hdlr = fullbox(b"hdlr", 0, 0, &hdlr);

        let media_header = if video {
            fullbox(b"vmhd", 0, 1, &[0; 8])
        } else {
            fullbox(b"smhd", 0, 0, &[0; 4])
        };
        let url = fullbox(b"url ", 0, 1, &[]);
        let mut dref = 1u32.to_be_bytes().to_vec();
        dref.extend(url);
        let dinf = bx(b"dinf", &fullbox(b"dref", 0, 0, &dref));

        let mut stbl = self.stsd(audio);
        stbl.extend(self.stts(t));
        if video {
            let keys: Vec<u32> = t.samples.iter().enumerate().filter(|(_, s)| s.key).map(|(i, _)| i as u32 + 1).collect();
            if keys.len() != t.samples.len() {
                let mut p = (keys.len() as u32).to_be_bytes().to_vec();
                p.extend(u32s(&keys));
                stbl.extend(fullbox(b"stss", 0, 0, &p));
            }
        }
        // stsc: run-length over chunk sample counts.
        let mut stsc: Vec<[u32; 3]> = Vec::new();
        for (i, &(_, n)) in t.chunks.iter().enumerate() {
            if stsc.last().map(|e| e[1]) != Some(n) {
                stsc.push([i as u32 + 1, n, 1]);
            }
        }
        let mut p = (stsc.len() as u32).to_be_bytes().to_vec();
        for e in &stsc {
            p.extend(u32s(e));
        }
        stbl.extend(fullbox(b"stsc", 0, 0, &p));
        let mut p = u32s(&[0, t.samples.len() as u32]);
        for s in &t.samples {
            p.extend_from_slice(&s.size.to_be_bytes());
        }
        stbl.extend(fullbox(b"stsz", 0, 0, &p));
        let big = t.chunks.iter().any(|c| c.0 > u32::MAX as u64);
        let mut p = (t.chunks.len() as u32).to_be_bytes().to_vec();
        for &(off, _) in &t.chunks {
            if big {
                p.extend_from_slice(&off.to_be_bytes());
            } else {
                p.extend_from_slice(&(off as u32).to_be_bytes());
            }
        }
        stbl.extend(fullbox(if big { b"co64" } else { b"stco" }, 0, 0, &p));

        let mut minf = media_header;
        minf.extend(dinf);
        minf.extend(bx(b"stbl", &stbl));
        let mut mdia = mdhd;
        mdia.extend(hdlr);
        mdia.extend(bx(b"minf", &minf));
        let mut trak = tkhd;
        trak.extend(bx(b"mdia", &mdia));
        bx(b"trak", &trak)
    }

    fn stts(&self, t: &TrackState) -> Vec<u8> {
        let dts = t.dts();
        let mut runs: Vec<(u32, u32)> = Vec::new();
        for w in dts.windows(2) {
            let d = (w[1] - w[0]).max(0) as u32;
            match runs.last_mut() {
                Some(r) if r.1 == d => r.0 += 1,
                _ => runs.push((1, d)),
            }
        }
        let mut p = (runs.len() as u32).to_be_bytes().to_vec();
        for (c, d) in runs {
            p.extend(u32s(&[c, d]));
        }
        fullbox(b"stts", 0, 0, &p)
    }

    fn stsd(&self, audio: Option<usize>) -> Vec<u8> {
        let entry = match audio {
            None => self.visual_entry(),
            Some(i) => self.audio_entry(i),
        };
        let mut p = 1u32.to_be_bytes().to_vec();
        p.extend(entry);
        fullbox(b"stsd", 0, 0, &p)
    }

    fn visual_entry(&self) -> Vec<u8> {
        let v = &self.video;
        let mut e = vec![0u8; 6];
        e.extend_from_slice(&1u16.to_be_bytes());
        e.extend_from_slice(&[0; 16]);
        e.extend_from_slice(&(v.width as u16).to_be_bytes());
        e.extend_from_slice(&(v.height as u16).to_be_bytes());
        e.extend(u32s(&[0x0048_0000, 0x0048_0000, 0]));
        e.extend_from_slice(&1u16.to_be_bytes());
        e.extend_from_slice(&[0; 32]);
        e.extend_from_slice(&0x0018u16.to_be_bytes());
        e.extend_from_slice(&0xFFFFu16.to_be_bytes());
        let (entry, cfg): (&[u8; 4], &[u8; 4]) = match v.codec {
            VideoCodec::H264 => (b"avc1", b"avcC"),
            VideoCodec::Av1 => (b"av01", b"av1C"),
            VideoCodec::Vp9 => (b"vp09", b"vpcC"),
        };
        if v.codec == VideoCodec::Vp9 {
            // The record already carries its version/flags word.
            e.extend(bx(cfg, &v.extradata));
        } else {
            e.extend(bx(cfg, &v.extradata));
        }
        // colr nclx: BT.709 primaries, transfer and matrix, limited range.
        let mut colr = b"nclx".to_vec();
        colr.extend_from_slice(&1u16.to_be_bytes());
        colr.extend_from_slice(&1u16.to_be_bytes());
        colr.extend_from_slice(&1u16.to_be_bytes());
        colr.push(0);
        e.extend(bx(b"colr", &colr));
        bx(entry, &e)
    }

    fn audio_entry(&self, index: usize) -> Vec<u8> {
        let a = &self.audio[index];
        let mut e = vec![0u8; 6];
        e.extend_from_slice(&1u16.to_be_bytes());
        e.extend_from_slice(&[0; 8]);
        e.extend_from_slice(&(a.channels as u16).to_be_bytes());
        e.extend_from_slice(&16u16.to_be_bytes());
        e.extend_from_slice(&[0; 4]);
        e.extend_from_slice(&(a.sample_rate << 16).to_be_bytes());
        let mut dops = vec![0u8, a.channels];
        dops.extend_from_slice(&a.pre_skip.to_be_bytes());
        dops.extend_from_slice(&a.sample_rate.to_be_bytes());
        dops.extend_from_slice(&0i16.to_be_bytes());
        dops.push(0);
        e.extend(bx(b"dOps", &dops));
        bx(b"Opus", &e)
    }
}
