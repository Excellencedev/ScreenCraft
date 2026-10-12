// Loudness helpers shared by the editor preview and the exporter.
// Microphones on laptops record very quietly, so each mic track is levelled
// automatically (never turned down) before the master volume is applied.

/** Master slider: 50% is the neutral position, 100% doubles the level. */
export const masterGain = (v) => Math.max(0, v ?? 0.5) * 2;

/** Linear gain that brings the loud parts of `buf` up to a healthy speech level. */
export function autoGain(buf) {
  const sr = buf.sampleRate;
  const win = Math.max(1, Math.round(sr * 0.05));
  // measured on the mono mix, which is how the microphone is played (see monoMix)
  const n = buf.numberOfChannels;
  const mix = new Float32Array(buf.length);
  for (let c = 0; c < n; c++) {
    const d = buf.getChannelData(c);
    for (let i = 0; i < mix.length; i++) mix[i] += d[i] / n;
  }
  const ch = [mix];
  const rms = [];
  let peak = 0;
  for (let i = 0; i + win <= buf.length; i += win) {
    let sum = 0;
    for (const d of ch) {
      for (let k = i; k < i + win; k++) {
        const x = d[k];
        sum += x * x;
        const a = x < 0 ? -x : x;
        if (a > peak) peak = a;
      }
    }
    rms.push(Math.sqrt(sum / (win * ch.length)));
  }
  if (!rms.length || peak < 1e-5) return 1;
  rms.sort((a, b) => a - b);
  // loudness of the top fifth of windows ≈ the speech, ignoring silence
  const top = rms.slice(Math.floor(rms.length * 0.8));
  const speech = top.reduce((s, v) => s + v, 0) / top.length;
  const byLevel = 0.1 / Math.max(speech, 1e-5);
  const byPeak = 0.9 / peak;
  return Math.min(60, Math.max(1, Math.min(byLevel, byPeak)));
}

/** A limiter so boosted audio never clips. */
export function makeLimiter(ctx) {
  const c = ctx.createDynamicsCompressor();
  c.threshold.value = -3;
  c.knee.value = 0;
  c.ratio.value = 20;
  c.attack.value = 0.003;
  c.release.value = 0.15;
  return c;
}

/**
 * The microphone is always played as a centred signal: its channels are averaged and
 * sent to both ears. (Many mics deliver two channels with one of them empty, which
 * otherwise sounds like left-ear-only audio.)
 */
export function monoMix(ctx) {
  const g = ctx.createGain();
  g.channelCount = 1;
  g.channelCountMode = "explicit";
  g.channelInterpretation = "speakers";
  return g;
}
