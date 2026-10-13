//! Pace tools: finding idle stretches of a recording.

use crate::commands::projects::read_input;
use crate::state::{AppState, CmdResult};
use screencraft_core::pace::idle_segments;
use screencraft_core::project::Project;
use std::path::Path;
use tauri::State;

/// Per-100 ms "someone is speaking" flags from the levelled microphone WAV (16-bit PCM).
fn speech_flags(dir: &Path, mic: &str, mic_offset_ms: f64, duration_ms: f64) -> Option<Vec<bool>> {
    // make sure the levelled file exists (cheap when it is already there)
    let name = crate::levelmic::level(dir, mic).ok()?;
    let bytes = std::fs::read(dir.join(name)).ok()?;
    if bytes.len() < 48 || &bytes[0..4] != b"RIFF" {
        return None;
    }
    // find the "data" chunk and the format
    let mut pos = 12;
    let (mut channels, mut rate) = (2usize, 48000usize);
    let mut data: Option<&[u8]> = None;
    while pos + 8 <= bytes.len() {
        let id = &bytes[pos..pos + 4];
        let len = u32::from_le_bytes(bytes[pos + 4..pos + 8].try_into().ok()?) as usize;
        let body = pos + 8;
        if id == b"fmt " && body + 16 <= bytes.len() {
            channels = u16::from_le_bytes(bytes[body + 2..body + 4].try_into().ok()?) as usize;
            rate = u32::from_le_bytes(bytes[body + 4..body + 8].try_into().ok()?) as usize;
        } else if id == b"data" {
            data = Some(&bytes[body..(body + len).min(bytes.len())]);
            break;
        }
        pos = body + len + (len & 1);
    }
    let data = data?;
    let frame = 2 * channels.max(1);
    let per_bucket = (rate as f64 * 0.1) as usize;
    if per_bucket == 0 {
        return None;
    }
    let n = (duration_ms / 100.0).ceil() as usize;
    // loudness of every 100 ms bucket of the (levelled) microphone
    let mut rms = vec![0.0f64; n];
    for (i, r) in rms.iter_mut().enumerate() {
        // microphone time = source time + offset
        let start = (((i as f64 * 100.0 + mic_offset_ms) / 1000.0) * rate as f64).round();
        if start < 0.0 {
            continue;
        }
        let a = start as usize * frame;
        let b = (a + per_bucket * frame).min(data.len());
        if a >= b {
            continue;
        }
        let mut sum = 0.0f64;
        let mut cnt = 0usize;
        for s in data[a..b].chunks_exact(frame) {
            let v = i16::from_le_bytes([s[0], s[1]]) as f64 / 32768.0;
            sum += v * v;
            cnt += 1;
        }
        if cnt > 0 {
            *r = (sum / cnt as f64).sqrt();
        }
    }
    // levelling lifts a quiet room to a healthy level, so judge against the room's own noise
    // floor (the 20th percentile) rather than a fixed number
    let mut sorted: Vec<f64> = rms.iter().copied().filter(|v| *v > 0.0).collect();
    sorted.sort_by(|a, b| a.partial_cmp(b).unwrap());
    let floor = sorted.get(sorted.len() / 5).copied().unwrap_or(0.0);
    let thr = (floor * 3.0).max(0.03);
    let mut out: Vec<bool> = rms.iter().map(|v| *v > thr).collect();
    // bridge short gaps between words so a sentence reads as one stretch
    let gap = 4;
    let mut last: Option<usize> = None;
    for i in 0..out.len() {
        if out[i] {
            if let Some(l) = last {
                if i - l <= gap {
                    for v in out.iter_mut().take(i).skip(l) {
                        *v = true;
                    }
                }
            }
            last = Some(i);
        }
    }
    Some(out)
}

/// Idle stretches `[start, end]` in source ms for the project's recording.
#[tauri::command]
pub async fn find_idle(
    state: State<'_, AppState>,
    dir: String,
    project: Project,
    min_idle_ms: f64,
    keep_speech: bool,
) -> CmdResult<Vec<[f64; 2]>> {
    let log = read_input(&state, Path::new(&dir), &project.media.input);
    let dur = project.media.duration_ms;
    let speech = if keep_speech {
        project
            .media
            .mic
            .as_deref()
            .and_then(|m| speech_flags(Path::new(&dir), m, project.media.mic_offset_ms, dur))
    } else {
        None
    };
    Ok(idle_segments(&log, dur, speech.as_deref(), min_idle_ms.max(500.0), 250.0))
}
