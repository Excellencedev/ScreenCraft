//! Constant-frame-rate resampling. Capture APIs only deliver a frame when the screen changes;
//! video needs one every 1/fps. `Cfr` maps (arrival time, frame) pairs onto numbered slots,
//! repeating the previous frame across idle stretches. Pure and deterministic.

pub struct Cfr<T: Clone> {
    slot_ns: f64,
    origin_ns: Option<u64>,
    next_slot: u64,
    last: Option<T>,
}

impl<T: Clone> Cfr<T> {
    pub fn new(fps: u32) -> Self {
        Self { slot_ns: 1e9 / fps.max(1) as f64, origin_ns: None, next_slot: 0, last: None }
    }

    /// Time of the first frame; slot 0 starts here.
    pub fn origin_ns(&self) -> Option<u64> {
        self.origin_ns
    }

    fn slot_start(&self, k: u64) -> f64 {
        k as f64 * self.slot_ns
    }

    /// A frame arrived at `at_ns`. Returns `(slot, frame)` pairs that are now final: every slot
    /// that began before this frame and wasn't covered yet shows the previous frame.
    pub fn push(&mut self, at_ns: u64, frame: T) -> Vec<(u64, T)> {
        let origin = *self.origin_ns.get_or_insert(at_ns);
        let rel = at_ns.saturating_sub(origin) as f64;
        let mut out = Vec::new();
        if let Some(prev) = &self.last {
            // Slot centres before this arrival belong to the previous picture.
            while self.slot_start(self.next_slot) + self.slot_ns * 0.5 < rel {
                out.push((self.next_slot, prev.clone()));
                self.next_slot += 1;
            }
        } else {
            self.next_slot = 0;
        }
        self.last = Some(frame);
        out
    }

    /// Recording ended at `end_ns`: flush the slots up to the end with the last picture.
    pub fn finish(&mut self, end_ns: u64) -> Vec<(u64, T)> {
        let (Some(origin), Some(last)) = (self.origin_ns, self.last.clone()) else { return Vec::new() };
        let rel = end_ns.saturating_sub(origin) as f64;
        let mut out = Vec::new();
        loop {
            out.push((self.next_slot, last.clone()));
            self.next_slot += 1;
            if self.slot_start(self.next_slot) + self.slot_ns * 0.5 > rel {
                break;
            }
        }
        out
    }

    /// Number of slots emitted so far (the frame count once finished).
    pub fn slots(&self) -> u64 {
        self.next_slot
    }

    /// Presentation time of a slot in microseconds.
    pub fn slot_pts_us(&self, slot: u64) -> i64 {
        (self.slot_start(slot) / 1000.0).round() as i64
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn idle_screen_repeats_frames() {
        let mut c = Cfr::<u32>::new(10); // 100 ms slots
        assert!(c.push(1_000_000_000, 1).is_empty());
        // Next change arrives 350 ms later: slots 0..=2 (centres 50, 150, 250 ms) show frame 1.
        let out = c.push(1_350_000_000, 2);
        assert_eq!(out.iter().map(|(s, f)| (*s, *f)).collect::<Vec<_>>(), vec![(0, 1), (1, 1), (2, 1)]);
        let tail = c.finish(1_700_000_000);
        assert_eq!(tail.first().map(|x| x.1), Some(2));
        assert_eq!(c.slots(), 7);
    }

    #[test]
    fn fast_source_never_exceeds_slots() {
        let mut c = Cfr::<u32>::new(30);
        let mut emitted = 0;
        for i in 0..240u64 {
            emitted += c.push(i * 1_000_000_000 / 120, i as u32).len(); // 120 fps source
        }
        emitted += c.finish(2_000_000_000).len();
        assert!((59..=61).contains(&emitted), "{emitted}");
    }
}
