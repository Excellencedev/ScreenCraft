//! Plain data shared by the encoders, muxers and the export pipeline.

/// Planar YUV 4:2:0, BT.709, limited range, tightly packed (stride == width).
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct I420 {
    pub width: u32,
    pub height: u32,
    pub y: Vec<u8>,
    pub u: Vec<u8>,
    pub v: Vec<u8>,
}

impl I420 {
    /// Both dimensions must be even.
    pub fn new(width: u32, height: u32) -> Self {
        let (w, h) = (width as usize, height as usize);
        Self { width, height, y: vec![16; w * h], u: vec![128; w * h / 4], v: vec![128; w * h / 4] }
    }
}

/// One encoded access unit. Timestamps are microseconds from the start of the file.
/// All encoders here run without frame reordering, so `dts == pts`.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Packet {
    pub data: Vec<u8>,
    pub pts_us: i64,
    pub key: bool,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum VideoCodec {
    H264,
    Av1,
    Vp9,
}

impl VideoCodec {
    pub fn name(self) -> &'static str {
        match self {
            VideoCodec::H264 => "H.264",
            VideoCodec::Av1 => "AV1",
            VideoCodec::Vp9 => "VP9",
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum Container {
    Mp4,
    WebM,
}

impl Container {
    pub fn extension(self) -> &'static str {
        match self {
            Container::Mp4 => "mp4",
            Container::WebM => "webm",
        }
    }

    pub fn supports(self, codec: VideoCodec) -> bool {
        match self {
            Container::Mp4 => true,
            Container::WebM => matches!(codec, VideoCodec::Av1 | VideoCodec::Vp9),
        }
    }
}

/// How much work the encoder may spend. Maps to each codec's own speed knob.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Effort {
    Fast,
    Balanced,
    Best,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum HwPreference {
    /// Use the platform encoder when there is one for the codec, otherwise software.
    Auto,
    Software,
    Hardware,
}

#[derive(Clone, Debug)]
pub struct VideoConfig {
    pub codec: VideoCodec,
    pub width: u32,
    pub height: u32,
    pub fps_num: u32,
    pub fps_den: u32,
    pub bitrate_kbps: u32,
    pub effort: Effort,
    pub hardware: HwPreference,
    /// Worker threads the codec may use; 0 = all cores. Output does not depend on this.
    pub threads: usize,
    /// Distance between key frames; 0 = pick from the frame rate (about 2 seconds).
    pub keyint: u32,
}

impl VideoConfig {
    pub fn new(codec: VideoCodec, width: u32, height: u32, fps: u32) -> Self {
        Self {
            codec,
            width,
            height,
            fps_num: fps.max(1),
            fps_den: 1,
            bitrate_kbps: default_bitrate_kbps(width, height, fps),
            effort: Effort::Balanced,
            hardware: HwPreference::Auto,
            threads: 0,
            keyint: 0,
        }
    }

    pub fn fps(&self) -> f64 {
        self.fps_num as f64 / self.fps_den.max(1) as f64
    }

    pub fn frame_duration_us(&self) -> i64 {
        (1_000_000u64 * self.fps_den as u64 / self.fps_num.max(1) as u64) as i64
    }

    pub fn effective_keyint(&self) -> u32 {
        if self.keyint > 0 {
            self.keyint
        } else {
            (self.fps().round() as u32 * 2).max(1)
        }
    }

    pub fn validate(&self) -> crate::Result<()> {
        if self.width < 16 || self.height < 16 || self.width % 2 != 0 || self.height % 2 != 0 {
            return Err(crate::Error::Config(format!(
                "frame size {}x{} must be even and at least 16x16",
                self.width, self.height
            )));
        }
        if self.fps_num == 0 || self.fps_den == 0 {
            return Err(crate::Error::Config("frame rate must be positive".into()));
        }
        Ok(())
    }
}

/// A rough bitrate for screen content, scaled by pixels and frame rate.
pub fn default_bitrate_kbps(width: u32, height: u32, fps: u32) -> u32 {
    let px = width as f64 * height as f64;
    let base = px / (1920.0 * 1080.0) * 8000.0;
    let rate = (fps.max(1) as f64 / 30.0).sqrt();
    ((base * rate).round() as u32).clamp(1000, 80_000)
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct AudioConfig {
    pub sample_rate: u32,
    pub channels: u8,
    pub bitrate_kbps: u32,
}

impl Default for AudioConfig {
    fn default() -> Self {
        Self { sample_rate: 48_000, channels: 2, bitrate_kbps: 160 }
    }
}

/// A file write the driver has to perform. The muxers never touch a file themselves.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct WriteAt {
    pub offset: u64,
    pub data: Vec<u8>,
}

/// Everything a muxer needs to describe the video track.
#[derive(Clone, Debug)]
pub struct VideoTrackInfo {
    pub codec: VideoCodec,
    pub width: u32,
    pub height: u32,
    pub fps_num: u32,
    pub fps_den: u32,
    /// avcC / av1C / vpcC payload (see each encoder's `extradata`).
    pub extradata: Vec<u8>,
}

#[derive(Clone, Debug)]
pub struct AudioTrackInfo {
    pub sample_rate: u32,
    pub channels: u8,
    /// Samples that the decoder must drop from the start (Opus pre-skip).
    pub pre_skip: u16,
}
