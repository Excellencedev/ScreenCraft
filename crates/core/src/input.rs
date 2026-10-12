//! Cursor / click / keyboard log captured next to the screen video.
//! All times are milliseconds since the first video frame; positions are in
//! global screen points (origin top-left).

use crate::project::Rect;
use serde::{Deserialize, Serialize};

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize, Default)]
#[serde(rename_all = "camelCase")]
pub enum CursorKind {
    #[default]
    Arrow,
    IBeam,
    Pointer,
    Crosshair,
    OpenHand,
    ClosedHand,
    ResizeLeftRight,
    ResizeUpDown,
    NotAllowed,
}

impl CursorKind {
    pub const ALL: [CursorKind; 9] = [
        CursorKind::Arrow,
        CursorKind::IBeam,
        CursorKind::Pointer,
        CursorKind::Crosshair,
        CursorKind::OpenHand,
        CursorKind::ClosedHand,
        CursorKind::ResizeLeftRight,
        CursorKind::ResizeUpDown,
        CursorKind::NotAllowed,
    ];

    pub fn index(self) -> usize {
        Self::ALL.iter().position(|k| *k == self).unwrap_or(0)
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Serialize, Deserialize)]
pub struct MoveEvent {
    pub t: f64,
    pub x: f64,
    pub y: f64,
    #[serde(default)]
    pub cursor: CursorKind,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum Button {
    Left,
    Right,
    Other,
}

#[derive(Clone, Copy, Debug, PartialEq, Serialize, Deserialize)]
pub struct ClickEvent {
    pub t: f64,
    pub x: f64,
    pub y: f64,
    pub button: Button,
    pub down: bool,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct KeyEvent {
    pub t: f64,
    pub key: String,
    pub down: bool,
}

/// Text caret rectangle (global points) sampled while typing.
#[derive(Clone, Copy, Debug, PartialEq, Serialize, Deserialize)]
pub struct CaretEvent {
    pub t: f64,
    pub x: f64,
    pub y: f64,
    pub w: f64,
    pub h: f64,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct InputLog {
    /// Captured area in global points.
    pub bounds: Rect,
    pub moves: Vec<MoveEvent>,
    pub clicks: Vec<ClickEvent>,
    pub keys: Vec<KeyEvent>,
    pub carets: Vec<CaretEvent>,
}

impl Default for InputLog {
    fn default() -> Self {
        Self {
            bounds: Rect { x: 0.0, y: 0.0, width: 1.0, height: 1.0 },
            moves: Vec::new(),
            clicks: Vec::new(),
            keys: Vec::new(),
            carets: Vec::new(),
        }
    }
}

impl InputLog {
    /// Convert a global point to 0..1 coordinates inside the captured area.
    pub fn normalize(&self, x: f64, y: f64) -> (f64, f64) {
        (
            (x - self.bounds.x) / self.bounds.width.max(1.0),
            (y - self.bounds.y) / self.bounds.height.max(1.0),
        )
    }

    /// Normalised cursor position at time `t` (linear interpolation).
    pub fn cursor_at(&self, t: f64) -> Option<(f64, f64, CursorKind)> {
        let m = &self.moves;
        if m.is_empty() {
            return None;
        }
        let i = m.partition_point(|e| e.t <= t);
        let (x, y, k) = if i == 0 {
            (m[0].x, m[0].y, m[0].cursor)
        } else if i >= m.len() {
            let l = m[m.len() - 1];
            (l.x, l.y, l.cursor)
        } else {
            let (a, b) = (m[i - 1], m[i]);
            let f = if b.t > a.t { (t - a.t) / (b.t - a.t) } else { 0.0 };
            (a.x + (b.x - a.x) * f, a.y + (b.y - a.y) * f, a.cursor)
        };
        let (nx, ny) = self.normalize(x, y);
        Some((nx, ny, k))
    }

    /// True when a key was pressed within `window_ms` before `t`.
    pub fn typing_at(&self, t: f64, window_ms: f64) -> bool {
        let i = self.keys.partition_point(|k| k.t <= t);
        i > 0 && t - self.keys[i - 1].t <= window_ms
    }

    /// True when a key will be pressed within `window_ms` after `t`.
    pub fn typing_ahead(&self, t: f64, window_ms: f64) -> bool {
        let i = self.keys.partition_point(|k| k.t <= t);
        i < self.keys.len() && self.keys[i].t - t <= window_ms
    }

    /// Caret position to look at around `t`: the latest sample before `t`, or —
    /// when typing is about to start — the first sample just after it.
    pub fn caret_near(&self, t: f64) -> Option<(f64, f64)> {
        if let Some(c) = self.caret_at(t) {
            return Some(c);
        }
        let i = self.carets.partition_point(|c| c.t <= t);
        let c = self.carets.get(i)?;
        (c.t - t <= 2500.0).then(|| self.normalize(c.x + c.w * 0.5, c.y + c.h * 0.5))
    }

    /// Caret position at `t`, linearly interpolated between samples so the camera
    /// glides with it instead of stepping at the sampling rate.
    pub fn caret_smooth(&self, t: f64) -> Option<(f64, f64)> {
        let i = self.carets.partition_point(|c| c.t <= t);
        if i > 0 && i < self.carets.len() {
            let (a, b) = (self.carets[i - 1], self.carets[i]);
            if b.t - a.t > 0.0 && b.t - a.t < 700.0 {
                let f = (t - a.t) / (b.t - a.t);
                let (ax, ay) = (a.x + a.w * 0.5, a.y + a.h * 0.5);
                let (bx, by) = (b.x + b.w * 0.5, b.y + b.h * 0.5);
                return Some(self.normalize(ax + (bx - ax) * f, ay + (by - ay) * f));
            }
        }
        self.caret_near(t)
    }

    /// Normalised caret centre most recently sampled before `t` (within 2.5 s).
    pub fn caret_at(&self, t: f64) -> Option<(f64, f64)> {
        let i = self.carets.partition_point(|c| c.t <= t);
        if i == 0 {
            return None;
        }
        let c = self.carets[i - 1];
        if t - c.t > 2500.0 {
            return None;
        }
        Some(self.normalize(c.x + c.w * 0.5, c.y + c.h * 0.5))
    }

    /// Some apps only expose the whole text field, not the caret inside it. For those
    /// samples (a wide, unchanged rectangle) estimate where the caret really is by
    /// advancing from the field's left edge one character width per key typed.
    pub fn refine_carets(&mut self) {
        const BACKSPACE: &str = "51";
        const ADVANCE_KEYS_SKIP: [&str; 9] = ["36", "76", "48", "53", "123", "124", "125", "126", "117"];
        let mut i = 0;
        while i < self.carets.len() {
            let c = self.carets[i];
            if c.w <= 40.0 || c.h > 80.0 {
                i += 1;
                continue;
            }
            // run of samples sharing this rectangle
            let mut j = i;
            while j < self.carets.len() && {
                let d = self.carets[j];
                d.x == c.x && d.y == c.y && d.w == c.w && d.h == c.h
            } {
                j += 1;
            }
            let ch = c.h.clamp(10.0, 24.0);
            let char_w = (ch * 0.38).max(6.0);
            let origin = c.t - 1500.0;
            let mut k = self.keys.partition_point(|key| key.t < origin);
            let mut chars: f64 = 0.0;
            for n in i..j {
                let t = self.carets[n].t;
                while k < self.keys.len() && self.keys[k].t <= t {
                    let key = &self.keys[k];
                    if key.key == BACKSPACE {
                        chars = (chars - 1.0).max(0.0);
                    } else if !ADVANCE_KEYS_SKIP.contains(&key.key.as_str()) {
                        chars += 1.0;
                    }
                    k += 1;
                }
                let x = (c.x + 6.0 + chars * char_w).min(c.x + c.w - 4.0);
                self.carets[n] = CaretEvent { t, x, y: c.y, w: 2.0, h: c.h };
            }
            i = j;
        }
    }

    pub fn button_down_at(&self, t: f64) -> bool {
        let mut down = false;
        for c in &self.clicks {
            if c.t > t {
                break;
            }
            if c.button == Button::Left {
                down = c.down;
            }
        }
        down
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn interpolates_and_normalises() {
        let log = InputLog {
            bounds: Rect { x: 0.0, y: 0.0, width: 100.0, height: 100.0 },
            moves: vec![
                MoveEvent { t: 0.0, x: 0.0, y: 0.0, cursor: CursorKind::Arrow },
                MoveEvent { t: 100.0, x: 100.0, y: 50.0, cursor: CursorKind::Arrow },
            ],
            ..Default::default()
        };
        let (x, y, _) = log.cursor_at(50.0).unwrap();
        assert!((x - 0.5).abs() < 1e-9 && (y - 0.25).abs() < 1e-9);
    }
}

#[cfg(test)]
mod refine_tests {
    use super::*;

    #[test]
    fn wide_field_caret_is_estimated_from_typed_characters() {
        let mut log = InputLog::default();
        log.bounds = Rect { x: 0.0, y: 0.0, width: 1000.0, height: 1000.0 };
        for n in 0..20 {
            log.keys.push(KeyEvent { t: 1000.0 + n as f64 * 100.0, key: "0".into(), down: true });
        }
        for n in 0..10 {
            log.carets.push(CaretEvent { t: 1050.0 + n as f64 * 200.0, x: 100.0, y: 500.0, w: 700.0, h: 20.0 });
        }
        log.refine_carets();
        let first = log.carets[0].x;
        let last = log.carets[9].x;
        assert!(first < 120.0, "first {first}");
        assert!(last > first + 80.0 && last < 800.0, "last {last}");
        assert!(log.carets.iter().all(|c| c.w < 5.0));
    }
}
