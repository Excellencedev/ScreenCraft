//! Finding the dull stretches of a recording: no clicks, no typing, a still cursor and
//! (optionally) nobody talking. No machine learning: just the logged input and the mic level.

use crate::input::InputLog;

const BUCKET_MS: f64 = 100.0;

/// Idle ranges `[start, end]` in source milliseconds.
///
/// * `min_idle_ms`: shortest idle stretch worth reporting.
/// * `speech`: optional per-100 ms "someone is talking" flags (index 0 = source time 0).
/// * `margin_ms`: kept at normal speed around the activity on either side of a range.
pub fn idle_segments(log: &InputLog, duration_ms: f64, speech: Option<&[bool]>, min_idle_ms: f64, margin_ms: f64) -> Vec<[f64; 2]> {
    let n = (duration_ms / BUCKET_MS).ceil().max(0.0) as usize;
    if n == 0 {
        return Vec::new();
    }
    let mut active = vec![false; n];
    let mark = |active: &mut Vec<bool>, t: f64, pad_ms: f64| {
        let a = ((t - pad_ms) / BUCKET_MS).floor().max(0.0) as usize;
        let b = (((t + pad_ms) / BUCKET_MS).ceil() as usize).min(n.saturating_sub(1));
        for v in active.iter_mut().take(b + 1).skip(a) {
            *v = true;
        }
    };
    for c in &log.clicks {
        mark(&mut active, c.t, 400.0);
    }
    for k in &log.keys {
        mark(&mut active, k.t, 400.0);
    }
    for c in &log.carets {
        mark(&mut active, c.t, 200.0);
    }
    // cursor motion: more than ~6 points per 100 ms bucket counts as activity
    let mut prev: Option<(f64, f64, f64)> = None;
    for m in &log.moves {
        if let Some((pt, px, py)) = prev {
            let dt = (m.t - pt).max(1.0);
            let speed = ((m.x - px).powi(2) + (m.y - py).powi(2)).sqrt() / dt * BUCKET_MS;
            if speed > 6.0 {
                mark(&mut active, m.t, 150.0);
            }
        }
        prev = Some((m.t, m.x, m.y));
    }
    if let Some(sp) = speech {
        for (i, &s) in sp.iter().enumerate().take(n) {
            if s {
                active[i] = true;
            }
        }
    }

    let mut out = Vec::new();
    let mut i = 0;
    while i < n {
        if active[i] {
            i += 1;
            continue;
        }
        let start = i;
        while i < n && !active[i] {
            i += 1;
        }
        let a = start as f64 * BUCKET_MS + margin_ms;
        let b = (i as f64 * BUCKET_MS).min(duration_ms) - margin_ms;
        if b - a >= min_idle_ms {
            out.push([a, b]);
        }
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::input::{Button, ClickEvent, MoveEvent, CursorKind};

    #[test]
    fn finds_quiet_stretch_between_clicks() {
        let log = InputLog {
            clicks: vec![
                ClickEvent { t: 500.0, x: 1.0, y: 1.0, button: Button::Left, down: true },
                ClickEvent { t: 9500.0, x: 1.0, y: 1.0, button: Button::Left, down: true },
            ],
            moves: vec![
                MoveEvent { t: 0.0, x: 1.0, y: 1.0, cursor: CursorKind::Arrow },
                MoveEvent { t: 10000.0, x: 1.0, y: 1.0, cursor: CursorKind::Arrow },
            ],
            ..Default::default()
        };
        let segs = idle_segments(&log, 10_000.0, None, 2000.0, 250.0);
        assert_eq!(segs.len(), 1);
        assert!(segs[0][0] > 1000.0 && segs[0][1] < 9300.0, "{segs:?}");
    }

    #[test]
    fn speech_keeps_a_stretch_active() {
        let log = InputLog::default();
        let mut speech = vec![false; 100];
        for s in speech.iter_mut().take(60).skip(20) {
            *s = true;
        }
        let segs = idle_segments(&log, 10_000.0, Some(&speech), 1000.0, 0.0);
        // idle before 2000 ms and after 6000 ms; the spoken part in between is kept
        assert_eq!(segs.len(), 2);
        assert!(segs[0][1] <= 2000.0);
        assert!(segs[1][0] >= 6000.0);
    }
}
