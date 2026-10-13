// Canvas preview drawing with adaptive resolution.
import { $, S, P, TRACK_FPS, media, dir } from "./state.js";
import { outToSrc } from "./timemap.js";
import { outputSize, contentRect, paintFrame, camAt } from "../render.js";
import { SC_BASE } from "../tauri.js";

const canvas = $("preview");
const ctx = canvas.getContext("2d", { alpha: false });
export const drawSrc = { screen: null, camera: null, bgImage: null, cache: {}, bounds: new Map(), geom: {}, combos: [] };

let markerShown = false;

/** On-screen rectangle actually covered by the (letterboxed) canvas picture. */
export function displayedRect() {
  const r = canvas.getBoundingClientRect();
  const ca = canvas.width / canvas.height, ra = r.width / r.height;
  let { width: w, height: h, left: x, top: y } = r;
  if (ra > ca) {
    w = r.height * ca;
    x += (r.width - w) / 2;
  } else {
    h = r.width / ca;
    y += (r.height - h) / 2;
  }
  return { left: x, top: y, width: w, height: h };
}

// Sharp stand-in for the proxy frame while paused: a full-resolution source frame is
// requested after the picture has rested for a moment (zoomed views look much crisper).
let hq = { key: null, bmp: null };
let hqTimer = 0;
function scheduleHq(srcMs) {
  clearTimeout(hqTimer);
  hqTimer = setTimeout(async () => {
    if (S.playing || S.scrubbing) return;
    try {
      const r = await fetch(`${SC_BASE}/src/screen?t=${srcMs.toFixed(3)}&d=${encodeURIComponent(dir)}&q=90`);
      if (!r.ok) return;
      const bmp = await createImageBitmap(await r.blob());
      if (S.playing || Math.abs(outToSrc(S.tOut).src - srcMs) > 0.5) {
        bmp.close();
        return;
      }
      hq.bmp?.close?.();
      hq = { key: srcMs.toFixed(3), bmp };
      draw();
    } catch {}
  }, 220);
}

export const frameIndex = () => Math.round((S.tOut * TRACK_FPS) / 1000);

function sizeCanvas() {
  // Fixed size: resizing a canvas clears it, which showed up as black flashes mid-playback.
  const { W, H } = outputSize(P(), S.previewRes);
  if (canvas.width !== W || canvas.height !== H) {
    canvas.width = W;
    canvas.height = H;
  }
  return { W, H };
}

export function draw() {
  if (!S.track || !P()) return;
  const { W, H } = sizeCanvas();
  const cam = camAt(S.track, frameIndex());
  const srcMs = outToSrc(S.tOut).src;
  const st = media.strips;
  const dirn = S.playing ? 1 : 0;
  if (st.screen) {
    const i = st.screen.index(srcMs);
    st.screen.want(i, dirn);
    drawSrc.screen = st.screen.peek(i);
    if (!S.playing && !S.scrubbing) {
      if (hq.bmp && hq.key === srcMs.toFixed(3)) drawSrc.screen = hq.bmp;
      else scheduleHq(srcMs);
    }
  }
  if (st.camera) {
    const i = st.camera.index(srcMs + P().media.cameraOffsetMs);
    st.camera.want(i, dirn);
    drawSrc.camera = st.camera.peek(i);
  }
  // cross-dissolve: the other clip's picture, only while one is being blended in
  drawSrc.screen2 = null;
  drawSrc.camera2 = null;
  if (cam.otherAlpha > 0.003) {
    if (st.screen) {
      const j = st.screen.index(cam.otherT);
      st.screen.want(j, dirn);
      drawSrc.screen2 = st.screen.peek(j);
    }
    if (st.camera) {
      const j = st.camera.index(cam.otherT + P().media.cameraOffsetMs);
      st.camera.want(j, dirn);
      drawSrc.camera2 = st.camera.peek(j);
    }
  }
  drawSrc.bgImage = media.bgImage;
  drawSrc.watermarkImg = media.watermarkImg;
  if (!drawSrc.screen) return;
  drawSrc.combos = S.track.combos || [];
  paintFrame(ctx, W, H, P(), cam, S.tOut, S.track.clicks, drawSrc);
  if (S.selected?.type === "annot") drawSelection(W, H);
  if (S.selected?.type === "zoom") updateManualMarker(cam);
  else if (markerShown) {
    $("manualMarker").hidden = true;
    markerShown = false;
  }
}

/** Editor-only outline and handles for the selected annotation (never exported). */
function drawSelection(W, H) {
  const a = (P().scene.annotations || []).find((x) => x.id === S.selected.id);
  const b = a && drawSrc.bounds.get(a.id);
  if (!b) return;
  const u = H / 1080;
  ctx.save();
  ctx.strokeStyle = "#6aaee8";
  ctx.fillStyle = "#6aaee8";
  ctx.lineWidth = 2 * u;
  ctx.setLineDash([6 * u, 4 * u]);
  if (b.arrow) {
    const [x1, y1, x2, y2] = b.arrow;
    ctx.setLineDash([]);
    for (const [x, y] of [[x1, y1], [x2, y2]]) {
      ctx.beginPath();
      ctx.arc(x, y, 7 * u, 0, Math.PI * 2);
      ctx.fill();
    }
  } else {
    ctx.strokeRect(b.x, b.y, b.w, b.h);
    ctx.setLineDash([]);
    if (a.kind !== "text") ctx.fillRect(b.x + b.w - 5 * u, b.y + b.h - 5 * u, 10 * u, 10 * u);
  }
  ctx.restore();
}

function updateManualMarker(cam) {
  const m = $("manualMarker");
  const z = P().scene.zooms.find((x) => x.id === S.selected.id);
  if (!z || z.kind !== "manual") {
    m.hidden = true;
    markerShown = false;
    return;
  }
  markerShown = true;
  const r = contentRect(P(), canvas.width, canvas.height);
  const crop = P().config.crop;
  const vwN = crop.width / cam.zoom, vhN = crop.height / cam.zoom;
  const nx = (z.manualTargetPoint.x - (cam.cx - vwN / 2)) / vwN;
  const ny = (z.manualTargetPoint.y - (cam.cy - vhN / 2)) / vhN;
  const rect = displayedRect(), wrap = $("canvasWrap").getBoundingClientRect();
  m.hidden = false;
  m.style.left = rect.left - wrap.left + ((r.x + nx * r.w) / canvas.width) * rect.width + "px";
  m.style.top = rect.top - wrap.top + ((r.y + ny * r.h) / canvas.height) * rect.height + "px";
}
