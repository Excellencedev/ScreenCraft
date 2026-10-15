//! MP4 demuxer: enough of ISO BMFF to read recordings made by ScreenCraft and ordinary
//! H.264 / Opus / AAC files. Only the sample tables are parsed, media bytes are read on demand.

use crate::{Error, Result};
use std::fs::File;
use std::io::{Read, Seek, SeekFrom};
use std::path::Path;
use std::sync::Mutex;

/// Random access to the bytes of a file.
pub trait ReadAt: Send + Sync {
    fn len(&self) -> u64;
    fn read_at(&self, offset: u64, buf: &mut [u8]) -> std::io::Result<()>;
}

impl ReadAt for Vec<u8> {
    fn len(&self) -> u64 {
        self.as_slice().len() as u64
    }
    fn read_at(&self, offset: u64, buf: &mut [u8]) -> std::io::Result<()> {
        let o = offset as usize;
        let src = self.get(o..o + buf.len()).ok_or_else(|| std::io::Error::from(std::io::ErrorKind::UnexpectedEof))?;
        buf.copy_from_slice(src);
        Ok(())
    }
}

pub struct FileSource {
    file: Mutex<File>,
    len: u64,
}

impl FileSource {
    pub fn open(path: &Path) -> std::io::Result<Self> {
        let file = File::open(path)?;
        let len = file.metadata()?.len();
        Ok(Self { file: Mutex::new(file), len })
    }
}

impl ReadAt for FileSource {
    fn len(&self) -> u64 {
        self.len
    }
    fn read_at(&self, offset: u64, buf: &mut [u8]) -> std::io::Result<()> {
        let mut f = self.file.lock().unwrap();
        f.seek(SeekFrom::Start(offset))?;
        f.read_exact(buf)
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum TrackKind {
    Video,
    Audio,
    Other,
}

#[derive(Clone, Debug)]
pub struct SampleRef {
    pub offset: u64,
    pub size: u32,
    pub dts_us: i64,
    pub pts_us: i64,
    pub key: bool,
}

#[derive(Clone, Debug)]
pub struct Track {
    pub id: u32,
    pub kind: TrackKind,
    pub timescale: u32,
    pub duration_us: i64,
    /// Sample entry fourcc: avc1, hvc1, hev1, av01, vp09, Opus, mp4a ...
    pub codec: [u8; 4],
    /// avcC / hvcC / av1C / vpcC / dOps / esds payload.
    pub config: Vec<u8>,
    pub width: u32,
    pub height: u32,
    pub channels: u16,
    pub sample_rate: u32,
    pub samples: Vec<SampleRef>,
}

impl Track {
    pub fn codec_str(&self) -> String {
        String::from_utf8_lossy(&self.codec).into_owned()
    }
}

pub struct Mp4File {
    src: Box<dyn ReadAt>,
    pub tracks: Vec<Track>,
    pub duration_us: i64,
}

struct Cur<'a> {
    d: &'a [u8],
    p: usize,
}

impl<'a> Cur<'a> {
    fn new(d: &'a [u8]) -> Self {
        Self { d, p: 0 }
    }
    fn u8(&mut self) -> Result<u8> {
        let v = *self.d.get(self.p).ok_or_else(eof)?;
        self.p += 1;
        Ok(v)
    }
    fn u16(&mut self) -> Result<u16> {
        Ok(((self.u8()? as u16) << 8) | self.u8()? as u16)
    }
    fn u32(&mut self) -> Result<u32> {
        Ok(((self.u16()? as u32) << 16) | self.u16()? as u32)
    }
    fn u64(&mut self) -> Result<u64> {
        Ok(((self.u32()? as u64) << 32) | self.u32()? as u64)
    }
    fn skip(&mut self, n: usize) -> Result<()> {
        if self.p + n > self.d.len() {
            return Err(eof());
        }
        self.p += n;
        Ok(())
    }
    fn rest(&self) -> &'a [u8] {
        &self.d[self.p..]
    }
}

fn eof() -> Error {
    Error::Format("truncated MP4 box".into())
}

/// Iterate the child boxes of `data`: (type, payload).
fn boxes(data: &[u8]) -> Vec<([u8; 4], &[u8])> {
    let mut out = Vec::new();
    let mut p = 0usize;
    while p + 8 <= data.len() {
        let mut size = u32::from_be_bytes(data[p..p + 4].try_into().unwrap()) as u64;
        let typ: [u8; 4] = data[p + 4..p + 8].try_into().unwrap();
        let mut hdr = 8;
        if size == 1 && p + 16 <= data.len() {
            size = u64::from_be_bytes(data[p + 8..p + 16].try_into().unwrap());
            hdr = 16;
        } else if size == 0 {
            size = (data.len() - p) as u64;
        }
        if size < hdr as u64 || p as u64 + size > data.len() as u64 {
            break;
        }
        out.push((typ, &data[p + hdr..p + size as usize]));
        p += size as usize;
    }
    out
}

fn find<'a>(data: &'a [u8], typ: &[u8; 4]) -> Option<&'a [u8]> {
    boxes(data).into_iter().find(|b| &b.0 == typ).map(|b| b.1)
}

impl Mp4File {
    pub fn open(path: &Path) -> Result<Self> {
        Self::from_source(Box::new(FileSource::open(path)?))
    }

    pub fn from_source(src: Box<dyn ReadAt>) -> Result<Self> {
        // Walk the top-level boxes by header only; the moov is the one we need in memory.
        let len = src.len();
        let mut pos = 0u64;
        let mut moov: Option<Vec<u8>> = None;
        while pos + 8 <= len {
            let mut h = [0u8; 16];
            let n = ((len - pos).min(16)) as usize;
            src.read_at(pos, &mut h[..n])?;
            let mut size = u32::from_be_bytes(h[0..4].try_into().unwrap()) as u64;
            let typ: [u8; 4] = h[4..8].try_into().unwrap();
            let mut hdr = 8u64;
            if size == 1 && n >= 16 {
                size = u64::from_be_bytes(h[8..16].try_into().unwrap());
                hdr = 16;
            } else if size == 0 {
                size = len - pos;
            }
            if size < hdr {
                return Err(Error::Format("bad box size".into()));
            }
            if &typ == b"moov" {
                let mut buf = vec![0u8; (size - hdr) as usize];
                src.read_at(pos + hdr, &mut buf)?;
                moov = Some(buf);
                break;
            }
            pos += size;
        }
        let moov = moov.ok_or_else(|| Error::Format("no moov box (recording was not finished?)".into()))?;
        let mut tracks = Vec::new();
        let mut movie_ts = 1000u32;
        if let Some(mvhd) = find(&moov, b"mvhd") {
            let mut c = Cur::new(mvhd);
            let v = c.u8()?;
            c.skip(3)?;
            c.skip(if v == 1 { 16 } else { 8 })?;
            movie_ts = c.u32()?.max(1);
        }
        let _ = movie_ts;
        for (t, body) in boxes(&moov) {
            if &t == b"trak" {
                if let Some(tr) = parse_trak(body)? {
                    tracks.push(tr);
                }
            }
        }
        let duration_us = tracks.iter().map(|t| t.duration_us).max().unwrap_or(0);
        Ok(Self { src, tracks, duration_us })
    }

    pub fn video(&self) -> Option<&Track> {
        self.tracks.iter().find(|t| t.kind == TrackKind::Video)
    }

    pub fn audio_tracks(&self) -> impl Iterator<Item = &Track> {
        self.tracks.iter().filter(|t| t.kind == TrackKind::Audio)
    }

    pub fn read_sample(&self, s: &SampleRef) -> Result<Vec<u8>> {
        let mut buf = vec![0u8; s.size as usize];
        self.src.read_at(s.offset, &mut buf)?;
        Ok(buf)
    }
}

fn parse_trak(trak: &[u8]) -> Result<Option<Track>> {
    let Some(tkhd) = find(trak, b"tkhd") else { return Ok(None) };
    let mdia = find(trak, b"mdia").ok_or_else(|| Error::Format("trak without mdia".into()))?;
    let mdhd = find(mdia, b"mdhd").ok_or_else(|| Error::Format("no mdhd".into()))?;
    let hdlr = find(mdia, b"hdlr").ok_or_else(|| Error::Format("no hdlr".into()))?;
    let stbl = find(mdia, b"minf").and_then(|m| find(m, b"stbl")).ok_or_else(|| Error::Format("no stbl".into()))?;

    let mut c = Cur::new(tkhd);
    let v = c.u8()?;
    c.skip(3)?;
    c.skip(if v == 1 { 16 } else { 8 })?;
    let id = c.u32()?;

    let mut c = Cur::new(mdhd);
    let v = c.u8()?;
    c.skip(3)?;
    c.skip(if v == 1 { 16 } else { 8 })?;
    let timescale = c.u32()?.max(1);
    let dur = if v == 1 { c.u64()? } else { c.u32()? as u64 };

    let mut c = Cur::new(hdlr);
    c.skip(8)?;
    let kind = match &c.u32()?.to_be_bytes() {
        b"vide" => TrackKind::Video,
        b"soun" => TrackKind::Audio,
        _ => TrackKind::Other,
    };

    // Sample description: only the first entry matters.
    let stsd = find(stbl, b"stsd").ok_or_else(|| Error::Format("no stsd".into()))?;
    let entries = boxes(&stsd[8.min(stsd.len())..]);
    let (codec, entry) = entries.first().copied().ok_or_else(|| Error::Format("empty stsd".into()))?;
    let (mut width, mut height, mut channels, mut sample_rate) = (0, 0, 0u16, 0u32);
    let mut config = Vec::new();
    match kind {
        TrackKind::Video => {
            let mut c = Cur::new(entry);
            c.skip(24)?;
            width = c.u16()? as u32;
            height = c.u16()? as u32;
            let kids = entry.get(78..).unwrap_or(&[]);
            for (t, body) in boxes(kids) {
                if matches!(&t, b"avcC" | b"hvcC" | b"av1C" | b"vpcC") {
                    config = body.to_vec();
                }
            }
        }
        TrackKind::Audio => {
            let mut c = Cur::new(entry);
            c.skip(8)?;
            let version = c.u16()?;
            c.skip(6)?;
            channels = c.u16()?;
            c.skip(6)?;
            sample_rate = c.u32()? >> 16;
            let extra = match version {
                1 => 16,
                2 => 36,
                _ => 0,
            };
            let kids = entry.get(28 + extra..).unwrap_or(&[]);
            for (t, body) in boxes(kids) {
                if matches!(&t, b"dOps" | b"esds") {
                    config = body.to_vec();
                }
            }
        }
        TrackKind::Other => {}
    }

    // Sample tables.
    let mut stts: Vec<(u32, u32)> = Vec::new();
    if let Some(b) = find(stbl, b"stts") {
        let mut c = Cur::new(b);
        c.skip(4)?;
        for _ in 0..c.u32()? {
            stts.push((c.u32()?, c.u32()?));
        }
    }
    let mut ctts: Vec<(u32, i64)> = Vec::new();
    if let Some(b) = find(stbl, b"ctts") {
        let mut c = Cur::new(b);
        let ver = c.u8()?;
        c.skip(3)?;
        for _ in 0..c.u32()? {
            let n = c.u32()?;
            let off = c.u32()?;
            ctts.push((n, if ver == 0 { off as i64 } else { off as i32 as i64 }));
        }
    }
    let mut stss: Option<Vec<u32>> = None;
    if let Some(b) = find(stbl, b"stss") {
        let mut c = Cur::new(b);
        c.skip(4)?;
        let n = c.u32()?;
        let mut v = Vec::with_capacity(n as usize);
        for _ in 0..n {
            v.push(c.u32()?);
        }
        stss = Some(v);
    }
    let mut stsc: Vec<(u32, u32)> = Vec::new();
    if let Some(b) = find(stbl, b"stsc") {
        let mut c = Cur::new(b);
        c.skip(4)?;
        for _ in 0..c.u32()? {
            let first = c.u32()?;
            let per = c.u32()?;
            c.skip(4)?;
            stsc.push((first, per));
        }
    }
    let mut sizes: Vec<u32> = Vec::new();
    if let Some(b) = find(stbl, b"stsz") {
        let mut c = Cur::new(b);
        c.skip(4)?;
        let fixed = c.u32()?;
        let n = c.u32()?;
        if fixed != 0 {
            sizes = vec![fixed; n as usize];
        } else {
            for _ in 0..n {
                sizes.push(c.u32()?);
            }
        }
    }
    let mut chunk_offsets: Vec<u64> = Vec::new();
    if let Some(b) = find(stbl, b"stco") {
        let mut c = Cur::new(b);
        c.skip(4)?;
        for _ in 0..c.u32()? {
            chunk_offsets.push(c.u32()? as u64);
        }
    } else if let Some(b) = find(stbl, b"co64") {
        let mut c = Cur::new(b);
        c.skip(4)?;
        for _ in 0..c.u32()? {
            chunk_offsets.push(c.u64()?);
        }
    }

    // Expand the tables into one entry per sample.
    let n = sizes.len();
    let mut offsets = Vec::with_capacity(n);
    let mut si = 0usize;
    for (ci, &base) in chunk_offsets.iter().enumerate() {
        let chunk_no = ci as u32 + 1;
        let per = stsc.iter().rev().find(|e| e.0 <= chunk_no).map(|e| e.1).unwrap_or(1);
        let mut off = base;
        for _ in 0..per {
            if si >= n {
                break;
            }
            offsets.push(off);
            off += sizes[si] as u64;
            si += 1;
        }
    }
    let mut dts = Vec::with_capacity(n);
    let mut t = 0i64;
    'outer: for &(count, delta) in &stts {
        for _ in 0..count {
            if dts.len() >= n {
                break 'outer;
            }
            dts.push(t);
            t += delta as i64;
        }
    }
    let mut cts_off = vec![0i64; n];
    let mut k = 0;
    for &(count, off) in &ctts {
        for _ in 0..count {
            if k < n {
                cts_off[k] = off;
                k += 1;
            }
        }
    }
    let us = |ticks: i64| ticks * 1_000_000 / timescale as i64;
    let count = offsets.len().min(dts.len()).min(n);
    let mut samples = Vec::with_capacity(count);
    for i in 0..count {
        let key = match &stss {
            Some(v) => v.binary_search(&(i as u32 + 1)).is_ok(),
            None => true,
        };
        samples.push(SampleRef {
            offset: offsets[i],
            size: sizes[i],
            dts_us: us(dts[i]),
            pts_us: us(dts[i] + cts_off[i]),
            key,
        });
    }
    Ok(Some(Track {
        id,
        kind,
        timescale,
        duration_us: us(dur as i64),
        codec,
        config,
        width,
        height,
        channels,
        sample_rate,
        samples,
    }))
}

/// avcC -> (length size, SPS/PPS NAL units).
pub fn parse_avcc(avcc: &[u8]) -> Result<(usize, Vec<Vec<u8>>)> {
    let mut c = Cur::new(avcc);
    c.skip(4)?;
    let length_size = (c.u8()? & 3) as usize + 1;
    let mut params = Vec::new();
    let n_sps = c.u8()? & 0x1F;
    for _ in 0..n_sps {
        let l = c.u16()? as usize;
        params.push(c.rest().get(..l).ok_or_else(eof)?.to_vec());
        c.skip(l)?;
    }
    let n_pps = c.u8()?;
    for _ in 0..n_pps {
        let l = c.u16()? as usize;
        params.push(c.rest().get(..l).ok_or_else(eof)?.to_vec());
        c.skip(l)?;
    }
    Ok((length_size, params))
}
