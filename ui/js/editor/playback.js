// Playback, seeking and render-track computation.
import { invoke } from "../tauri.js";
import { $, S, P, dir, TRACK_FPS, media, clamp, fmt, hooks } from "./state.js";
import { outDuration, outToSrc, sliceOutStart } from "./timemap.js";
import { draw } from "./draw.js";
import { lookup } from "./scrubcache.js";
import { masterGain, makeLimiter, monoMix } from "../audiolevel.js";

// ---- transport UI (throttled)
let lastTransport = 0;
let tlW = 0;
export function setTimelineWidth(w) {
  tlW = w;
}
export function updateTransport(force = true) {
  const now = performance.now();
  const dur = outDuration();
  if (!tlW) tlW = Math.max(100, $("tlInner").clientWidth);
  $("playhead").style.transform = `translateX(${dur ? (S.tOut / dur) * tlW : 0}px)`;
  if (!force && now - lastTransport < 66) return;
  lastTransport = now;
  $("tCur").textContent = fmt(S.tOut);
}

// ---- render track
let trackTimer = null;
export function scheduleTrack() {
  clearTimeout(trackTimer);
  trackTimer = setTimeout(computeTrack, 50);
}
export async function computeTrack() {
  try {
    S.track = await invoke("compute_render_track", { dir, project: P(), fps: TRACK_FPS });
    S.tOut = clamp(S.tOut, 0, S.track.durationMs);
    $("tDur").textContent = fmt(S.track.durationMs);
    hooks.renderTimeline();
    if (!S.playing) seekOut(S.tOut);
  } catch (e) {
    console.error(e);
  }
}

// ---- seeking: latest request wins, one seek in flight at a time
let seeking = false;
let pending = null;
let seekGuard = 0;

/**
 * Keep a secondary element (camera video, mic) lined up with the screen video without
 * audible skips: small drift is absorbed by nudging its speed, only big drift seeks.
 */
function chase(el, targetMs, base = media.screen.playbackRate) {
  const err = el.currentTime * 1000 - targetMs; // > 0: ahead
  if (Math.abs(err) > 600) {
    el.currentTime = Math.max(0, targetMs / 1000);
    el.playbackRate = base;
  } else if (Math.abs(err) > 35) {
    el.playbackRate = base * (err > 0 ? 0.97 : 1.03);
  } else if (el.playbackRate !== base) {
    el.playbackRate = base;
  }
}

function syncCamera(srcMs) {
  const cam = media.camera;
  if (!cam) return;
  const t = Math.max(0, (srcMs + P().media.cameraOffsetMs) / 1000);
  if (Math.abs(cam.currentTime - t) > 0.15 || !S.playing) cam.currentTime = t;
}

function syncMic(srcMs) {
  const m = media.mic;
  if (!m) return;
  const t = Math.max(0, (srcMs + (P().media.micOffsetMs || 0)) / 1000);
  if (Math.abs(m.currentTime - t) > 0.12 || !S.playing) m.currentTime = t;
}

function pump() {
  if (!pending) return;
  const { src, fast } = pending;
  pending = null;
  const v = media.screen;
  // a hair past 0: some recordings have no frame exactly at 0 and would paint black
  const target = Math.max(src / 1000, 0.05);
  syncCamera(src);
  syncMic(src);
  if (Math.abs(v.currentTime - target) < 0.001) {
    draw();
    if (pending) pump();
    return;
  }
  seeking = true;
  clearTimeout(seekGuard);
  const done = () => {
    v.removeEventListener("seeked", done);
    clearTimeout(seekGuard);
    seeking = false;
    draw();
    if (pending) pump();
  };
  v.addEventListener("seeked", done);
  seekGuard = setTimeout(done, 500); // never get stuck
  if (fast && v.fastSeek) v.fastSeek(target);
  else v.currentTime = target;
}

/** Move the playhead. While dragging pass fast=true for keyframe-only seeks. */
let proxyRaf = 0;
let settleTimer = 0;
export function requestSeek(t, fast = false) {
  S.tOut = clamp(t, 0, outDuration());
  const m = outToSrc(S.tOut);
  S.curSlice = m.idx;
  updateTransport(false);
  clearTimeout(settleTimer);
  const hit = fast && !S.playing ? lookup(m.src) : null;
  if (hit) {
    // paint straight from the in-memory proxy; settle on the exact frame once the pointer rests
    S.proxy = hit;
    pending = null;
    if (!proxyRaf) proxyRaf = requestAnimationFrame(() => { proxyRaf = 0; draw(); });
    settleTimer = setTimeout(() => requestSeek(S.tOut, false), 140);
    return;
  }
  S.proxy = null;
  pending = { src: m.src, fast };
  if (!seeking) pump();
}
export const seekOut = (t) => requestSeek(t, false);

// ---- playback
// Audio goes through WebAudio so the master volume and the mic's auto-level can exceed
// the 100% an <audio>/<video> element allows, with a limiter against clipping.
const bus = { ctx: null, dev: null, mic: null, music: {}, limiter: null };
function ensureAudio() {
  if (bus.ctx) {
    bus.ctx.resume?.().catch(() => {});
    return;
  }
  try {
    const ctx = new AudioContext();
    const limiter = makeLimiter(ctx);
    limiter.connect(ctx.destination);
    bus.limiter = limiter;
    const wire = (el, mono) => {
      if (!el) return null;
      try {
        const g = ctx.createGain();
        const src = ctx.createMediaElementSource(el);
        if (mono) src.connect(monoMix(ctx)).connect(g);
        else src.connect(g);
        g.connect(limiter);
        el.volume = 1;
        return g;
      } catch {
        return null;
      }
    };
    bus.dev = wire(media.screen, false);
    bus.mic = wire(media.mic, true);
    for (const [f, el] of Object.entries(media.musicEls)) bus.music[f] = wire(el, false);
    bus.ctx = ctx;
    ctx.resume?.().catch(() => {});
  } catch {}
}

function setLevel(el, node, gain) {
  if (node) node.gain.value = Math.max(0, gain);
  else el.volume = clamp(gain, 0, 1);
}

function applySliceAudio(s) {
  const a = P().config.audio;
  const m = s.volume * masterGain(a.volume);
  media.screen.playbackRate = clamp(s.timeScale, 0.0625, 16);
  setLevel(media.screen, bus.dev, m * a.deviceVolume);
  if (media.camera) media.camera.playbackRate = media.screen.playbackRate;
  if (media.mic) {
    media.mic.playbackRate = media.screen.playbackRate;
    setLevel(media.mic, bus.mic, m * a.micVolume * (media.micLevel || 1));
  }
  applyMusicLevel();
}

function applyMusicLevel() {
  const a = P().config.audio;
  const lvl = masterGain(a.volume) * (a.musicVolume ?? 0.5);
  for (const [f, el] of Object.entries(media.musicEls)) {
    if (!bus.music[f] && bus.ctx) {
      // added after playback started: wire it now
      try {
        const g = bus.ctx.createGain();
        bus.ctx.createMediaElementSource(el).connect(g);
        g.connect(bus.limiter);
        el.volume = 1;
        bus.music[f] = g;
      } catch {}
    }
    setLevel(el, bus.music[f], lvl);
  }
}

/** Music clips sit on the output timeline, so they follow tOut rather than source time. */
function musicTick() {
  const list = P().scene.musicClips || [];
  const active = new Map();
  for (const c of list) {
    const t = S.tOut - c.offsetMs;
    if (S.playing && t >= 0 && t < c.durMs - 30 && !active.has(c.file)) active.set(c.file, { c, t });
  }
  applyMusicLevel();
  for (const [f, el] of Object.entries(media.musicEls)) {
    const a = active.get(f);
    if (!a) {
      if (!el.paused) el.pause();
      continue;
    }
    const target = a.c.inMs + a.t;
    if (el.paused) {
      el.currentTime = target / 1000;
      el.playbackRate = 1;
      el.play().catch(() => {});
    } else {
      chase(el, target, 1);
    }
  }
}
export const syncMusic = () => {
  applyMusicLevel();
};
export const refreshSliceAudio = () => applySliceAudio(P().scene.slices[S.curSlice]);

/** SVG elements have no `.hidden` property, so toggle the attribute directly. */
function setPlayIcon(playing) {
  $("icoPlay").toggleAttribute("hidden", playing);
  $("icoPause").toggleAttribute("hidden", !playing);
  $("btnPlay").title = playing ? "Pause (Space)" : "Play (Space)";
}

export function play() {
  if (S.playing || !S.track) return;
  S.proxy = null;
  ensureAudio();
  if (S.tOut >= outDuration() - 30) seekOut(0);
  const m = outToSrc(S.tOut);
  S.curSlice = m.idx;
  media.screen.currentTime = m.src / 1000;
  applySliceAudio(P().scene.slices[m.idx]);
  syncCamera(m.src);
  syncMic(m.src);
  media.screen.play().catch(console.error);
  media.camera?.play().catch(() => {});
  media.mic?.play().catch(() => {});
  S.playing = true;
  musicTick();
  setPlayIcon(true);
  requestAnimationFrame(loop);
}
export function pause() {
  if (!S.playing) return;
  S.playing = false;
  media.screen.pause();
  media.camera?.pause();
  media.mic?.pause();
  for (const el of Object.values(media.musicEls)) el.pause();
  setPlayIcon(false);
  draw();
}
function loop() {
  if (!S.playing) return;
  const sl = P().scene.slices;
  const s = sl[S.curSlice];
  const v = media.screen;
  const src = v.currentTime * 1000;
  if (src >= s.sourceEndMs - 8 || v.ended) {
    if (S.curSlice + 1 < sl.length) {
      S.curSlice++;
      const n = sl[S.curSlice];
      v.currentTime = n.sourceStartMs / 1000;
      applySliceAudio(n);
      syncCamera(n.sourceStartMs);
      syncMic(n.sourceStartMs);
    } else {
      S.tOut = outDuration();
      pause();
      updateTransport();
      draw();
      return;
    }
  } else if (src < s.sourceStartMs - 50) {
    v.currentTime = s.sourceStartMs / 1000;
  }
  const cur = sl[S.curSlice];
  S.tOut = sliceOutStart(S.curSlice) + (clamp(v.currentTime * 1000, cur.sourceStartMs, cur.sourceEndMs) - cur.sourceStartMs) / cur.timeScale;
  if (media.camera) chase(media.camera, v.currentTime * 1000 + P().media.cameraOffsetMs);
  if (media.mic) chase(media.mic, v.currentTime * 1000 + (P().media.micOffsetMs || 0));
  updateTransport(false);
  musicTick();
  draw();
  requestAnimationFrame(loop);
}
