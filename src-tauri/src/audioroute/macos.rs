//! CoreAudio plumbing for `audioroute` (just the few properties it needs).

use super::{InputInfo, Route};
use core_foundation::base::TCFType;
use core_foundation::string::{CFString, CFStringRef};
use std::ffi::c_void;
use std::sync::Mutex;

#[repr(C)]
struct Address {
    selector: u32,
    scope: u32,
    element: u32,
}

#[link(name = "CoreAudio", kind = "framework")]
extern "C" {
    fn AudioObjectGetPropertyDataSize(id: u32, addr: *const Address, qsize: u32, q: *const c_void, out: *mut u32) -> i32;
    fn AudioObjectGetPropertyData(id: u32, addr: *const Address, qsize: u32, q: *const c_void, size: *mut u32, out: *mut c_void) -> i32;
    fn AudioObjectSetPropertyData(id: u32, addr: *const Address, qsize: u32, q: *const c_void, size: u32, data: *const c_void) -> i32;
}

const fn fcc(s: &[u8; 4]) -> u32 {
    ((s[0] as u32) << 24) | ((s[1] as u32) << 16) | ((s[2] as u32) << 8) | s[3] as u32
}
const SYSTEM: u32 = 1;
const GLOBAL: u32 = fcc(b"glob");
const INPUT: u32 = fcc(b"inpt");
const DEVICES: u32 = fcc(b"dev#");
const DEFAULT_IN: u32 = fcc(b"dIn ");
const DEFAULT_OUT: u32 = fcc(b"dOut");
const TRANSPORT: u32 = fcc(b"tran");
const UID: u32 = fcc(b"uid ");
const NAME: u32 = fcc(b"lnam");
const STREAM_CONFIG: u32 = fcc(b"slay");

/// The input the app pinned, and the one it replaced: (previous default, pinned device).
static PINNED: Mutex<Option<(u32, u32)>> = Mutex::new(None);

fn addr(selector: u32, scope: u32) -> Address {
    Address { selector, scope, element: 0 }
}

fn get_u32(id: u32, selector: u32, scope: u32) -> Option<u32> {
    let a = addr(selector, scope);
    let mut v = 0u32;
    let mut size = 4u32;
    let st = unsafe { AudioObjectGetPropertyData(id, &a, 0, std::ptr::null(), &mut size, &mut v as *mut u32 as *mut c_void) };
    (st == 0).then_some(v)
}

fn get_string(id: u32, selector: u32) -> Option<String> {
    let a = addr(selector, GLOBAL);
    let mut r: CFStringRef = std::ptr::null();
    let mut size = std::mem::size_of::<CFStringRef>() as u32;
    let st = unsafe { AudioObjectGetPropertyData(id, &a, 0, std::ptr::null(), &mut size, &mut r as *mut _ as *mut c_void) };
    if st != 0 || r.is_null() {
        return None;
    }
    Some(unsafe { CFString::wrap_under_create_rule(r) }.to_string())
}

fn all_devices() -> Vec<u32> {
    let a = addr(DEVICES, GLOBAL);
    let mut size = 0u32;
    if unsafe { AudioObjectGetPropertyDataSize(SYSTEM, &a, 0, std::ptr::null(), &mut size) } != 0 || size == 0 {
        return Vec::new();
    }
    let mut ids = vec![0u32; size as usize / 4];
    let mut sz = size;
    if unsafe { AudioObjectGetPropertyData(SYSTEM, &a, 0, std::ptr::null(), &mut sz, ids.as_mut_ptr() as *mut c_void) } != 0 {
        return Vec::new();
    }
    ids.truncate(sz as usize / 4);
    ids
}

fn has_channels(id: u32, scope: u32) -> bool {
    let a = addr(STREAM_CONFIG, scope);
    let mut size = 0u32;
    if unsafe { AudioObjectGetPropertyDataSize(id, &a, 0, std::ptr::null(), &mut size) } != 0 || size < 8 {
        return false;
    }
    let mut buf = vec![0u8; size as usize];
    let mut sz = size;
    if unsafe { AudioObjectGetPropertyData(id, &a, 0, std::ptr::null(), &mut sz, buf.as_mut_ptr() as *mut c_void) } != 0 {
        return false;
    }
    // AudioBufferList { u32 count; AudioBuffer[count] { u32 channels; u32 bytes; ptr } }
    let n = u32::from_ne_bytes(buf[0..4].try_into().unwrap()) as usize;
    (0..n).any(|i| {
        let o = 8 + i * 16;
        o + 4 <= buf.len() && u32::from_ne_bytes(buf[o..o + 4].try_into().unwrap()) > 0
    })
}

fn transport_name(t: u32) -> &'static str {
    match t {
        x if x == fcc(b"bltn") => "builtin",
        x if x == fcc(b"blue") || x == fcc(b"blea") => "bluetooth",
        x if x == fcc(b"usb ") => "usb",
        x if x == fcc(b"virt") || x == fcc(b"grup") => "virtual",
        _ => "other",
    }
}

fn transport(id: u32) -> &'static str {
    get_u32(id, TRANSPORT, GLOBAL).map(transport_name).unwrap_or("other")
}

/// Inputs, the default routes, and whether Bluetooth is involved.
pub fn route() -> Route {
    let def_in = get_u32(SYSTEM, DEFAULT_IN, GLOBAL).filter(|d| *d != 0);
    let def_out = get_u32(SYSTEM, DEFAULT_OUT, GLOBAL).filter(|d| *d != 0);
    let mut r = Route { pinned: PINNED.lock().unwrap().is_some(), ..Default::default() };
    for id in all_devices() {
        if !has_channels(id, INPUT) {
            continue;
        }
        let (Some(uid), Some(name)) = (get_string(id, UID), get_string(id, NAME)) else { continue };
        let t = transport(id);
        if t == "builtin" && r.builtin_input_uid.is_none() {
            r.builtin_input_uid = Some(uid.clone());
        }
        let is_default = Some(id) == def_in;
        if is_default {
            r.default_input_uid = Some(uid.clone());
            r.default_input_bluetooth = t == "bluetooth";
        }
        r.inputs.push(InputInfo { uid, name, transport: t.into(), is_default });
    }
    if let Some(o) = def_out {
        r.default_output_bluetooth = transport(o) == "bluetooth";
        r.default_output_name = get_string(o, NAME);
    }
    r
}

fn builtin_input_id() -> Option<u32> {
    all_devices().into_iter().find(|id| has_channels(*id, INPUT) && transport(*id) == "builtin")
}

fn set_default_input(id: u32) -> Result<(), String> {
    let a = addr(DEFAULT_IN, GLOBAL);
    let st = unsafe { AudioObjectSetPropertyData(SYSTEM, &a, 0, std::ptr::null(), 4, &id as *const u32 as *const c_void) };
    if st == 0 { Ok(()) } else { Err(format!("could not change the default input (error {st})")) }
}

/// If the default input is a Bluetooth device, switch it to the built-in microphone and remember
/// the original. Returns the name of the microphone now in use, or `None` when nothing changed.
pub fn pin_builtin_input() -> Result<Option<String>, String> {
    let cur = get_u32(SYSTEM, DEFAULT_IN, GLOBAL).filter(|d| *d != 0);
    let Some(cur) = cur else { return Ok(None) };
    if transport(cur) != "bluetooth" {
        return Ok(None);
    }
    let Some(bi) = builtin_input_id() else { return Ok(None) };
    {
        let mut p = PINNED.lock().unwrap();
        // keep the very first original if we are asked twice
        let prev = p.map(|(prev, _)| prev).unwrap_or(cur);
        set_default_input(bi)?;
        *p = Some((prev, bi));
    }
    Ok(get_string(bi, NAME))
}

/// Put the default input back, if the app moved it and nobody else has changed it since.
pub fn restore_input() {
    let Some((prev, pinned)) = PINNED.lock().unwrap().take() else { return };
    let cur = get_u32(SYSTEM, DEFAULT_IN, GLOBAL);
    if cur == Some(pinned) && all_devices().contains(&prev) {
        let _ = set_default_input(prev);
    }
}

#[cfg(test)]
mod tests {
    /// `cargo test -p screencraft audioroute -- --nocapture`: prints this Mac's audio route.
    #[test]
    fn lists_route() {
        let r = super::route();
        println!("{r:#?}");
        assert!(r.default_input_uid.is_some() || r.inputs.is_empty());
    }
}
