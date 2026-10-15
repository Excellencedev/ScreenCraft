//! VP9 through libvpx (enabled with the `vp9` feature). No alt-ref lag: one frame in, one out.

use crate::codec::VideoEncoder;
use crate::types::*;
use crate::{Error, Result};
use std::mem::MaybeUninit;
use vpx_sys::*;

pub struct Vp9Encoder {
    ctx: Box<vpx_codec_ctx_t>,
    width: u32,
    height: u32,
    frame_dur: u64,
    extradata: Vec<u8>,
}

// libvpx contexts have no thread affinity; we only ever use one from one thread at a time.
unsafe impl Send for Vp9Encoder {}

fn check(what: &str, r: vpx_codec_err_t) -> Result<()> {
    if r == vpx_codec_err_t::VPX_CODEC_OK {
        Ok(())
    } else {
        Err(Error::Codec(format!("libvpx {what}: error {}", r as i32)))
    }
}

/// vpcC record: profile 0, 8-bit 4:2:0, BT.709, limited range.
fn vpcc(width: u32, height: u32) -> Vec<u8> {
    let level = if width * height <= 1920 * 1088 {
        41
    } else if width * height <= 4096 * 2176 {
        51
    } else {
        62
    };
    vec![1, 0, 0, 0, 0, level, (8 << 4) | (1 << 1), 1, 1, 1, 0, 0]
}

impl Vp9Encoder {
    pub fn new(cfg: &VideoConfig) -> Result<Self> {
        unsafe {
            let iface = vpx_codec_vp9_cx();
            let mut c = MaybeUninit::<vpx_codec_enc_cfg_t>::zeroed();
            check("config", vpx_codec_enc_config_default(iface, c.as_mut_ptr(), 0))?;
            let mut c = c.assume_init();
            c.g_w = cfg.width;
            c.g_h = cfg.height;
            // Timebase of one microsecond: packets carry exactly the pts we were given.
            c.g_timebase.num = 1;
            c.g_timebase.den = 1_000_000;
            c.g_threads = if cfg.threads == 0 { crate::color::default_threads() as u32 } else { cfg.threads as u32 };
            c.g_lag_in_frames = 0;
            c.g_error_resilient = 0;
            c.rc_end_usage = vpx_rc_mode::VPX_VBR;
            c.rc_target_bitrate = cfg.bitrate_kbps;
            c.kf_mode = vpx_kf_mode::VPX_KF_AUTO;
            c.kf_min_dist = 0;
            c.kf_max_dist = cfg.effective_keyint();

            let mut ctx: Box<vpx_codec_ctx_t> = Box::new(std::mem::zeroed());
            check("init", vpx_codec_enc_init_ver(&mut *ctx, iface, &c, 0, VPX_ENCODER_ABI_VERSION as i32))?;
            let cpu_used: i32 = match cfg.effort {
                Effort::Fast => 8,
                Effort::Balanced => 5,
                Effort::Best => 2,
            };
            let _ = vpx_codec_control_(&mut *ctx, vp8e_enc_control_id::VP8E_SET_CPUUSED as i32, cpu_used);
            let _ = vpx_codec_control_(&mut *ctx, vp8e_enc_control_id::VP9E_SET_ROW_MT as i32, 1i32);
            let _ = vpx_codec_control_(&mut *ctx, vp8e_enc_control_id::VP9E_SET_TILE_COLUMNS as i32, 2i32);
            let _ = vpx_codec_control_(&mut *ctx, vp8e_enc_control_id::VP9E_SET_AQ_MODE as i32, 3i32);
            let _ = vpx_codec_control_(&mut *ctx, vp8e_enc_control_id::VP9E_SET_TUNE_CONTENT as i32, 1i32);
            Ok(Self {
                ctx,
                width: cfg.width,
                height: cfg.height,
                frame_dur: cfg.frame_duration_us().max(1) as u64,
                extradata: vpcc(cfg.width, cfg.height),
            })
        }
    }

    fn collect(&mut self, out: &mut Vec<Packet>, pts_map: impl Fn(i64) -> i64) {
        unsafe {
            let mut iter: vpx_codec_iter_t = std::ptr::null();
            loop {
                let p = vpx_codec_get_cx_data(&mut *self.ctx, &mut iter);
                if p.is_null() {
                    break;
                }
                let p = &*p;
                if p.kind == vpx_codec_cx_pkt_kind::VPX_CODEC_CX_FRAME_PKT {
                    let f = p.data.frame;
                    let data = std::slice::from_raw_parts(f.buf as *const u8, f.sz).to_vec();
                    out.push(Packet { data, pts_us: pts_map(f.pts), key: f.flags & VPX_FRAME_IS_KEY != 0 });
                }
            }
        }
    }
}

impl VideoEncoder for Vp9Encoder {
    fn name(&self) -> &'static str {
        "libvpx"
    }

    fn extradata(&self) -> Vec<u8> {
        self.extradata.clone()
    }

    fn encode(&mut self, f: &I420, pts_us: i64) -> Result<Vec<Packet>> {
        if f.width != self.width || f.height != self.height {
            return Err(Error::Config("frame size changed mid-stream".into()));
        }
        unsafe {
            let mut img = MaybeUninit::<vpx_image_t>::zeroed();
            vpx_img_wrap(img.as_mut_ptr(), vpx_img_fmt::VPX_IMG_FMT_I420, f.width, f.height, 1, std::ptr::null_mut());
            let mut img = img.assume_init();
            // Wrapped planes point at our buffers; libvpx never writes to them.
            img.planes[0] = f.y.as_ptr() as *mut u8;
            img.planes[1] = f.u.as_ptr() as *mut u8;
            img.planes[2] = f.v.as_ptr() as *mut u8;
            img.stride[0] = f.width as i32;
            img.stride[1] = (f.width / 2) as i32;
            img.stride[2] = (f.width / 2) as i32;
            check(
                "encode",
                vpx_codec_encode(&mut *self.ctx, &img, pts_us, self.frame_dur, 0, VPX_DL_GOOD_QUALITY as u64),
            )?;
        }
        let mut out = Vec::new();
        self.collect(&mut out, |p| p);
        Ok(out)
    }

    fn flush(&mut self) -> Result<Vec<Packet>> {
        unsafe {
            check("flush", vpx_codec_encode(&mut *self.ctx, std::ptr::null(), -1, 1, 0, VPX_DL_GOOD_QUALITY as u64))?;
        }
        let mut out = Vec::new();
        self.collect(&mut out, |p| p);
        Ok(out)
    }
}

impl Drop for Vp9Encoder {
    fn drop(&mut self) {
        unsafe {
            vpx_codec_destroy(&mut *self.ctx);
        }
    }
}
