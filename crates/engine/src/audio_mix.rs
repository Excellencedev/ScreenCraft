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

/// A music clip placed on the edited timeline.
pub struct MusicPlay {
    pub pcm: std::sync::Arc<Vec<f32>>,
    pub offset_ms: f64,
    pub in_ms: f64,
    pub dur_ms: f64,
    pub fade_in_ms: f64,
    pub fade_out_ms: f64,
}

/// Everything besides the recorded tracks: music (with fades and ducking under speech) and the
/// click / typing sound effects.
#[derive(Default)]
pub struct Extras {
    pub music: Vec<MusicPlay>,
    /// Master x music volume.
    pub music_gain: f32,
    /// Output-time ranges (ms) where someone speaks; the music ducks under them.
    pub duck_ranges: Vec<[f64; 2]>,
    pub duck_level: f32,
    pub hits: Vec<crate::sfx::Hit>,
}

const ATTACK_MS: f64 = 150.0;
const RELEASE_MS: f64 = 450.0;

/// Music gain under speech at output time `t` (1 = untouched).
fn duck_gain(ranges: &[[f64; 2]], level: f32, t: f64) -> f32 {
    let lvl = level.clamp(0.02, 1.0);
    let mut g = 1.0f32;
    // ranges are sorted by start; only those near `t` matter
    let hi = ranges.partition_point(|r| r[0] - ATTACK_MS <= t);
    for r in ranges[..hi].iter().rev().take(8) {
        let v = if t < r[0] {
            1.0 - (1.0 - lvl) * ((t - (r[0] - ATTACK_MS)) / ATTACK_MS) as f32
        } else if t <= r[1] {
            lvl
        } else if t < r[1] + RELEASE_MS {
            lvl + (1.0 - lvl) * ((t - r[1]) / RELEASE_MS) as f32
        } else {
            1.0
        };
        g = g.min(v);
    }
    g
}

pub struct Soundtrack {
    slices: Vec<Slice>,
    starts: Vec<f64>,
    lanes: Vec<Lane>,
    extras: Extras,
}

impl Soundtrack {
    pub fn new(project: &Project, lanes: Vec<Lane>) -> Self {
        let slices = project.scene.slices.clone();
        let starts = slice_output_starts(&slices);
        Self { slices, starts, lanes, extras: Extras::default() }
    }

    pub fn with_extras(mut self, e: Extras) -> Self {
        self.extras = e;
        self
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
        self.mix_extras(&mut out, start_frame);
        screencraft_audio::mix::limit(&mut out);
        out
    }

    fn mix_extras(&self, out: &mut [f32], start_frame: u64) {
        let len = out.len() / 2;
        let (t0, t1) = (start_frame as f64 * 1000.0 / RATE, (start_frame + len as u64) as f64 * 1000.0 / RATE);
        let e = &self.extras;
        for m in &e.music {
            let end = m.offset_ms + m.dur_ms;
            if end <= t0 || m.offset_ms >= t1 {
                continue;
            }
            for k in 0..len {
                let t = (start_frame + k as u64) as f64 * 1000.0 / RATE;
                if t < m.offset_ms || t >= end {
                    continue;
                }
                let idx = (((m.in_ms + (t - m.offset_ms)) * RATE / 1000.0) as usize) * 2;
                if idx + 1 >= m.pcm.len() {
                    continue;
                }
                let fi = if m.fade_in_ms > 0.0 { ((t - m.offset_ms) / m.fade_in_ms).min(1.0) } else { 1.0 };
                let fo = if m.fade_out_ms > 0.0 { ((end - t) / m.fade_out_ms).min(1.0) } else { 1.0 };
                let g = e.music_gain * fi.min(fo) as f32 * duck_gain(&e.duck_ranges, e.duck_level, t);
                out[k * 2] += m.pcm[idx] * g;
                out[k * 2 + 1] += m.pcm[idx + 1] * g;
            }
        }
        if !e.hits.is_empty() {
            let (a, b) = (start_frame as i64, (start_frame + len as u64) as i64);
            let first = e.hits.partition_point(|h| h.start_frame < a - 96_000);
            for h in &e.hits[first..] {
                if h.start_frame >= b {
                    break;
                }
                let frames = (h.pcm.len() / 2) as f64 / h.rate;
                if (h.start_frame as f64 + frames) < a as f64 {
                    continue;
                }
                let from = (a - h.start_frame).max(0);
                let last = h.pcm.len() / 2 - 1;
                for k in from..(b - h.start_frame) {
                    let pos = k as f64 * h.rate;
                    if pos >= last as f64 {
                        break;
                    }
                    let (i0, f) = (pos as usize, (pos - pos.floor()) as f32);
                    let i1 = (i0 + 1).min(last);
                    let o = ((h.start_frame + k - a) * 2) as usize;
                    out[o] += (h.pcm[i0 * 2] * (1.0 - f) + h.pcm[i1 * 2] * f) * h.gain;
                    out[o + 1] += (h.pcm[i0 * 2 + 1] * (1.0 - f) + h.pcm[i1 * 2 + 1] * f) * h.gain;
                }
            }
        }
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
