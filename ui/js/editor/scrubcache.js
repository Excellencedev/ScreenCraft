// Scrub proxy: a background pass decodes the recording once at a few times real
// speed and keeps small bitmaps of it. Dragging the playhead then paints from
// memory instead of asking the video decoder to seek on every pointer move.
import { S, P, media } from "./state.js";

const MAX_SLOTS = 400;
const SCREEN_W = 480;
const CAM_W = 240;

export const cache = { step: 100, screen: [], camera: [], done: false };

export function lookup(srcMs) {
  const i = Math.round(srcMs / cache.step);
  for (let d = 0; d <= 2; d++) {
    for (const j of d ? [i - d, i + d] : [i]) {
      const s = cache.screen[j];
      if (s) return { screen: s, camera: cache.camera[Math.round((j * cache.step) / cache.step)] || null };
    }
  }
  return null;
}

function capture(url, offsetMs, store, width, isDone) {
  const v = document.createElement("video");
  v.muted = true;
  v.playsInline = true;
  v.preload = "auto";
  v.src = url;
  v.playbackRate = 4;
  Object.assign(v.style, { position: "fixed", left: "-9999px", top: "0", width: "320px", opacity: "0.01", pointerEvents: "none" });
  document.body.appendChild(v);
  const watch = setInterval(() => {
    if (S.playing) v.pause();
    else if (v.paused && !v.ended) v.play().catch(() => {});
  }, 300);
  const finish = () => {
    clearInterval(watch);
    v.remove();
    isDone?.();
  };
  const tick = (_now, meta) => {
    const i = Math.round((meta.mediaTime * 1000 - offsetMs) / cache.step);
    if (i >= 0 && !store[i] && v.videoWidth) {
      store[i] = null; // reserve so we don't queue it twice
      const h = Math.round((width * v.videoHeight) / v.videoWidth);
      createImageBitmap(v, { resizeWidth: width, resizeHeight: h, resizeQuality: "low" })
        .then((b) => { store[i] = b; })
        .catch(() => { store[i] = undefined; });
    }
    if (v.ended) finish();
    else v.requestVideoFrameCallback(tick);
  };
  v.onended = finish;
  v.onerror = finish;
  v.addEventListener("loadeddata", () => {
    v.requestVideoFrameCallback(tick);
    v.play().catch(finish);
  }, { once: true });
}

export function buildScrubCache(urls) {
  if (!("requestVideoFrameCallback" in HTMLVideoElement.prototype)) return;
  const dur = P().media.durationMs;
  cache.step = Math.max(1000 / 12, dur / MAX_SLOTS);
  capture(urls.screen, 0, cache.screen, SCREEN_W, () => { cache.done = true; });
  if (urls.camera) capture(urls.camera, P().media.cameraOffsetMs || 0, cache.camera, CAM_W);
}
