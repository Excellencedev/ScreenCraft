//! JPEG encoding through Apple's ImageIO (hardware/SIMD accelerated). The pure-Rust
//! encoder needed ~200 ms for a 5 MP frame, which capped export at ~20 fps.

use core_foundation::base::{CFType, TCFType};
use foreign_types_shared::ForeignType;
use core_foundation::dictionary::CFDictionary;
use core_foundation::number::CFNumber;
use core_foundation::string::{CFString, CFStringRef};
use core_graphics::color_space::CGColorSpace;
use core_graphics::data_provider::CGDataProvider;
use core_graphics::image::CGImage;
use std::os::raw::c_void;
use std::sync::Arc;

type CGImageDestinationRef = *mut c_void;

#[link(name = "ImageIO", kind = "framework")]
extern "C" {
    fn CGImageDestinationCreateWithData(
        data: *mut c_void,
        ty: CFStringRef,
        count: usize,
        options: core_foundation::dictionary::CFDictionaryRef,
    ) -> CGImageDestinationRef;
    fn CGImageDestinationAddImage(
        dest: CGImageDestinationRef,
        image: core_graphics::sys::CGImageRef,
        props: core_foundation::dictionary::CFDictionaryRef,
    );
    fn CGImageDestinationFinalize(dest: CGImageDestinationRef) -> bool;
    static kCGImageDestinationLossyCompressionQuality: CFStringRef;
}

extern "C" {
    fn CFRelease(cf: *const c_void);
    fn CFDataCreateMutable(allocator: *const c_void, capacity: isize) -> *mut c_void;
    fn CFDataGetLength(data: *const c_void) -> isize;
    fn CFDataGetBytePtr(data: *const c_void) -> *const u8;
}

/// `bgra` is tightly packed 32-bit BGRA (alpha ignored), `quality` 1..=100.
pub fn encode_bgra(bgra: Vec<u8>, w: usize, h: usize, quality: u8) -> Option<Vec<u8>> {
    // CGBitmapInfo: kCGImageAlphaNoneSkipFirst (6) | kCGBitmapByteOrder32Little (2 << 12)
    const BITMAP_INFO: u32 = 6 | (2 << 12);
    let provider = CGDataProvider::from_buffer(Arc::new(bgra));
    let cs = CGColorSpace::create_device_rgb();
    let image = CGImage::new(w, h, 8, 32, w * 4, &cs, BITMAP_INFO, &provider, false, 0);
    unsafe {
        let out = CFDataCreateMutable(std::ptr::null(), 0);
        if out.is_null() {
            return None;
        }
        let ty = CFString::new("public.jpeg");
        let dest = CGImageDestinationCreateWithData(out, ty.as_concrete_TypeRef(), 1, std::ptr::null());
        if dest.is_null() {
            CFRelease(out);
            return None;
        }
        let key = CFString::wrap_under_get_rule(kCGImageDestinationLossyCompressionQuality);
        let props: CFDictionary<CFString, CFType> =
            CFDictionary::from_CFType_pairs(&[(key, CFNumber::from(quality as f64 / 100.0).as_CFType())]);
        CGImageDestinationAddImage(dest, image.as_ptr(), props.as_concrete_TypeRef());
        let ok = CGImageDestinationFinalize(dest);
        CFRelease(dest as *const c_void);
        let result = if ok {
            let len = CFDataGetLength(out) as usize;
            let ptr = CFDataGetBytePtr(out);
            Some(std::slice::from_raw_parts(ptr, len).to_vec())
        } else {
            None
        };
        CFRelease(out);
        result
    }
}
