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

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct InputLog {
    /// Captured area in global points.
    pub bounds: Rect,
    pub moves: Vec<MoveEvent>,
    pub clicks: Vec<ClickEvent>,
    pub keys: Vec<KeyEvent>,
}

impl Default for InputLog {
    fn default() -> Self {
        Self {
            bounds: Rect { x: 0.0, y: 0.0, width: 1.0, height: 1.0 },
            moves: Vec::new(),
            clicks: Vec::new(),
            keys: Vec::new(),
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
