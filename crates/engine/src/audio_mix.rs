//! Builds the edited soundtrack: source audio mapped through the slices (cuts, speed changes),
//! scaled by the per-slice and project volumes, summed and limited.

use screencraft_core::project::{Project, Slice};
use screencraft_core::timeline::slice_output_starts;

pub const RATE: f64 = 48_000.0;

/// What an audio source must provide: 48 kHz stereo at an arbitrary source time.
pub trait PcmSource {
    fn read(&mut self, start_ms: f64, frames: usize) -> Vec<f32>;
}

impl PcmSource for crate::source::AudioSource {
    fn read(&mut self, start_ms: f64, frames: usize) -> Vec<f32> {
        crate::source::AudioSource::read(self, start_ms, frames).unwrap_or_else(|_| vec![0.0; frames * 2])
    }
}

pub struct Lane {
    pub source: Box<dyn PcmSource + Send>,
    pub gain: f32,
}

pub struct Soundtrack {
    slices: Vec<Slice>,
    starts: Vec<f64>,
    lanes: Vec<Lane>,
}

impl Soundtrack {
    pub fn new(project: &Project, lanes: Vec<Lane>) -> Self {
        let slices = project.scene.slices.clone();
        let starts = slice_output_starts(&slices);
        Self { slices, starts, lanes }
    }

    /// Interleaved stereo for output frames `[start, start + len)` at 48 kHz.
    pub fn render(&mut self, start_frame: u64, len: usize) -> Vec<f32> {
        let mut out = vec![0f32; len * 2];
        let mut done = 0usize;
        while done < len {
            let t_ms = (start_frame + done as u64) as f64 * 1000.0 / RATE;
            // Slice containing t_ms and how many output frames remain in it.
            let idx = self
                .starts
                .iter()
                .rposition(|&s| s <= t_ms + 1e-6)
                .filter(|&i| {
                    let s = &self.slices[i];
                    t_ms < self.starts[i] + (s.source_end_ms - s.source_start_ms) / s.time_scale.max(1e-3) - 1e-6
                });
            let Some(i) = idx else {
                break; // past the end of the edit
            };
            let s = &self.slices[i];
            let scale = s.time_scale.max(1e-3);
            let slice_out_end_ms = self.starts[i] + (s.source_end_ms - s.source_start_ms) / scale;
            let remain = (((slice_out_end_ms - t_ms) * RATE / 1000.0).ceil() as usize).clamp(1, len - done);
            let silent = s.freeze || scale < 0.05;
            if !silent {
                let src_start = s.source_start_ms + (t_ms - self.starts[i]) * scale;
                let src_frames = ((remain as f64 * scale).ceil() as usize + 2).max(2);
                for lane in &mut self.lanes {
                    let g = lane.gain * s.volume as f32;
                    if g <= 0.0 {
                        continue;
                    }
                    let pcm = lane.source.read(src_start, src_frames);
                    let last = pcm.len() / 2 - 1;
                    for k in 0..remain {
                        // Linear resample to the output grid (scale == 1 hits exact samples).
                        let pos = k as f64 * scale;
                        let a = (pos.floor() as usize).min(last);
                        let b = (a + 1).min(last);
                        let f = (pos - a as f64) as f32;
                        let o = (done + k) * 2;
                        out[o] += (pcm[a * 2] * (1.0 - f) + pcm[b * 2] * f) * g;
                        out[o + 1] += (pcm[a * 2 + 1] * (1.0 - f) + pcm[b * 2 + 1] * f) * g;
                    }
                }
            }
            done += remain;
        }
        screencraft_audio::mix::limit(&mut out);
        out
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use screencraft_core::project::{Media, Project};

    struct Ramp;
    impl PcmSource for Ramp {
        /// Value = source time in seconds, so mapping errors are visible.
        fn read(&mut self, start_ms: f64, frames: usize) -> Vec<f32> {
            (0..frames)
                .flat_map(|i| {
                    let v = (start_ms / 1000.0 + i as f64 / RATE) as f32 * 0.1;
                    [v, v]
                })
                .collect()
        }
    }

    fn project(slices: Vec<Slice>) -> Project {
        let mut p = Project::from_recording("t", Media { duration_ms: 10_000.0, ..Default::default() });
        p.scene.slices = slices;
        p
    }

    #[test]
    fn cuts_skip_source_time() {
        // keep 0-1 s and 3-4 s of the source
        let p = project(vec![
            Slice { id: "a".into(), source_start_ms: 0.0, source_end_ms: 1000.0, ..Default::default() },
            Slice { id: "b".into(), source_start_ms: 3000.0, source_end_ms: 4000.0, ..Default::default() },
        ]);
        let mut s = Soundtrack::new(&p, vec![Lane { source: Box::new(Ramp), gain: 1.0 }]);
        let pcm = s.render(48_000 + 100, 4);
        // output 1.002 s is source 3.002 s
        assert!((pcm[0] - 0.3002).abs() < 0.01, "{}", pcm[0]);
    }

    #[test]
    fn speed_change_scales_time() {
        let p = project(vec![Slice { id: "a".into(), source_start_ms: 0.0, source_end_ms: 4000.0, time_scale: 2.0, ..Default::default() }]);
        let mut s = Soundtrack::new(&p, vec![Lane { source: Box::new(Ramp), gain: 1.0 }]);
        // output 1 s -> source 2 s
        let pcm = s.render(48_000, 2);
        assert!((pcm[0] - 0.2).abs() < 0.01, "{}", pcm[0]);
        // the edit is 2 s long: beyond it, silence
        let tail = s.render(48_000 * 2 + 10, 4);
        assert!(tail.iter().all(|&x| x == 0.0));
    }

    #[test]
    fn volume_and_mute_apply() {
        let p = project(vec![Slice { id: "a".into(), source_start_ms: 1000.0, source_end_ms: 2000.0, volume: 0.5, ..Default::default() }]);
        let mut s = Soundtrack::new(
            &p,
            vec![Lane { source: Box::new(Ramp), gain: 1.0 }, Lane { source: Box::new(Ramp), gain: 0.0 }],
        );
        let pcm = s.render(0, 2);
        assert!((pcm[0] - 0.05).abs() < 0.005, "{}", pcm[0]);
    }
}
