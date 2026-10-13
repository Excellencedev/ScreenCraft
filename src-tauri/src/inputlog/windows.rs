//! Input tracker on Windows. The cursor position, buttons and cursor shape are polled
//! at ~120 Hz; keys come from a low-level keyboard hook (no permission needed); the
//! text caret is read from the focused thread's Win32 caret while typing. Positions
//! are converted to the recorded display's logical coordinates (see `monitors.rs`).

use crate::monitors::Display;
use ::windows::Win32::Foundation::{HWND, LPARAM, LRESULT, POINT, WPARAM};
use ::windows::Win32::Graphics::Gdi::ClientToScreen;
use ::windows::Win32::System::LibraryLoader::GetModuleHandleW;
use ::windows::Win32::System::Threading::GetCurrentThreadId;
use ::windows::Win32::UI::Input::KeyboardAndMouse::*;
use ::windows::Win32::UI::WindowsAndMessaging::*;
use screencraft_core::input::{Button, CaretEvent, ClickEvent, CursorKind, InputLog, KeyEvent, MoveEvent};
use screencraft_core::project::Rect;
use std::collections::HashSet;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex};
use std::thread::{self, JoinHandle};
use std::time::{Duration, Instant};

#[derive(Default)]
struct Buffers {
    moves: Vec<(Instant, f64, f64, CursorKind)>,
    clicks: Vec<(Instant, f64, f64, Button, bool)>,
    keys: Vec<(Instant, &'static str, u8, String)>,
    carets: Vec<(Instant, f64, f64, f64, f64)>,
}

/// Modifier bits currently held: 1 = windows, 2 = shift, 4 = alt, 8 = control.
fn held_mods() -> u8 {
    let down = |vk: VIRTUAL_KEY| unsafe { GetAsyncKeyState(vk.0 as i32) } as u16 & 0x8000 != 0;
    let mut m = 0u8;
    if down(VK_LWIN) || down(VK_RWIN) { m |= 1; }
    if down(VK_SHIFT) { m |= 2; }
    if down(VK_MENU) { m |= 4; }
    if down(VK_CONTROL) { m |= 8; }
    m
}

/// Printable name of a key for the on-screen shortcut display. Ordinary typing is never stored
/// as text: only shortcuts (windows / alt / control held) and non-character keys get one.
fn key_label(vk: u32, mods: u8) -> String {
    let v = vk as u16;
    let special = match VIRTUAL_KEY(v) {
        VK_RETURN => Some("Return"),
        VK_TAB => Some("Tab"),
        VK_ESCAPE => Some("Esc"),
        VK_BACK => Some("Backspace"),
        VK_DELETE => Some("Delete"),
        VK_LEFT => Some("←"),
        VK_RIGHT => Some("→"),
        VK_UP => Some("↑"),
        VK_DOWN => Some("↓"),
        VK_HOME => Some("Home"),
        VK_END => Some("End"),
        VK_PRIOR => Some("Page Up"),
        VK_NEXT => Some("Page Down"),
        _ => None,
    };
    if let Some(n) = special {
        return n.to_string();
    }
    if (0x70..=0x7B).contains(&v) {
        return format!("F{}", v - 0x6F);
    }
    if mods & (1 | 4 | 8) != 0 {
        if (0x30..=0x39).contains(&v) || (0x41..=0x5A).contains(&v) {
            return (v as u8 as char).to_string();
        }
        if v == VK_SPACE.0 {
            return "Space".to_string();
        }
    }
    String::new()
}

/// The keyboard hook is a plain function, so it reaches the active buffers here.
static HOOK_TARGET: Mutex<Option<Arc<Mutex<Buffers>>>> = Mutex::new(None);
static HELD: Mutex<Option<HashSet<u32>>> = Mutex::new(None);

pub struct InputTracker {
    stop: Arc<AtomicBool>,
    buffers: Arc<Mutex<Buffers>>,
    poll: Option<JoinHandle<()>>,
    hook: Option<(JoinHandle<()>, u32)>,
}

/// Windows virtual-key codes -> the macOS key codes the typing logic understands.
/// Everything that types a character becomes "0"; modifiers are ignored.
fn key_code(vk: u32) -> Option<&'static str> {
    let vk = VIRTUAL_KEY(vk as u16);
    Some(match vk {
        VK_SHIFT | VK_LSHIFT | VK_RSHIFT | VK_CONTROL | VK_LCONTROL | VK_RCONTROL | VK_MENU | VK_LMENU | VK_RMENU | VK_LWIN
        | VK_RWIN | VK_CAPITAL | VK_NUMLOCK | VK_SCROLL | VK_APPS => return None,
        VK_BACK => "51",
        VK_RETURN => "36",
        VK_TAB => "48",
        VK_ESCAPE => "53",
        VK_LEFT => "123",
        VK_RIGHT => "124",
        VK_DOWN => "125",
        VK_UP => "126",
        VK_DELETE => "117",
        _ => "0",
    })
}

unsafe extern "system" fn keyboard_hook(code: i32, wparam: WPARAM, lparam: LPARAM) -> LRESULT {
    if code == HC_ACTION as i32 {
        let info = unsafe { &*(lparam.0 as *const KBDLLHOOKSTRUCT) };
        let msg = wparam.0 as u32;
        let mut held = HELD.lock().unwrap();
        let held = held.get_or_insert_with(Default::default);
        if msg == WM_KEYDOWN || msg == WM_SYSKEYDOWN {
            // held keys auto-repeat; only the first press counts
            if held.insert(info.vkCode) {
                if let (Some(k), Some(b)) = (key_code(info.vkCode), HOOK_TARGET.lock().unwrap().as_ref()) {
                    let mods = held_mods();
                    let label = key_label(info.vkCode, mods);
                    b.lock().unwrap().keys.push((Instant::now(), k, mods, label));
                }
            }
        } else if msg == WM_KEYUP || msg == WM_SYSKEYUP {
            held.remove(&info.vkCode);
        }
    }
    unsafe { CallNextHookEx(None, code, wparam, lparam) }
}

fn hook_loop(ready: std::sync::mpsc::Sender<u32>) {
    unsafe {
        let module = GetModuleHandleW(None).ok();
        let hook = SetWindowsHookExW(WH_KEYBOARD_LL, Some(keyboard_hook), module.map(Into::into), 0);
        let _ = ready.send(GetCurrentThreadId());
        let Ok(hook) = hook else {
            eprintln!("[screencraft] keyboard hook unavailable");
            return;
        };
        let mut msg = MSG::default();
        while GetMessageW(&mut msg, None, 0, 0).as_bool() {
            let _ = TranslateMessage(&msg);
            DispatchMessageW(&msg);
        }
        let _ = UnhookWindowsHookEx(hook);
    }
}

fn cursor_kinds() -> Vec<(isize, CursorKind)> {
    let pairs = [
        (IDC_ARROW, CursorKind::Arrow),
        (IDC_IBEAM, CursorKind::IBeam),
        (IDC_HAND, CursorKind::Pointer),
        (IDC_CROSS, CursorKind::Crosshair),
        (IDC_SIZEALL, CursorKind::OpenHand),
        (IDC_SIZEWE, CursorKind::ResizeLeftRight),
        (IDC_SIZENS, CursorKind::ResizeUpDown),
        (IDC_NO, CursorKind::NotAllowed),
    ];
    pairs
        .iter()
        .filter_map(|(id, k)| unsafe { LoadCursorW(None, *id).ok().map(|h| (h.0 as isize, *k)) })
        .collect()
}

fn current_cursor(kinds: &[(isize, CursorKind)]) -> CursorKind {
    unsafe {
        let mut ci = CURSORINFO { cbSize: std::mem::size_of::<CURSORINFO>() as u32, ..Default::default() };
        if GetCursorInfo(&mut ci).is_err() {
            return CursorKind::Arrow;
        }
        kinds.iter().find(|(h, _)| *h == ci.hCursor.0 as isize).map(|(_, k)| *k).unwrap_or(CursorKind::Arrow)
    }
}

/// The focused thread's caret in physical screen pixels, when the app has one.
fn caret() -> Option<(f64, f64, f64, f64)> {
    unsafe {
        let fg = GetForegroundWindow();
        let tid = GetWindowThreadProcessId(fg, None);
        let mut gui = GUITHREADINFO { cbSize: std::mem::size_of::<GUITHREADINFO>() as u32, ..Default::default() };
        GetGUIThreadInfo(tid, &mut gui).ok()?;
        if gui.hwndCaret == HWND::default() {
            return None;
        }
        let r = gui.rcCaret;
        let mut p = POINT { x: r.left, y: r.top };
        if !ClientToScreen(gui.hwndCaret, &mut p).as_bool() {
            return None;
        }
        Some((p.x as f64, p.y as f64, (r.right - r.left).max(1) as f64, (r.bottom - r.top).max(1) as f64))
    }
}

fn poll_loop(stop: Arc<AtomicBool>, buffers: Arc<Mutex<Buffers>>, display: Display) {
    let kinds = cursor_kinds();
    let mut last_pos = (i32::MIN, i32::MIN);
    let mut kind = CursorKind::Arrow;
    let (mut left, mut right) = (false, false);
    let mut tick = 0u32;
    while !stop.load(Ordering::SeqCst) {
        let now = Instant::now();
        let mut p = POINT::default();
        if unsafe { GetCursorPos(&mut p) }.is_ok() {
            // cursor shape is comparatively expensive; refresh at ~30 Hz
            if tick % 4 == 0 {
                kind = current_cursor(&kinds);
            }
            let (x, y) = display.to_logical(p.x as f64, p.y as f64);
            let down = |vk: VIRTUAL_KEY| unsafe { GetAsyncKeyState(vk.0 as i32) } as u16 & 0x8000 != 0;
            let (l, r) = (down(VK_LBUTTON), down(VK_RBUTTON));
            let mut b = buffers.lock().unwrap();
            let kind_changed = b.moves.last().map(|m| m.3 != kind).unwrap_or(true);
            if (p.x, p.y) != last_pos || kind_changed {
                b.moves.push((now, x, y, kind));
                last_pos = (p.x, p.y);
            }
            if l != left {
                left = l;
                b.clicks.push((now, x, y, Button::Left, l));
            }
            if r != right {
                right = r;
                b.clicks.push((now, x, y, Button::Right, r));
            }
        }
        // while typing, follow the text caret (~16 Hz)
        if tick % 7 == 0 {
            let typing = buffers.lock().unwrap().keys.last().map(|k| now.duration_since(k.0) < Duration::from_secs(2)).unwrap_or(false);
            if typing {
                if let Some((x, y, w, h)) = caret() {
                    let (lx, ly) = display.to_logical(x, y);
                    buffers.lock().unwrap().carets.push((now, lx, ly, w / display.scale, h / display.scale));
                }
            }
        }
        tick = tick.wrapping_add(1);
        thread::sleep(Duration::from_millis(8));
    }
}

impl InputTracker {
    /// Start logging; positions are mapped through `display`'s coordinate space.
    pub fn start(display: Display) -> Self {
        let stop = Arc::new(AtomicBool::new(false));
        let buffers = Arc::new(Mutex::new(Buffers::default()));
        *HOOK_TARGET.lock().unwrap() = Some(buffers.clone());
        let (s, b) = (stop.clone(), buffers.clone());
        let poll = thread::Builder::new().name("screencraft-input".into()).spawn(move || poll_loop(s, b, display)).ok();
        let (tx, rx) = std::sync::mpsc::channel();
        let hook = thread::Builder::new()
            .name("screencraft-keys".into())
            .spawn(move || hook_loop(tx))
            .ok()
            .and_then(|h| rx.recv_timeout(Duration::from_secs(2)).ok().map(|tid| (h, tid)));
        Self { stop, buffers, poll, hook }
    }

    /// Stop logging and convert the log to times relative to `zero`.
    pub fn finish(mut self, zero: Instant, bounds: Rect) -> InputLog {
        self.stop.store(true, Ordering::SeqCst);
        if let Some((h, tid)) = self.hook.take() {
            unsafe {
                let _ = PostThreadMessageW(tid, WM_QUIT, WPARAM(0), LPARAM(0));
            }
            let _ = h.join();
        }
        if let Some(t) = self.poll.take() {
            let _ = t.join();
        }
        *HOOK_TARGET.lock().unwrap() = None;
        let buf = std::mem::take(&mut *self.buffers.lock().unwrap());
        let rel = |at: Instant| at.saturating_duration_since(zero).as_secs_f64() * 1000.0;
        InputLog {
            bounds,
            moves: buf.moves.iter().map(|m| MoveEvent { t: rel(m.0), x: m.1, y: m.2, cursor: m.3 }).collect(),
            clicks: buf.clicks.iter().map(|c| ClickEvent { t: rel(c.0), x: c.1, y: c.2, button: c.3, down: c.4 }).collect(),
            keys: buf.keys.iter().map(|k| KeyEvent { t: rel(k.0), key: k.1.to_string(), down: true, mods: k.2, label: k.3.clone() }).collect(),
            carets: buf.carets.iter().map(|c| CaretEvent { t: rel(c.0), x: c.1, y: c.2, w: c.3, h: c.4 }).collect(),
        }
    }
}
