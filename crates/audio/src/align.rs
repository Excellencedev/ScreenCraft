//! Re-times one captured stream onto the shared timeline.
//!
//! Input blocks arrive with the time their first sample was captured, at the device's own
//! sample rate and channel count. Output is 48 kHz stereo that starts exactly at `origin_ns`
//! and never has a hole: gaps are filled with silence, overlaps are trimmed, and a small
//! persistent clock difference is absorbed by nudging the resampling ratio.

pub const OUT_RATE: u32 = 48_000;
pub const OUT_CHANNELS: usize = 2;

/// Gap or overlap (in output frames) tolerated before a hard correction: 15 ms.
const HARD_THRESHOLD: i64 = 720;
/// Largest ratio nudge: 0.3 %.
const MAX_NUDGE: f64 = 0.003;

#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct AlignStats {
    pub frames_out: u64,
    pub silence_inserted: u64,
    pub frames_dropped: u64,
    pub max_drift_ms: f64,
}

pub struct StreamAligner {
    origin_ns: u64,
    in_rate: f64,
    in_channels: usize,
    /// Output frames emitted so far.
    emitted: u64,
    /// Position of the next output frame in input-sample space, relative to `window[0]`.
    pos: f64,
    /// Last few input frames (stereo) kept for the interpolator.
    window: Vec<[f32; 2]>,
    started: bool,
    pub stats: AlignStats,
}

fn stereo(frame: &[f32], ch: usize) -> [f32; 2] {
    match ch {
        1 => [frame[0], frame[0]],
        _ => [frame[0], frame[1]],
    }
}

fn cubic(p: [f32; 4], t: f32) -> f32 {
    // Catmull-Rom between p[1] and p[2].
    let (a, b, c, d) = (p[0], p[1], p[2], p[3]);
    0.5 * ((2.0 * b) + (-a + c) * t + (2.0 * a - 5.0 * b + 4.0 * c - d) * t * t + (-a + 3.0 * b - 3.0 * c + d) * t * t * t)
}

impl StreamAligner {
    pub fn new(origin_ns: u64, in_rate: u32, in_channels: u16) -> Self {
        Self {
            origin_ns,
            in_rate: in_rate.max(1) as f64,
            in_channels: in_channels.max(1) as usize,
            emitted: 0,
            pos: 0.0,
            window: Vec::new(),
            started: false,
            stats: AlignStats::default(),
        }
    }

    /// Output frame index that corresponds to a capture time.
    fn expected_frame(&self, pts_ns: u64) -> i64 {
        ((pts_ns as i128 - self.origin_ns as i128) * OUT_RATE as i128 / 1_000_000_000) as i64
    }

    /// Feed one device block. Returns interleaved stereo f32 at 48 kHz (possibly empty).
    pub fn push(&mut self, pts_ns: u64, interleaved: &[f32]) -> Vec<f32> {
        let ch = self.in_channels;
        let n_in = interleaved.len() / ch;
        if n_in == 0 {
            return Vec::new();
        }
        let block_end_ns = pts_ns + (n_in as u64 * 1_000_000_000 / self.in_rate as u64);
        if block_end_ns <= self.origin_ns {
            return Vec::new(); // entirely before the recording started
        }
        let mut out: Vec<f32> = Vec::new();
        let mut skip_in = 0usize;
        let mut pts = pts_ns;
        if pts < self.origin_ns {
            // Trim the part before the origin.
            let cut = ((self.origin_ns - pts) as f64 * self.in_rate / 1e9) as usize;
            skip_in = cut.min(n_in);
            pts = self.origin_ns;
        }

        if !self.started {
            // First usable block: any delay since the origin becomes leading silence.
            self.started = true;
            let lead = self.expected_frame(pts).max(0) as u64;
            if lead > 0 {
                out.extend(std::iter::repeat(0.0).take(lead as usize * OUT_CHANNELS));
                self.emitted = lead;
                self.stats.silence_inserted += lead;
            }
        } else {
            // Compare where this block should start with where our output has reached.
            let expected = self.expected_frame(pts);
            let err = expected - self.emitted as i64; // >0: we are behind (gap), <0: ahead
            let drift_ms = err as f64 * 1000.0 / OUT_RATE as f64;
            if drift_ms.abs() > self.stats.max_drift_ms {
                self.stats.max_drift_ms = drift_ms.abs();
            }
            if err > HARD_THRESHOLD {
                let n = err as usize;
                out.extend(std::iter::repeat(0.0).take(n * OUT_CHANNELS));
                self.emitted += n as u64;
                self.stats.silence_inserted += n as u64;
                // The interpolator's history belongs to before the gap.
                self.window.clear();
                self.pos = 0.0;
            } else if err < -HARD_THRESHOLD {
                let drop_out = (-err) as f64;
                let drop_in = (drop_out * self.in_rate / OUT_RATE as f64) as usize;
                skip_in = (skip_in + drop_in).min(n_in);
                self.stats.frames_dropped += (-err) as u64;
                // Pretend we emitted them so the following blocks line up.
                self.emitted += 0;
            }
        }

        for i in skip_in..n_in {
            self.window.push(stereo(&interleaved[i * ch..(i + 1) * ch], ch));
        }

        // Continuous ratio nudge toward the expected position.
        let expected_now = self.expected_frame(pts + (self.window.len() as u64 * 1_000_000_000 / self.in_rate as u64));
        let err_now = expected_now - (self.emitted as i64 + (self.window.len() as f64 * OUT_RATE as f64 / self.in_rate) as i64);
        let nudge = (err_now as f64 / (OUT_RATE as f64 * 2.0)).clamp(-MAX_NUDGE, MAX_NUDGE);
        let step = self.in_rate / OUT_RATE as f64 * (1.0 + nudge);

        // Emit every output frame whose 4-tap neighbourhood is available.
        while self.pos + 2.0 < self.window.len() as f64 {
            let i = self.pos.floor() as usize;
            let t = (self.pos - i as f64) as f32;
            let g = |k: isize| -> [f32; 2] {
                let idx = (i as isize + k).clamp(0, self.window.len() as isize - 1) as usize;
                self.window[idx]
            };
            for c in 0..2 {
                let v = cubic([g(-1)[c], g(0)[c], g(1)[c], g(2)[c]], t);
                out.push(v.clamp(-1.0, 1.0));
            }
            self.emitted += 1;
            self.pos += step;
        }
        // Keep two frames of history before the cursor.
        let keep_from = (self.pos.floor() as usize).saturating_sub(2);
        if keep_from > 0 {
            self.window.drain(..keep_from);
            self.pos -= keep_from as f64;
        }
        self.stats.frames_out = self.emitted;
        out
    }

    /// Pad with silence up to `end_ns` so the stream is as long as the video.
    pub fn finish(&mut self, end_ns: u64) -> Vec<f32> {
        let want = self.expected_frame(end_ns).max(0) as u64;
        if want > self.emitted {
            let n = (want - self.emitted) as usize;
            self.emitted = want;
            self.stats.silence_inserted += n as u64;
            self.stats.frames_out = want;
            vec![0.0; n * OUT_CHANNELS]
        } else {
            Vec::new()
        }
    }

    pub fn frames_emitted(&self) -> u64 {
        self.emitted
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn sine(rate: u32, frames: usize, start: usize) -> Vec<f32> {
        (0..frames)
            .flat_map(|i| {
                let s = ((start + i) as f32 * 1000.0 * std::f32::consts::TAU / rate as f32).sin() * 0.5;
                [s, s]
            })
            .collect()
    }

    #[test]
    fn resamples_44100_to_48000_length() {
        let mut a = StreamAligner::new(0, 44_100, 2);
        let mut total = 0usize;
        for b in 0..100 {
            let pts = (b as u64 * 441) * 1_000_000_000 / 44_100;
            total += a.push(pts, &sine(44_100, 441, b * 441)).len() / 2;
        }
        // 1 second of input -> about 48000 frames out (minus the interpolator's 2-frame tail).
        assert!((total as i64 - 48_000).abs() < 40, "got {total}");
    }

    #[test]
    fn gap_is_filled_with_silence() {
        let mut a = StreamAligner::new(0, 48_000, 2);
        let mut total = a.push(0, &sine(48_000, 4_800, 0)).len() / 2;
        // 100 ms of nothing, then audio resumes at t = 200 ms.
        total += a.push(200_000_000, &sine(48_000, 4_800, 0)).len() / 2;
        assert!((total as i64 - 14_400).abs() < 40, "got {total}");
        assert!(a.stats.silence_inserted >= 4_700);
    }

    #[test]
    fn late_start_gets_leading_silence() {
        let mut a = StreamAligner::new(0, 48_000, 2);
        let out = a.push(500_000_000, &sine(48_000, 480, 0));
        assert!(out.len() / 2 >= 24_000);
        assert!(out[..24_000 * 2].iter().all(|&s| s == 0.0));
    }

    #[test]
    fn audio_before_origin_is_trimmed() {
        let mut a = StreamAligner::new(1_000_000_000, 48_000, 2);
        assert!(a.push(0, &sine(48_000, 4_800, 0)).is_empty());
        let out = a.push(950_000_000, &sine(48_000, 4_800, 0));
        // 50 ms before the origin is dropped, 50 ms remain.
        assert!((out.len() as i64 / 2 - 2_400).abs() < 40, "got {}", out.len() / 2);
    }

    #[test]
    fn finish_pads_to_video_length() {
        let mut a = StreamAligner::new(0, 48_000, 1);
        a.push(0, &vec![0.1; 4_800]);
        let tail = a.finish(1_000_000_000);
        assert_eq!((a.frames_emitted()), 48_000);
        assert!(!tail.is_empty());
    }
}
