//! H.264 helpers (Annex-B / AVCC) and the decoder front door. ScreenCraft ships no H.264 codec of its
//! own: encoding and decoding go through the operating system's licensed implementation
//! (VideoToolbox on macOS, Media Foundation on Windows).

use crate::types::*;
#[allow(unused_imports)]
use crate::{Error, Result};

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

/// Decoder for our own recordings and imported MP4s, backed by the OS.
pub struct H264Decoder {
    #[cfg(target_os = "macos")]
    inner: crate::vt_decode::VtH264Decoder,
    #[cfg(target_os = "windows")]
    inner: crate::mf::MfH264Decoder,
}

impl H264Decoder {
    /// `params` are the SPS/PPS NAL units from the avcC record, `length_size` its NAL length prefix.
    #[allow(unused_variables)]
    pub fn new(params: &[Vec<u8>], length_size: usize) -> Result<Self> {
        #[cfg(target_os = "macos")]
        return Ok(Self { inner: crate::vt_decode::VtH264Decoder::new(params, length_size)? });
        #[cfg(target_os = "windows")]
        return Ok(Self { inner: crate::mf::MfH264Decoder::new(params, length_size)? });
        #[cfg(not(any(target_os = "macos", target_os = "windows")))]
        return Err(Error::Codec(
            "this platform has no built-in H.264 decoder, and ScreenCraft does not bundle one".into(),
        ));
    }

    /// Feed one length-prefixed access unit (as stored in MP4). Returns the picture when ready.
    #[allow(unused_variables)]
    pub fn decode(&mut self, sample: &[u8]) -> Result<Option<I420>> {
        #[cfg(any(target_os = "macos", target_os = "windows"))]
        return self.inner.decode(sample);
        #[cfg(not(any(target_os = "macos", target_os = "windows")))]
        return Err(Error::Codec("no H.264 decoder".into()));
    }
}
