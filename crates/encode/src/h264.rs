//! H.264 through OpenH264 (software) plus Annex-B / AVCC helpers shared with the hardware path.

use crate::codec::VideoEncoder;
use crate::types::*;
use crate::{Error, Result};
use openh264::encoder::{
    BitRate, Complexity, Encoder, EncoderConfig, FrameRate, IntraFramePeriod, Profile, RateControlMode, UsageType,
    VuiConfig,
};
use openh264::formats::{YUVSlices, YUVSource};
use openh264::OpenH264API;

/// Split an Annex-B stream into NAL units (without start codes).
pub fn split_annexb(data: &[u8]) -> Vec<&[u8]> {
    let mut starts = Vec::new();
    let mut i = 0;
    while i + 3 <= data.len() {
        if data[i] == 0 && data[i + 1] == 0 && data[i + 2] == 1 {
            starts.push(i + 3);
            i += 3;
        } else {
            i += 1;
        }
    }
    let mut out = Vec::new();
    for (k, &s) in starts.iter().enumerate() {
        let mut e = if k + 1 < starts.len() { starts[k + 1] - 3 } else { data.len() };
        while e > s && data[e - 1] == 0 {
            e -= 1; // trailing zero_byte of a 4-byte start code
        }
        if e > s {
            out.push(&data[s..e]);
        }
    }
    out
}

pub fn build_avcc(sps: &[u8], pps: &[u8]) -> Vec<u8> {
    let mut v = vec![1, sps[1], sps[2], sps[3], 0xFF, 0xE1];
    v.extend_from_slice(&(sps.len() as u16).to_be_bytes());
    v.extend_from_slice(sps);
    v.push(1);
    v.extend_from_slice(&(pps.len() as u16).to_be_bytes());
    v.extend_from_slice(pps);
    v
}

/// Annex-B access unit -> length-prefixed samples. Parameter sets are returned separately.
pub fn annexb_to_avcc(data: &[u8]) -> (Vec<u8>, Option<(Vec<u8>, Vec<u8>)>, bool) {
    let mut out = Vec::with_capacity(data.len());
    let (mut sps, mut pps) = (None, None);
    let mut key = false;
    for nal in split_annexb(data) {
        match nal[0] & 0x1F {
            7 => sps = Some(nal.to_vec()),
            8 => pps = Some(nal.to_vec()),
            9 | 6 => {} // access unit delimiter, SEI
            t => {
                if t == 5 {
                    key = true;
                }
                out.extend_from_slice(&(nal.len() as u32).to_be_bytes());
                out.extend_from_slice(nal);
            }
        }
    }
    (out, sps.zip(pps), key)
}

/// Length-prefixed sample -> Annex-B, with the parameter sets in front of key frames.
pub fn avcc_to_annexb(sample: &[u8], length_size: usize, params: Option<&[Vec<u8>]>) -> Vec<u8> {
    let mut out = Vec::with_capacity(sample.len() + 64);
    if let Some(ps) = params {
        for p in ps {
            out.extend_from_slice(&[0, 0, 0, 1]);
            out.extend_from_slice(p);
        }
    }
    let mut i = 0;
    while i + length_size <= sample.len() {
        let mut n = 0usize;
        for k in 0..length_size {
            n = (n << 8) | sample[i + k] as usize;
        }
        i += length_size;
        if i + n > sample.len() {
            break;
        }
        out.extend_from_slice(&[0, 0, 0, 1]);
        out.extend_from_slice(&sample[i..i + n]);
        i += n;
    }
    out
}

pub struct H264Encoder {
    enc: Encoder,
    cfg: VideoConfig,
    avcc: Vec<u8>,
}

impl H264Encoder {
    pub fn new(cfg: &VideoConfig) -> Result<Self> {
        let complexity = match cfg.effort {
            Effort::Fast => Complexity::Low,
            Effort::Balanced => Complexity::Medium,
            Effort::Best => Complexity::High,
        };
        // One internal thread: OpenH264's multi-slice mode changes the bitstream with the thread
        // count, and exports must be byte-identical on every machine.
        let ec = EncoderConfig::new()
            .bitrate(BitRate::from_bps(cfg.bitrate_kbps.saturating_mul(1000)))
            .max_frame_rate(FrameRate::from_hz(cfg.fps() as f32))
            .rate_control_mode(RateControlMode::Bitrate)
            .usage_type(UsageType::ScreenContentRealTime)
            .profile(Profile::High)
            .complexity(complexity)
            .skip_frames(true)
            .adaptive_quantization(false)
            .background_detection(false)
            .num_threads(1)
            .intra_frame_period(IntraFramePeriod::from_num_frames(cfg.effective_keyint()))
            .vui(VuiConfig::bt709());
        let enc = Encoder::with_api_config(OpenH264API::from_source(), ec)
            .map_err(|e| Error::Codec(format!("OpenH264 init: {e}")))?;
        Ok(Self { enc, cfg: cfg.clone(), avcc: Vec::new() })
    }
}

impl VideoEncoder for H264Encoder {
    fn name(&self) -> &'static str {
        "OpenH264"
    }

    fn extradata(&self) -> Vec<u8> {
        self.avcc.clone()
    }

    fn encode(&mut self, f: &I420, pts_us: i64) -> Result<Vec<Packet>> {
        if f.width != self.cfg.width || f.height != self.cfg.height {
            return Err(Error::Config("frame size changed mid-stream".into()));
        }
        let (w, h) = (f.width as usize, f.height as usize);
        let src = YUVSlices::new((&f.y, &f.u, &f.v), (w, h), (w, w / 2, w / 2));
        let bits = self.enc.encode(&src).map_err(|e| Error::Codec(format!("OpenH264 encode: {e}")))?;
        let annexb = bits.to_vec();
        if annexb.is_empty() {
            return Ok(Vec::new());
        }
        let (data, ps, key) = annexb_to_avcc(&annexb);
        if let Some((sps, pps)) = ps {
            if self.avcc.is_empty() {
                self.avcc = build_avcc(&sps, &pps);
            }
        }
        if data.is_empty() {
            return Ok(Vec::new());
        }
        Ok(vec![Packet { data, pts_us, key }])
    }

    fn flush(&mut self) -> Result<Vec<Packet>> {
        Ok(Vec::new()) // no look-ahead, nothing is held back
    }
}

/// Software H.264 decoder for our own recordings and imported baseline/high MP4s.
pub struct H264Decoder {
    dec: openh264::decoder::Decoder,
}

impl H264Decoder {
    pub fn new() -> Result<Self> {
        let dec = openh264::decoder::Decoder::new().map_err(|e| Error::Codec(format!("OpenH264 decoder: {e}")))?;
        Ok(Self { dec })
    }

    /// Feed one Annex-B access unit. Returns the picture when one is ready.
    pub fn decode(&mut self, annexb: &[u8]) -> Result<Option<I420>> {
        let out = self.dec.decode(annexb).map_err(|e| Error::Codec(format!("H.264 decode: {e}")))?;
        let Some(yuv) = out else { return Ok(None) };
        let (w, h) = yuv.dimensions();
        let (sy, su, sv) = yuv.strides();
        let mut f = I420::new(w as u32, h as u32);
        for r in 0..h {
            f.y[r * w..(r + 1) * w].copy_from_slice(&yuv.y()[r * sy..r * sy + w]);
        }
        for r in 0..h / 2 {
            f.u[r * (w / 2)..(r + 1) * (w / 2)].copy_from_slice(&yuv.u()[r * su..r * su + w / 2]);
            f.v[r * (w / 2)..(r + 1) * (w / 2)].copy_from_slice(&yuv.v()[r * sv..r * sv + w / 2]);
        }
        Ok(Some(f))
    }
}
