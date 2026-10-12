//! Text-caret lookup through the macOS Accessibility API. Needs the
//! Accessibility permission; everything degrades to "no caret" without it.

use core_foundation::base::{CFRelease, CFTypeRef, TCFType};
use core_foundation::boolean::CFBoolean;
use core_foundation::string::{CFString, CFStringRef};
use std::collections::HashSet;
use std::ffi::c_void;

#[link(name = "ApplicationServices", kind = "framework")]
extern "C" {
    fn AXIsProcessTrusted() -> bool;
    fn AXUIElementCreateApplication(pid: i32) -> CFTypeRef;
    fn AXUIElementCopyAttributeValue(element: CFTypeRef, attribute: CFStringRef, value: *mut CFTypeRef) -> i32;
    fn AXUIElementSetAttributeValue(element: CFTypeRef, attribute: CFStringRef, value: CFTypeRef) -> i32;
    fn AXUIElementCopyParameterizedAttributeValue(
        element: CFTypeRef,
        attribute: CFStringRef,
        parameter: CFTypeRef,
        value: *mut CFTypeRef,
    ) -> i32;
    fn AXValueGetValue(value: CFTypeRef, kind: u32, out: *mut c_void) -> bool;
    fn AXValueCreate(kind: u32, value: *const c_void) -> CFTypeRef;
}

const AX_POINT: u32 = 1;
const AX_SIZE: u32 = 2;
const AX_RECT: u32 = 3;
const AX_RANGE: u32 = 4;

#[repr(C)]
#[derive(Default, Clone, Copy)]
struct Rect {
    x: f64,
    y: f64,
    w: f64,
    h: f64,
}
#[repr(C)]
#[derive(Default, Clone, Copy)]
struct Pt {
    x: f64,
    y: f64,
}
#[repr(C)]
#[derive(Default, Clone, Copy)]
struct Sz {
    w: f64,
    h: f64,
}
#[repr(C)]
#[derive(Default, Clone, Copy)]
struct Range {
    location: isize,
    length: isize,
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

unsafe fn value<T: Default + Copy>(v: CFTypeRef, kind: u32) -> Option<T> {
    let mut out = T::default();
    AXValueGetValue(v, kind, &mut out as *mut T as *mut c_void).then_some(out)
}

unsafe fn bounds_for_range(el: CFTypeRef, range: Range) -> Option<Rect> {
    let rv = AXValueCreate(AX_RANGE, &range as *const Range as *const c_void);
    if rv.is_null() {
        return None;
    }
    let key = CFString::new("AXBoundsForRange");
    let mut out: CFTypeRef = std::ptr::null();
    let err = AXUIElementCopyParameterizedAttributeValue(el, key.as_concrete_TypeRef(), rv, &mut out);
    CFRelease(rv);
    if err != 0 || out.is_null() {
        return None;
    }
    let r = value::<Rect>(out, AX_RECT);
    CFRelease(out);
    r
}

fn frontmost_pid() -> Option<i32> {
    use objc2_app_kit::NSWorkspace;
    let app = NSWorkspace::sharedWorkspace().frontmostApplication()?;
    Some(app.processIdentifier())
}

/// Finds the caret of whatever app is in front. Remembers which apps it has
/// already asked to expose their accessibility tree (Chromium / Electron need it).
#[derive(Default)]
pub struct CaretFinder {
    primed: HashSet<i32>,
}

impl CaretFinder {
    /// Caret rectangle in global points (top-left origin).
    pub fn caret(&mut self) -> Option<(f64, f64, f64, f64)> {
        let pid = frontmost_pid()?;
        unsafe {
            let app = AXUIElementCreateApplication(pid);
            if app.is_null() {
                return None;
            }
            if self.primed.insert(pid) {
                let yes = CFBoolean::true_value();
                for name in ["AXManualAccessibility", "AXEnhancedUserInterface"] {
                    let k = CFString::new(name);
                    AXUIElementSetAttributeValue(app, k.as_concrete_TypeRef(), yes.as_CFTypeRef());
                }
            }
            let result = (|| {
                let focused = attr(app, "AXFocusedUIElement")?;
                let r = Self::from_element(focused);
                CFRelease(focused);
                r
            })();
            CFRelease(app);
            result
        }
    }

    unsafe fn from_element(el: CFTypeRef) -> Option<(f64, f64, f64, f64)> {
        // 1) exact caret via the selected text range
        if let Some(rv) = attr(el, "AXSelectedTextRange") {
            let range = value::<Range>(rv, AX_RANGE);
            CFRelease(rv);
            if let Some(r) = range {
                let mut probes = vec![r];
                if r.length == 0 && r.location > 0 {
                    probes.push(Range { location: r.location - 1, length: 1 });
                }
                for (i, p) in probes.iter().enumerate() {
                    if let Some(b) = bounds_for_range(el, *p) {
                        if b.h > 0.0 && b.x.is_finite() && (b.x != 0.0 || b.y != 0.0) {
                            // the previous character's right edge is where the caret sits
                            let x = if i == 1 { b.x + b.w } else { b.x };
                            return Some((x, b.y, b.w.min(3.0), b.h));
                        }
                    }
                }
            }
        }
        // 1b) web content (Chromium / WebKit) exposes the caret as a text-marker range
        if let Some(mr) = attr(el, "AXSelectedTextMarkerRange") {
            let key = CFString::new("AXBoundsForTextMarkerRange");
            let mut out: CFTypeRef = std::ptr::null();
            let err = AXUIElementCopyParameterizedAttributeValue(el, key.as_concrete_TypeRef(), mr, &mut out);
            CFRelease(mr);
            if err == 0 && !out.is_null() {
                let r = value::<Rect>(out, AX_RECT);
                CFRelease(out);
                if let Some(b) = r {
                    if b.h > 0.0 && b.x.is_finite() && (b.x != 0.0 || b.y != 0.0) {
                        return Some((b.x, b.y, b.w.min(3.0), b.h));
                    }
                }
            }
        }
        // 2) fall back to the focused element's frame when it is reasonably small
        let pos = attr(el, "AXPosition").and_then(|v| {
            let r = value::<Pt>(v, AX_POINT);
            CFRelease(v);
            r
        })?;
        let size = attr(el, "AXSize").and_then(|v| {
            let r = value::<Sz>(v, AX_SIZE);
            CFRelease(v);
            r
        })?;
        (size.w > 0.0 && size.w <= 1100.0 && size.h <= 420.0).then_some((pos.x, pos.y, size.w, size.h))
    }
}
