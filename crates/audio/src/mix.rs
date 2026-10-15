//! Mixing helpers for export and preview: sum tracks with gain and keep samples in range.

/// Add `src` into `dst` with `gain`, sample by sample. Extra source samples are ignored.
pub fn mix_into(dst: &mut [f32], src: &[f32], gain: f32) {
    for (d, s) in dst.iter_mut().zip(src) {
        *d += s * gain;
    }
}

/// Soft-limit to [-1, 1] so a hot mix never wraps or hard-clips harshly.
pub fn limit(buf: &mut [f32]) {
    for s in buf {
        let x = *s;
        *s = if x.abs() <= 0.9 { x } else { x.signum() * (0.9 + 0.1 * ((x.abs() - 0.9) * 10.0).tanh()) };
    }
}

/// Peak level 0..1 of an interleaved buffer, for meters.
pub fn peak(buf: &[f32]) -> f32 {
    buf.iter().fold(0.0f32, |m, s| m.max(s.abs()))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn limiter_stays_in_range() {
        let mut v = vec![0.0, 0.5, 1.5, -3.0, 0.95];
        limit(&mut v);
        assert!(v.iter().all(|s| s.abs() <= 1.0));
        assert_eq!(v[1], 0.5);
    }
}
