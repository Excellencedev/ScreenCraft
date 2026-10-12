//! Automatic zoom regions from click activity.
//!
//! Observed behaviour of the reference recorder: a region starts a little
//! before the click, holds for a couple of seconds after the last click of a
//! burst, and clicks that land too close to the end of the recording are
//! ignored.

use crate::input::{Button, InputLog};
use crate::project::{ZoomKind, ZoomRegion};

#[derive(Clone, Debug)]
pub struct AutoZoomOptions {
    pub zoom: f64,
    pub lead_ms: f64,
    pub hold_ms: f64,
    /// Clicks closer than this to the previous group's end join that group.
    pub merge_gap_ms: f64,
    /// Regions shorter than this are dropped (e.g. clicks at the very end).
    pub min_len_ms: f64,
}

impl Default for AutoZoomOptions {
    fn default() -> Self {
        Self { zoom: 2.0, lead_ms: 300.0, hold_ms: 2500.0, merge_gap_ms: 1200.0, min_len_ms: 1500.0 }
    }
}

/// Zoom regions from clicks and typing bursts.
pub fn generate(log: &InputLog, duration_ms: f64, opts: &AutoZoomOptions) -> Vec<ZoomRegion> {
    let mut downs: Vec<f64> = log
        .clicks
        .iter()
        .filter(|c| c.down && c.button == Button::Left)
        .map(|c| c.t)
        .collect();
    // typing: one trigger at the start of each burst of key presses
    let mut last_key = f64::NEG_INFINITY;
    for k in log.keys.iter().filter(|k| k.down) {
        if k.t - last_key > 2000.0 {
            downs.push(k.t);
        }
        last_key = k.t;
    }
    downs.sort_by(|a, b| a.partial_cmp(b).unwrap());

    let mut groups: Vec<(f64, f64)> = Vec::new(); // (start, end)
    for t in downs {
        let start = (t - opts.lead_ms).max(0.0);
        let end = (t + opts.hold_ms).min(duration_ms);
        match groups.last_mut() {
            Some(g) if t - opts.lead_ms <= g.1 + opts.merge_gap_ms => g.1 = end.max(g.1),
            _ => groups.push((start, end)),
        }
    }

    groups
        .into_iter()
        .filter(|(s, e)| e - s >= opts.min_len_ms)
        .enumerate()
        .map(|(i, (s, e))| ZoomRegion {
            id: format!("zoom-{}", i),
            zoom: opts.zoom,
            kind: ZoomKind::FollowCursor,
            source_start_ms: s,
            source_end_ms: e,
            ..Default::default()
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::input::ClickEvent;

    fn click(t: f64) -> ClickEvent {
        ClickEvent { t, x: 0.0, y: 0.0, button: Button::Left, down: true }
    }

    #[test]
    fn matches_reference_recording() {
        // Clicks seen in a real 19.4 s session: 1.57 s, 7.60 s and one right at the end.
        let log = InputLog { clicks: vec![click(1568.0), click(7596.0), click(19380.0)], ..Default::default() };
        let z = generate(&log, 19401.0, &Default::default());
        assert_eq!(z.len(), 2);
        assert!((z[0].source_start_ms - 1268.0).abs() < 1.0);
        assert!((z[1].source_start_ms - 7296.0).abs() < 1.0);
        assert!((z[1].source_end_ms - 10096.0).abs() < 1.0);
    }

    #[test]
    fn merges_bursts() {
        let log = InputLog { clicks: vec![click(1000.0), click(2000.0), click(3000.0)], ..Default::default() };
        let z = generate(&log, 20000.0, &Default::default());
        assert_eq!(z.len(), 1);
        assert_eq!(z[0].source_end_ms, 5500.0);
    }
}
