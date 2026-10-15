//! Displays and windows on Windows, in one coordinate convention for the whole app.
//!
//! Each display has a physical rectangle (desktop pixels) and a logical one: its
//! physical origin divided by its own scale, and its size in logical pixels. This is
//! what Tauri calls a logical position, and it plays the role of macOS points: the
//! source list, the area picker, the recording bounds and the input log all use it.

use ::windows::core::BOOL;
use ::windows::Win32::Foundation::{HWND, LPARAM, RECT};
use ::windows::Win32::Graphics::Dwm::{DwmGetWindowAttribute, DWMWA_CLOAKED, DWMWA_EXTENDED_FRAME_BOUNDS};
use ::windows::Win32::Graphics::Gdi::{EnumDisplayMonitors, GetMonitorInfoW, MonitorFromWindow, HDC, HMONITOR, MONITORINFO, MONITOR_DEFAULTTONEAREST};
use ::windows::Win32::UI::HiDpi::{GetDpiForMonitor, MDT_EFFECTIVE_DPI};
use ::windows::Win32::UI::WindowsAndMessaging::*;

#[derive(Clone, Copy, Debug)]
pub struct Display {
    /// Position in the list (primary display first).
    pub id: u32,
    pub handle: HMONITOR,
    /// Physical rectangle in desktop pixels.
    pub phys: (i32, i32, i32, i32),
    pub scale: f64,
    pub is_primary: bool,
}

// HMONITOR is a plain handle value
unsafe impl Send for Display {}

impl Display {
    /// Logical origin and size.
    pub fn logical(&self) -> (f64, f64, f64, f64) {
        let (x, y, w, h) = self.phys;
        (x as f64 / self.scale, y as f64 / self.scale, w as f64 / self.scale, h as f64 / self.scale)
    }

    /// Physical desktop point -> logical point (relative to this display's mapping).
    pub fn to_logical(&self, px: f64, py: f64) -> (f64, f64) {
        let (lx, ly, _, _) = self.logical();
        (lx + (px - self.phys.0 as f64) / self.scale, ly + (py - self.phys.1 as f64) / self.scale)
    }

    /// Logical point -> physical desktop point.
    pub fn to_physical(&self, x: f64, y: f64) -> (f64, f64) {
        let (lx, ly, _, _) = self.logical();
        (self.phys.0 as f64 + (x - lx) * self.scale, self.phys.1 as f64 + (y - ly) * self.scale)
    }
}

fn info(handle: HMONITOR) -> Option<(RECT, bool, f64)> {
    unsafe {
        let mut mi = MONITORINFO { cbSize: std::mem::size_of::<MONITORINFO>() as u32, ..Default::default() };
        if !GetMonitorInfoW(handle, &mut mi).as_bool() {
            return None;
        }
        let (mut dx, mut dy) = (96u32, 96u32);
        let _ = GetDpiForMonitor(handle, MDT_EFFECTIVE_DPI, &mut dx, &mut dy);
        Some((mi.rcMonitor, mi.dwFlags & 1 != 0, dx.max(48) as f64 / 96.0))
    }
}

/// All displays, the primary one first.
pub fn displays() -> Vec<Display> {
    unsafe extern "system" fn each(m: HMONITOR, _: HDC, _: *mut RECT, data: LPARAM) -> BOOL {
        unsafe { (*(data.0 as *mut Vec<HMONITOR>)).push(m) };
        true.into()
    }
    let mut handles: Vec<HMONITOR> = Vec::new();
    unsafe {
        let _ = EnumDisplayMonitors(None, None, Some(each), LPARAM(&mut handles as *mut _ as isize));
    }
    let mut list: Vec<Display> = handles
        .into_iter()
        .filter_map(|h| {
            let (r, primary, scale) = info(h)?;
            Some(Display { id: 0, handle: h, phys: (r.left, r.top, r.right - r.left, r.bottom - r.top), scale, is_primary: primary })
        })
        .collect();
    list.sort_by_key(|d| !d.is_primary);
    for (i, d) in list.iter_mut().enumerate() {
        d.id = i as u32;
    }
    list
}

pub fn display(id: Option<u32>) -> Option<Display> {
    let all = displays();
    match id {
        Some(id) => all.into_iter().find(|d| d.id == id),
        None => all.into_iter().next(),
    }
}

/// The display a window is (mostly) on.
pub fn display_of_window(hwnd: HWND) -> Option<Display> {
    let m = unsafe { MonitorFromWindow(hwnd, MONITOR_DEFAULTTONEAREST) };
    displays().into_iter().find(|d| d.handle == m)
}

#[derive(Clone, Debug)]
pub struct WindowEntry {
    pub hwnd: HWND,
    pub title: String,
    pub app: String,
    /// Physical frame without the invisible resize borders.
    pub phys: (i32, i32, i32, i32),
}

/// The visible frame of a window in physical pixels.
pub fn window_frame(hwnd: HWND) -> Option<(i32, i32, i32, i32)> {
    unsafe {
        let mut r = RECT::default();
        if DwmGetWindowAttribute(hwnd, DWMWA_EXTENDED_FRAME_BOUNDS, &mut r as *mut _ as *mut _, std::mem::size_of::<RECT>() as u32).is_err() {
            GetWindowRect(hwnd, &mut r).ok()?;
        }
        Some((r.left, r.top, r.right - r.left, r.bottom - r.top))
    }
}

fn exe_name(pid: u32) -> String {
    use ::windows::Win32::System::Threading::{OpenProcess, QueryFullProcessImageNameW, PROCESS_NAME_WIN32, PROCESS_QUERY_LIMITED_INFORMATION};
    unsafe {
        let Ok(h) = OpenProcess(PROCESS_QUERY_LIMITED_INFORMATION, false, pid) else { return String::new() };
        let mut buf = [0u16; 512];
        let mut len = buf.len() as u32;
        let ok = QueryFullProcessImageNameW(h, PROCESS_NAME_WIN32, ::windows::core::PWSTR(buf.as_mut_ptr()), &mut len).is_ok();
        let _ = ::windows::Win32::Foundation::CloseHandle(h);
        if !ok {
            return String::new();
        }
        let path = String::from_utf16_lossy(&buf[..len as usize]);
        let file = path.rsplit('\\').next().unwrap_or(&path);
        file.strip_suffix(".exe").or_else(|| file.strip_suffix(".EXE")).unwrap_or(file).to_string()
    }
}

/// Top-level windows worth recording: visible, titled, not cloaked (other virtual
/// desktops, suspended UWP apps), not tool windows, not ours.
pub fn windows() -> Vec<WindowEntry> {
    unsafe extern "system" fn each(h: HWND, data: LPARAM) -> BOOL {
        unsafe { (*(data.0 as *mut Vec<HWND>)).push(h) };
        true.into()
    }
    let mut handles: Vec<HWND> = Vec::new();
    unsafe {
        let _ = EnumWindows(Some(each), LPARAM(&mut handles as *mut _ as isize));
    }
    let me = std::process::id();
    handles
        .into_iter()
        .filter_map(|h| unsafe {
            if !IsWindowVisible(h).as_bool() || IsIconic(h).as_bool() {
                return None;
            }
            let ex = GetWindowLongW(h, GWL_EXSTYLE) as u32;
            if ex & WS_EX_TOOLWINDOW.0 != 0 {
                return None;
            }
            let mut cloaked = 0u32;
            let _ = DwmGetWindowAttribute(h, DWMWA_CLOAKED, &mut cloaked as *mut _ as *mut _, 4);
            if cloaked != 0 {
                return None;
            }
            let mut pid = 0u32;
            GetWindowThreadProcessId(h, Some(&mut pid));
            if pid == me {
                return None;
            }
            let mut buf = [0u16; 512];
            let n = GetWindowTextW(h, &mut buf);
            if n <= 0 {
                return None;
            }
            let phys = window_frame(h)?;
            Some(WindowEntry { hwnd: h, title: String::from_utf16_lossy(&buf[..n as usize]), app: exe_name(pid), phys })
        })
        .collect()
}
