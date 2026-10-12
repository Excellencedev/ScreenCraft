// Playback, seeking and render-track computation.
import { invoke } from "../tauri.js";
import { $, S, P, dir, TRACK_FPS, media, clamp, fmt, hooks } from "./state.js";
import { outDuration, outToSrc, sliceOutStart } from "./timemap.js";
import { draw } from "./draw.js";
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
 * Keep a secondary element (camera video, mic, music) lined up with the screen video.
 * Only a hard re-seek is used when it drifts far: changing `playbackRate` on the fly
 * (a speed nudge) can deadlock AVFoundation's player in WebKit's GPU process, which
 * froze the whole editor.
 */
function chase(el, targetMs, base = media.screen.playbackRate, limit = 350) {
  const err = Math.abs(el.currentTime * 1000 - targetMs);
  if (err > limit) el.currentTime = Math.max(0, targetMs / 1000);
  if (el.playbackRate !== base) el.playbackRate = base;
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
export function requestSeek(t, fast = false) {
  S.tOut = clamp(t, 0, outDuration());
  const m = outToSrc(S.tOut);
  S.curSlice = m.idx;
  updateTransport(false);
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
  // intentionally empty: playback uses plain media elements (no WebAudio graph)
}

function setLevel(el, node, gain) {
  el.volume = clamp(gain, 0, 1);
}

function applySliceAudio(s) {
  const a = P().config.audio;
  const m = s.volume * masterGain(a.volume);
  const rate = clamp(s.timeScale, 0.0625, 16);
  if (media.screen.playbackRate !== rate) media.screen.playbackRate = rate;
  setLevel(media.screen, null, m * a.deviceVolume);
  if (media.camera && media.camera.playbackRate !== rate) media.camera.playbackRate = rate;
  if (media.mic) {
    if (media.mic.playbackRate !== rate) media.mic.playbackRate = rate;
    // the baked mic already carries its auto-level at 2x; the master slider scales that back
    const micGain = media.micBaked ? (m / 2) * a.micVolume : m * a.micVolume * (media.micLevel || 1);
    setLevel(media.mic, null, micGain);
  }
  applyMusicLevel();
}

function applyMusicLevel() {
  const a = P().config.audio;
  const lvl = masterGain(a.volume) * (a.musicVolume ?? 0.5);
  for (const el of Object.values(media.musicEls)) el.volume = clamp(lvl, 0, 1);
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
    const target = (a.c.inMs + a.t) / 1000;
    if (el.paused) {
      if (Math.abs(el.currentTime - target) > 0.15) el.currentTime = target;
      el.play().catch(() => {});
    } else if (Math.abs(el.currentTime - target) > 1.5) {
      el.currentTime = target; // far out of step: one correction
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

/** Seek `el` to `t` seconds and resolve once it has actually landed there. */
function settle(el, t) {
  return new Promise((res) => {
    if (!el) return res();
    if (Math.abs(el.currentTime - t) < 0.03 && el.readyState >= 2) return res();
    let g = 0;
    const done = () => {
      clearTimeout(g);
      el.removeEventListener("seeked", done);
      res();
    };
    g = setTimeout(done, 700);
    el.addEventListener("seeked", done);
    el.currentTime = Math.max(0, t);
  });
}

let starting = false;

/**
 * Start playback. Every element is first parked on exactly the right frame/sample and
 * only then are they all started together; while playing nothing is seeked or re-rated
 * (seeks and rate changes mid-playback are what deadlock WebKit's AVFoundation player).
 */
export async function play() {
  if (S.playing || starting || !S.track) return;
  starting = true;
  try {
    S.proxy = null;
    if (S.tOut >= outDuration() - 30) seekOut(0);
    const m = outToSrc(S.tOut);
    S.curSlice = m.idx;
    applySliceAudio(P().scene.slices[m.idx]);
    const mu = P().scene.musicClips || [];
    const jobs = [
      settle(media.screen, Math.max(m.src / 1000, 0.05)),
      settle(media.camera, Math.max(0, (m.src + P().media.cameraOffsetMs) / 1000)),
      settle(media.mic, Math.max(0, (m.src + (P().media.micOffsetMs || 0)) / 1000)),
    ];
    for (const c of mu) {
      const t = S.tOut - c.offsetMs;
      const el = media.musicEls[c.file];
      if (el && t >= 0 && t < c.durMs - 30) jobs.push(settle(el, (c.inMs + t) / 1000));
    }
    await Promise.all(jobs);
    if (S.playing) return;
    S.playing = true;
    media.screen.play().catch(console.error);
    media.camera?.play().catch(() => {});
    media.mic?.play().catch(() => {});
    musicTick();
    setPlayIcon(true);
    requestAnimationFrame(loop);
  } finally {
    starting = false;
  }
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
  hooks.swapMic?.();
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
  // only a gross drift (> 1.5 s) is corrected; small drift is left alone on purpose
  if (media.camera) chase(media.camera, v.currentTime * 1000 + P().media.cameraOffsetMs, undefined, 1500);
  if (media.mic) chase(media.mic, v.currentTime * 1000 + (P().media.micOffsetMs || 0), undefined, 1500);
  updateTransport(false);
  musicTick();
  draw();
  requestAnimationFrame(loop);
}
