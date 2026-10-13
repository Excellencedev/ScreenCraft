//! MP4 writer for screen recordings: captured BGRA textures are cropped, scaled and
//! converted to NV12 on the GPU (D3D11 video processor) and handed to Media
//! Foundation's sink writer, which encodes H.264 (hardware when available) and,
//! optionally, AAC audio from 16-bit stereo PCM.

use ::windows::core::{Interface, HSTRING};
use ::windows::Win32::Foundation::RECT;
use ::windows::Win32::Graphics::Direct3D11::*;
use ::windows::Win32::Graphics::Dxgi::Common::*;
use ::windows::Win32::Media::MediaFoundation::*;
use std::mem::ManuallyDrop;
use std::path::Path;

pub struct Writer {
    device: ID3D11Device,
    vdev: ID3D11VideoDevice,
    vctx: ID3D11VideoContext,
    /// Video processor for the current input size.
    vp: Option<(u32, u32, ID3D11VideoProcessorEnumerator, ID3D11VideoProcessor)>,
    writer: IMFSinkWriter,
    _manager: IMFDXGIDeviceManager,
    video: u32,
    audio: Option<u32>,
    pub w: u32,
    pub h: u32,
    frame_hns: i64,
    /// Last encoded frame, re-sent when the screen is idle and at the end.
    last: Option<(ID3D11Texture2D, i64)>,
    pub audio_rate: u32,
}

// The D3D objects belong to a multithread-protected device and the sink writer is
// free-threaded; the writer is only ever used behind a mutex.
unsafe impl Send for Writer {}

fn e(what: &'static str) -> impl Fn(::windows::core::Error) -> String {
    move |err| format!("{what}: {err}")
}

fn size64(a: u32, b: u32) -> u64 {
    (a as u64) << 32 | b as u64
}

impl Writer {
    /// `audio_rate` adds an AAC track fed by `audio()` (44100 or 48000 Hz).
    pub fn new(
        device: &ID3D11Device,
        path: &Path,
        w: u32,
        h: u32,
        fps: u32,
        audio_rate: Option<u32>,
    ) -> Result<Writer, String> {
        crate::mf::init_thread();
        unsafe {
            let mut token = 0u32;
            let mut manager = None;
            MFCreateDXGIDeviceManager(&mut token, &mut manager).map_err(e("device manager"))?;
            let manager = manager.ok_or("device manager")?;
            manager.ResetDevice(device, token).map_err(e("device manager"))?;

            let mut attrs = None;
            MFCreateAttributes(&mut attrs, 4).map_err(e("attributes"))?;
            let attrs = attrs.ok_or("attributes")?;
            attrs.SetUnknown(&MF_SINK_WRITER_D3D_MANAGER, &manager).map_err(e("attributes"))?;
            attrs.SetUINT32(&MF_READWRITE_ENABLE_HARDWARE_TRANSFORMS, 1).map_err(e("attributes"))?;
            attrs.SetGUID(&MF_TRANSCODE_CONTAINERTYPE, &MFTranscodeContainerType_MPEG4).map_err(e("attributes"))?;
            let _ = std::fs::remove_file(path);
            let writer = MFCreateSinkWriterFromURL(&HSTRING::from(path.as_os_str()), None, &attrs).map_err(e("create file"))?;

            // video: NV12 in, H.264 out
            let bitrate = ((w as f64 * h as f64 * fps as f64 * 0.08) as u32).clamp(4_000_000, 60_000_000);
            let out = MFCreateMediaType().map_err(e("video type"))?;
            out.SetGUID(&MF_MT_MAJOR_TYPE, &MFMediaType_Video).map_err(e("video type"))?;
            out.SetGUID(&MF_MT_SUBTYPE, &MFVideoFormat_H264).map_err(e("video type"))?;
            out.SetUINT32(&MF_MT_AVG_BITRATE, bitrate).map_err(e("video type"))?;
            out.SetUINT32(&MF_MT_INTERLACE_MODE, MFVideoInterlace_Progressive.0 as u32).map_err(e("video type"))?;
            out.SetUINT32(&MF_MT_MPEG2_PROFILE, eAVEncH264VProfile_High.0 as u32).map_err(e("video type"))?;
            out.SetUINT64(&MF_MT_FRAME_SIZE, size64(w, h)).map_err(e("video type"))?;
            out.SetUINT64(&MF_MT_FRAME_RATE, size64(fps, 1)).map_err(e("video type"))?;
            out.SetUINT64(&MF_MT_PIXEL_ASPECT_RATIO, size64(1, 1)).map_err(e("video type"))?;
            let video = writer.AddStream(&out).map_err(e("video stream"))?;
            let inp = MFCreateMediaType().map_err(e("video type"))?;
            inp.SetGUID(&MF_MT_MAJOR_TYPE, &MFMediaType_Video).map_err(e("video type"))?;
            inp.SetGUID(&MF_MT_SUBTYPE, &MFVideoFormat_NV12).map_err(e("video type"))?;
            inp.SetUINT32(&MF_MT_INTERLACE_MODE, MFVideoInterlace_Progressive.0 as u32).map_err(e("video type"))?;
            inp.SetUINT64(&MF_MT_FRAME_SIZE, size64(w, h)).map_err(e("video type"))?;
            inp.SetUINT64(&MF_MT_FRAME_RATE, size64(fps, 1)).map_err(e("video type"))?;
            inp.SetUINT64(&MF_MT_PIXEL_ASPECT_RATIO, size64(1, 1)).map_err(e("video type"))?;
            inp.SetUINT32(&MF_MT_YUV_MATRIX, MFVideoTransferMatrix_BT709.0 as u32).map_err(e("video type"))?;
            inp.SetUINT32(&MF_MT_VIDEO_NOMINAL_RANGE, MFNominalRange_16_235.0 as u32).map_err(e("video type"))?;
            writer.SetInputMediaType(video, &inp, None).map_err(e("video encoder"))?;

            // audio: 16-bit stereo PCM in, AAC out
            let audio = match audio_rate {
                Some(rate) => {
                    let out = MFCreateMediaType().map_err(e("audio type"))?;
                    out.SetGUID(&MF_MT_MAJOR_TYPE, &MFMediaType_Audio).map_err(e("audio type"))?;
                    out.SetGUID(&MF_MT_SUBTYPE, &MFAudioFormat_AAC).map_err(e("audio type"))?;
                    out.SetUINT32(&MF_MT_AUDIO_SAMPLES_PER_SECOND, rate).map_err(e("audio type"))?;
                    out.SetUINT32(&MF_MT_AUDIO_NUM_CHANNELS, 2).map_err(e("audio type"))?;
                    out.SetUINT32(&MF_MT_AUDIO_BITS_PER_SAMPLE, 16).map_err(e("audio type"))?;
                    out.SetUINT32(&MF_MT_AUDIO_AVG_BYTES_PER_SECOND, 24_000).map_err(e("audio type"))?;
                    let idx = writer.AddStream(&out).map_err(e("audio stream"))?;
                    let inp = MFCreateMediaType().map_err(e("audio type"))?;
                    inp.SetGUID(&MF_MT_MAJOR_TYPE, &MFMediaType_Audio).map_err(e("audio type"))?;
                    inp.SetGUID(&MF_MT_SUBTYPE, &MFAudioFormat_PCM).map_err(e("audio type"))?;
                    inp.SetUINT32(&MF_MT_AUDIO_SAMPLES_PER_SECOND, rate).map_err(e("audio type"))?;
                    inp.SetUINT32(&MF_MT_AUDIO_NUM_CHANNELS, 2).map_err(e("audio type"))?;
                    inp.SetUINT32(&MF_MT_AUDIO_BITS_PER_SAMPLE, 16).map_err(e("audio type"))?;
                    inp.SetUINT32(&MF_MT_AUDIO_BLOCK_ALIGNMENT, 4).map_err(e("audio type"))?;
                    inp.SetUINT32(&MF_MT_AUDIO_AVG_BYTES_PER_SECOND, rate * 4).map_err(e("audio type"))?;
                    writer.SetInputMediaType(idx, &inp, None).map_err(e("audio encoder"))?;
                    Some(idx)
                }
                None => None,
            };
            writer.BeginWriting().map_err(e("start writing"))?;

            let vdev: ID3D11VideoDevice = device.cast().map_err(e("video device"))?;
            let vctx: ID3D11VideoContext =
                device.GetImmediateContext().map_err(e("device context"))?.cast().map_err(e("video context"))?;
            Ok(Writer {
                device: device.clone(),
                vdev,
                vctx,
                vp: None,
                writer,
                _manager: manager,
                video,
                audio,
                w,
                h,
                frame_hns: 10_000_000 / fps.max(1) as i64,
                last: None,
                audio_rate: audio_rate.unwrap_or(0),
            })
        }
    }

    fn processor(&mut self, iw: u32, ih: u32) -> Result<(ID3D11VideoProcessorEnumerator, ID3D11VideoProcessor), String> {
        if let Some((w, h, en, vp)) = &self.vp {
            if *w == iw && *h == ih {
                return Ok((en.clone(), vp.clone()));
            }
        }
        unsafe {
            let rate = DXGI_RATIONAL { Numerator: 60, Denominator: 1 };
            let desc = D3D11_VIDEO_PROCESSOR_CONTENT_DESC {
                InputFrameFormat: D3D11_VIDEO_FRAME_FORMAT_PROGRESSIVE,
                InputFrameRate: rate,
                InputWidth: iw,
                InputHeight: ih,
                OutputFrameRate: rate,
                OutputWidth: self.w,
                OutputHeight: self.h,
                Usage: D3D11_VIDEO_USAGE_OPTIMAL_QUALITY,
            };
            let en = self.vdev.CreateVideoProcessorEnumerator(&desc).map_err(e("video processor"))?;
            let vp = self.vdev.CreateVideoProcessor(&en, 0).map_err(e("video processor"))?;
            // input: full-range RGB; output: BT.709 limited-range YCbCr (tagged on the stream)
            self.vctx.VideoProcessorSetStreamColorSpace(&vp, 0, &D3D11_VIDEO_PROCESSOR_COLOR_SPACE { _bitfield: 0 });
            self.vctx.VideoProcessorSetOutputColorSpace(&vp, &D3D11_VIDEO_PROCESSOR_COLOR_SPACE { _bitfield: (1 << 2) | (1 << 4) });
            self.vctx.VideoProcessorSetStreamAutoProcessingMode(&vp, 0, false);
            self.vp = Some((iw, ih, en.clone(), vp.clone()));
            Ok((en, vp))
        }
    }

    /// Encode `tex` (BGRA). `size` is the part of the texture holding the picture and
    /// `crop` (within it) the region to record; it is scaled to fit the output.
    pub fn frame(&mut self, tex: &ID3D11Texture2D, size: (u32, u32), crop: Option<RECT>, t_hns: i64) -> Result<(), String> {
        let mut desc = D3D11_TEXTURE2D_DESC::default();
        unsafe { tex.GetDesc(&mut desc) };
        let (en, vp) = self.processor(desc.Width, desc.Height)?;
        let (cw, ch) = (size.0.min(desc.Width) as i32, size.1.min(desc.Height) as i32);
        let src = match crop {
            Some(r) => RECT { left: r.left.clamp(0, cw), top: r.top.clamp(0, ch), right: r.right.clamp(0, cw), bottom: r.bottom.clamp(0, ch) },
            None => RECT { left: 0, top: 0, right: cw, bottom: ch },
        };
        if src.right - src.left < 2 || src.bottom - src.top < 2 {
            return Ok(());
        }
        // fit the source into the output, keeping its aspect (a resized window gets bars)
        let (sw, sh) = ((src.right - src.left) as f64, (src.bottom - src.top) as f64);
        let k = (self.w as f64 / sw).min(self.h as f64 / sh);
        let (dw, dh) = ((sw * k).round() as i32, (sh * k).round() as i32);
        let (dx, dy) = ((self.w as i32 - dw) / 2, (self.h as i32 - dh) / 2);
        let dst = RECT { left: dx, top: dy, right: dx + dw, bottom: dy + dh };
        let full = RECT { left: 0, top: 0, right: self.w as i32, bottom: self.h as i32 };
        unsafe {
            let out_desc = D3D11_TEXTURE2D_DESC {
                Width: self.w,
                Height: self.h,
                MipLevels: 1,
                ArraySize: 1,
                Format: DXGI_FORMAT_NV12,
                SampleDesc: DXGI_SAMPLE_DESC { Count: 1, Quality: 0 },
                Usage: D3D11_USAGE_DEFAULT,
                BindFlags: D3D11_BIND_RENDER_TARGET.0 as u32,
                CPUAccessFlags: 0,
                MiscFlags: 0,
            };
            let mut out = None;
            self.device.CreateTexture2D(&out_desc, None, Some(&mut out)).map_err(e("frame texture"))?;
            let out = out.ok_or("frame texture")?;

            let mut ov = None;
            let ov_desc = D3D11_VIDEO_PROCESSOR_OUTPUT_VIEW_DESC {
                ViewDimension: D3D11_VPOV_DIMENSION_TEXTURE2D,
                Anonymous: D3D11_VIDEO_PROCESSOR_OUTPUT_VIEW_DESC_0 { Texture2D: D3D11_TEX2D_VPOV { MipSlice: 0 } },
            };
            self.vdev.CreateVideoProcessorOutputView(&out, &en, &ov_desc, Some(&mut ov)).map_err(e("output view"))?;
            let mut iv = None;
            let iv_desc = D3D11_VIDEO_PROCESSOR_INPUT_VIEW_DESC {
                FourCC: 0,
                ViewDimension: D3D11_VPIV_DIMENSION_TEXTURE2D,
                Anonymous: D3D11_VIDEO_PROCESSOR_INPUT_VIEW_DESC_0 { Texture2D: D3D11_TEX2D_VPIV { MipSlice: 0, ArraySlice: 0 } },
            };
            self.vdev.CreateVideoProcessorInputView(tex, &en, &iv_desc, Some(&mut iv)).map_err(e("input view"))?;

            self.vctx.VideoProcessorSetStreamFrameFormat(&vp, 0, D3D11_VIDEO_FRAME_FORMAT_PROGRESSIVE);
            self.vctx.VideoProcessorSetStreamSourceRect(&vp, 0, true, Some(&src));
            self.vctx.VideoProcessorSetStreamDestRect(&vp, 0, true, Some(&dst));
            self.vctx.VideoProcessorSetOutputTargetRect(&vp, true, Some(&full));
            // black bars: Y=16 Cb=Cr=128 in YCbCr (the flag says the colour is YCbCr)
            let black = D3D11_VIDEO_COLOR { Anonymous: D3D11_VIDEO_COLOR_0 { YCbCr: D3D11_VIDEO_COLOR_YCbCrA { Y: 0.0625, Cb: 0.5, Cr: 0.5, A: 1.0 } } };
            self.vctx.VideoProcessorSetOutputBackgroundColor(&vp, true, &black);
            let stream = D3D11_VIDEO_PROCESSOR_STREAM { Enable: true.into(), pInputSurface: ManuallyDrop::new(iv), ..Default::default() };
            let streams = [stream];
            let r = self.vctx.VideoProcessorBlt(&vp, ov.as_ref().ok_or("output view")?, 0, &streams);
            let [stream] = streams;
            drop(ManuallyDrop::into_inner(stream.pInputSurface));
            r.map_err(e("convert frame"))?;
            self.write_texture(&out, t_hns)?;
            self.last = Some((out, t_hns));
        }
        Ok(())
    }

    fn write_texture(&mut self, tex: &ID3D11Texture2D, t_hns: i64) -> Result<(), String> {
        unsafe {
            let buf = MFCreateDXGISurfaceBuffer(&ID3D11Texture2D::IID, tex, 0, false).map_err(e("frame buffer"))?;
            let len = buf.cast::<IMF2DBuffer>().and_then(|b| b.GetContiguousLength()).unwrap_or(self.w * self.h * 3 / 2);
            buf.SetCurrentLength(len).map_err(e("frame buffer"))?;
            let sample = MFCreateSample().map_err(e("frame sample"))?;
            sample.AddBuffer(&buf).map_err(e("frame sample"))?;
            sample.SetSampleTime(t_hns).map_err(e("frame sample"))?;
            sample.SetSampleDuration(self.frame_hns).map_err(e("frame sample"))?;
            self.writer.WriteSample(self.video, &sample).map_err(e("encode frame"))
        }
    }

    /// Time of the last frame written, if any.
    pub fn last_frame_time(&self) -> Option<i64> {
        self.last.as_ref().map(|l| l.1)
    }

    /// Re-send the last frame at `t_hns` (the screen did not change, or the recording ends).
    pub fn repeat(&mut self, t_hns: i64) -> Result<(), String> {
        let Some((tex, t)) = self.last.clone() else { return Ok(()) };
        if t_hns <= t {
            return Ok(());
        }
        self.write_texture(&tex, t_hns)?;
        self.last = Some((tex, t_hns));
        Ok(())
    }

    /// 16-bit interleaved stereo PCM starting at `t_hns`.
    pub fn audio(&mut self, pcm: &[i16], t_hns: i64) -> Result<(), String> {
        let Some(idx) = self.audio else { return Ok(()) };
        if pcm.is_empty() {
            return Ok(());
        }
        let bytes = pcm.len() * 2;
        unsafe {
            let buf = MFCreateMemoryBuffer(bytes as u32).map_err(e("audio buffer"))?;
            let mut ptr = std::ptr::null_mut();
            buf.Lock(&mut ptr, None, None).map_err(e("audio buffer"))?;
            std::ptr::copy_nonoverlapping(pcm.as_ptr() as *const u8, ptr, bytes);
            let _ = buf.Unlock();
            buf.SetCurrentLength(bytes as u32).map_err(e("audio buffer"))?;
            let sample = MFCreateSample().map_err(e("audio sample"))?;
            sample.AddBuffer(&buf).map_err(e("audio sample"))?;
            sample.SetSampleTime(t_hns).map_err(e("audio sample"))?;
            sample.SetSampleDuration((pcm.len() / 2) as i64 * 10_000_000 / self.audio_rate as i64).map_err(e("audio sample"))?;
            self.writer.WriteSample(idx, &sample).map_err(e("encode audio"))
        }
    }

    pub fn finish(self) -> Result<(), String> {
        unsafe { self.writer.Finalize().map_err(e("finish file")) }
    }
}
