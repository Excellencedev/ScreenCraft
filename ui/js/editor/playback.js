// Playback, seeking and render-track computation.
import { invoke } from "../tauri.js";
import { $, S, P, dir, TRACK_FPS, media, clamp, fmt, hooks } from "./state.js";
import { outDuration, outToSrc, sliceOutStart } from "./timemap.js";
import { draw } from "./draw.js";

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

function syncCamera(srcMs) {
  const cam = media.camera;
  if (!cam) return;
  const t = Math.max(0, (srcMs + P().media.cameraOffsetMs) / 1000);
  if (Math.abs(cam.currentTime - t) > 0.15 || !S.playing) cam.currentTime = t;
}

function pump() {
  if (!pending) return;
  const { src, fast } = pending;
  pending = null;
  const v = media.screen;
  const target = src / 1000;
  syncCamera(src);
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
  pending = { src: m.src, fast };
  if (!seeking) pump();
}
export const seekOut = (t) => requestSeek(t, false);

// ---- playback
function applySliceAudio(s) {
  media.screen.playbackRate = clamp(s.timeScale, 0.0625, 16);
  media.screen.volume = clamp(s.volume * P().config.audio.volume, 0, 1);
  if (media.camera) media.camera.playbackRate = media.screen.playbackRate;
}
export const refreshSliceAudio = () => applySliceAudio(P().scene.slices[S.curSlice]);

export function play() {
  if (S.playing || !S.track) return;
  if (S.tOut >= outDuration() - 30) seekOut(0);
  const m = outToSrc(S.tOut);
  S.curSlice = m.idx;
  media.screen.currentTime = m.src / 1000;
  applySliceAudio(P().scene.slices[m.idx]);
  syncCamera(m.src);
  media.screen.play().catch(console.error);
  media.camera?.play().catch(() => {});
  S.playing = true;
  $("icoPlay").hidden = true;
  $("icoPause").hidden = false;
  requestAnimationFrame(loop);
}
export function pause() {
  if (!S.playing) return;
  S.playing = false;
  media.screen.pause();
  media.camera?.pause();
  $("icoPlay").hidden = false;
  $("icoPause").hidden = true;
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
  if (media.camera && Math.abs(media.camera.currentTime * 1000 - (v.currentTime * 1000 + P().media.cameraOffsetMs)) > 250) syncCamera(v.currentTime * 1000);
  updateTransport(false);
  draw();
  requestAnimationFrame(loop);
}
