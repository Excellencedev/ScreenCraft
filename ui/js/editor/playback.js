// Playback, seeking and render-track computation.
import { invoke } from "../tauri.js";
import { $, S, P, dir, TRACK_FPS, clamp, fmt, hooks } from "./state.js";
import { outDuration, outToSrc, sliceOutStart } from "./timemap.js";
import { draw } from "./draw.js";
import { startAudio, stopAudio, applyLevels, primeMusic } from "./audio.js";

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

// ---- seeking: the preview shows decoded proxy frames, so a seek is just "want frame N"
export function requestSeek(t) {
  S.tOut = clamp(t, 0, outDuration());
  S.curSlice = outToSrc(S.tOut).idx;
  updateTransport(false);
  draw();
}
export const seekOut = (t) => requestSeek(t);

// ---- playback: the output timeline runs on the wall clock; audio is scheduled to match
function setPlayIcon(playing) {
  $("icoPlay").toggleAttribute("hidden", playing);
  $("icoPause").toggleAttribute("hidden", !playing);
  $("btnPlay").title = playing ? "Pause (Space)" : "Play (Space)";
}

export const refreshSliceAudio = () => applyLevels();
export const syncMusic = async () => {
  await primeMusic();
  applyLevels();
  if (S.playing) restartAudio();
};

const LEAD = 0.06; // seconds between "start" and the first audible sample
let perfStart = 0;
let outStart = 0;
let raf = 0;

async function restartAudio() {
  outStart = S.tOut;
  perfStart = performance.now() + LEAD * 1000;
  await startAudio(outStart, LEAD);
}

export async function play() {
  if (S.playing || !S.track) return;
  if (S.tOut >= outDuration() - 30) S.tOut = 0;
  S.playing = true;
  setPlayIcon(true);
  S.curSlice = outToSrc(S.tOut).idx;
  await primeMusic();
  outStart = S.tOut;
  perfStart = performance.now() + LEAD * 1000;
  startAudio(outStart, LEAD); // fire and forget: the clock below does not wait for it
  cancelAnimationFrame(raf);
  raf = requestAnimationFrame(loop);
}

export function pause() {
  if (!S.playing) return;
  S.playing = false;
  cancelAnimationFrame(raf);
  stopAudio();
  setPlayIcon(false);
  draw();
}

function loop() {
  if (!S.playing) return;
  const dur = outDuration();
  S.tOut = Math.min(dur, outStart + Math.max(0, performance.now() - perfStart));
  S.curSlice = outToSrc(S.tOut).idx;
  if (S.tOut >= dur - 1) {
    pause();
    updateTransport();
    return;
  }
  updateTransport(false);
  draw();
  raf = requestAnimationFrame(loop);
}
