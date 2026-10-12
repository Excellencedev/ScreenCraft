//! Text-caret lookup through the macOS Accessibility API. Needs the
//! Accessibility permission; everything degrades to "no caret" without it.

use core_foundation::base::{CFRelease, CFTypeRef, TCFType};
use core_foundation::string::{CFString, CFStringRef};
use std::ffi::c_void;

#[link(name = "ApplicationServices", kind = "framework")]
extern "C" {
    fn AXIsProcessTrusted() -> bool;
    fn AXUIElementCreateSystemWide() -> CFTypeRef;
    fn AXUIElementCopyAttributeValue(element: CFTypeRef, attribute: CFStringRef, value: *mut CFTypeRef) -> i32;
    fn AXUIElementCopyParameterizedAttributeValue(
        element: CFTypeRef,
        attribute: CFStringRef,
        parameter: CFTypeRef,
        value: *mut CFTypeRef,
    ) -> i32;
    fn AXValueGetValue(value: CFTypeRef, kind: u32, out: *mut c_void) -> bool;
}

const AX_CGRECT: u32 = 3;

#[repr(C)]
#[derive(Default, Clone, Copy)]
struct CGRect {
    x: f64,
    y: f64,
    w: f64,
    h: f64,
}

pub fn trusted() -> bool {
    unsafe { AXIsProcessTrusted() }
}

unsafe fn attr(el: CFTypeRef, name: &str) -> Option<CFTypeRef> {
    let key = CFString::new(name);
    let mut out: CFTypeRef = std::ptr::null();
    let err = AXUIElementCopyAttributeValue(el, key.as_concrete_TypeRef(), &mut out);
    (err == 0 && !out.is_null()).then_some(out)
}

/// Screen rectangle (global points, top-left origin) of the text caret in the
/// focused UI element, if there is one.
pub fn caret_rect() -> Option<(f64, f64, f64, f64)> {
    unsafe {
        let sys = AXUIElementCreateSystemWide();
        if sys.is_null() {
            return None;
        }
        let result = (|| {
            let focused = attr(sys, "AXFocusedUIElement")?;
            let r = (|| {
                let range = attr(focused, "AXSelectedTextRange")?;
                let key = CFString::new("AXBoundsForRange");
                let mut bounds: CFTypeRef = std::ptr::null();
                let err = AXUIElementCopyParameterizedAttributeValue(focused, key.as_concrete_TypeRef(), range, &mut bounds);
                CFRelease(range);
                if err != 0 || bounds.is_null() {
                    return None;
                }
                let mut rect = CGRect::default();
                let ok = AXValueGetValue(bounds, AX_CGRECT, &mut rect as *mut _ as *mut c_void);
                CFRelease(bounds);
                (ok && (rect.x != 0.0 || rect.y != 0.0)).then_some((rect.x, rect.y, rect.w, rect.h))
            })();
            CFRelease(focused);
            r
        })();
        CFRelease(sys);
        result
    }
}
