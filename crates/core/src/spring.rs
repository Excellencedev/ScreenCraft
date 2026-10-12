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
