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

function wavBlob(buf) {
  const n = buf.length, ch = 2, bytes = n * ch * 2;
  const ab = new ArrayBuffer(44 + bytes);
  const v = new DataView(ab);
  const w = (o, t) => { for (let i = 0; i < t.length; i++) v.setUint8(o + i, t.charCodeAt(i)); };
  w(0, "RIFF"); v.setUint32(4, 36 + bytes, true); w(8, "WAVE"); w(12, "fmt ");
  v.setUint32(16, 16, true); v.setUint16(20, 1, true); v.setUint16(22, ch, true);
  v.setUint32(24, buf.sampleRate, true); v.setUint32(28, buf.sampleRate * ch * 2, true);
  v.setUint16(32, ch * 2, true); v.setUint16(34, 16, true); w(36, "data"); v.setUint32(40, bytes, true);
  const l = buf.getChannelData(0), r = buf.getChannelData(buf.numberOfChannels > 1 ? 1 : 0);
  let o = 44;
  for (let i = 0; i < n; i++) {
    v.setInt16(o, Math.max(-1, Math.min(1, l[i])) * 32767, true);
    v.setInt16(o + 2, Math.max(-1, Math.min(1, r[i])) * 32767, true);
    o += 4;
  }
  return new Blob([ab], { type: "audio/wav" });
}

/**
 * Pre-render the microphone: centred in both ears, levelled and limited, at the loudness
 * the "100%" master position needs. Playback can then use a plain <audio> element
 * (volume <= 1) instead of routing it through WebAudio, which stutters when the
 * main thread is busy drawing.
 */
export async function bakeMic(buf, level) {
  const gain = (level ?? autoGain(buf)) * 2;
  const off = new OfflineAudioContext(2, buf.length, buf.sampleRate);
  const src = off.createBufferSource();
  src.buffer = buf;
  const g = off.createGain();
  g.gain.value = gain;
  src.connect(monoMix(off)).connect(g).connect(makeLimiter(off)).connect(off.destination);
  src.start();
  const out = await off.startRendering();
  return URL.createObjectURL(await wavBlobAsync(out));
}

const breathe = () => new Promise((r) => setTimeout(r, 0));

/** autoGain that yields to the page regularly (for long recordings in the editor). */
export async function autoGainAsync(buf) {
  const sr = buf.sampleRate;
  const win = Math.max(1, Math.round(sr * 0.05));
  const n = buf.numberOfChannels;
  const chs = [];
  for (let c = 0; c < n; c++) chs.push(buf.getChannelData(c));
  const rms = [];
  let peak = 0;
  let t0 = performance.now();
  for (let i = 0; i + win <= buf.length; i += win) {
    let sum = 0;
    for (let k = i; k < i + win; k++) {
      let m = 0;
      for (let c = 0; c < n; c++) m += chs[c][k];
      m /= n;
      sum += m * m;
      const a = m < 0 ? -m : m;
      if (a > peak) peak = a;
    }
    rms.push(Math.sqrt(sum / win));
    if (performance.now() - t0 > 12) {
      await breathe();
      t0 = performance.now();
    }
  }
  if (!rms.length || peak < 1e-5) return 1;
  rms.sort((a, b) => a - b);
  const top = rms.slice(Math.floor(rms.length * 0.8));
  const speech = top.reduce((s, v) => s + v, 0) / top.length;
  return Math.min(60, Math.max(1, Math.min(0.1 / Math.max(speech, 1e-5), 0.9 / peak)));
}

async function wavBlobAsync(buf) {
  const n = buf.length, ch = 2, bytes = n * ch * 2;
  const ab = new ArrayBuffer(44 + bytes);
  const v = new DataView(ab);
  const w = (o, t) => { for (let i = 0; i < t.length; i++) v.setUint8(o + i, t.charCodeAt(i)); };
  w(0, "RIFF"); v.setUint32(4, 36 + bytes, true); w(8, "WAVE"); w(12, "fmt ");
  v.setUint32(16, 16, true); v.setUint16(20, 1, true); v.setUint16(22, ch, true);
  v.setUint32(24, buf.sampleRate, true); v.setUint32(28, buf.sampleRate * ch * 2, true);
  v.setUint16(32, ch * 2, true); v.setUint16(34, 16, true); w(36, "data"); v.setUint32(40, bytes, true);
  const l = buf.getChannelData(0), r = buf.getChannelData(buf.numberOfChannels > 1 ? 1 : 0);
  let o = 44;
  let t0 = performance.now();
  for (let i = 0; i < n; i++) {
    v.setInt16(o, Math.max(-1, Math.min(1, l[i])) * 32767, true);
    v.setInt16(o + 2, Math.max(-1, Math.min(1, r[i])) * 32767, true);
    o += 4;
    if ((i & 8191) === 0 && performance.now() - t0 > 12) {
      await breathe();
      t0 = performance.now();
    }
  }
  return new Blob([ab], { type: "audio/wav" });
}
