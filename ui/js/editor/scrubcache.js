// Scrub proxy: a background pass steps a hidden <video> through the recording and
// snapshots small canvases. Dragging the playhead then paints from memory instead of
// asking the video decoder to seek on every pointer move. (Seek-stepping is used
// because WebKit refuses to autoplay a hidden video without a user gesture.)
import { S, P } from "./state.js";

const MAX_SLOTS = 400;
const SCREEN_W = 480;
const CAM_W = 240;

export const cache = { step: 100, screen: [], camera: [], filled: 0, slots: 0 };

export function lookup(srcMs) {
  const i = Math.round(srcMs / cache.step);
  for (let d = 0; d <= 4; d++) {
    for (const j of d ? [i - d, i + d] : [i]) {
      const s = cache.screen[j];
      if (s) return { screen: s, camera: cache.camera[j] || null };
    }
  }
  return null;
}

const idle = () => new Promise((r) => setTimeout(r, 0));

function openVideo(url) {
  const v = document.createElement("video");
  v.muted = true;
  v.playsInline = true;
  v.preload = "auto";
  v.src = url;
  Object.assign(v.style, { position: "fixed", left: "0", bottom: "0", width: "2px", height: "2px", opacity: "0.01", pointerEvents: "none" });
  document.body.appendChild(v);
  return new Promise((res) => {
    if (v.readyState >= 2) return res(v);
    v.addEventListener("loadeddata", () => res(v), { once: true });
    v.addEventListener("error", () => res(null), { once: true });
  });
}

function seekTo(v, t) {
  return new Promise((res) => {
    const done = () => {
      clearTimeout(g);
      v.removeEventListener("seeked", done);
      res();
    };
    const g = setTimeout(done, 1500);
    v.addEventListener("seeked", done);
    v.currentTime = t;
  });
}

async function fill(url, offsetMs, store, width, onFill) {
  const v = await openVideo(url);
  if (!v || !v.videoWidth) return v?.remove();
  const h = Math.round((width * v.videoHeight) / v.videoWidth);
  const done = new Set();
  for (const stride of [8, 4, 2, 1]) {
    for (let i = 0; i < cache.slots; i += stride) {
      if (done.has(i)) continue;
      done.add(i);
      // stay out of the way of playback and scrubbing
      while (S.playing || S.scrubbing) await new Promise((r) => setTimeout(r, 150));
      await seekTo(v, Math.max(0.05, i * cache.step + offsetMs) / 1000);
      const c = document.createElement("canvas");
      c.width = width;
      c.height = h;
      c.getContext("2d").drawImage(v, 0, 0, width, h);
      store[i] = c;
      onFill?.();
      await idle();
    }
  }
  v.remove();
}

export function buildScrubCache(urls) {
  const dur = P().media.durationMs;
  cache.step = Math.max(1000 / 12, dur / MAX_SLOTS);
  cache.slots = Math.ceil(dur / cache.step);
  fill(urls.screen, 0, cache.screen, SCREEN_W, () => { cache.filled++; }).catch(console.error);
  if (urls.camera) fill(urls.camera, P().media.cameraOffsetMs || 0, cache.camera, CAM_W).catch(console.error);
}
