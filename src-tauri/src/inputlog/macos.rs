//! Global mouse tracker. Polls the cursor position, button state and the
//! current system cursor shape at ~120 Hz. None of this needs the
//! Accessibility permission.

#![allow(deprecated)]

use core_graphics::event::CGEvent;
use core_graphics::event_source::{CGEventSource, CGEventSourceStateID};
use objc2::runtime::NSObjectProtocol;
use objc2_app_kit::NSCursor;

use screencraft_core::input::{Button, CaretEvent, ClickEvent, CursorKind, InputLog, KeyEvent, MoveEvent};
use screencraft_core::project::Rect;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex};
use std::thread::{self, JoinHandle};
use std::time::{Duration, Instant};

#[link(name = "CoreGraphics", kind = "framework")]
extern "C" {
    fn CGEventSourceButtonState(state_id: i32, button: u32) -> bool;
}

const COMBINED_SESSION: i32 = 0;
/// Modifier-only key codes never count as typing.
const MODIFIERS: [u16; 10] = [54, 55, 56, 57, 58, 59, 60, 61, 62, 63];

fn button_down(button: u32) -> bool {
    unsafe { CGEventSourceButtonState(COMBINED_SESSION, button) }
}

fn current_cursor_kind() -> CursorKind {
    let Some(cur) = NSCursor::currentSystemCursor() else { return CursorKind::Arrow };
    let eq = |other: objc2::rc::Retained<NSCursor>| cur.isEqual(Some(&*other));
    if eq(NSCursor::IBeamCursor()) || eq(NSCursor::IBeamCursorForVerticalLayout()) {
        CursorKind::IBeam
    } else if eq(NSCursor::pointingHandCursor()) {
        CursorKind::Pointer
    } else if eq(NSCursor::crosshairCursor()) {
        CursorKind::Crosshair
    } else if eq(NSCursor::openHandCursor()) {
        CursorKind::OpenHand
    } else if eq(NSCursor::closedHandCursor()) {
        CursorKind::ClosedHand
    } else if eq(NSCursor::resizeLeftRightCursor()) {
        CursorKind::ResizeLeftRight
    } else if eq(NSCursor::resizeUpDownCursor()) {
        CursorKind::ResizeUpDown
    } else if eq(NSCursor::operationNotAllowedCursor()) {
        CursorKind::NotAllowed
    } else {
        CursorKind::Arrow
    }
}

#[derive(Clone, Copy)]
struct Sample {
    at: Instant,
    x: f64,
    y: f64,
    cursor: CursorKind,
}

#[derive(Clone, Copy)]
struct Click {
    at: Instant,
    x: f64,
    y: f64,
    button: Button,
    down: bool,
}

#[derive(Clone, Copy)]
struct Key {
    at: Instant,
    code: u16,
}

#[derive(Clone, Copy)]
struct Caret {
    at: Instant,
    x: f64,
    y: f64,
    w: f64,
    h: f64,
}

#[derive(Default)]
struct Buffers {
    samples: Vec<Sample>,
    clicks: Vec<Click>,
    keys: Vec<Key>,
    carets: Vec<Caret>,
}

pub struct InputTracker {
    stop: Arc<AtomicBool>,
    buffers: Arc<Mutex<Buffers>>,
    thread: Option<JoinHandle<()>>,
    key_thread: Option<JoinHandle<()>>,
}

/// Keyboard activity through a listen-only event tap (needs Input Monitoring).
fn key_tap_loop(stop: Arc<AtomicBool>, buffers: Arc<Mutex<Buffers>>) {
    use core_foundation::runloop::{kCFRunLoopDefaultMode, CFRunLoop};
    use core_graphics::event::{
        CGEventTap, CGEventTapLocation, CGEventTapOptions, CGEventTapPlacement, CGEventType, CallbackResult, EventField,
    };
    let b = buffers.clone();
    let res = CGEventTap::with_enabled(
        CGEventTapLocation::Session,
        CGEventTapPlacement::HeadInsertEventTap,
        CGEventTapOptions::ListenOnly,
        vec![CGEventType::KeyDown],
        move |_, _, ev| {
            if ev.get_integer_value_field(EventField::KEYBOARD_EVENT_AUTOREPEAT) == 0 {
                let code = ev.get_integer_value_field(EventField::KEYBOARD_EVENT_KEYCODE) as u16;
                if !MODIFIERS.contains(&code) {
                    b.lock().unwrap().keys.push(Key { at: Instant::now(), code });
                }
            }
            CallbackResult::Keep
        },
        || {
            while !stop.load(Ordering::SeqCst) {
                unsafe {
                    CFRunLoop::run_in_mode(kCFRunLoopDefaultMode, Duration::from_millis(100), false);
                }
            }
        },
    );
    if res.is_err() {
        eprintln!("[screencraft] keyboard tap unavailable (Input Monitoring not granted)");
    }
}

impl InputTracker {
    pub fn start() -> Self {
        let stop = Arc::new(AtomicBool::new(false));
        let buffers = Arc::new(Mutex::new(Buffers::default()));
        let (s, b) = (stop.clone(), buffers.clone());
        let thread = thread::Builder::new()
            .name("screencraft-input".into())
            .spawn(move || poll_loop(s, b))
            .ok();
        let (s2, b2) = (stop.clone(), buffers.clone());
        let key_thread = thread::Builder::new()
            .name("screencraft-keys".into())
            .spawn(move || key_tap_loop(s2, b2))
            .ok();
        Self { stop, buffers, thread, key_thread }
    }

    /// Stop polling and convert the log to times relative to `zero`.
    pub fn finish(mut self, zero: Instant, bounds: Rect) -> InputLog {
        self.stop.store(true, Ordering::SeqCst);
        if let Some(t) = self.thread.take() {
            let _ = t.join();
        }
        if let Some(t) = self.key_thread.take() {
            let _ = t.join();
        }
        let buf = std::mem::take(&mut *self.buffers.lock().unwrap());
        let rel = |at: Instant| at.saturating_duration_since(zero).as_secs_f64() * 1000.0;
        InputLog {
            bounds,
            moves: buf
                .samples
                .iter()
                .map(|s| MoveEvent { t: rel(s.at), x: s.x, y: s.y, cursor: s.cursor })
                .collect(),
            clicks: buf
                .clicks
                .iter()
                .map(|c| ClickEvent { t: rel(c.at), x: c.x, y: c.y, button: c.button, down: c.down })
                .collect(),
            keys: buf
                .keys
                .iter()
                .map(|k| KeyEvent { t: rel(k.at), key: k.code.to_string(), down: true })
                .collect(),
            carets: buf
                .carets
                .iter()
                .map(|c| CaretEvent { t: rel(c.at), x: c.x, y: c.y, w: c.w, h: c.h })
                .collect(),
        }
    }
}

fn poll_loop(stop: Arc<AtomicBool>, buffers: Arc<Mutex<Buffers>>) {
    let Ok(source) = CGEventSource::new(CGEventSourceStateID::CombinedSessionState) else { return };
    let mut last_pos = (f64::NAN, f64::NAN);
    let mut last_kind = CursorKind::Arrow;
    let mut left = false;
    let mut right = false;
    let mut tick = 0u32;
    let mut finder = crate::ax::CaretFinder::default();
    let ax_ok = crate::ax::trusted();
    while !stop.load(Ordering::SeqCst) {
        let now = Instant::now();
        if let Ok(ev) = CGEvent::new(source.clone()) {
            let p = ev.location();
            // Cursor shape is comparatively expensive; refresh at ~30 Hz.
            if tick % 4 == 0 {
                last_kind = current_cursor_kind();
            }
            let (l, r) = (button_down(0), button_down(1));
            let mut b = buffers.lock().unwrap();
            let moved = p.x != last_pos.0 || p.y != last_pos.1;
            let kind_changed = b.samples.last().map(|s| s.cursor != last_kind).unwrap_or(true);
            if moved || kind_changed || b.samples.is_empty() {
                b.samples.push(Sample { at: now, x: p.x, y: p.y, cursor: last_kind });
                last_pos = (p.x, p.y);
            }
            if l != left {
                left = l;
                b.clicks.push(Click { at: now, x: p.x, y: p.y, button: Button::Left, down: l });
            }
            if r != right {
                right = r;
                b.clicks.push(Click { at: now, x: p.x, y: p.y, button: Button::Right, down: r });
            }
        }
        // While typing, follow the text caret (~16 Hz).
        if ax_ok && tick % 7 == 0 {
            let typing = buffers
                .lock()
                .unwrap()
                .keys
                .last()
                .map(|k| now.duration_since(k.at) < Duration::from_millis(2000))
                .unwrap_or(false);
            if typing {
                if let Some((x, y, w, h)) = finder.caret() {
                    buffers.lock().unwrap().carets.push(Caret { at: now, x, y, w, h });
                }
            }
        }
        tick = tick.wrapping_add(1);
        thread::sleep(Duration::from_micros(8000));
    }
}
