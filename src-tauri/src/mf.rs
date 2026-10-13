//! Video decoding through Media Foundation's source reader. Frames come out as
//! tightly packed 32-bit BGRA (alpha undefined), optionally scaled by MF's video
//! processor, with their presentation time in seconds.

use ::windows::core::{Interface, GUID, HSTRING};
use ::windows::Win32::Media::MediaFoundation::*;
use ::windows::Win32::System::Com::{CoInitializeEx, COINIT_MULTITHREADED};
use std::sync::Once;

const VIDEO: u32 = MF_SOURCE_READER_FIRST_VIDEO_STREAM.0 as u32;

/// COM (multithreaded) for the calling thread plus Media Foundation for the process.
/// Safe to call any number of times, on any thread.
pub fn init_thread() {
    static MF: Once = Once::new();
    unsafe {
        // S_FALSE (already initialised) and mode mismatches are fine: MF works either way
        let _ = CoInitializeEx(None, COINIT_MULTITHREADED);
        MF.call_once(|| {
            let _ = MFStartup(MF_VERSION, MFSTARTUP_FULL);
        });
    }
}

pub struct Frame {
    pub pts: f64,
    pub w: usize,
    pub h: usize,
    pub bgra: Vec<u8>,
}

pub struct Reader {
    reader: IMFSourceReader,
    /// Size of the frames this reader outputs.
    pub w: u32,
    pub h: u32,
    /// Size of the video as stored.
    pub src_w: u32,
    pub src_h: u32,
    /// Seconds; 0 when unknown.
    pub duration: f64,
    /// File time of the first frame. MF ignores the MP4 edit list that hides the
    /// B-frame start delay, so times are shifted to make the first frame t = 0, as
    /// AVFoundation reports them.
    start: f64,
    /// The first frame, read while opening to learn `start`.
    peeked: Option<Frame>,
}

fn split_size(v: u64) -> (u32, u32) {
    ((v >> 32) as u32, v as u32)
}

impl Reader {
    /// Open the first video stream of `path`. With `width`, frames are scaled to that
    /// width (even, never upscaled, aspect kept); otherwise they come at native size.
    pub fn open(path: &str, width: Option<u32>) -> Result<Reader, String> {
        init_thread();
        let e = |what: &'static str| move |err: ::windows::core::Error| format!("{what}: {err}");
        unsafe {
            let mut attrs = None;
            MFCreateAttributes(&mut attrs, 2).map_err(e("attributes"))?;
            let attrs = attrs.ok_or("attributes")?;
            attrs.SetUINT32(&MF_SOURCE_READER_ENABLE_ADVANCED_VIDEO_PROCESSING, 1).map_err(e("attributes"))?;
            attrs.SetUINT32(&MF_READWRITE_ENABLE_HARDWARE_TRANSFORMS, 1).map_err(e("attributes"))?;
            let reader = MFCreateSourceReaderFromURL(&HSTRING::from(path), &attrs).map_err(e("open video"))?;
            reader.SetStreamSelection(MF_SOURCE_READER_ALL_STREAMS.0 as u32, false).map_err(e("select"))?;
            reader.SetStreamSelection(VIDEO, true).map_err(|_| "no video track".to_string())?;

            let native = reader.GetNativeMediaType(VIDEO, 0).map_err(e("video type"))?;
            let (src_w, src_h) = split_size(native.GetUINT64(&MF_MT_FRAME_SIZE).map_err(e("frame size"))?);
            if src_w < 2 || src_h < 2 {
                return Err("empty video".into());
            }

            let out = MFCreateMediaType().map_err(e("media type"))?;
            out.SetGUID(&MF_MT_MAJOR_TYPE, &MFMediaType_Video).map_err(e("media type"))?;
            out.SetGUID(&MF_MT_SUBTYPE, &MFVideoFormat_RGB32).map_err(e("media type"))?;
            if let Some(tw) = width {
                let w = tw.min(src_w) & !1;
                let h = ((w as f64 * src_h as f64 / src_w as f64).round() as u32) & !1;
                out.SetUINT64(&MF_MT_FRAME_SIZE, (w as u64) << 32 | h as u64).map_err(e("media type"))?;
            }
            reader.SetCurrentMediaType(VIDEO, None, &out).map_err(e("decoder"))?;
            let cur = reader.GetCurrentMediaType(VIDEO).map_err(e("decoder"))?;
            let (w, h) = split_size(cur.GetUINT64(&MF_MT_FRAME_SIZE).map_err(e("frame size"))?);

            let duration = reader
                .GetPresentationAttribute(MF_SOURCE_READER_MEDIASOURCE.0 as u32, &MF_PD_DURATION)
                .ok()
                .and_then(|v| u64::try_from(&v).ok())
                .map(|hns| hns as f64 / 1e7)
                .unwrap_or(0.0);
            let mut r = Reader { reader, w, h, src_w, src_h, duration, start: 0.0, peeked: None };
            let first = r.read().ok_or("no frames could be decoded")?;
            r.start = first.pts;
            r.duration = (r.duration - r.start).max(0.0);
            r.peeked = Some(Frame { pts: 0.0, ..first });
            Ok(r)
        }
    }

    /// Jump to `t` seconds. The next frames start at the key frame before it.
    pub fn seek(&mut self, t: f64) -> bool {
        self.peeked = None;
        let hns = ((t.max(0.0) + self.start) * 1e7) as i64;
        let pos = ::windows::Win32::System::Com::StructuredStorage::PROPVARIANT::from(hns);
        unsafe { self.reader.SetCurrentPosition(&GUID::zeroed(), &pos).is_ok() }
    }

    /// The next decoded frame, or `None` at the end (or on a decode error).
    pub fn next(&mut self) -> Option<Frame> {
        if let Some(f) = self.peeked.take() {
            return Some(f);
        }
        let f = self.read()?;
        Some(Frame { pts: f.pts - self.start, ..f })
    }

    /// The next frame with its raw file time.
    fn read(&mut self) -> Option<Frame> {
        unsafe {
            loop {
                let mut flags = 0u32;
                let mut ts = 0i64;
                let mut sample = None;
                self.reader.ReadSample(VIDEO, 0, None, Some(&mut flags), Some(&mut ts), Some(&mut sample)).ok()?;
                if flags & MF_SOURCE_READERF_CURRENTMEDIATYPECHANGED.0 as u32 != 0 {
                    if let Ok(t) = self.reader.GetCurrentMediaType(VIDEO) {
                        if let Ok(v) = t.GetUINT64(&MF_MT_FRAME_SIZE) {
                            (self.w, self.h) = split_size(v);
                        }
                    }
                }
                if let Some(sample) = sample {
                    if let Some(bgra) = self.pixels(&sample) {
                        return Some(Frame { pts: ts as f64 / 1e7, w: self.w as usize, h: self.h as usize, bgra });
                    }
                }
                if flags & (MF_SOURCE_READERF_ENDOFSTREAM.0 as u32 | MF_SOURCE_READERF_ERROR.0 as u32) != 0 {
                    return None;
                }
            }
        }
    }

    /// Copy a sample's pixels out, top row first, without row padding.
    unsafe fn pixels(&self, sample: &IMFSample) -> Option<Vec<u8>> {
        let (w, h) = (self.w as usize, self.h as usize);
        let row = w * 4;
        let buf = sample.ConvertToContiguousBuffer().ok()?;
        let mut out = vec![0u8; row * h];
        if let Ok(b2) = buf.cast::<IMF2DBuffer>() {
            let mut scan0 = std::ptr::null_mut();
            let mut pitch = 0i32;
            b2.Lock2D(&mut scan0, &mut pitch).ok()?;
            if !scan0.is_null() && pitch.unsigned_abs() as usize >= row {
                for y in 0..h {
                    let src = scan0.offset(y as isize * pitch as isize);
                    std::ptr::copy_nonoverlapping(src, out.as_mut_ptr().add(y * row), row);
                }
            }
            let _ = b2.Unlock2D();
            return Some(out);
        }
        let mut ptr = std::ptr::null_mut();
        let mut len = 0u32;
        buf.Lock(&mut ptr, None, Some(&mut len)).ok()?;
        if !ptr.is_null() && len as usize >= row * h {
            // RGB from a plain buffer is stored bottom-up
            for y in 0..h {
                std::ptr::copy_nonoverlapping(ptr.add((h - 1 - y) * row), out.as_mut_ptr().add(y * row), row);
            }
        }
        let _ = buf.Unlock();
        Some(out)
    }
}
