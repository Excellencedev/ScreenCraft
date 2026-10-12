// Scrub proxy: a background pass steps a hidden <video> through the recording and
// snapshots small canvases. Dragging the playhead then paints from memory instead of
// asking the video decoder to seek on every pointer move. (Seek-stepping is used
// because WebKit refuses to autoplay a hidden video without a user gesture.)
import { S, P } from "./state.js";

const MAX_SLOTS = 160;
const SCREEN_W = 480;
const CAM_W = 240;

export const cache = { step: 100, screen: [], camera: [], filled: 0, camFilled: 0, slots: 0 };

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

const pause = (ms) => new Promise((r) => setTimeout(r, ms));

/**
 * Walk the slots coarse-to-fine, decoding the screen and then the camera for each one.
 * Strictly one decoder at a time with a short breather in between, so the page (and
 * the machine) stays responsive while this runs.
 */
async function fillAll(urls, camOffsetMs, onScreen, onCam) {
  const [vs, vc] = await Promise.all([openVideo(urls.screen), urls.camera ? openVideo(urls.camera) : null]);
  if (!vs || !vs.videoWidth) return;
  const hs = Math.round((SCREEN_W * vs.videoHeight) / vs.videoWidth);
  const hc = vc && vc.videoWidth ? Math.round((CAM_W * vc.videoHeight) / vc.videoWidth) : 0;
  const done = new Set();
  for (const stride of [8, 4, 2, 1]) {
    for (let i = 0; i < cache.slots; i += stride) {
      if (done.has(i)) continue;
      done.add(i);
      while (S.playing || S.scrubbing) await pause(150);
      await seekTo(vs, Math.max(0.05, i * cache.step) / 1000);
      const c = document.createElement("canvas");
      c.width = SCREEN_W;
      c.height = hs;
      c.getContext("2d").drawImage(vs, 0, 0, SCREEN_W, hs);
      cache.screen[i] = c;
      onScreen();
      await pause(8);
      if (vc && hc) {
        await seekTo(vc, Math.max(0.05, i * cache.step + camOffsetMs) / 1000);
        const k = document.createElement("canvas");
        k.width = CAM_W;
        k.height = hc;
        k.getContext("2d").drawImage(vc, 0, 0, CAM_W, hc);
        cache.camera[i] = k;
        onCam();
      }
      await pause(22);
    }
  }
  vs.remove();
  vc?.remove();
}

/**
 * Starts the background pass. `onProgress(f)` gets 0..1 where 1 means "good enough to
 * scrub smoothly" (every other slot filled, so the nearest frame is always close).
 */
export function buildScrubCache(urls, onProgress) {
  const dur = P().media.durationMs;
  cache.step = Math.max(1000 / 12, dur / MAX_SLOTS);
  cache.slots = Math.ceil(dur / cache.step);
  const need = Math.max(1, cache.slots * 0.5);
  const report = () => {
    const a = cache.filled / need;
    const b = urls.camera ? cache.camFilled / need : 1;
    onProgress?.(Math.min(1, a, b));
  };
  fillAll(urls, P().media.cameraOffsetMs || 0, () => { cache.filled++; report(); }, () => { cache.camFilled++; report(); }).catch(console.error);
  report();
}
