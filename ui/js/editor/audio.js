// Audio for the preview: everything is decoded once into AudioBuffers and scheduled on a
// WebAudio clock. No <audio>/<video> element is ever played, seeked or re-rated.
import { invoke, convertFileSrc } from "../tauri.js";
import { P, S, dir } from "./state.js";
import { masterGain, makeLimiter, autoGain } from "../audiolevel.js";
import { loadSet, scheduleMarks } from "../sounds.js";

const A = {
  ctx: null,
  limiter: null,
  gain: { dev: null, mic: null, music: null, click: null, key: null },
  fx: { click: null, key: null, clickStyle: "", keyStyle: "" },
  play: null, // { t0, fromOut, rate, horizon }
  bufs: { dev: null, mic: null, micExtra: 1 },
  music: new Map(), // file -> Promise<AudioBuffer|null>
  nodes: [],
};

function ensureCtx() {
  if (A.ctx) return A.ctx;
  A.ctx = new AudioContext({ sampleRate: 48000 });
  A.limiter = makeLimiter(A.ctx);
  A.limiter.connect(A.ctx.destination);
  for (const k of ["dev", "mic", "music", "click", "key"]) {
    A.gain[k] = A.ctx.createGain();
    A.gain[k].connect(A.limiter);
  }
  return A.ctx;
}

async function decode(url) {
  try {
    const bytes = await (await fetch(url)).arrayBuffer();
    return await ensureCtx().decodeAudioData(bytes);
  } catch {
    return null;
  }
}

/** Decode (once) a music file from the project folder. */
export function getMusicBuffer(file) {
  if (!A.music.has(file)) A.music.set(file, decode(convertFileSrc(`${dir}/${file}`)));
  return A.music.get(file);
}

/** Decode device audio + the (natively levelled) microphone. Safe to call in the background. */
export async function loadAudio() {
  ensureCtx();
  const m = P().media;
  const jobs = [];
  if (m.hasSystemAudio) jobs.push(decode(convertFileSrc(`${dir}/${m.screen}`)).then((b) => (A.bufs.dev = b)));
  if (m.mic) {
    jobs.push(
      (async () => {
        let url = convertFileSrc(`${dir}/${m.mic}`);
        let leveled = false;
        try {
          const name = await invoke("level_mic", { dir, file: m.mic });
          url = convertFileSrc(`${dir}/${name}`);
          leveled = true;
        } catch {}
        const b = await decode(url);
        A.bufs.mic = b;
        A.bufs.micExtra = b && !leveled ? autoGain(b) : 1;
      })(),
    );
  }
  await Promise.all(jobs);
  await loadMusic();
  applyLevels();
}

export async function loadMusic() {
  await Promise.all((P().scene.musicClips || []).map((c) => getMusicBuffer(c.file)));
}

/** Track levels from the current project settings (live: safe while playing). */
export function applyLevels() {
  if (!A.ctx) return;
  const a = P().config.audio;
  const master = masterGain(a.volume);
  A.gain.dev.gain.value = master * a.deviceVolume;
  A.gain.mic.gain.value = master * a.micVolume * A.bufs.micExtra;
  A.gain.music.gain.value = master * (a.musicVolume ?? 0.5);
  A.gain.click.gain.value = master * (a.clickVolume ?? 0.6);
  A.gain.key.gain.value = master * (a.keyVolume ?? 0.5);
}

/** Decode the selected click / typing sound sets (no-op when already loaded). */
async function loadEffects() {
  const ctx = ensureCtx();
  const a = P().config.audio;
  if (a.clickSounds && A.fx.clickStyle !== a.clickStyle) {
    A.fx.click = await loadSet(ctx, "click", a.clickStyle || "soft");
    A.fx.clickStyle = a.clickStyle;
  }
  if (a.keySounds && A.fx.keyStyle !== a.keyStyle) {
    A.fx.key = await loadSet(ctx, "key", a.keyStyle || "mechanical");
    A.fx.keyStyle = a.keyStyle;
  }
}

const FX_WINDOW_MS = 90000;
/** Place click / typing effects between output times `from` and `to` on the audio clock. */
function scheduleEffects(from, to) {
  const ctx = A.ctx, p = A.play;
  const a = P().config.audio;
  const tr = S.track;
  if (!ctx || !p || !tr) return;
  const when = (t) => p.t0 + (t - p.fromOut) / 1000 / p.rate;
  if (a.clickSounds && A.fx.click) {
    A.nodes.push(...scheduleMarks(ctx, A.gain.click, tr.clicks || [], A.fx.click, from, to, when));
  }
  if (a.keySounds && A.fx.key) {
    A.nodes.push(...scheduleMarks(ctx, A.gain.key, (tr.keys || []).map((k) => ({ tOutMs: k.tOutMs, kind: k.kind })), A.fx.key, from, to, when));
  }
}

/** Called regularly while playing: keeps effects scheduled ahead of the playhead. */
export function topUpEffects(tOut) {
  const p = A.play;
  if (!p) return;
  if (tOut + 20000 < p.horizon) return;
  const next = tOut + FX_WINDOW_MS;
  scheduleEffects(p.horizon, next);
  p.horizon = next;
}

/** Play one hit of a sound set (used when the style is picked in the Audio panel). */
export async function previewSound(kind, style) {
  const ctx = ensureCtx();
  await ctx.resume().catch(() => {});
  applyLevels();
  const set = await loadSet(ctx, kind, style);
  const buf = set?.normal?.[0];
  if (!buf) return;
  const src = ctx.createBufferSource();
  src.buffer = buf;
  src.connect(kind === "click" ? A.gain.click : A.gain.key);
  src.start();
}

export function stopAudio() {
  A.play = null;
  for (const n of A.nodes) {
    try { n.stop(); } catch {}
    try { n.disconnect(); } catch {}
  }
  A.nodes = [];
}

/**
 * Schedule every audible piece from output time `fromOut` (ms). Returns the AudioContext
 * time at which `fromOut` is heard.
 */
export async function startAudio(fromOut, leadSec = 0.06, rate = 1) {
  const ctx = ensureCtx();
  await ctx.resume().catch(() => {});
  await loadEffects().catch(() => {});
  stopAudio();
  applyLevels();
  const t0 = ctx.currentTime + leadSec;
  const gr = Math.max(0.25, rate);
  const m = P().media;
  let acc = 0;
  const addNode = (buf, dest, when, offsetSec, durSec, rate, vol) => {
    if (!buf || durSec <= 0.001) return;
    let off = offsetSec;
    let w = when;
    if (off < 0) {
      w += -off / rate;
      durSec += off;
      off = 0;
    }
    if (durSec <= 0.001 || off >= buf.duration) return;
    const src = ctx.createBufferSource();
    src.buffer = buf;
    src.playbackRate.value = rate;
    const g = ctx.createGain();
    g.gain.value = vol;
    src.connect(g).connect(dest);
    src.start(w, off, Math.min(durSec, buf.duration - off));
    A.nodes.push(src);
  };
  for (const s of P().scene.slices) {
    const len = Math.max(0, s.sourceEndMs - s.sourceStartMs) / Math.max(1e-3, s.timeScale);
    const sliceEndOut = acc + len;
    if (sliceEndOut > fromOut) {
      const intoSlice = Math.max(0, fromOut - acc); // output ms already past
      const srcStart = s.sourceStartMs + intoSlice * s.timeScale;
      const srcDur = (s.sourceEndMs - srcStart) / 1000;
      const when = t0 + Math.max(0, acc - fromOut) / 1000 / gr;
      const rate = Math.min(16, Math.max(0.0625, s.timeScale * gr));
      addNode(A.bufs.dev, A.gain.dev, when, srcStart / 1000, srcDur, rate, s.volume);
      addNode(A.bufs.mic, A.gain.mic, when, (srcStart + (m.micOffsetMs || 0)) / 1000, srcDur, rate, s.volume);
    }
    acc = sliceEndOut;
  }
  for (const c of P().scene.musicClips || []) {
    const end = c.offsetMs + c.durMs;
    if (end <= fromOut) continue;
    const startAt = Math.max(c.offsetMs, fromOut);
    getMusicBuffer(c.file).then(() => {}); // ensure requested (no-op when decoded)
    const buf = peekMusic(c.file);
    addNode(buf, A.gain.music, t0 + (startAt - fromOut) / 1000 / gr, (c.inMs + (startAt - c.offsetMs)) / 1000, (end - startAt) / 1000, gr, 1);
  }
  A.play = { t0, fromOut, rate: gr, horizon: fromOut + FX_WINDOW_MS };
  scheduleEffects(fromOut, A.play.horizon);
  return t0;
}

const decoded = new Map();
function peekMusic(file) {
  if (!decoded.has(file)) {
    getMusicBuffer(file).then((b) => decoded.set(file, b));
    return null;
  }
  return decoded.get(file);
}
export async function primeMusic() {
  await Promise.all((P().scene.musicClips || []).map(async (c) => decoded.set(c.file, await getMusicBuffer(c.file))));
}
