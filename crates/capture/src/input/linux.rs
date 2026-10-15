//! Input logging on Linux.
//!
//! * Pointer position while recording comes from the PipeWire stream's cursor metadata
//!   (works on Wayland and X11), pushed in by the capture backend.
//! * Clicks and key presses come from X11 (`rdev`, XRecord). Wayland has no global input
//!   hook for ordinary applications, so on a pure Wayland session those stay empty and the
//!   recording simply has no click / typing based auto-zoom.
//!
//! Key *contents* are never stored; shortcuts get a label for the on-screen key display.

use screencraft_core::input::{Button, ClickEvent, CursorKind, InputLog, KeyEvent, MoveEvent};
use screencraft_core::project::Rect;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex, OnceLock};
use std::time::Instant;

#[derive(Default)]
struct Buffers {
    /// (time, normalised x, normalised y) in the captured frame.
    cursor: Vec<(Instant, f64, f64)>,
    /// (time, x, y) in X11 screen pixels, from the global listener.
    pointer: Vec<(Instant, f64, f64)>,
    clicks: Vec<(Instant, f64, f64, Button, bool)>,
    keys: Vec<(Instant, u8, String, String)>,
}

static TARGET: Mutex<Option<Arc<Mutex<Buffers>>>> = Mutex::new(None);
static LISTENER: OnceLock<()> = OnceLock::new();
static MODS: Mutex<u8> = Mutex::new(0);

/// Called by the PipeWire backend for every frame that carries cursor metadata.
/// `nx`, `ny` are 0..1 across the captured picture.
pub(crate) fn push_cursor(at: Instant, nx: f64, ny: f64) {
    if let Some(b) = TARGET.lock().unwrap().as_ref() {
        b.lock().unwrap().cursor.push((at, nx, ny));
    }
}

fn key_label(key: rdev::Key, mods: u8) -> String {
    use rdev::Key::*;
    let special = match key {
        Return | KpReturn => Some("Return"),
        Tab => Some("Tab"),
        Escape => Some("Esc"),
        Backspace => Some("Backspace"),
        Delete => Some("Delete"),
        LeftArrow => Some("←"),
        RightArrow => Some("→"),
        UpArrow => Some("↑"),
        DownArrow => Some("↓"),
        Home => Some("Home"),
        End => Some("End"),
        PageUp => Some("Page Up"),
        PageDown => Some("Page Down"),
        F1 => Some("F1"),
        F2 => Some("F2"),
        F3 => Some("F3"),
        F4 => Some("F4"),
        F5 => Some("F5"),
        F6 => Some("F6"),
        F7 => Some("F7"),
        F8 => Some("F8"),
        F9 => Some("F9"),
        F10 => Some("F10"),
        F11 => Some("F11"),
        F12 => Some("F12"),
        _ => None,
    };
    if let Some(s) = special {
        return s.to_string();
    }
    // Only shortcuts (super / alt / ctrl held) are labelled; plain typing never is.
    if mods & (1 | 4 | 8) == 0 {
        return String::new();
    }
    let s = match key {
        KeyA => "A", KeyB => "B", KeyC => "C", KeyD => "D", KeyE => "E", KeyF => "F", KeyG => "G",
        KeyH => "H", KeyI => "I", KeyJ => "J", KeyK => "K", KeyL => "L", KeyM => "M", KeyN => "N",
        KeyO => "O", KeyP => "P", KeyQ => "Q", KeyR => "R", KeyS => "S", KeyT => "T", KeyU => "U",
        KeyV => "V", KeyW => "W", KeyX => "X", KeyY => "Y", KeyZ => "Z",
        Num0 => "0", Num1 => "1", Num2 => "2", Num3 => "3", Num4 => "4", Num5 => "5", Num6 => "6",
        Num7 => "7", Num8 => "8", Num9 => "9", Space => "Space",
        _ => "",
    };
    s.to_string()
}

fn mod_bit(key: rdev::Key) -> u8 {
    use rdev::Key::*;
    match key {
        MetaLeft | MetaRight => 1,
        ShiftLeft | ShiftRight => 2,
        Alt | AltGr => 4,
        ControlLeft | ControlRight => 8,
        _ => 0,
    }
}

fn start_listener() {
    LISTENER.get_or_init(|| {
        let _ = std::thread::Builder::new().name("screencraft-xinput".into()).spawn(|| {
            // Blocks for the life of the process; fails immediately without an X server.
            let mut last = (0.0f64, 0.0f64);
            let res = rdev::listen(move |ev| {
                let Some(b) = TARGET.lock().unwrap().clone() else { return };
                let now = Instant::now();
                match ev.event_type {
                    rdev::EventType::MouseMove { x, y } => {
                        last = (x, y);
                        b.lock().unwrap().pointer.push((now, x, y));
                    }
                    rdev::EventType::ButtonPress(btn) | rdev::EventType::ButtonRelease(btn) => {
                        let down = matches!(ev.event_type, rdev::EventType::ButtonPress(_));
                        let button = match btn {
                            rdev::Button::Left => Button::Left,
                            rdev::Button::Right => Button::Right,
                            _ => return,
                        };
                        b.lock().unwrap().clicks.push((now, last.0, last.1, button, down));
                    }
                    rdev::EventType::KeyPress(k) => {
                        let bit = mod_bit(k);
                        let mut m = MODS.lock().unwrap();
                        if bit != 0 {
                            *m |= bit;
                        } else {
                            let label = key_label(k, *m);
                            let code = if matches!(k, rdev::Key::Return | rdev::Key::KpReturn) {
                                "36"
                            } else if matches!(k, rdev::Key::Tab) {
                                "48"
                            } else if matches!(k, rdev::Key::Backspace) {
                                "51"
                            } else {
                                "0"
                            };
                            b.lock().unwrap().keys.push((now, *m, code.to_string(), label));
                        }
                    }
                    rdev::EventType::KeyRelease(k) => {
                        let bit = mod_bit(k);
                        if bit != 0 {
                            *MODS.lock().unwrap() &= !bit;
                        }
                    }
                    _ => {}
                }
            });
            if res.is_err() {
                eprintln!("[screencraft] global input hook unavailable (Wayland session?)");
            }
        });
    });
}

pub struct InputTracker {
    buffers: Arc<Mutex<Buffers>>,
    done: Arc<AtomicBool>,
}

impl InputTracker {
    pub fn start(_display_id: Option<u32>) -> Self {
        let buffers = Arc::new(Mutex::new(Buffers::default()));
        *TARGET.lock().unwrap() = Some(buffers.clone());
        *MODS.lock().unwrap() = 0;
        start_listener();
        Self { buffers, done: Arc::new(AtomicBool::new(false)) }
    }

    pub fn finish(self, zero: Instant, bounds: Rect) -> InputLog {
        self.done.store(true, Ordering::SeqCst);
        *TARGET.lock().unwrap() = None;
        let buf = std::mem::take(&mut *self.buffers.lock().unwrap());
        let rel = |at: Instant| at.saturating_duration_since(zero).as_secs_f64() * 1000.0;
        // Prefer stream cursor metadata (exact for the captured area); fall back to X11 pointer.
        let moves: Vec<MoveEvent> = if !buf.cursor.is_empty() {
            buf.cursor
                .iter()
                .map(|&(t, nx, ny)| MoveEvent {
                    t: rel(t),
                    x: bounds.x + nx * bounds.width,
                    y: bounds.y + ny * bounds.height,
                    cursor: CursorKind::Arrow,
                })
                .collect()
        } else {
            buf.pointer.iter().map(|&(t, x, y)| MoveEvent { t: rel(t), x, y, cursor: CursorKind::Arrow }).collect()
        };
        InputLog {
            bounds,
            moves,
            clicks: buf
                .clicks
                .iter()
                .map(|c| ClickEvent { t: rel(c.0), x: c.1, y: c.2, button: c.3, down: c.4 })
                .collect(),
            keys: buf
                .keys
                .iter()
                .map(|k| KeyEvent { t: rel(k.0), key: k.2.clone(), down: true, mods: k.1, label: k.3.clone() })
                .collect(),
            carets: Vec::new(),
        }
    }
}
