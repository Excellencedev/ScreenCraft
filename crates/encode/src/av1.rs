//! AV1 through rav1e (pure Rust). Low-latency mode: no frame reordering, so pts == dts.

use crate::codec::VideoEncoder;
use crate::types::*;
use crate::{Error, Result};
use rav1e::prelude::{
    ChromaSampling, ColorDescription, ColorPrimaries, Config, Context, EncoderConfig, EncoderStatus, FrameType,
    MatrixCoefficients, PixelRange, Rational, TransferCharacteristics,
};
use std::collections::VecDeque;

pub struct Av1Encoder {
    ctx: Context<u8>,
    pts: VecDeque<i64>,
    sent: u64,
    done: u64,
    extradata: Vec<u8>,
    width: u32,
    height: u32,
}

/// Remove temporal delimiter OBUs; MP4 and WebM carry them implicitly.
pub fn strip_temporal_delimiters(data: &[u8]) -> Vec<u8> {
    let mut out = Vec::with_capacity(data.len());
    let mut i = 0;
    while i < data.len() {
        let start = i;
        let hdr = data[i];
        let ty = (hdr >> 3) & 0xF;
        let ext = (hdr >> 2) & 1 == 1;
        let has_size = (hdr >> 1) & 1 == 1;
        i += 1 + ext as usize;
        if !has_size {
            // The last OBU may run to the end without a size field.
            if ty != 2 {
                out.extend_from_slice(&data[start..]);
            }
            return out;
        }
        let mut size = 0usize;
        let mut shift = 0;
        loop {
            if i >= data.len() {
                return out;
            }
            let b = data[i];
            i += 1;
            size |= ((b & 0x7F) as usize) << shift;
            shift += 7;
            if b & 0x80 == 0 {
                break;
            }
        }
        i += size;
        if i > data.len() {
            return out;
        }
        if ty != 2 {
            out.extend_from_slice(&data[start..i]);
        }
    }
    out
}

impl Av1Encoder {
    pub fn new(cfg: &VideoConfig) -> Result<Self> {
        let preset = match cfg.effort {
            Effort::Fast => 10,
            Effort::Balanced => 8,
            Effort::Best => 6,
        };
        let mut enc = EncoderConfig::with_speed_preset(preset);
        enc.width = cfg.width as usize;
        enc.height = cfg.height as usize;
        enc.time_base = Rational::new(cfg.fps_den as u64, cfg.fps_num as u64);
        enc.bit_depth = 8;
        enc.chroma_sampling = ChromaSampling::Cs420;
        enc.pixel_range = PixelRange::Limited;
        enc.color_description = Some(ColorDescription {
            color_primaries: ColorPrimaries::BT709,
            transfer_characteristics: TransferCharacteristics::BT709,
            matrix_coefficients: MatrixCoefficients::BT709,
        });
        enc.bitrate = (cfg.bitrate_kbps as i32).saturating_mul(1000);
        enc.low_latency = true;
        enc.max_key_frame_interval = cfg.effective_keyint() as u64;
        enc.min_key_frame_interval = 0;
        enc.speed_settings.rdo_lookahead_frames = 1;
        let threads = if cfg.threads == 0 { crate::color::default_threads() } else { cfg.threads };
        let config = Config::new().with_encoder_config(enc).with_threads(threads);
        let ctx: Context<u8> = config.new_context().map_err(|e| Error::Codec(format!("rav1e init: {e}")))?;
        let extradata = ctx.container_sequence_header();
        Ok(Self { ctx, pts: VecDeque::new(), sent: 0, done: 0, extradata, width: cfg.width, height: cfg.height })
    }

    fn drain(&mut self) -> Result<Vec<Packet>> {
        let mut out = Vec::new();
        loop {
            match self.ctx.receive_packet() {
                Ok(p) => {
                    // Packets come back in input order without reordering.
                    let pts = self.pts.pop_front().unwrap_or(0);
                    self.done += 1;
                    out.push(Packet {
                        data: strip_temporal_delimiters(&p.data),
                        pts_us: pts,
                        key: p.frame_type == FrameType::KEY,
                    });
                }
                Err(EncoderStatus::Encoded) => continue,
                Err(EncoderStatus::NeedMoreData) | Err(EncoderStatus::LimitReached) => break,
                Err(e) => return Err(Error::Codec(format!("rav1e: {e}"))),
            }
        }
        Ok(out)
    }
}

impl VideoEncoder for Av1Encoder {
    fn name(&self) -> &'static str {
        "rav1e"
    }

    fn extradata(&self) -> Vec<u8> {
        self.extradata.clone()
    }

    fn encode(&mut self, f: &I420, pts_us: i64) -> Result<Vec<Packet>> {
        if f.width != self.width || f.height != self.height {
            return Err(Error::Config("frame size changed mid-stream".into()));
        }
        let w = f.width as usize;
        let mut frame = self.ctx.new_frame();
        frame.planes[0].copy_from_raw_u8(&f.y, w, 1);
        frame.planes[1].copy_from_raw_u8(&f.u, w / 2, 1);
        frame.planes[2].copy_from_raw_u8(&f.v, w / 2, 1);
        self.pts.push_back(pts_us);
        self.sent += 1;
        self.ctx.send_frame(frame).map_err(|e| Error::Codec(format!("rav1e send: {e}")))?;
        self.drain()
    }

    fn flush(&mut self) -> Result<Vec<Packet>> {
        self.ctx.flush();
        self.drain()
    }
}
