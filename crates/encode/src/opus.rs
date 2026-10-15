//! Opus audio. 48 kHz, 20 ms frames, interleaved f32 in.

use crate::types::AudioConfig;
use crate::{Error, Result};
use audiopus::coder::{Decoder, Encoder};
use audiopus::{Application, Bitrate, Channels, SampleRate};

pub const FRAME_SAMPLES: usize = 960; // 20 ms at 48 kHz

fn channels(n: u8) -> Result<Channels> {
    match n {
        1 => Ok(Channels::Mono),
        2 => Ok(Channels::Stereo),
        _ => Err(Error::Config("Opus supports mono or stereo here".into())),
    }
}

pub struct OpusEncoder {
    enc: Encoder,
    channels: usize,
    pre_skip: u16,
}

impl OpusEncoder {
    pub fn new(cfg: &AudioConfig) -> Result<Self> {
        if cfg.sample_rate != 48_000 {
            return Err(Error::Config("audio must be 48 kHz before encoding".into()));
        }
        let mut enc = Encoder::new(SampleRate::Hz48000, channels(cfg.channels)?, Application::Audio)
            .map_err(|e| Error::Codec(format!("Opus init: {e}")))?;
        enc.set_bitrate(Bitrate::BitsPerSecond((cfg.bitrate_kbps * 1000) as i32))
            .map_err(|e| Error::Codec(format!("Opus bitrate: {e}")))?;
        let pre_skip = enc.lookahead().unwrap_or(312).max(0) as u16;
        Ok(Self { enc, channels: cfg.channels as usize, pre_skip })
    }

    pub fn pre_skip(&self) -> u16 {
        self.pre_skip
    }

    /// Encode exactly `FRAME_SAMPLES` frames of interleaved samples.
    pub fn encode(&mut self, pcm: &[f32]) -> Result<Vec<u8>> {
        debug_assert_eq!(pcm.len(), FRAME_SAMPLES * self.channels);
        let mut buf = vec![0u8; 4000];
        let n = self.enc.encode_float(pcm, &mut buf).map_err(|e| Error::Codec(format!("Opus encode: {e}")))?;
        buf.truncate(n);
        Ok(buf)
    }
}

pub struct OpusDecoder {
    dec: Decoder,
    channels: usize,
}

impl OpusDecoder {
    pub fn new(ch: u8) -> Result<Self> {
        let dec = Decoder::new(SampleRate::Hz48000, channels(ch)?).map_err(|e| Error::Codec(format!("Opus decoder: {e}")))?;
        Ok(Self { dec, channels: ch as usize })
    }

    /// Decode one packet into interleaved f32.
    pub fn decode(&mut self, packet: &[u8]) -> Result<Vec<f32>> {
        let mut out = vec![0f32; 5760 * self.channels];
        let n = self
            .dec
            .decode_float(
                Some(audiopus::packet::Packet::try_from(packet).map_err(|e| Error::Codec(format!("Opus packet: {e}")))?),
                audiopus::MutSignals::try_from(&mut out[..]).map_err(|e| Error::Codec(format!("Opus buffer: {e}")))?,
                false,
            )
            .map_err(|e| Error::Codec(format!("Opus decode: {e}")))?;
        out.truncate(n * self.channels);
        Ok(out)
    }
}
