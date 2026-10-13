//! Automatic zoom regions from click activity and typing bursts.
//!
//! Regions start *before* the action so the eased zoom-in finishes right as
//! you click or begin typing, which keeps the motion feeling anticipatory and
//! smooth. Click zooms linger a couple of seconds; typing zooms end shortly
//! after the last keystroke.

use crate::input::{Button, InputLog};
use crate::project::{ZoomKind, ZoomRegion};

#[derive(Clone, Debug)]
pub struct AutoZoomOptions {
    pub zoom: f64,
    /// How long before a click the zoom starts.
    pub click_lead_ms: f64,
    /// How long a click zoom is held after the click.
    pub click_hold_ms: f64,
    /// How long before the first key of a typing burst the zoom starts.
    pub type_lead_ms: f64,
    /// How long after the last key of a burst the zoom ends.
    pub type_tail_ms: f64,
    /// Keys further apart than this start a new typing burst.
    pub type_gap_ms: f64,
    /// Regions closer than this are merged into one.
    pub merge_gap_ms: f64,
    /// Regions shorter than this are dropped (e.g. clicks at the very end).
    pub min_len_ms: f64,
}

impl Default for AutoZoomOptions {
    fn default() -> Self {
        Self {
            zoom: 2.0,
            click_lead_ms: 650.0,
            click_hold_ms: 2500.0,
            type_lead_ms: 800.0,
            type_tail_ms: 700.0,
            type_gap_ms: 1200.0,
            merge_gap_ms: 400.0,
            min_len_ms: 1000.0,
        }
    }
}

impl AutoZoomOptions {
    /// Lead times that line the end of the ease-in up with the action.
    pub fn for_ease(zoom: f64, ease_in_ms: f64) -> Self {
        Self {
            zoom,
            click_lead_ms: ease_in_ms + 150.0,
            type_lead_ms: ease_in_ms + 300.0,
            ..Default::default()
        }
    }
}

pub fn generate(log: &InputLog, duration_ms: f64, opts: &AutoZoomOptions) -> Vec<ZoomRegion> {
    let mut spans: Vec<(f64, f64)> = Vec::new();

    // click zooms
    for c in log.clicks.iter().filter(|c| c.down && c.button == Button::Left) {
        spans.push(((c.t - opts.click_lead_ms).max(0.0), (c.t + opts.click_hold_ms).min(duration_ms)));
    }

    // typing bursts: [first key - lead, last key + tail]
    let mut keys: Vec<f64> = log.keys.iter().filter(|k| k.down).map(|k| k.t).collect();
    keys.sort_by(|a, b| a.partial_cmp(b).unwrap());
    let mut i = 0;
    while i < keys.len() {
        let first = keys[i];
        let mut last = first;
        let mut j = i + 1;
        while j < keys.len() && keys[j] - last <= opts.type_gap_ms {
            last = keys[j];
            j += 1;
        }
        spans.push(((first - opts.type_lead_ms).max(0.0), (last + opts.type_tail_ms).min(duration_ms)));
        i = j;
    }

    // union overlapping / nearly touching spans
    spans.sort_by(|a, b| a.0.partial_cmp(&b.0).unwrap());
    let mut merged: Vec<(f64, f64)> = Vec::new();
    for (s, e) in spans {
        match merged.last_mut() {
            Some(m) if s <= m.1 + opts.merge_gap_ms => m.1 = m.1.max(e),
            _ => merged.push((s, e)),
        }
    }

    merged
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
    use crate::input::{ClickEvent, KeyEvent};

    fn click(t: f64) -> ClickEvent {
        ClickEvent { t, x: 0.0, y: 0.0, button: Button::Left, down: true }
    }
    fn key(t: f64) -> KeyEvent {
        KeyEvent { t, key: "0".into(), down: true, ..Default::default() }
    }

    #[test]
    fn click_zoom_starts_before_the_click() {
        let o = AutoZoomOptions::default();
        let log = InputLog { clicks: vec![click(1568.0), click(7596.0), click(19380.0)], ..Default::default() };
        let z = generate(&log, 19401.0, &o);
        assert_eq!(z.len(), 2, "a click at the very end is ignored");
        assert!((z[0].source_start_ms - (1568.0 - o.click_lead_ms)).abs() < 1.0);
        assert!((z[1].source_end_ms - (7596.0 + o.click_hold_ms)).abs() < 1.0);
    }

    #[test]
    fn click_bursts_merge() {
        let log = InputLog { clicks: vec![click(1000.0), click(2000.0), click(3000.0)], ..Default::default() };
        let z = generate(&log, 20000.0, &Default::default());
        assert_eq!(z.len(), 1);
        assert_eq!(z[0].source_end_ms, 5500.0);
    }

    #[test]
    fn typing_zoom_starts_early_and_ends_with_the_last_key() {
        let o = AutoZoomOptions::default();
        // a burst of typing from 5.0 s to 8.0 s with small gaps
        let keys: Vec<KeyEvent> = (0..=30).map(|i| key(5000.0 + i as f64 * 100.0)).collect();
        let log = InputLog { keys, ..Default::default() };
        let z = generate(&log, 30000.0, &o);
        assert_eq!(z.len(), 1);
        assert!((z[0].source_start_ms - (5000.0 - o.type_lead_ms)).abs() < 1.0);
        // ends shortly after the final key, not seconds later
        assert!((z[0].source_end_ms - (8000.0 + o.type_tail_ms)).abs() < 1.0);
    }

    #[test]
    fn pauses_split_typing_into_separate_zooms() {
        let mut keys: Vec<KeyEvent> = (0..10).map(|i| key(2000.0 + i as f64 * 100.0)).collect();
        keys.extend((0..10).map(|i| key(9000.0 + i as f64 * 100.0)));
        let log = InputLog { keys, ..Default::default() };
        assert_eq!(generate(&log, 30000.0, &Default::default()).len(), 2);
    }

    #[test]
    fn lead_matches_ease_in() {
        let o = AutoZoomOptions::for_ease(2.0, 500.0);
        assert!(o.click_lead_ms > 500.0 && o.type_lead_ms > o.click_lead_ms);
    }
}
