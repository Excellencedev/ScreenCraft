//! Damped spring integrator used for camera and cursor motion.

use serde::{Deserialize, Serialize};

#[derive(Clone, Copy, Debug, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", default)]
pub struct SpringConfig {
    pub stiffness: f64,
    pub damping: f64,
    pub mass: f64,
    /// Never overshoot the target.
    pub clamp: bool,
    /// Distance/velocity under which the spring snaps to the target.
    pub precision: f64,
}

impl Default for SpringConfig {
    fn default() -> Self {
        Self { stiffness: 125.0, damping: 12.0, mass: 1.5, clamp: true, precision: 0.0005 }
    }
}

#[derive(Clone, Copy, Debug, Default)]
pub struct Spring {
    pub x: f64,
    pub v: f64,
}

impl Spring {
    pub fn new(x: f64) -> Self {
        Self { x, v: 0.0 }
    }

    /// Advance by `dt` seconds toward `target`.
    pub fn step(&mut self, target: f64, dt: f64, cfg: &SpringConfig) {
        const MAX_H: f64 = 1.0 / 240.0;
        let steps = (dt / MAX_H).ceil().max(1.0) as usize;
        let h = dt / steps as f64;
        for _ in 0..steps {
            let before = self.x - target;
            let a = (-cfg.stiffness * before - cfg.damping * self.v) / cfg.mass;
            self.v += a * h;
            self.x += self.v * h;
            let after = self.x - target;
            if cfg.clamp && before != 0.0 && before.signum() != after.signum() && after != 0.0 {
                self.x = target;
                self.v = 0.0;
            }
        }
        if (self.x - target).abs() < cfg.precision && self.v.abs() < cfg.precision * 10.0 {
            self.x = target;
            self.v = 0.0;
        }
    }
}

/// Critically damped smoothing toward `target` (Unity-style SmoothDamp).
/// `smooth_time` is roughly the time in seconds to reach the target.
pub fn smooth_damp(current: f64, target: f64, velocity: &mut f64, smooth_time: f64, dt: f64) -> f64 {
    let st = smooth_time.max(1e-3);
    let omega = 2.0 / st;
    let x = omega * dt;
    let e = 1.0 / (1.0 + x + 0.48 * x * x + 0.235 * x * x * x);
    let change = current - target;
    let temp = (*velocity + omega * change) * dt;
    *velocity = (*velocity - omega * temp) * e;
    target + (change + temp) * e
}

/// Smootherstep easing, 0..1 -> 0..1 with zero velocity at both ends.
pub fn ease(t: f64) -> f64 {
    let t = t.clamp(0.0, 1.0);
    t * t * t * (t * (t * 6.0 - 15.0) + 10.0)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn converges_without_overshoot_when_clamped() {
        let cfg = SpringConfig::default();
        let mut s = Spring::new(0.0);
        let mut max = 0.0f64;
        for _ in 0..600 {
            s.step(1.0, 1.0 / 60.0, &cfg);
            max = max.max(s.x);
        }
        assert!(max <= 1.0 + 1e-9, "overshot: {max}");
        assert!((s.x - 1.0).abs() < 1e-3);
    }

    #[test]
    fn smooth_damp_converges_without_overshoot() {
        let (mut x, mut v) = (0.0, 0.0);
        let mut max = 0.0f64;
        for _ in 0..240 {
            x = smooth_damp(x, 1.0, &mut v, 0.4, 1.0 / 60.0);
            max = max.max(x);
        }
        assert!(max <= 1.001 && (x - 1.0).abs() < 1e-3);
    }

    #[test]
    fn ease_is_monotonic_and_bounded() {
        let mut prev = 0.0;
        for i in 0..=100 {
            let e = ease(i as f64 / 100.0);
            assert!(e >= prev - 1e-12 && (0.0..=1.0).contains(&e));
            prev = e;
        }
    }

    #[test]
    fn unclamped_overshoots() {
        let cfg = SpringConfig { clamp: false, ..Default::default() };
        let mut s = Spring::new(0.0);
        let mut max = 0.0f64;
        for _ in 0..600 {
            s.step(1.0, 1.0 / 60.0, &cfg);
            max = max.max(s.x);
        }
        assert!(max > 1.05);
    }
}
