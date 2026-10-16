//! On-device caption generation with whisper.cpp (through the `whisper-rs` bindings).
//!
//! Nothing is uploaded. The only network request is the optional, user-started download of a
//! ggml model file; after that the speech recognition runs entirely on this machine.

use screencraft_core::project::Project;
use screencraft_engine::Package;
use std::io::{Read, Write};
use std::path::{Path, PathBuf};
use std::sync::mpsc::Sender;
use whisper_rs::{FullParams, SamplingStrategy, WhisperContext, WhisperContextParameters};

pub struct Caption {
    pub text: String,
    pub start_ms: f64,
    pub end_ms: f64,
    /// Start of each word in `text`, on the same clock as `start_ms`.
    pub word_starts: Vec<f64>,
}

pub enum CapEvent {
    Progress(String),
    Done(Result<Vec<Caption>, String>),
}

pub struct Request {
    pub package: Package,
    pub project: Project,
    pub model: String,
    pub language: String,
}

/// (id, file, approximate bytes)
const MODELS: &[(&str, &str, u64)] = &[
    ("tiny.en", "ggml-tiny.en.bin", 77_704_715),
    ("base.en", "ggml-base.en.bin", 147_964_211),
    ("small.en", "ggml-small.en.bin", 487_614_201),
    ("base", "ggml-base.bin", 147_951_465),
    ("large-v3-turbo-q5_0", "ggml-large-v3-turbo-q5_0.bin", 574_041_195),
];
const BASE_URL: &str = "https://huggingface.co/ggerganov/whisper.cpp/resolve/main/";

fn model_dir() -> Result<PathBuf, String> {
    let dir = dirs::data_dir().ok_or("no data folder")?.join("ScreenCraft").join("models");
    std::fs::create_dir_all(&dir).map_err(|e| e.to_string())?;
    Ok(dir)
}

fn download(tx: &Sender<CapEvent>, id: &str) -> Result<PathBuf, String> {
    let (_, file, approx) = *MODELS.iter().find(|m| m.0 == id).ok_or_else(|| format!("unknown model {id}"))?;
    let dest = model_dir()?.join(file);
    if dest.exists() {
        return Ok(dest);
    }
    // ClipCraft keeps the same ggml files; reuse them instead of downloading again
    if let Some(data) = dirs::data_dir() {
        let sib = data.join("dev.clipcraft.app").join("models").join(file);
        if sib.exists() && (std::fs::hard_link(&sib, &dest).is_ok() || std::fs::copy(&sib, &dest).is_ok()) {
            return Ok(dest);
        }
    }
    let part = dest.with_extension("part");
    let resp = ureq::get(&format!("{BASE_URL}{file}")).call().map_err(|e| format!("download failed: {e}"))?;
    let total = resp.header("content-length").and_then(|v| v.parse::<u64>().ok()).unwrap_or(approx);
    let mut reader = resp.into_reader();
    let mut out = std::fs::File::create(&part).map_err(|e| e.to_string())?;
    let mut buf = vec![0u8; 256 * 1024];
    let (mut done, mut last_pct) = (0u64, -1i64);
    loop {
        let n = reader.read(&mut buf).map_err(|e| format!("download interrupted: {e}"))?;
        if n == 0 {
            break;
        }
        out.write_all(&buf[..n]).map_err(|e| e.to_string())?;
        done += n as u64;
        let pct = (done as f64 / total as f64 * 100.0).min(100.0);
        if pct as i64 != last_pct {
            last_pct = pct as i64;
            let _ = tx.send(CapEvent::Progress(format!("Downloading model {}%", pct.round())));
        }
    }
    out.flush().map_err(|e| e.to_string())?;
    drop(out);
    // a truncated file would crash the model loader, so refuse anything far from the expected size
    if done < approx / 2 {
        let _ = std::fs::remove_file(&part);
        return Err("the model download was incomplete".into());
    }
    std::fs::rename(&part, &dest).map_err(|e| e.to_string())?;
    Ok(dest)
}

/// Box-average downsample to 16 kHz, the rate whisper expects.
fn to_16k(mono: &[f32], rate: u32) -> Vec<f32> {
    if rate == 16_000 {
        return mono.to_vec();
    }
    let ratio = rate as f64 / 16_000.0;
    let n = (mono.len() as f64 / ratio) as usize;
    let mut out = Vec::with_capacity(n);
    for i in 0..n {
        let a = (i as f64 * ratio) as usize;
        let b = (((i + 1) as f64 * ratio) as usize).clamp(a + 1, mono.len());
        out.push(mono[a..b].iter().sum::<f32>() / (b - a) as f32);
    }
    out
}

/// Run the model over 16 kHz mono samples. Timestamps are shifted back by `offset_ms`
/// (the microphone clock runs `offset_ms` ahead of the recording's).
fn recognize(model_file: &Path, pcm: &[f32], model: &str, language: &str, offset_ms: f64, on_progress: impl FnMut(i32) + 'static) -> Result<Vec<Caption>, String> {
    let ctx = WhisperContext::new_with_params(model_file.to_str().ok_or("bad model path")?, WhisperContextParameters::default()).map_err(|e| format!("could not load the model: {e}"))?;
    let mut state = ctx.create_state().map_err(|e| e.to_string())?;
    let mut params = FullParams::new(SamplingStrategy::Greedy { best_of: 1 });
    let threads = std::thread::available_parallelism().map(|n| n.get() as i32).unwrap_or(4).min(8);
    params.set_n_threads(threads);
    let lang = if model.ends_with(".en") { "en" } else { language };
    params.set_language(Some(lang));
    params.set_translate(false);
    params.set_print_special(false);
    params.set_print_progress(false);
    params.set_print_realtime(false);
    params.set_print_timestamps(false);
    params.set_suppress_blank(true);
    params.set_suppress_nst(true);
    // one segment per word with token timestamps: exact word times let us cut clean caption
    // lines ourselves. No context keeps one wrong segment from derailing the rest.
    params.set_no_context(true);
    params.set_token_timestamps(true);
    params.set_max_len(1);
    params.set_split_on_word(true);
    params.set_initial_prompt("Hello, and welcome. Today, we're going to talk about something important.");
    params.set_progress_callback_safe(on_progress);
    state.full(params, pcm).map_err(|e| format!("transcription failed: {e}"))?;

    let mut words: Vec<(String, f64, f64)> = Vec::new();
    for seg in state.as_iter() {
        let text = seg.to_str_lossy().map_err(|e| e.to_string())?.trim().to_string();
        // whisper emits markers like [BLANK_AUDIO] / (music) for non-speech
        if text.is_empty() || text.starts_with("[_") || (text.starts_with('[') && text.ends_with(']')) || (text.starts_with('(') && text.ends_with(')')) || text.starts_with('♪') {
            continue;
        }
        // centiseconds on the microphone clock; the project uses the source clock
        let s = (seg.start_timestamp() as f64 * 10.0 - offset_ms).max(0.0);
        let e = seg.end_timestamp() as f64 * 10.0 - offset_ms;
        if e <= 0.0 {
            continue;
        }
        words.push((text, s, e));
    }
    Ok(chunk(clean(words)))
}

/// Make the word times monotone without overlaps.
fn clean(raw: Vec<(String, f64, f64)>) -> Vec<(String, f64, f64)> {
    let mut out: Vec<(String, f64, f64)> = Vec::with_capacity(raw.len());
    for (text, mut t0, mut t1) in raw {
        if let Some(prev) = out.last_mut() {
            if t0 < prev.1 {
                t0 = prev.1;
            }
            if t0 < prev.2 {
                if t0 >= prev.1 + 40.0 {
                    prev.2 = t0;
                } else {
                    t0 = prev.2;
                }
            }
        }
        t1 = t1.max(t0 + 40.0);
        out.push((text, t0, t1));
    }
    out
}

/// Group words into short on-screen lines: at most 3 words / 24 characters, broken at sentence
/// ends, commas and pauses, and held briefly so lines never flicker.
pub fn chunk(words: Vec<(String, f64, f64)>) -> Vec<Caption> {
    const MAX_WORDS: usize = 3;
    let mut out: Vec<Caption> = Vec::new();
    let mut cur: Vec<(String, f64, f64)> = Vec::new();
    let mut chars = 0usize;
    let flush = |cur: &mut Vec<(String, f64, f64)>, out: &mut Vec<Caption>, chars: &mut usize| {
        if cur.is_empty() {
            return;
        }
        let text = cur.iter().map(|w| w.0.as_str()).collect::<Vec<_>>().join(" ");
        let word_starts = cur.iter().map(|w| w.1).collect();
        out.push(Caption { text, start_ms: cur[0].1, end_ms: cur[cur.len() - 1].2, word_starts });
        cur.clear();
        *chars = 0;
    };
    let mut last_end: Option<f64> = None;
    for (text, t0, t1) in words {
        let gap = last_end.map(|e| t0 - e).unwrap_or(0.0);
        let n = text.chars().count();
        if !cur.is_empty() && (gap > 450.0 || cur.len() >= MAX_WORDS || chars + n > 24) {
            flush(&mut cur, &mut out, &mut chars);
        }
        chars += n + 1;
        last_end = Some(t1);
        let t = text.trim_end_matches(['"', '\'', ')', '”', '’']).to_string();
        cur.push((text, t0, t1));
        let hard = t.ends_with('.') || t.ends_with('?') || t.ends_with('!');
        let soft = t.ends_with(',') || t.ends_with(';') || t.ends_with(':') || t.ends_with('—');
        if hard || (soft && cur.len() >= 2) {
            flush(&mut cur, &mut out, &mut chars);
        }
    }
    flush(&mut cur, &mut out, &mut chars);
    // hold each line until the next starts when the gap is short, else a beat longer
    for i in 0..out.len() {
        let hold = out[i].end_ms + 320.0;
        out[i].end_ms = match out.get(i + 1).map(|c| c.start_ms) {
            Some(n) if n - out[i].end_ms < 320.0 => n,
            _ => hold,
        };
    }
    out
}

/// The speech to transcribe as mono samples + rate, and the microphone's clock offset.
fn speech_audio(package: &Package, project: &Project) -> Result<(Vec<f32>, u32, f64), String> {
    let m = &project.media;
    // a separate microphone file (0.3 recordings)
    if let Some(rel) = m.mic.as_deref() {
        let p = package.dir.join(rel);
        if p.exists() {
            let pcm = screencraft_audio::decode_file(&p)?;
            let mono: Vec<f32> = pcm.chunks_exact(2).map(|f| (f[0] + f[1]) * 0.5).collect();
            return Ok((mono, 48_000, m.mic_offset_ms));
        }
    }
    // the microphone (else system audio) track of the screen recording
    let track = m.mic_track.or(m.system_track).ok_or("this recording has no audio to transcribe")?;
    let file = std::sync::Arc::new(screencraft_encode::mp4read::Mp4File::open(&package.screen_path(project)).map_err(|e| e.to_string())?);
    let mut src = screencraft_engine::AudioSource::open(file, track as usize)?;
    let total = src.duration_ms;
    let mut mono = Vec::with_capacity((total * 48.0) as usize);
    let mut t = 0.0;
    while t < total {
        let pcm = src.read(t, 48_000)?;
        mono.extend(pcm.chunks_exact(2).map(|f| (f[0] + f[1]) * 0.5));
        t += 1000.0;
    }
    Ok((mono, 48_000, if m.mic_track.is_some() { m.mic_offset_ms } else { 0.0 }))
}

fn transcribe(tx: &Sender<CapEvent>, req: &Request) -> Result<Vec<Caption>, String> {
    let model_file = download(tx, &req.model)?;
    let _ = tx.send(CapEvent::Progress("Reading audio…".into()));
    let (mono, rate, offset_ms) = speech_audio(&req.package, &req.project)?;
    let pcm = to_16k(&mono, rate);
    drop(mono);
    let _ = tx.send(CapEvent::Progress("Loading model…".into()));
    let tx2 = tx.clone();
    recognize(&model_file, &pcm, &req.model, &req.language, offset_ms, move |p| {
        let _ = tx2.send(CapEvent::Progress(format!("Transcribing {p}%")));
    })
}

pub fn spawn(tx: Sender<CapEvent>, req: Request) {
    std::thread::Builder::new()
        .name("screencraft-whisper".into())
        .spawn(move || {
            let r = transcribe(&tx, &req);
            let _ = tx.send(CapEvent::Done(r));
        })
        .ok();
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn chunks_words_into_short_lines() {
        let words: Vec<(String, f64, f64)> = ["Hello", "there,", "this", "is", "a", "test."].iter().enumerate().map(|(i, w)| (w.to_string(), i as f64 * 300.0, i as f64 * 300.0 + 250.0)).collect();
        let caps = chunk(words);
        assert!(caps.len() >= 2);
        assert!(caps.iter().all(|c| c.text.split_whitespace().count() <= 3));
        assert_eq!(caps[0].text, "Hello there,");
        // each line holds until the next one starts
        assert_eq!(caps[0].end_ms, caps[1].start_ms);
    }
}
