// Canvas preview drawing with adaptive resolution.
import { $, S, P, TRACK_FPS, media } from "./state.js";
import { outToSrc } from "./timemap.js";
import { outputSize, contentRect, paintFrame, camAt } from "../render.js";

const canvas = $("preview");
const ctx = canvas.getContext("2d", { alpha: false });
const drawSrc = { screen: null, camera: null, bgImage: null, cache: {} };

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
  }
  if (st.camera) {
    const i = st.camera.index(srcMs + P().media.cameraOffsetMs);
    st.camera.want(i, dirn);
    drawSrc.camera = st.camera.peek(i);
  }
  drawSrc.bgImage = media.bgImage;
  if (!drawSrc.screen) return;
  paintFrame(ctx, W, H, P(), cam, S.tOut, S.track.clicks, drawSrc);
  if (S.selected?.type === "zoom") updateManualMarker(cam);
  else if (markerShown) {
    $("manualMarker").hidden = true;
    markerShown = false;
  }
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
