//! H.264 decoding on macOS through VideoToolbox (the operating system's licensed decoder).
//! One synchronous session; samples are length-prefixed (avcC style) exactly as stored in MP4.

#![allow(non_snake_case, non_upper_case_globals, non_camel_case_types)]

use crate::types::*;
use crate::{Error, Result};
use std::ffi::c_void;
use std::sync::Mutex;

type OSStatus = i32;
type CFTypeRef = *const c_void;
type CFAllocatorRef = *const c_void;
type CMSampleBufferRef = *mut c_void;
type VTDecompressionSessionRef = *mut c_void;

#[repr(C)]
#[derive(Clone, Copy)]
struct CMTime {
    value: i64,
    timescale: i32,
    flags: u32,
    epoch: i64,
}

#[repr(C)]
struct VTDecompressionOutputCallbackRecord {
    callback: extern "C" fn(*mut c_void, *mut c_void, OSStatus, u32, *mut c_void, CMTime, CMTime),
    refcon: *mut c_void,
}

#[link(name = "VideoToolbox", kind = "framework")]
extern "C" {
    fn VTDecompressionSessionCreate(
        alloc: CFAllocatorRef,
        format: *const c_void,
        decoder_spec: CFTypeRef,
        dest_attrs: CFTypeRef,
        cb: *const VTDecompressionOutputCallbackRecord,
        out: *mut VTDecompressionSessionRef,
    ) -> OSStatus;
    fn VTDecompressionSessionDecodeFrame(
        s: VTDecompressionSessionRef,
        sbuf: CMSampleBufferRef,
        flags: u32,
        source_refcon: *mut c_void,
        info_out: *mut u32,
    ) -> OSStatus;
    fn VTDecompressionSessionWaitForAsynchronousFrames(s: VTDecompressionSessionRef) -> OSStatus;
    fn VTDecompressionSessionInvalidate(s: VTDecompressionSessionRef);
}

#[link(name = "CoreMedia", kind = "framework")]
extern "C" {
    fn CMVideoFormatDescriptionCreateFromH264ParameterSets(
        alloc: CFAllocatorRef,
        count: usize,
        ptrs: *const *const u8,
        sizes: *const usize,
        nal_header_len: i32,
        out: *mut *const c_void,
    ) -> OSStatus;
    fn CMBlockBufferCreateWithMemoryBlock(
        alloc: CFAllocatorRef,
        memory: *mut c_void,
        len: usize,
        block_alloc: CFAllocatorRef,
        custom: *const c_void,
        offset: usize,
        data_len: usize,
        flags: u32,
        out: *mut *mut c_void,
    ) -> OSStatus;
    fn CMBlockBufferReplaceDataBytes(src: *const c_void, block: *mut c_void, offset: usize, len: usize) -> OSStatus;
    fn CMSampleBufferCreate(
        alloc: CFAllocatorRef,
        data: *mut c_void,
        ready: u8,
        make_ready: *const c_void,
        refcon: *mut c_void,
        format: *const c_void,
        n_samples: isize,
        n_timing: isize,
        timing: *const c_void,
        n_sizes: isize,
        sizes: *const usize,
        out: *mut CMSampleBufferRef,
    ) -> OSStatus;
}

#[link(name = "CoreVideo", kind = "framework")]
extern "C" {
    static kCVPixelBufferPixelFormatTypeKey: CFTypeRef;
    fn CVPixelBufferLockBaseAddress(pb: *mut c_void, flags: u64) -> i32;
    fn CVPixelBufferUnlockBaseAddress(pb: *mut c_void, flags: u64) -> i32;
    fn CVPixelBufferGetWidth(pb: *mut c_void) -> usize;
    fn CVPixelBufferGetHeight(pb: *mut c_void) -> usize;
    fn CVPixelBufferGetBaseAddressOfPlane(pb: *mut c_void, plane: usize) -> *mut u8;
    fn CVPixelBufferGetBytesPerRowOfPlane(pb: *mut c_void, plane: usize) -> usize;
}

#[link(name = "CoreFoundation", kind = "framework")]
extern "C" {
    static kCFAllocatorDefault: CFAllocatorRef;
    fn CFNumberCreate(alloc: CFAllocatorRef, ty: i32, value: *const c_void) -> CFTypeRef;
    fn CFRelease(obj: CFTypeRef);
    fn CFDictionaryCreate(
        alloc: CFAllocatorRef,
        keys: *const *const c_void,
        values: *const *const c_void,
        n: isize,
        key_cbs: *const c_void,
        val_cbs: *const c_void,
    ) -> CFTypeRef;
    static kCFTypeDictionaryKeyCallBacks: c_void;
    static kCFTypeDictionaryValueCallBacks: c_void;
}

const kCFNumberSInt32Type: i32 = 3;
const kCVPixelFormatType_420YpCbCr8BiPlanarVideoRange: i32 = 0x3432_3076; // '420v'

#[derive(Default)]
struct Out {
    picture: Option<I420>,
    error: Option<String>,
}

extern "C" fn on_frame(refcon: *mut c_void, _src: *mut c_void, status: OSStatus, _flags: u32, image: *mut c_void, _pts: CMTime, _dur: CMTime) {
    // SAFETY: refcon is the boxed Mutex<Out> owned by the decoder, alive until Drop.
    let out = unsafe { &*(refcon as *const Mutex<Out>) };
    let mut o = out.lock().unwrap();
    if status != 0 || image.is_null() {
        o.error = Some(format!("VideoToolbox could not decode a frame ({status})"));
        return;
    }
    unsafe {
        CVPixelBufferLockBaseAddress(image, 1);
        let (w, h) = (CVPixelBufferGetWidth(image), CVPixelBufferGetHeight(image));
        let mut f = I420::new(w as u32, h as u32);
        let (yp, ys) = (CVPixelBufferGetBaseAddressOfPlane(image, 0) as *const u8, CVPixelBufferGetBytesPerRowOfPlane(image, 0));
        for r in 0..h {
            std::ptr::copy_nonoverlapping(yp.add(r * ys), f.y.as_mut_ptr().add(r * w), w);
        }
        let (cp, cs) = (CVPixelBufferGetBaseAddressOfPlane(image, 1) as *const u8, CVPixelBufferGetBytesPerRowOfPlane(image, 1));
        let cw = w / 2;
        for r in 0..h / 2 {
            let row = cp.add(r * cs);
            for x in 0..cw {
                f.u[r * cw + x] = *row.add(x * 2);
                f.v[r * cw + x] = *row.add(x * 2 + 1);
            }
        }
        CVPixelBufferUnlockBaseAddress(image, 1);
        o.picture = Some(f);
    }
}

pub struct VtH264Decoder {
    session: VTDecompressionSessionRef,
    format: *const c_void,
    out: *mut Mutex<Out>,
}

// The raw handles are only touched through &mut self; VideoToolbox sessions are thread-safe.
unsafe impl Send for VtH264Decoder {}

impl VtH264Decoder {
    /// `params` are the SPS and PPS NAL units (without start codes).
    pub fn new(params: &[Vec<u8>], length_size: usize) -> Result<Self> {
        if params.len() < 2 {
            return Err(Error::Codec("the video has no H.264 parameter sets".into()));
        }
        unsafe {
            let ptrs: Vec<*const u8> = params.iter().map(|p| p.as_ptr()).collect();
            let sizes: Vec<usize> = params.iter().map(|p| p.len()).collect();
            let mut format: *const c_void = std::ptr::null();
            let st = CMVideoFormatDescriptionCreateFromH264ParameterSets(
                kCFAllocatorDefault,
                params.len(),
                ptrs.as_ptr(),
                sizes.as_ptr(),
                length_size as i32,
                &mut format,
            );
            if st != 0 || format.is_null() {
                return Err(Error::Codec(format!("VideoToolbox rejected the H.264 headers ({st})")));
            }
            let fmt = kCVPixelFormatType_420YpCbCr8BiPlanarVideoRange;
            let num = CFNumberCreate(kCFAllocatorDefault, kCFNumberSInt32Type, &fmt as *const i32 as *const c_void);
            let keys = [kCVPixelBufferPixelFormatTypeKey];
            let vals = [num];
            let attrs = CFDictionaryCreate(
                kCFAllocatorDefault,
                keys.as_ptr(),
                vals.as_ptr(),
                1,
                &kCFTypeDictionaryKeyCallBacks as *const c_void,
                &kCFTypeDictionaryValueCallBacks as *const c_void,
            );
            let out = Box::into_raw(Box::new(Mutex::new(Out::default())));
            let rec = VTDecompressionOutputCallbackRecord { callback: on_frame, refcon: out as *mut c_void };
            let mut session: VTDecompressionSessionRef = std::ptr::null_mut();
            let st = VTDecompressionSessionCreate(kCFAllocatorDefault, format, std::ptr::null(), attrs, &rec, &mut session);
            CFRelease(attrs);
            CFRelease(num);
            if st != 0 || session.is_null() {
                CFRelease(format);
                drop(Box::from_raw(out));
                return Err(Error::Codec(format!("VideoToolbox decoder failed to start ({st})")));
            }
            Ok(Self { session, format, out })
        }
    }

    /// Decode one length-prefixed access unit. Returns the picture when one came out.
    pub fn decode(&mut self, sample: &[u8]) -> Result<Option<I420>> {
        unsafe {
            let mut block: *mut c_void = std::ptr::null_mut();
            let st = CMBlockBufferCreateWithMemoryBlock(
                kCFAllocatorDefault,
                std::ptr::null_mut(),
                sample.len(),
                kCFAllocatorDefault,
                std::ptr::null(),
                0,
                sample.len(),
                1, // kCMBlockBufferAssureMemoryNowFlag
                &mut block,
            );
            if st != 0 || block.is_null() {
                return Err(Error::Codec(format!("could not allocate a decode buffer ({st})")));
            }
            CMBlockBufferReplaceDataBytes(sample.as_ptr() as *const c_void, block, 0, sample.len());
            let mut sbuf: CMSampleBufferRef = std::ptr::null_mut();
            let size = sample.len();
            let st = CMSampleBufferCreate(
                kCFAllocatorDefault,
                block,
                1,
                std::ptr::null(),
                std::ptr::null_mut(),
                self.format,
                1,
                0,
                std::ptr::null(),
                1,
                &size,
                &mut sbuf,
            );
            CFRelease(block as CFTypeRef);
            if st != 0 || sbuf.is_null() {
                return Err(Error::Codec(format!("could not wrap the sample ({st})")));
            }
            let st = VTDecompressionSessionDecodeFrame(self.session, sbuf, 0, std::ptr::null_mut(), std::ptr::null_mut());
            VTDecompressionSessionWaitForAsynchronousFrames(self.session);
            CFRelease(sbuf as CFTypeRef);
            let mut o = (*self.out).lock().unwrap();
            if let Some(e) = o.error.take() {
                return Err(Error::Codec(e));
            }
            if st != 0 {
                return Err(Error::Codec(format!("VideoToolbox decode failed ({st})")));
            }
            Ok(o.picture.take())
        }
    }
}

impl Drop for VtH264Decoder {
    fn drop(&mut self) {
        unsafe {
            VTDecompressionSessionInvalidate(self.session);
            CFRelease(self.session as CFTypeRef);
            CFRelease(self.format);
            drop(Box::from_raw(self.out));
        }
    }
}
