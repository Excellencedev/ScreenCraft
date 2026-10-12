//! Mapping between output (edited) time and source (recorded) time.
//! Slices are played in order; each may be sped up or slowed down.
//! Anything not covered by a slice is cut.

use crate::project::Slice;

pub fn output_duration_ms(slices: &[Slice]) -> f64 {
    slices
        .iter()
        .map(|s| (s.source_end_ms - s.source_start_ms).max(0.0) / s.time_scale.max(1e-3))
        .sum()
}

/// Output start time of every slice.
pub fn slice_output_starts(slices: &[Slice]) -> Vec<f64> {
    let mut acc = 0.0;
    slices
        .iter()
        .map(|s| {
            let start = acc;
            acc += (s.source_end_ms - s.source_start_ms).max(0.0) / s.time_scale.max(1e-3);
            start
        })
        .collect()
}

/// Source time and slice index for an output time.
pub fn output_to_source(slices: &[Slice], t_out: f64) -> Option<(f64, usize)> {
    let mut acc = 0.0;
    for (i, s) in slices.iter().enumerate() {
        let len = (s.source_end_ms - s.source_start_ms).max(0.0) / s.time_scale.max(1e-3);
        if t_out < acc + len || (i == slices.len() - 1 && t_out <= acc + len) {
            return Some((s.source_start_ms + (t_out - acc) * s.time_scale, i));
        }
        acc += len;
    }
    None
}

/// Output time for a source time, or `None` if that moment was cut.
pub fn source_to_output(slices: &[Slice], t_src: f64) -> Option<f64> {
    let mut acc = 0.0;
    for s in slices {
        let len = (s.source_end_ms - s.source_start_ms).max(0.0) / s.time_scale.max(1e-3);
        if t_src >= s.source_start_ms && t_src <= s.source_end_ms {
            return Some(acc + (t_src - s.source_start_ms) / s.time_scale.max(1e-3));
        }
        acc += len;
    }
    None
}

/// Split the slice containing `t_src` in two. Returns false if `t_src` is not
/// strictly inside a slice.
pub fn split_at(slices: &mut Vec<Slice>, t_src: f64, new_id: String) -> bool {
    for i in 0..slices.len() {
        let s = &slices[i];
        if t_src > s.source_start_ms + 1.0 && t_src < s.source_end_ms - 1.0 {
            let mut right = s.clone();
            right.id = new_id;
            right.source_start_ms = t_src;
            slices[i].source_end_ms = t_src;
            slices.insert(i + 1, right);
            return true;
        }
    }
    false
}

#[cfg(test)]
mod tests {
    use super::*;

    fn s(a: f64, b: f64, scale: f64) -> Slice {
        Slice { source_start_ms: a, source_end_ms: b, time_scale: scale, ..Default::default() }
    }

    #[test]
    fn maps_with_cut_and_speed() {
        // 0-1000 normal, cut 1000-2000, 2000-3000 at 2x
        let sl = vec![s(0.0, 1000.0, 1.0), s(2000.0, 3000.0, 2.0)];
        assert_eq!(output_duration_ms(&sl), 1500.0);
        assert_eq!(output_to_source(&sl, 500.0).unwrap().0, 500.0);
        assert_eq!(output_to_source(&sl, 1250.0).unwrap().0, 2500.0);
        assert_eq!(source_to_output(&sl, 2500.0), Some(1250.0));
        assert_eq!(source_to_output(&sl, 1500.0), None);
    }

    #[test]
    fn split_inserts_slice() {
        let mut sl = vec![s(0.0, 1000.0, 1.0)];
        assert!(split_at(&mut sl, 400.0, "b".into()));
        assert_eq!(sl.len(), 2);
        assert_eq!(sl[0].source_end_ms, 400.0);
        assert_eq!(sl[1].source_start_ms, 400.0);
    }
}
