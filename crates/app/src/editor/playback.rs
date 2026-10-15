//! Audio playback for the editor: the edited soundtrack, rendered ahead of time by a worker
//! thread into a small ring buffer that the output device drains.

use cpal::traits::{DeviceTrait, HostTrait, StreamTrait};
use screencraft_engine::audio_mix::Soundtrack;
use std::collections::VecDeque;
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::{Arc, Mutex};
use std::time::Duration;

struct Shared {
    buf: Mutex<VecDeque<f32>>,
    generation: AtomicU64,
}

pub struct AudioPlayback {
    _stream: cpal::Stream,
    shared: Arc<Shared>,
    rate: u32,
    channels: usize,
}

impl AudioPlayback {
    pub fn new() -> Option<Self> {
        let host = cpal::default_host();
        let device = host.default_output_device()?;
        let conf = device.default_output_config().ok()?;
        let rate = conf.sample_rate();
        let channels = conf.channels() as usize;
        let shared = Arc::new(Shared { buf: Mutex::new(VecDeque::new()), generation: AtomicU64::new(0) });
        let sc = conf.config();
        let sh = shared.clone();
        let fill = move |out: &mut [f32]| {
            let mut b = sh.buf.lock().unwrap();
            for s in out.iter_mut() {
                *s = b.pop_front().unwrap_or(0.0);
            }
        };
        let err = |e| eprintln!("[screencraft] audio output error: {e}");
        let stream = match conf.sample_format() {
            cpal::SampleFormat::F32 => {
                let fill = fill;
                device.build_output_stream(&sc, move |d: &mut [f32], _| fill(d), err, None)
            }
            cpal::SampleFormat::I16 => {
                let fill = fill;
                let mut tmp: Vec<f32> = Vec::new();
                device.build_output_stream(
                    &sc,
                    move |d: &mut [i16], _| {
                        tmp.resize(d.len(), 0.0);
                        fill(&mut tmp);
                        for (o, s) in d.iter_mut().zip(&tmp) {
                            *o = (s.clamp(-1.0, 1.0) * 32767.0) as i16;
                        }
                    },
                    err,
                    None,
                )
            }
            _ => return None,
        }
        .ok()?;
        stream.play().ok()?;
        Some(Self { _stream: stream, shared, rate, channels })
    }

    /// Start playing `soundtrack` from output time `start_ms`. Replaces whatever played before.
    pub fn play(&mut self, mut soundtrack: Soundtrack, start_ms: f64) {
        let gen = self.shared.generation.fetch_add(1, Ordering::SeqCst) + 1;
        self.shared.buf.lock().unwrap().clear();
        let shared = self.shared.clone();
        let (rate, channels) = (self.rate, self.channels);
        std::thread::Builder::new()
            .name("screencraft-playback".into())
            .spawn(move || {
                let mut pos = (start_ms * 48.0).max(0.0) as u64;
                let chunk = 4800usize; // 100 ms
                let target = (rate as usize * channels) * 6 / 10; // ~600 ms buffered
                // Linear resampler state: fractional read position across chunks.
                let step = 48_000.0 / rate as f64;
                let mut frac = 0.0f64;
                let mut carry: Option<[f32; 2]> = None;
                while shared.generation.load(Ordering::SeqCst) == gen {
                    if shared.buf.lock().unwrap().len() >= target {
                        std::thread::sleep(Duration::from_millis(15));
                        continue;
                    }
                    let pcm = soundtrack.render(pos, chunk);
                    pos += chunk as u64;
                    let mut out: Vec<f32> = Vec::with_capacity(chunk * channels * 2);
                    let n = pcm.len() / 2;
                    let at = |i: isize| -> [f32; 2] {
                        if i < 0 {
                            carry.unwrap_or([0.0, 0.0])
                        } else {
                            let i = (i as usize).min(n - 1);
                            [pcm[i * 2], pcm[i * 2 + 1]]
                        }
                    };
                    while frac < n as f64 - 1.0 {
                        let i = frac.floor() as isize;
                        let t = (frac - i as f64) as f32;
                        let (a, b) = (at(i), at(i + 1));
                        let l = a[0] * (1.0 - t) + b[0] * t;
                        let r = a[1] * (1.0 - t) + b[1] * t;
                        match channels {
                            1 => out.push((l + r) * 0.5),
                            2 => {
                                out.push(l);
                                out.push(r);
                            }
                            c => {
                                out.push(l);
                                out.push(r);
                                out.extend(std::iter::repeat(0.0).take(c - 2));
                            }
                        }
                        frac += step;
                    }
                    frac -= n as f64;
                    carry = Some([pcm[(n - 1) * 2], pcm[(n - 1) * 2 + 1]]);
                    if shared.generation.load(Ordering::SeqCst) != gen {
                        break;
                    }
                    shared.buf.lock().unwrap().extend(out);
                }
            })
            .ok();
    }

    pub fn stop(&mut self) {
        self.shared.generation.fetch_add(1, Ordering::SeqCst);
        self.shared.buf.lock().unwrap().clear();
    }
}
