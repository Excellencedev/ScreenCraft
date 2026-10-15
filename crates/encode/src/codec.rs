//! The encoder interface and the factory that picks hardware or software.

use crate::types::*;
use crate::{Error, Result};

pub trait VideoEncoder: Send {
    /// "OpenH264", "rav1e", "libvpx", "VideoToolbox" ...
    fn name(&self) -> &'static str;
    /// avcC / av1C / vpcC record. Only meaningful after the first packet came out.
    fn extradata(&self) -> Vec<u8>;
    /// Submit a frame. May return zero or more finished packets.
    fn encode(&mut self, frame: &I420, pts_us: i64) -> Result<Vec<Packet>>;
    /// Drain everything still buffered. The encoder is unusable afterwards.
    fn flush(&mut self) -> Result<Vec<Packet>>;
}

/// Open an encoder for `cfg`. `Hardware` fails when the platform has none for the codec.
pub fn create_video_encoder(cfg: &VideoConfig) -> Result<Box<dyn VideoEncoder>> {
    cfg.validate()?;
    let want_hw = matches!(cfg.hardware, HwPreference::Auto | HwPreference::Hardware);
    if want_hw && cfg.codec == VideoCodec::H264 {
        #[cfg(target_os = "macos")]
        match crate::hw_videotoolbox::VtH264Encoder::new(cfg) {
            Ok(e) => return Ok(Box::new(e)),
            Err(e) if cfg.hardware == HwPreference::Hardware => return Err(e),
            Err(_) => {}
        }
    }
    if cfg.hardware == HwPreference::Hardware {
        return Err(Error::Config(format!("no hardware encoder for {} on this platform", cfg.codec.name())));
    }
    match cfg.codec {
        VideoCodec::H264 => Ok(Box::new(crate::h264::H264Encoder::new(cfg)?)),
        VideoCodec::Av1 => Ok(Box::new(crate::av1::Av1Encoder::new(cfg)?)),
        VideoCodec::Vp9 => {
            #[cfg(feature = "vp9")]
            {
                Ok(Box::new(crate::vp9::Vp9Encoder::new(cfg)?))
            }
            #[cfg(not(feature = "vp9"))]
            {
                Err(Error::Config("this build was made without VP9 (libvpx) support".into()))
            }
        }
    }
}

/// Codecs this build can encode (software or hardware).
pub fn available_video_codecs() -> Vec<VideoCodec> {
    let mut v = vec![VideoCodec::H264, VideoCodec::Av1];
    if cfg!(feature = "vp9") {
        v.push(VideoCodec::Vp9);
    }
    v
}

/// Does `create_video_encoder` hand out a hardware encoder for this codec here?
pub fn has_hardware_encoder(codec: VideoCodec) -> bool {
    cfg!(target_os = "macos") && codec == VideoCodec::H264
}
