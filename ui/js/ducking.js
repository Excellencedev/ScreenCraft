// Music ducking: find where someone is speaking (microphone level, no ML) and turn that
// into gain automation for the music. Shared by the editor preview and by export.

/**
 * Stretches of `buf` (an AudioBuffer) whose level is above a speech threshold, in
 * microphone-buffer milliseconds. `gain` is the same levelling factor the mix applies.
 */
export function speechSegments(buf, gain = 1, minRms = 0.02) {
  if (!buf || !buf.length) return [];
  const ch = buf.getChannelData(0);
  const win = Math.max(1, Math.round(buf.sampleRate * 0.05));
  const n = Math.floor(ch.length / win);
  const lvl = new Float32Array(n);
  for (let i = 0; i < n; i++) {
    let s = 0;
    for (let j = i * win; j < (i + 1) * win; j++) s += ch[j] * ch[j];
    lvl[i] = Math.sqrt(s / win) * gain;
  }
  // judge against the room's own noise floor (20th percentile), not a fixed level
  const sorted = Float32Array.from(lvl).sort();
  const floor = sorted[Math.floor(n / 5)] || 0;
  const threshold = Math.max(minRms, floor * 3);
  const loud = new Array(n);
  for (let i = 0; i < n; i++) loud[i] = lvl[i] > threshold;
  // hold 300 ms after the last loud window and bridge gaps up to 350 ms
  const segs = [];
  let start = -1, last = -1;
  const holdWins = 6, bridge = 7;
  for (let i = 0; i < n; i++) {
    if (!loud[i]) continue;
    if (start < 0) start = i;
    else if (i - last > bridge) {
      segs.push([start * 50, (last + holdWins) * 50]);
      start = i;
    }
    last = i;
  }
  if (start >= 0) segs.push([start * 50, (last + holdWins) * 50]);
  return segs.filter(([a, b]) => b - a >= 150);
}

/**
 * Map microphone-time segments to output-timeline ranges through the clip list.
 * `offsetMs` is the mic's offset (mic time = source time + offset).
 */
export function toOutputRanges(segs, offsetMs, slices) {
  const out = [];
  let acc = 0;
  for (const s of slices) {
    const scale = Math.max(1e-3, s.timeScale);
    const len = Math.max(0, s.sourceEndMs - s.sourceStartMs) / scale;
    if (!s.freeze) {
      for (const [a, b] of segs) {
        const lo = Math.max(a - offsetMs, s.sourceStartMs);
        const hi = Math.min(b - offsetMs, s.sourceEndMs);
        if (hi > lo) out.push([acc + (lo - s.sourceStartMs) / scale, acc + (hi - s.sourceStartMs) / scale]);
      }
    }
    acc += len;
  }
  out.sort((x, y) => x[0] - y[0]);
  // merge ranges that touch across clip boundaries
  const merged = [];
  for (const r of out) {
    const l = merged[merged.length - 1];
    if (l && r[0] - l[1] < 120) l[1] = Math.max(l[1], r[1]);
    else merged.push([r[0], r[1]]);
  }
  return merged;
}

const ATTACK_MS = 150, RELEASE_MS = 450;

/**
 * Automate `param` (a GainParam on the music bus): 1 normally, `level` while speech plays.
 * `timeOf(outMs)` maps an output time to AudioContext time; only ranges after `fromOut` are set.
 */
export function scheduleDuck(param, ranges, level, fromOut, timeOf) {
  const lvl = Math.max(0.02, Math.min(1, level));
  let started = false;
  const first = ranges.find((r) => r[1] + RELEASE_MS > fromOut);
  param.cancelScheduledValues(0);
  const inside = first && first[0] - ATTACK_MS <= fromOut;
  param.setValueAtTime(inside ? lvl : 1, timeOf(fromOut));
  started = true;
  for (const [a, b] of ranges) {
    if (b + RELEASE_MS <= fromOut) continue;
    const t0 = a - ATTACK_MS;
    if (t0 > fromOut) {
      param.setValueAtTime(1, timeOf(t0));
      param.linearRampToValueAtTime(lvl, timeOf(a));
    } else if (a > fromOut) {
      param.linearRampToValueAtTime(lvl, timeOf(a));
    }
    param.setValueAtTime(lvl, timeOf(Math.max(fromOut, b)));
    param.linearRampToValueAtTime(1, timeOf(Math.max(fromOut, b) + RELEASE_MS));
  }
  return started;
}
