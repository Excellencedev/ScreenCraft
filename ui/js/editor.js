import { invoke, convertFileSrc, dialog, currentWindow } from "./tauri.js";
import { outputSize, contentRect, paintFrame, camAt } from "./render.js";
import { WALLPAPERS, drawWallpaper } from "./wallpapers.js";
import { drawCursor } from "./cursors.js";
import { exportMp4, exportGif } from "./export.js";

const $ = (id) => document.getElementById(id);
const dir = new URLSearchParams(location.search).get("dir");
const TRACK_FPS = 60;

// ------------------------------------------------------------------ state
const S = {
  project: null,
  track: null,
  tOut: 0,
  playing: false,
  selected: null, // {type:'zoom'|'slice', id}
  tab: "background",
  previewRes: 720,
  history: [],
  future: [],
  lastKey: null,
  lastT: 0,
  curSlice: 0,
};
let screenV, cameraV, bgImage = null;
const ctx = $("preview").getContext("2d", { alpha: false });
const drawSrc = { screen: null, camera: null, bgImage: null, cache: {} };
let dyn = 1; // adaptive preview scale while playing
let drawMs = 8;

const uid = () => Math.random().toString(36).slice(2, 10);
const clamp = (v, a, b) => Math.min(b, Math.max(a, v));
const P = () => S.project;

// ------------------------------------------------------------------ time maps
const sliceLen = (s) => Math.max(0, s.sourceEndMs - s.sourceStartMs) / Math.max(1e-3, s.timeScale);
const outDuration = () => P().scene.slices.reduce((a, s) => a + sliceLen(s), 0);
function outToSrc(t) {
  let acc = 0;
  const sl = P().scene.slices;
  for (let i = 0; i < sl.length; i++) {
    const len = sliceLen(sl[i]);
    if (t < acc + len || i === sl.length - 1) {
      return { src: clamp(sl[i].sourceStartMs + (t - acc) * sl[i].timeScale, sl[i].sourceStartMs, sl[i].sourceEndMs), idx: i };
    }
    acc += len;
  }
  return { src: 0, idx: 0 };
}
/** output time for a source time; times inside a cut snap to the next kept moment */
function srcToOut(t) {
  let acc = 0;
  for (const s of P().scene.slices) {
    if (t < s.sourceStartMs) return acc;
    if (t <= s.sourceEndMs) return acc + (t - s.sourceStartMs) / s.timeScale;
    acc += sliceLen(s);
  }
  return acc;
}
function fmt(ms) {
  const s = Math.max(0, ms) / 1000;
  return `${Math.floor(s / 60)}:${(s % 60).toFixed(1).padStart(4, "0")}`;
}

// ------------------------------------------------------------------ history / persistence
function snapshot() {
  return JSON.stringify({ config: P().config, scene: P().scene });
}
function restore(json) {
  const o = JSON.parse(json);
  P().config = o.config;
  P().scene = o.scene;
}
let saveTimer = null;
function scheduleSave() {
  clearTimeout(saveTimer);
  saveTimer = setTimeout(() => invoke("save_project", { dir, project: P() }).catch(console.error), 600);
}
/** Apply a mutation with undo support. `key` coalesces rapid edits (slider drags). */
function change(key, mutate, { track = true, panel = false } = {}) {
  const now = performance.now();
  if (!(key && key === S.lastKey && now - S.lastT < 1200)) {
    S.history.push(snapshot());
    if (S.history.length > 100) S.history.shift();
    S.future = [];
  }
  S.lastKey = key;
  S.lastT = now;
  mutate();
  scheduleSave();
  updateUndo();
  if (track) scheduleTrack();
  else draw();
  renderTimeline();
  if (panel) renderPanel();
}
function undo() {
  if (!S.history.length) return;
  S.future.push(snapshot());
  restore(S.history.pop());
  S.lastKey = null;
  afterHistory();
}
function redo() {
  if (!S.future.length) return;
  S.history.push(snapshot());
  restore(S.future.pop());
  S.lastKey = null;
  afterHistory();
}
function afterHistory() {
  scheduleSave();
  updateUndo();
  scheduleTrack();
  renderTimeline();
  renderPanel();
  syncControls();
}
function updateUndo() {
  $("btnUndo").disabled = !S.history.length;
  $("btnRedo").disabled = !S.future.length;
}

// ------------------------------------------------------------------ render track
let trackTimer = null;
function scheduleTrack() {
  clearTimeout(trackTimer);
  trackTimer = setTimeout(computeTrack, 50);
}
async function computeTrack() {
  try {
    S.track = await invoke("compute_render_track", { dir, project: P(), fps: TRACK_FPS });
    S.tOut = clamp(S.tOut, 0, S.track.durationMs);
    $("tDur").textContent = fmt(S.track.durationMs);
    renderTimeline();
    if (!S.playing) seekOut(S.tOut);
  } catch (e) {
    console.error(e);
  }
}

// ------------------------------------------------------------------ drawing
function frameIndex() {
  return Math.round((S.tOut * TRACK_FPS) / 1000);
}
function sizeCanvas() {
  const res = Math.round(S.previewRes * (S.playing ? dyn : 1));
  const { W, H } = outputSize(P(), res);
  const c = $("preview");
  if (c.width !== W || c.height !== H) {
    c.width = W;
    c.height = H;
  }
  return { W, H };
}
function draw() {
  if (!S.track || !P()) return;
  const { W, H } = sizeCanvas();
  const cam = camAt(S.track, frameIndex());
  drawSrc.screen = screenV;
  drawSrc.camera = cameraV;
  drawSrc.bgImage = bgImage;
  const t0 = performance.now();
  paintFrame(ctx, W, H, P(), cam, S.tOut, S.track.clicks, drawSrc);
  if (S.selected?.type === "zoom") updateManualMarker(cam);
  else if (markerShown) { $("manualMarker").hidden = true; markerShown = false; }
  // adapt preview resolution to keep playback smooth
  if (S.playing) {
    drawMs = drawMs * 0.85 + (performance.now() - t0) * 0.15;
    if (drawMs > 20 && dyn > 0.5) { dyn = Math.max(0.5, dyn - 0.25); drawMs = 10; }
    else if (drawMs < 6 && dyn < 1) { dyn = Math.min(1, dyn + 0.25); drawMs = 10; }
  }
}
let markerShown = false;
function updateManualMarker(cam) {
  const m = $("manualMarker");
  const z = S.selected?.type === "zoom" && P().scene.zooms.find((x) => x.id === S.selected.id);
  if (!z || z.kind !== "manual") { m.hidden = true; markerShown = false; return; }
  markerShown = true;
  const c = $("preview");
  const r = contentRect(P(), c.width, c.height);
  const crop = P().config.crop;
  const vwN = crop.width / cam.zoom, vhN = crop.height / cam.zoom;
  const nx = (z.manualTargetPoint.x - (cam.cx - vwN / 2)) / vwN;
  const ny = (z.manualTargetPoint.y - (cam.cy - vhN / 2)) / vhN;
  const rect = c.getBoundingClientRect(), wrap = $("canvasWrap").getBoundingClientRect();
  m.hidden = false;
  m.style.left = rect.left - wrap.left + ((r.x + nx * r.w) / c.width) * rect.width + "px";
  m.style.top = rect.top - wrap.top + ((r.y + ny * r.h) / c.height) * rect.height + "px";
}

// ------------------------------------------------------------------ playback
function sliceAt(src) {
  const sl = P().scene.slices;
  for (let i = 0; i < sl.length; i++) if (src >= sl[i].sourceStartMs - 2 && src <= sl[i].sourceEndMs + 2) return i;
  return Math.min(S.curSlice, sl.length - 1);
}
function syncCamera(srcMs) {
  if (!cameraV) return;
  const t = Math.max(0, (srcMs + P().media.cameraOffsetMs) / 1000);
  if (Math.abs(cameraV.currentTime - t) > 0.15 || !S.playing) cameraV.currentTime = t;
}
function seekOut(t) {
  S.tOut = clamp(t, 0, outDuration());
  const m = outToSrc(S.tOut);
  S.curSlice = m.idx;
  const target = m.src / 1000;
  if (Math.abs(screenV.currentTime - target) > 0.0005) screenV.currentTime = target;
  syncCamera(m.src);
  updateTransport();
  draw();
}
function applySliceAudio(s) {
  screenV.playbackRate = clamp(s.timeScale, 0.0625, 16);
  screenV.volume = clamp(s.volume * P().config.audio.volume, 0, 1);
  if (cameraV) cameraV.playbackRate = screenV.playbackRate;
}
function play() {
  if (S.playing || !S.track) return;
  if (S.tOut >= outDuration() - 30) seekOut(0);
  const m = outToSrc(S.tOut);
  S.curSlice = m.idx;
  screenV.currentTime = m.src / 1000;
  applySliceAudio(P().scene.slices[m.idx]);
  syncCamera(m.src);
  screenV.play().catch(console.error);
  cameraV?.play().catch(() => {});
  S.playing = true;
  $("icoPlay").hidden = true;
  $("icoPause").hidden = false;
  requestAnimationFrame(loop);
}
function pause() {
  if (!S.playing) return;
  S.playing = false;
  screenV.pause();
  cameraV?.pause();
  $("icoPlay").hidden = false;
  $("icoPause").hidden = true;
  draw();
}
function loop() {
  if (!S.playing) return;
  const sl = P().scene.slices;
  const s = sl[S.curSlice];
  const src = screenV.currentTime * 1000;
  if (src >= s.sourceEndMs - 8 || screenV.ended) {
    if (S.curSlice + 1 < sl.length) {
      S.curSlice++;
      const n = sl[S.curSlice];
      screenV.currentTime = n.sourceStartMs / 1000;
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
    screenV.currentTime = s.sourceStartMs / 1000;
  }
  const cur = sl[S.curSlice];
  S.tOut = srcToOut(clamp(screenV.currentTime * 1000, cur.sourceStartMs, cur.sourceEndMs));
  if (cameraV && Math.abs(cameraV.currentTime * 1000 - (screenV.currentTime * 1000 + P().media.cameraOffsetMs)) > 250) syncCamera(screenV.currentTime * 1000);
  updateTransport(false);
  draw();
  requestAnimationFrame(loop);
}
let lastTransport = 0;
let tlW = 0;
function updateTransport(force = true) {
  const now = performance.now();
  if (!force && now - lastTransport < 66) return;
  lastTransport = now;
  $("tCur").textContent = fmt(S.tOut);
  const dur = outDuration();
  if (!tlW) tlW = tlWidth();
  $("playhead").style.transform = `translateX(${dur ? (S.tOut / dur) * tlW : 0}px)`;
}

// ------------------------------------------------------------------ timeline
const tlWidth = () => Math.max(100, $("tlInner").clientWidth);
function renderTimeline() {
  if (!P()) return;
  const dur = outDuration();
  const W = tlWidth();
  tlW = W;
  const pxPerMs = W / Math.max(1, dur);
  // ruler
  const ruler = $("ruler");
  ruler.innerHTML = "";
  const steps = [100, 200, 500, 1000, 2000, 5000, 10000, 15000, 30000, 60000, 120000, 300000];
  const step = steps.find((s) => s * pxPerMs >= 80) || 300000;
  for (let t = 0; t <= dur; t += step) {
    const x = t * pxPerMs;
    ruler.insertAdjacentHTML("beforeend", `<div class="tick" style="left:${x}px"></div><div class="lab" style="left:${x}px">${fmt(t).replace(/\.0$/, "")}</div>`);
  }
  // clip track
  const clip = $("trackClip");
  clip.innerHTML = "";
  let acc = 0;
  for (const s of P().scene.slices) {
    const len = sliceLen(s);
    const el = document.createElement("div");
    el.className = "block clip" + (S.selected?.type === "slice" && S.selected.id === s.id ? " sel" : "");
    el.style.left = acc * pxPerMs + "px";
    el.style.width = Math.max(4, len * pxPerMs - 2) + "px";
    el.innerHTML = `<span>Clip</span><span class="t">${(len / 1000).toFixed(1)}s${s.timeScale !== 1 ? ` · ${s.timeScale}×` : ""}</span>`;
    el.onpointerdown = (e) => {
      e.stopPropagation();
      S.selected = { type: "slice", id: s.id };
      const rect = $("tlInner").getBoundingClientRect();
      seekOut((e.clientX - rect.left) / pxPerMs);
      renderTimeline();
      renderPanel();
      syncControls();
    };
    clip.appendChild(el);
    acc += len;
  }
  // zoom track
  const zt = $("trackZoom");
  zt.innerHTML = "";
  for (const z of P().scene.zooms) {
    const a = srcToOut(z.sourceStartMs), b = srcToOut(z.sourceEndMs);
    if (b - a < 1) continue;
    const el = document.createElement("div");
    el.className = "block zoom" + (z.isDisabled ? " disabled" : "") + (S.selected?.type === "zoom" && S.selected.id === z.id ? " sel" : "");
    el.style.left = a * pxPerMs + "px";
    el.style.width = Math.max(14, (b - a) * pxPerMs - 2) + "px";
    el.innerHTML = `<span>Zoom</span><span class="t">${z.zoom.toFixed(1)}× · ${z.kind === "manual" ? "Manual" : "Auto"}</span><div class="h l"></div><div class="h r"></div>`;
    el.onpointerdown = (e) => beginZoomDrag(e, z, el, pxPerMs);
    zt.appendChild(el);
  }
  updateTransport();
}

function beginZoomDrag(e, z, el, pxPerMs) {
  e.stopPropagation();
  e.preventDefault();
  S.selected = { type: "zoom", id: z.id };
  S.tab = "zoom";
  const mode = e.target.classList.contains("l") ? "l" : e.target.classList.contains("r") ? "r" : "move";
  const x0 = e.clientX;
  const a0 = srcToOut(z.sourceStartMs), b0 = srcToOut(z.sourceEndMs);
  const orig = { s: z.sourceStartMs, e: z.sourceEndMs };
  let moved = false;
  const toSrc = (t) => outToSrc(clamp(t, 0, outDuration())).src;
  const mv = (ev) => {
    const dMs = (ev.clientX - x0) / pxPerMs;
    if (Math.abs(ev.clientX - x0) > 2) moved = true;
    if (!moved) return;
    let a = a0, b = b0;
    if (mode === "move") { a += dMs; b += dMs; const w = b0 - a0; if (a < 0) { a = 0; b = w; } if (b > outDuration()) { b = outDuration(); a = b - w; } }
    if (mode === "l") a = Math.min(a0 + dMs, b0 - 300);
    if (mode === "r") b = Math.max(b0 + dMs, a0 + 300);
    z.sourceStartMs = toSrc(a);
    z.sourceEndMs = toSrc(b);
    renderTimeline();
  };
  const up = () => {
    removeEventListener("pointermove", mv);
    removeEventListener("pointerup", up);
    if (moved) {
      const ns = z.sourceStartMs, ne = z.sourceEndMs;
      z.sourceStartMs = orig.s; z.sourceEndMs = orig.e;
      change("zoom-drag-" + z.id, () => { z.sourceStartMs = ns; z.sourceEndMs = ne; });
    } else {
      renderTimeline();
    }
    renderPanel();
    syncControls();
  };
  addEventListener("pointermove", mv);
  addEventListener("pointerup", up);
  renderTimeline();
  renderPanel();
}

function scrubFromEvent(e) {
  const rect = $("tlInner").getBoundingClientRect();
  const dur = outDuration();
  seekOut(clamp(((e.clientX - rect.left) / rect.width) * dur, 0, dur));
}
let scrubbing = false;
function bindScrub() {
  const down = (e) => {
    if (e.target.closest(".block")) return;
    scrubbing = true;
    const wasPlaying = S.playing;
    if (wasPlaying) pause();
    scrubFromEvent(e);
    const mv = (ev) => scrubFromEvent(ev);
    const up = () => {
      scrubbing = false;
      removeEventListener("pointermove", mv);
      removeEventListener("pointerup", up);
      if (wasPlaying) play();
    };
    addEventListener("pointermove", mv);
    addEventListener("pointerup", up);
  };
  $("tlInner").addEventListener("pointerdown", (e) => {
    if (e.target.closest(".block")) return;
    S.selected = null;
    down(e);
    renderTimeline();
    renderPanel();
  });
}

function splitAtPlayhead() {
  const { src, idx } = outToSrc(S.tOut);
  const s = P().scene.slices[idx];
  if (src <= s.sourceStartMs + 50 || src >= s.sourceEndMs - 50) return;
  change(null, () => {
    const right = { ...s, id: uid(), sourceStartMs: src };
    s.sourceEndMs = src;
    P().scene.slices.splice(idx + 1, 0, right);
    S.selected = { type: "slice", id: right.id };
  }, { panel: true });
}
function deleteSelected() {
  const sel = S.selected;
  if (!sel) return;
  if (sel.type === "zoom") {
    change(null, () => { P().scene.zooms = P().scene.zooms.filter((z) => z.id !== sel.id); S.selected = null; }, { panel: true });
  } else if (P().scene.slices.length > 1) {
    change(null, () => { P().scene.slices = P().scene.slices.filter((s) => s.id !== sel.id); S.selected = null; }, { panel: true });
    seekOut(Math.min(S.tOut, outDuration()));
  }
}
function addZoomAtPlayhead() {
  const { src, idx } = outToSrc(S.tOut);
  const sl = P().scene.slices[idx];
  const end = Math.min(src + 2500, sl.sourceEndMs);
  if (end - src < 400) return;
  const z = {
    id: uid(), zoom: P().config.animations.defaultZoom, kind: "follow-cursor", manualTargetPoint: { x: 0.5, y: 0.5 },
    isDisabled: false, sourceStartMs: src, sourceEndMs: end, instant: false,
  };
  change(null, () => { P().scene.zooms.push(z); S.selected = { type: "zoom", id: z.id }; S.tab = "zoom"; }, { panel: true });
}
async function autoZoom() {
  const zooms = await invoke("auto_zoom", { dir, project: P(), zoom: P().config.animations.defaultZoom });
  change(null, () => { P().scene.zooms = zooms.map((z) => ({ ...z, id: uid() })); S.selected = null; }, { panel: true });
}

// ------------------------------------------------------------------ panel widgets
const el = (html) => { const t = document.createElement("template"); t.innerHTML = html.trim(); return t.content.firstChild; };

function slider(label, min, max, step, get, set, { fmt: f = (v) => v, track = false, key } = {}) {
  const w = el(`<div class="field"><label><span>${label}</span><span class="val"></span></label><input type="range" min="${min}" max="${max}" step="${step}"></div>`);
  const inp = w.querySelector("input"), val = w.querySelector(".val");
  inp.value = get();
  val.textContent = f(get());
  inp.oninput = () => {
    const v = parseFloat(inp.value);
    val.textContent = f(v);
    change(key || label, () => set(v), { track });
  };
  return w;
}
function toggle(label, hint, get, set, { track = false } = {}) {
  const w = el(`<div class="row-t"><div>${label}${hint ? `<small>${hint}</small>` : ""}</div><div class="toggle"></div></div>`);
  const t = w.querySelector(".toggle");
  t.classList.toggle("on", !!get());
  t.onclick = () => { change(null, () => set(!get()), { track, panel: true }); };
  return w;
}
function segmented(label, options, get, set, { track = false } = {}) {
  const w = el(`<div class="field"><label><span>${label}</span></label><div class="seg"></div></div>`);
  const seg = w.querySelector(".seg");
  for (const [v, text] of options) {
    const b = el(`<button class="${get() === v ? "on" : ""}">${text}</button>`);
    b.onclick = () => change(null, () => set(v), { track, panel: true });
    seg.appendChild(b);
  }
  return w;
}
function colorField(label, get, set) {
  const w = el(`<div class="row-t"><div>${label}</div><input type="color"></div>`);
  const i = w.querySelector("input");
  i.value = get();
  i.oninput = () => change(label, () => set(i.value));
  return w;
}
function h3(t) { return el(`<h3>${t}</h3>`); }

// ------------------------------------------------------------------ panels
const TABS = [
  ["background", "Appearance", '<path d="M4 5h16v14H4zM4 15l5-5 4 4 3-3 4 4"/>'],
  ["cursor", "Cursor", '<path d="M5 3l14 8-6 2-2 6z"/>'],
  ["camera", "Camera", '<rect x="3" y="7" width="13" height="10" rx="2"/><path d="M16 11l5-3v8l-5-3z"/>'],
  ["zoom", "Zoom", '<circle cx="11" cy="11" r="6"/><path d="M21 21l-5-5M11 8v6M8 11h6"/>'],
  ["audio", "Audio", '<path d="M4 10v4h4l5 4V6L8 10z"/><path d="M16 9a4 4 0 010 6"/>'],
  ["crop", "Crop", '<path d="M6 2v14a2 2 0 002 2h14M2 6h14a2 2 0 012 2v14"/>'],
];
function renderRail() {
  const rail = $("rail");
  rail.innerHTML = "";
  for (const [id, title, path] of TABS) {
    const b = el(`<button title="${title}" class="${S.tab === id ? "on" : ""}"><svg viewBox="0 0 24 24">${path}</svg></button>`);
    b.onclick = () => { S.tab = id; renderRail(); renderPanel(); };
    rail.appendChild(b);
  }
}
function renderPanel() {
  renderRail();
  const p = $("panel");
  p.innerHTML = "";
  if (!P()) return;
  ({ background: panelBackground, cursor: panelCursor, camera: panelCamera, zoom: panelZoom, audio: panelAudio, crop: panelCrop }[S.tab] || panelBackground)(p);
}

function swatchCanvas(name) {
  const c = document.createElement("canvas");
  c.width = 120; c.height = 90;
  drawWallpaper(c.getContext("2d"), 120, 90, name);
  return c;
}
function panelBackground(p) {
  const c = P().config, st = c.styles, bg = st.background;
  p.append(h3("Background"));
  p.append(segmented("Type", [["wallpaper", "Wallpaper"], ["gradient", "Gradient"], ["color", "Color"], ["image", "Image"]], () => bg.kind, (v) => (bg.kind = v)));
  if (bg.kind === "wallpaper") {
    const grid = el('<div class="swatches"></div>');
    for (const name of WALLPAPERS) {
      const cv = swatchCanvas(name);
      if (bg.wallpaper === name) cv.classList.add("on");
      cv.onclick = () => change(null, () => (bg.wallpaper = name), { panel: true });
      grid.appendChild(cv);
    }
    p.append(grid);
    p.append(el('<button class="btn-line">Pick random wallpaper</button>'));
    p.lastChild.onclick = () => change(null, () => (bg.wallpaper = WALLPAPERS[Math.floor(Math.random() * WALLPAPERS.length)]), { panel: true });
  } else if (bg.kind === "gradient") {
    p.append(colorField("From", () => bg.gradientFrom, (v) => (bg.gradientFrom = v)));
    p.append(colorField("To", () => bg.gradientTo, (v) => (bg.gradientTo = v)));
    p.append(slider("Angle", 0, 360, 1, () => bg.gradientAngle, (v) => (bg.gradientAngle = v), { fmt: (v) => v + "°" }));
  } else if (bg.kind === "color") {
    p.append(colorField("Color", () => bg.color, (v) => (bg.color = v)));
  } else {
    const b = el('<button class="btn-line">Choose image…</button>');
    b.onclick = () => {
      const i = document.createElement("input");
      i.type = "file"; i.accept = "image/*";
      i.onchange = () => {
        const f = i.files[0];
        if (!f) return;
        const r = new FileReader();
        r.onload = () => change(null, () => (bg.image = r.result), { panel: true, track: false });
        r.readAsDataURL(f);
      };
      i.click();
    };
    p.append(b);
  }
  if (bg.kind !== "color") p.append(slider("Background blur", 0, 40, 1, () => bg.blur, (v) => (bg.blur = v)));
  p.append(slider("Padding", 0, 0.3, 0.005, () => st.paddingRatio, (v) => (st.paddingRatio = v), { fmt: (v) => Math.round(v * 100) + "%" }));
  p.append(slider("Screen rounded corners", 0, 60, 1, () => st.cornerRadius, (v) => (st.cornerRadius = v)));
  p.append(toggle("Inset", "Add inset around the screen.", () => st.inset.enabled, (v) => (st.inset.enabled = v)));
  if (st.inset.enabled) {
    p.append(slider("Inset size", 1, 30, 1, () => st.inset.size, (v) => (st.inset.size = v)));
    p.append(colorField("Inset color", () => st.inset.color, (v) => (st.inset.color = v)));
  }
  p.append(slider("Shadow", 0, 1, 0.01, () => st.shadow.intensity, (v) => (st.shadow.intensity = v), { fmt: (v) => Math.round(v * 100) + "%" }));
  p.append(segmented("Shadow style", [[false, "Classic"], [true, "Directional"]], () => st.shadow.directional, (v) => (st.shadow.directional = v)));
  p.append(slider("Shadow distance", 0, 80, 1, () => st.shadow.distance, (v) => (st.shadow.distance = v)));
  p.append(slider("Shadow blur", 0, 60, 1, () => st.shadow.blur, (v) => (st.shadow.blur = v)));
  p.append(slider("Shadow angle", 0, 360, 1, () => st.shadow.angle, (v) => (st.shadow.angle = v), { fmt: (v) => v + "°" }));
}

function panelCursor(p) {
  const c = P().config.cursor;
  p.append(h3("Cursor"));
  p.append(slider("Size", 10, 80, 1, () => c.size, (v) => (c.size = v), { fmt: (v) => v + " pt" }));
  const styles = [["macos", "Dark"], ["light", "Light"], ["accent", "Accent"], ["dot", "Dot"]];
  const row = el('<div class="field"><label><span>Cursor style</span></label><div class="seg"></div></div>');
  for (const [v, name] of styles) {
    const cv = document.createElement("canvas");
    cv.width = 72; cv.height = 48;
    const g = cv.getContext("2d");
    g.fillStyle = "#2a2c3d"; g.fillRect(0, 0, 72, 48);
    drawCursor(g, 0, 28, 8, 30, v);
    const b = el(`<button class="${c.style === v ? "on" : ""}" title="${name}" style="padding:0;overflow:hidden"></button>`);
    b.appendChild(cv);
    b.onclick = () => change(null, () => (c.style = v), { panel: true, track: false });
    row.querySelector(".seg").appendChild(b);
  }
  p.append(row);
  p.append(toggle("Always use pointer cursor", "Don't change cursor, even if selecting text, etc.", () => c.alwaysUsePointer, (v) => (c.alwaysUsePointer = v)));
  p.append(toggle("Hide cursor if not moving", null, () => c.hideNotMovingAfterMs != null, (v) => (c.hideNotMovingAfterMs = v ? 2000 : null), { track: true }));
  p.append(toggle("Loop cursor position", "Near the end of the video, cursor will move back to its initial position.", () => c.loopPositionBeforeEndMs != null, (v) => (c.loopPositionBeforeEndMs = v ? 1200 : null), { track: true }));
  p.append(toggle("Tilt with movement", "Rotate the cursor slightly in the direction it moves.", () => c.rotateWithVelocity, (v) => (c.rotateWithVelocity = v), { track: true }));
  p.append(toggle("Hide cursor", null, () => c.hide, (v) => (c.hide = v), { track: true }));
  p.append(segmented("Click effect", [["none", "None"], ["circle", "Circle"], ["ripple", "Ripple"]], () => c.clickEffect, (v) => (c.clickEffect = v)));
  if (c.clickEffect !== "none") p.append(colorField("Effect color", () => c.clickEffectColor, (v) => (c.clickEffectColor = v)));
  const a = P().config.animations;
  p.append(h3("Motion"));
  p.append(slider("Cursor smoothing", 40, 400, 5, () => a.mouseSpring.stiffness, (v) => (a.mouseSpring.stiffness = v), { track: true, key: "mouse-stiff", fmt: (v) => (v >= 400 ? "Off" : v) }));
  p.append(slider("Camera stiffness", 40, 400, 5, () => a.screenSpring.stiffness, (v) => (a.screenSpring.stiffness = v), { track: true, key: "screen-stiff" }));
  p.append(slider("Camera damping", 4, 40, 0.5, () => a.screenSpring.damping, (v) => (a.screenSpring.damping = v), { track: true, key: "screen-damp" }));
}

function panelCamera(p) {
  const c = P().config.camera;
  p.append(h3("Camera"));
  if (!P().media.camera) {
    p.append(el('<div class="muted">This recording has no camera track. Enable a camera in the recorder toolbar before recording.</div>'));
    return;
  }
  p.append(toggle("Hide camera", null, () => c.hide, (v) => (c.hide = v)));
  p.append(toggle("Mirror", null, () => c.mirror, (v) => (c.mirror = v)));
  p.append(slider("Size", 0.1, 0.5, 0.01, () => c.sizeRatio, (v) => (c.sizeRatio = v), { fmt: (v) => Math.round(v * 100) + "%" }));
  p.append(slider("Roundness", 0, 0.5, 0.01, () => c.cornerRadiusRatio, (v) => (c.cornerRadiusRatio = v), { fmt: (v) => Math.round(v * 200) + "%" }));
  p.append(slider("Padding", 0, 2, 0.05, () => c.paddingRatio, (v) => (c.paddingRatio = v)));
  p.append(slider("Scale while zoomed", 0.4, 1, 0.01, () => c.zoomedScale, (v) => (c.zoomedScale = v), { fmt: (v) => Math.round(v * 100) + "%" }));
  p.append(segmented("Shape", [[1, "Square"], [4 / 3, "4:3"], [16 / 9, "16:9"], [3 / 4, "3:4"]], () => c.aspectRatio, (v) => (c.aspectRatio = v)));
  const pos = el('<div class="field"><label><span>Position</span></label><div class="seg"></div></div>');
  for (const [name, x, y] of [["↖", 0, 0], ["↗", 1, 0], ["↙", 0, 1], ["↘", 1, 1]]) {
    const b = el(`<button class="${c.position.x === x && c.position.y === y ? "on" : ""}">${name}</button>`);
    b.onclick = () => change(null, () => (c.position = { x, y }), { panel: true, track: false });
    pos.querySelector(".seg").appendChild(b);
  }
  p.append(pos);
  p.append(toggle("Shadow", null, () => c.shadow, (v) => (c.shadow = v)));
  p.append(toggle("Border", null, () => c.border, (v) => (c.border = v)));
}

function panelZoom(p) {
  const sel = S.selected;
  const z = sel?.type === "zoom" && P().scene.zooms.find((x) => x.id === sel.id);
  if (!z) {
    p.append(h3("Zoom"));
    p.append(el('<div class="muted">Zooms are created automatically from your clicks. Select a zoom block on the timeline to edit it, or add one at the playhead.</div>'));
    p.append(slider("Default zoom level", 1.2, 4, 0.1, () => P().config.animations.defaultZoom, (v) => (P().config.animations.defaultZoom = v), { fmt: (v) => v.toFixed(1) + "×" }));
    const add = el('<button class="btn-line">Add zoom at playhead</button>');
    add.onclick = addZoomAtPlayhead;
    const auto = el('<button class="btn-line">Regenerate from clicks</button>');
    auto.onclick = autoZoom;
    p.append(add, auto);
    return;
  }
  p.append(h3("Zoom region"));
  p.append(slider("Zoom level", 1.1, 5, 0.05, () => z.zoom, (v) => (z.zoom = v), { track: true, key: "zl-" + z.id, fmt: (v) => v.toFixed(2) + "×" }));
  p.append(segmented("Zoom position", [["follow-cursor", "Auto"], ["manual", "Manual"]], () => z.kind, (v) => (z.kind = v), { track: true }));
  p.append(el(`<div class="hint muted">${z.kind === "manual" ? "Click the preview to choose where the camera looks." : "Camera follows the cursor and keeps it in view."}</div>`));
  p.append(toggle("Instant animation", "Apply the zoom immediately without easing.", () => z.instant, (v) => (z.instant = v), { track: true }));
  p.append(toggle("Disabled", null, () => z.isDisabled, (v) => (z.isDisabled = v), { track: true }));
  const del = el('<button class="btn-line danger">Delete zoom</button>');
  del.onclick = deleteSelected;
  p.append(del);
}

function panelAudio(p) {
  const a = P().config.audio;
  p.append(h3("Audio"));
  if (!P().media.hasMicrophone && !P().media.hasSystemAudio) {
    p.append(el('<div class="muted">No audio was recorded. Enable the microphone or system audio in the recorder toolbar.</div>'));
  }
  p.append(slider("Volume", 0, 1, 0.01, () => a.volume, (v) => { a.volume = v; if (screenV) screenV.volume = clamp(v, 0, 1); }, { fmt: (v) => Math.round(v * 100) + "%" }));
}

function panelCrop(p) {
  const c = P().config.crop;
  p.append(h3("Crop"));
  const left = c.x * 100, top = c.y * 100, right = (1 - c.x - c.width) * 100, bottom = (1 - c.y - c.height) * 100;
  const apply = (l, t, r, b) => {
    l = clamp(l, 0, 80) / 100; t = clamp(t, 0, 80) / 100; r = clamp(r, 0, 80) / 100; b = clamp(b, 0, 80) / 100;
    c.x = l; c.y = t; c.width = Math.max(0.1, 1 - l - r); c.height = Math.max(0.1, 1 - t - b);
  };
  const v = { l: left, t: top, r: right, b: bottom };
  for (const [k, name] of [["l", "Left"], ["t", "Top"], ["r", "Right"], ["b", "Bottom"]]) {
    p.append(slider(name, 0, 60, 0.5, () => v[k], (n) => { v[k] = n; apply(v.l, v.t, v.r, v.b); }, { track: true, key: "crop-" + k, fmt: (n) => n.toFixed(1) + "%" }));
  }
  const reset = el('<button class="btn-line">Reset crop</button>');
  reset.onclick = () => change(null, () => Object.assign(c, { x: 0, y: 0, width: 1, height: 1 }), { panel: true });
  p.append(reset);
}

// ------------------------------------------------------------------ presets
const BUILTIN_PRESETS = {
  Clean: { styles: { paddingRatio: 0.08, cornerRadius: 14, background: { kind: "wallpaper", wallpaper: "aurora" } } },
  Minimal: { styles: { paddingRatio: 0.04, cornerRadius: 10, background: { kind: "color", color: "#0b0c12" }, shadow: { intensity: 0.15 } } },
  Vivid: { styles: { paddingRatio: 0.12, cornerRadius: 24, background: { kind: "gradient", gradientFrom: "#ff6ad5", gradientTo: "#7a5cff", gradientAngle: 135 } }, cursor: { clickEffect: "ripple", size: 34 } },
  Sunset: { styles: { paddingRatio: 0.1, cornerRadius: 18, background: { kind: "wallpaper", wallpaper: "sunset" } } },
};
const userPresets = () => { try { return JSON.parse(localStorage.getItem("sc-presets") || "{}"); } catch { return {}; } };
function deepMerge(t, s) {
  for (const k of Object.keys(s)) {
    if (s[k] && typeof s[k] === "object" && !Array.isArray(s[k])) deepMerge((t[k] ??= {}), s[k]);
    else t[k] = s[k];
  }
}
function applyPreset(pr) {
  change(null, () => {
    const c = P().config;
    if (pr.styles) deepMerge(c.styles, JSON.parse(JSON.stringify(pr.styles)));
    if (pr.cursor) deepMerge(c.cursor, JSON.parse(JSON.stringify(pr.cursor)));
    if (pr.camera) deepMerge(c.camera, JSON.parse(JSON.stringify(pr.camera)));
  }, { panel: true });
}
function showMenu(anchor, items) {
  const m = $("menu");
  m.innerHTML = "";
  for (const it of items) {
    if (it === "-") { m.appendChild(document.createElement("hr")); continue; }
    const b = document.createElement("button");
    b.textContent = it.label;
    b.onclick = () => { m.hidden = true; it.run(); };
    m.appendChild(b);
  }
  const r = anchor.getBoundingClientRect();
  m.hidden = false;
  m.style.top = r.bottom + 6 + "px";
  m.style.left = Math.max(8, r.right - m.offsetWidth) + "px";
  const close = (e) => { if (!m.contains(e.target)) { m.hidden = true; removeEventListener("pointerdown", close, true); } };
  setTimeout(() => addEventListener("pointerdown", close, true), 0);
}

// ------------------------------------------------------------------ export dialog
const exp = { format: "mp4", res: 1080, fps: 60, blur: 1, codec: "avc", quality: 55, running: false, cancel: false };
function bindSeg(id, key, conv = (v) => v) {
  const seg = $(id);
  seg.querySelectorAll("button").forEach((b) => {
    b.onclick = () => {
      seg.querySelectorAll("button").forEach((x) => x.classList.remove("on"));
      b.classList.add("on");
      exp[key] = conv(b.dataset.v);
    };
  });
}
async function runExport() {
  if (exp.running) return;
  const ext = exp.format;
  const base = (P().name || "ScreenCraft").replace(/[\\/:]/g, "-");
  const path = await dialog.save({ defaultPath: `${base}.${ext}`, filters: [{ name: ext.toUpperCase(), extensions: [ext] }] });
  if (!path) return;
  exp.running = true;
  exp.cancel = false;
  pause();
  $("expProgress").hidden = false;
  $("expGo").disabled = true;
  const onProgress = (f, text) => {
    $("expBar").style.width = Math.round(f * 100) + "%";
    $("expText").textContent = text;
  };
  try {
    const common = { project: JSON.parse(JSON.stringify(P())), dir, path, resolution: exp.res, onProgress, shouldCancel: () => exp.cancel };
    if (ext === "gif") await exportGif(common);
    else await exportMp4({ ...common, fps: exp.fps, motionBlur: exp.blur ? 0.8 : 0, codec: exp.codec, quality: exp.quality });
    $("expText").textContent = "Saved! Revealing in Finder…";
    invoke("reveal_in_finder", { path });
    setTimeout(closeExport, 900);
  } catch (e) {
    $("expText").textContent = exp.cancel ? "Cancelled" : "Export failed: " + (e?.message || e);
  } finally {
    exp.running = false;
    $("expGo").disabled = false;
  }
}
function closeExport() {
  if (exp.running) { exp.cancel = true; return; }
  $("modal").hidden = true;
  $("expProgress").hidden = true;
  $("expBar").style.width = "0";
}

// ------------------------------------------------------------------ controls
function syncControls() {
  $("selAspect").value = P().config.output.aspectRatio;
  const sel = S.selected?.type === "slice" && P().scene.slices.find((s) => s.id === S.selected.id);
  const s = sel || P().scene.slices[S.curSlice] || P().scene.slices[0];
  $("selSpeed").value = String(s.timeScale);
  $("title").textContent = P().name;
}

function bindUI() {
  $("btnPlay").onclick = () => (S.playing ? pause() : play());
  $("btnStart").onclick = () => { pause(); seekOut(0); };
  $("btnEnd").onclick = () => { pause(); seekOut(outDuration()); };
  $("btnUndo").onclick = undo;
  $("btnRedo").onclick = redo;
  $("btnSplit").onclick = splitAtPlayhead;
  $("btnDelete").onclick = deleteSelected;
  $("btnAddZoom").onclick = addZoomAtPlayhead;
  $("btnAutoZoom").onclick = autoZoom;
  $("btnFolder").onclick = () => invoke("reveal_in_finder", { path: dir });
  $("btnCrop").onclick = () => { S.tab = "crop"; renderPanel(); };
  $("selAspect").onchange = (e) => change(null, () => (P().config.output.aspectRatio = e.target.value), { track: false });
  $("selPreview").onchange = (e) => { S.previewRes = parseInt(e.target.value, 10); draw(); };
  $("selSpeed").onchange = (e) => {
    const v = parseFloat(e.target.value);
    const target = S.selected?.type === "slice" ? P().scene.slices.find((s) => s.id === S.selected.id) : P().scene.slices[S.curSlice];
    if (!target) return;
    change(null, () => (target.timeScale = v));
    if (S.playing) applySliceAudio(P().scene.slices[S.curSlice]);
  };
  $("btnPresets").onclick = (e) => {
    const items = Object.keys(BUILTIN_PRESETS).map((n) => ({ label: n, run: () => applyPreset(BUILTIN_PRESETS[n]) }));
    const mine = userPresets();
    const names = Object.keys(mine);
    if (names.length) { items.push("-"); names.forEach((n) => items.push({ label: n, run: () => applyPreset(mine[n]) })); }
    items.push("-", {
      label: "Save current as preset",
      run: () => {
        const all = userPresets();
        all["My preset " + (Object.keys(all).length + 1)] = JSON.parse(JSON.stringify({ styles: P().config.styles, cursor: P().config.cursor, camera: P().config.camera }));
        localStorage.setItem("sc-presets", JSON.stringify(all));
      },
    });
    showMenu(e.currentTarget, items);
  };
  $("btnExport").onclick = () => { $("modal").hidden = false; };
  $("expCancel").onclick = closeExport;
  $("expGo").onclick = runExport;
  bindSeg("expFormat", "format");
  bindSeg("expRes", "res", (v) => parseInt(v, 10));
  bindSeg("expFps", "fps", (v) => parseInt(v, 10));
  bindSeg("expBlur", "blur", (v) => parseInt(v, 10));
  bindSeg("expCodec", "codec");
  bindSeg("expQuality", "quality", (v) => parseInt(v, 10));

  // manual zoom target: click on the preview
  $("preview").addEventListener("pointerdown", (e) => {
    const z = S.selected?.type === "zoom" && P().scene.zooms.find((x) => x.id === S.selected.id);
    if (!z || z.kind !== "manual") { S.playing ? pause() : play(); return; }
    const c = $("preview"), rect = c.getBoundingClientRect();
    const px = ((e.clientX - rect.left) / rect.width) * c.width, py = ((e.clientY - rect.top) / rect.height) * c.height;
    const r = contentRect(P(), c.width, c.height);
    const cam = camAt(S.track, frameIndex());
    const crop = P().config.crop;
    const vwN = crop.width / cam.zoom, vhN = crop.height / cam.zoom;
    const nx = cam.cx - vwN / 2 + ((px - r.x) / r.w) * vwN, ny = cam.cy - vhN / 2 + ((py - r.y) / r.h) * vhN;
    change("manual-" + z.id, () => (z.manualTargetPoint = { x: clamp(nx, 0, 1), y: clamp(ny, 0, 1) }));
  });

  addEventListener("keydown", (e) => {
    if (e.target.tagName === "INPUT" || e.target.tagName === "SELECT") return;
    const mod = e.metaKey || e.ctrlKey;
    if (e.code === "Space") { e.preventDefault(); S.playing ? pause() : play(); }
    else if (e.key === "s" && !mod) splitAtPlayhead();
    else if (e.key === "z" && !mod) addZoomAtPlayhead();
    else if (e.key === "Backspace" || e.key === "Delete") { e.preventDefault(); deleteSelected(); }
    else if (e.key === "ArrowLeft") { pause(); seekOut(S.tOut - (e.shiftKey ? 1000 : 1000 / 30)); }
    else if (e.key === "ArrowRight") { pause(); seekOut(S.tOut + (e.shiftKey ? 1000 : 1000 / 30)); }
    else if (mod && e.key.toLowerCase() === "z") { e.preventDefault(); e.shiftKey ? redo() : undo(); }
    else if (mod && e.key.toLowerCase() === "e") { e.preventDefault(); $("modal").hidden = false; }
    else if (mod && e.key.toLowerCase() === "s") { e.preventDefault(); invoke("save_project", { dir, project: P() }); }
  });
  addEventListener("resize", () => { renderTimeline(); draw(); });
  bindScrub();
}

// ------------------------------------------------------------------ boot
function loadVideo(url) {
  const v = document.createElement("video");
  v.preload = "auto";
  v.playsInline = true;
  v.crossOrigin = "anonymous";
  v.src = url;
  return new Promise((res, rej) => {
    v.onloadeddata = () => res(v);
    v.onerror = () => rej(new Error("Could not load " + url));
  });
}

async function boot() {
  if (!dir) { $("loading").textContent = "No project specified."; return; }
  try {
    const loaded = await invoke("load_project", { dir });
    S.project = loaded.project;
    $("title").textContent = P().name;
    currentWindow().setTitle?.(P().name).catch?.(() => {});
    screenV = await loadVideo(convertFileSrc(`${dir}/${P().media.screen}`));
    if (P().media.camera) cameraV = await loadVideo(convertFileSrc(`${dir}/${P().media.camera}`)).catch(() => null);
    if (cameraV) cameraV.muted = true;
    // trust the real file over the recorded numbers
    if (screenV.videoWidth) {
      P().media.width = screenV.videoWidth;
      P().media.height = screenV.videoHeight;
    }
    if (isFinite(screenV.duration) && screenV.duration > 0 && Math.abs(screenV.duration * 1000 - P().media.durationMs) > 250) {
      const d = screenV.duration * 1000;
      for (const s of P().scene.slices) s.sourceEndMs = Math.min(s.sourceEndMs || d, d);
      P().media.durationMs = d;
    }
    const bgi = P().config.styles.background;
    if (bgi.kind === "image" && bgi.image) {
      bgImage = new Image();
      bgImage.src = bgi.image;
    }
    bindUI();
    renderRail();
    renderPanel();
    syncControls();
    updateUndo();
    screenV.addEventListener("seeked", () => { if (!S.playing) draw(); });
    cameraV?.addEventListener("seeked", () => { if (!S.playing) draw(); });
    await computeTrack();
    $("loading").hidden = true;
    seekOut(0);
  } catch (e) {
    $("loading").textContent = "Could not open project: " + (e?.message || e);
    console.error(e);
  }
}
boot();
