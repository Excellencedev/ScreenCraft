// Timeline rendering and interaction. Dragging touches only the dragged
// element's style; the DOM is rebuilt only when the edit is committed.
import { invoke } from "../tauri.js";
import { $, S, P, dir, uid, clamp, fmt, hooks } from "./state.js";
import { sliceLen, outDuration, outToSrc, spansOut } from "./timemap.js";
import { addMusic, clips as musicClips, drawWave, removeMusicClip } from "./music.js";
import { change } from "./history.js";
import { requestSeek, pause, play, updateTransport, setTimelineWidth } from "./playback.js";

/** Ids of every selected zoom region (the first one drives the panel). */
export const selectedZoomIds = () => (S.multi?.length ? S.multi : S.selected?.type === "zoom" ? [S.selected.id] : []);
const isZoomSel = (id) => selectedZoomIds().includes(id);

const tlWidth = () => Math.max(100, $("tlInner").clientWidth);
let rulerKey = "";

// ---- timeline zoom: the inner strip is made wider than the viewport and scrolled
const ZOOM_MAX = 60;
const rangeToZoom = (v) => Math.pow(ZOOM_MAX, v / 100);
const zoomToRange = (z) => (Math.log(z) / Math.log(ZOOM_MAX)) * 100;

/** Set the timeline zoom (1 = fit). The time under `anchorX` (client px) stays put. */
export function setTimelineZoom(z, anchorX) {
  z = clamp(z, 1, ZOOM_MAX);
  const scroll = $("tlScroll"), inner = $("tlInner");
  const rect = scroll.getBoundingClientRect();
  const ax = anchorX ?? rect.left + rect.width / 2;
  const frac = (scroll.scrollLeft + (ax - rect.left)) / Math.max(1, inner.clientWidth);
  S.tlZoom = z;
  inner.style.width = z === 1 ? "" : `${z * 100}%`;
  $("tlZoomRange").value = String(zoomToRange(z));
  renderTimeline();
  scroll.scrollLeft = frac * inner.clientWidth - (ax - rect.left);
}

export function bindTimelineZoom() {
  // when the timeline is short the tracks scroll vertically: keep the labels in step
  $("tlScroll").addEventListener("scroll", () => { $("tlLabels").scrollTop = $("tlScroll").scrollTop; });
  const step = (f) => setTimelineZoom(S.tlZoom * f);
  $("tlZoomIn").onclick = () => step(1.6);
  $("tlZoomOut").onclick = () => step(1 / 1.6);
  $("tlZoomFit").onclick = () => { setTimelineZoom(1); $("tlScroll").scrollLeft = 0; };
  $("tlZoomRange").oninput = (e) => setTimelineZoom(rangeToZoom(parseFloat(e.target.value)));
  // ⌘/Ctrl + wheel (and trackpad pinch) zooms around the pointer
  $("tlScroll").addEventListener("wheel", (e) => {
    if (!(e.ctrlKey || e.metaKey)) return;
    e.preventDefault();
    setTimelineZoom(S.tlZoom * Math.exp(-e.deltaY * 0.01), e.clientX);
  }, { passive: false });
}

function renderRuler(dur, W, pxPerMs) {
  const key = `${Math.round(dur)}|${W}`;
  if (key === rulerKey) return;
  rulerKey = key;
  const ruler = $("ruler");
  const steps = [100, 200, 500, 1000, 2000, 5000, 10000, 15000, 30000, 60000, 120000, 300000];
  const step = steps.find((s) => s * pxPerMs >= 80) || 300000;
  let html = "";
  for (let t = 0; t <= dur; t += step) {
    const x = t * pxPerMs;
    html += `<div class="tick" style="left:${x}px"></div><div class="lab" style="left:${x}px">${fmt(t).replace(/\.0$/, "")}</div>`;
  }
  ruler.innerHTML = html;
}

export function renderTimeline() {
  if (!P()) return;
  const dur = outDuration();
  const W = tlWidth();
  setTimelineWidth(W);
  const pxPerMs = W / Math.max(1, dur);
  renderRuler(dur, W, pxPerMs);

  // clip track
  const clip = $("trackClip");
  const frag = document.createDocumentFragment();
  let acc = 0;
  const transMarks = [];
  const xText = (k) => `${+k.toFixed(2)}×`;
  const slices = P().scene.slices;
  const marker = (s, at) => {
    // a small marker on every cut: click it to choose a transition
    const tm = document.createElement("div");
    tm.className = "block trans-wrap";
    tm.style.cssText = `left:${at * pxPerMs - 9}px;width:18px;top:50%;bottom:auto;height:18px;margin-top:-9px;overflow:visible;background:transparent;border:0;padding:0;z-index:6`;
    const dot = document.createElement("div");
    dot.className = "trans";
    dot.style.left = "0";
    dot.textContent = s.transition ? "◐" : "+";
    dot.title = s.transition ? `Transition: ${s.transition.kind}` : "Add a transition";
    dot.style.opacity = s.transition ? "1" : "0.55";
    dot.onpointerdown = (e) => {
      e.stopPropagation();
      S.selected = { type: "slice", id: s.id };
      S.tab = "pace";
      renderTimeline();
      hooks.renderPanel();
      hooks.syncControls();
    };
    tm.appendChild(dot);
    transMarks.push(tm);
  };
  for (let si = 0; si < slices.length; si++) {
    const s = slices[si];
    if (s.ramp) {
      // the staircase of a speed ramp is one block, not a dozen slivers
      let sj = si, glen = 0;
      while (sj < slices.length && slices[sj].ramp) glen += sliceLen(slices[sj++]);
      const first = slices[si], last = slices[sj - 1];
      const el = document.createElement("div");
      el.className = "block clip ramp" + (S.selected?.type === "slice" && slices.slice(si, sj).some((x) => x.id === S.selected.id) ? " sel" : "");
      el.style.left = acc * pxPerMs + "px";
      el.style.width = Math.max(6, glen * pxPerMs - 2) + "px";
      if (glen * pxPerMs < 52) el.classList.add("tiny");
      el.innerHTML = `<span class="pill-x">${xText(first.timeScale)} → ${xText(last.timeScale)}</span>`;
      el.title = "Speed ramp";
      el.onpointerdown = (e) => {
        e.stopPropagation();
        e.preventDefault();
        S.selected = { type: "slice", id: first.id };
        hooks.renderPanel();
        hooks.syncControls();
        renderTimeline();
        const rect = $("tlInner").getBoundingClientRect();
        requestSeek((e.clientX - rect.left) / pxPerMs);
      };
      frag.appendChild(el);
      if (si > 0) marker(first, acc);
      acc += glen;
      si = sj - 1;
      continue;
    }
    const len = sliceLen(s);
    const el = document.createElement("div");
    el.className = "block clip" + (S.selected?.type === "slice" && S.selected.id === s.id ? " sel" : "");
    el.style.left = acc * pxPerMs + "px";
    el.style.width = Math.max(4, len * pxPerMs - 2) + "px";
    if (len * pxPerMs < 24) el.classList.add("tiny"); // too short for handles or labels
    el.dataset.id = s.id;
    if (s.freeze) el.classList.add("freeze");
    const sped = s.timeScale !== 1 && !s.freeze;
    el.innerHTML = sped
      ? `<div class="h l"></div><span class="pill-x">${xText(s.timeScale)}</span><div class="h r"></div>`
      : `<div class="h l"></div><span>${s.freeze ? "Freeze" : "Clip"}</span><span class="t">${(len / 1000).toFixed(1)}s</span><div class="h r"></div>`;
    el.onpointerdown = (e) => beginClipDrag(e, s, el, pxPerMs);
    frag.appendChild(el);
    if (si > 0) marker(s, acc);
    acc += len;
  }
  for (const tm of transMarks) frag.appendChild(tm);
  clip.replaceChildren(frag);

  // zoom track: one block per stretch of the timeline that shows the region
  const zt = $("trackZoom");
  const zf = document.createDocumentFragment();
  for (const z of P().scene.zooms) {
    for (const [a, b] of spansOut(z.sourceStartMs, z.sourceEndMs)) {
      if (b - a < 1) continue;
      const el = document.createElement("div");
      el.className = "block zoom" + (z.isDisabled ? " disabled" : "") + (isZoomSel(z.id) ? " sel" : "");
      el.style.left = a * pxPerMs + "px";
      el.style.width = Math.max(14, (b - a) * pxPerMs - 2) + "px";
      el.dataset.zid = z.id;
      el.innerHTML = `<div class="h l"></div><span>Zoom</span><span class="t">${z.zoom.toFixed(1)}× · ${z.kind === "manual" ? "Manual" : "Auto"}</span><div class="h r"></div>`;
      el.onpointerdown = (e) => beginZoomDrag(e, z, el, pxPerMs, a, b);
      zf.appendChild(el);
    }
  }
  zt.replaceChildren(zf);

  // camera layouts: one block per stretch with a non-default camera look
  const ct = $("trackCamera");
  const cf = document.createDocumentFragment();
  for (const r of P().scene.cameraLayouts || []) {
    for (const [a, b] of spansOut(r.sourceStartMs, r.sourceEndMs)) {
      if (b - a < 1) continue;
      const el = document.createElement("div");
      el.className = "block layout" + (S.selected?.type === "layout" && S.selected.id === r.id ? " sel" : "");
      el.style.left = a * pxPerMs + "px";
      el.style.width = Math.max(14, (b - a) * pxPerMs - 2) + "px";
      el.innerHTML = `<div class="h l"></div><span>${LAYOUT_NAMES[r.layout] || r.layout}${r.settings ? " · custom" : ""}</span><div class="h r"></div>`;
      el.onpointerdown = (e) => beginRegionDrag(e, r, "layout", el, pxPerMs, a, b);
      cf.appendChild(el);
    }
  }
  ct.replaceChildren(cf);
  if (!(P().scene.cameraLayouts || []).length) {
    ct.innerHTML = '<div class="track-hint">Change how the camera looks over time: add a layout from the Camera tab.</div>';
  }

  // notes (annotations): greedy lanes so overlapping ones stack
  const nt = $("trackNotes");
  const nf = document.createDocumentFragment();
  const lanes = [];
  const items = [];
  for (const an of P().scene.annotations || []) {
    for (const [a, b] of spansOut(an.sourceStartMs, an.sourceEndMs)) {
      if (b - a >= 1) items.push({ an, a, b });
    }
  }
  items.sort((x, y) => x.a - y.a);
  for (const it of items) {
    let lane = lanes.findIndex((end) => end <= it.a + 1);
    if (lane < 0) lane = lanes.push(0) - 1;
    lanes[lane] = it.b;
    it.lane = lane;
  }
  for (const { an, a, b, lane } of items) {
    const el = document.createElement("div");
    el.className = `block note k-${an.kind}` + (S.selected?.type === "annot" && S.selected.id === an.id ? " sel" : "");
    el.style.left = a * pxPerMs + "px";
    el.style.width = Math.max(14, (b - a) * pxPerMs - 2) + "px";
    el.style.marginTop = 4 + lane * 32 + "px";
    const label = an.kind === "text" ? `“${(an.text || "").slice(0, 28)}”` : NOTE_NAMES[an.kind] || an.kind;
    el.innerHTML = `<div class="h l"></div><span>${label.replace(/</g, "&lt;")}</span><div class="h r"></div>`;
    el.onpointerdown = (e) => beginRegionDrag(e, an, "annot", el, pxPerMs, a, b);
    nf.appendChild(el);
  }
  nt.replaceChildren(nf);
  nt.style.height = Math.max(38, 8 + Math.max(1, lanes.length) * 32) + "px";
  if (!items.length) nt.innerHTML = '<div class="track-hint">Text, arrows, blur and spotlight appear here: add them from the Annotate tab.</div>';
  // captions
  const kt = $("trackCaptions");
  const kf = document.createDocumentFragment();
  let capCount = 0;
  for (const c of P().scene.subtitles || []) {
    for (const [a, b] of spansOut(c.sourceStartMs, c.sourceEndMs)) {
      if (b - a < 1) continue;
      capCount++;
      const el = document.createElement("div");
      el.className = "block caption" + (S.selected?.type === "caption" && S.selected.id === c.id ? " sel" : "");
      el.style.left = a * pxPerMs + "px";
      el.style.width = Math.max(14, (b - a) * pxPerMs - 2) + "px";
      const label = (c.text || "").replace(/\s+/g, " ").slice(0, 40) || "(empty)";
      el.innerHTML = `<div class="h l"></div><span></span><div class="h r"></div>`;
      el.querySelector("span").textContent = label;
      el.onpointerdown = (e) => beginRegionDrag(e, c, "caption", el, pxPerMs, a, b);
      kf.appendChild(el);
    }
  }
  kt.replaceChildren(kf);
  if (!capCount) kt.innerHTML = '<div class="track-hint">Captions go here: add them from the Captions tab, or import an SRT.</div>';
  // keep the labels column aligned with the (variable height) tracks
  const labs = $("tlLabels").children;
  ["trackClip", "trackZoom", "trackCamera", "trackNotes", "trackCaptions", "trackMusic"].forEach((id, i) => {
    if (labs[i]) labs[i].style.height = $(id).offsetHeight + "px";
  });

  // music track: one block per clip, with the file's waveform drawn inside
  const mt = $("trackMusic");
  const list = musicClips();
  if (!list.length) {
    const b = document.createElement("button");
    b.className = "add-music";
    b.textContent = "+ Add music";
    b.onclick = addMusic;
    mt.replaceChildren(b);
  } else {
    const mf = document.createDocumentFragment();
    for (const c of list) {
      const el = document.createElement("div");
      el.className = "block music" + (S.selected?.type === "music" && S.selected.id === c.id ? " sel" : "");
      el.style.left = c.offsetMs * pxPerMs + "px";
      el.style.width = Math.max(14, c.durMs * pxPerMs - 2) + "px";
      el.dataset.id = c.id;
      el.innerHTML = `<div class="h l"></div><canvas class="wave"></canvas><span class="mname">♪ ${c.name || "Music"}</span><div class="h r"></div>`;
      el.onpointerdown = (e) => beginMusicDrag(e, c, el, pxPerMs);
      mf.appendChild(el);
    }
    mt.replaceChildren(mf);
    for (const el of mt.children) {
      const c = list.find((x) => x.id === el.dataset.id);
      if (c) drawWave(el.querySelector("canvas.wave"), c);
    }
  }
  updateTransport();
}

export const LAYOUT_NAMES = {
  bubble: "Bubble", hidden: "Camera off", large: "Large bubble", camera: "Camera only",
  "side-right": "Side by side", "side-left": "Side by side ◂",
};
export const NOTE_NAMES = {
  text: "Text", box: "Box", ellipse: "Ellipse", arrow: "Arrow", highlight: "Highlight",
  blur: "Blur", pixelate: "Pixelate", spotlight: "Spotlight",
};

/** Layout / annotation block: edge handles trim, the body moves it, a click selects it. */
function beginRegionDrag(e, item, type, el, pxPerMs, a0, b0) {
  e.stopPropagation();
  e.preventDefault();
  S.selected = { type, id: item.id };
  S.multi = null;
  S.tab = type === "layout" ? "camera" : type === "caption" ? "captions" : "annotate";
  document.querySelectorAll(".block.sel").forEach((b) => b.classList.remove("sel"));
  el.classList.add("sel");
  el.setPointerCapture(e.pointerId);
  hooks.renderPanel();
  hooks.syncControls();
  hooks.draw();
  const mode = e.target.classList.contains("l") ? "l" : e.target.classList.contains("r") ? "r" : "move";
  const x0 = e.clientX;
  const total = outDuration();
  let na = a0, nb = b0, moved = false;
  el.onpointermove = (ev) => {
    if (Math.abs(ev.clientX - x0) < 3 && !moved) return;
    moved = true;
    const d = (ev.clientX - x0) / pxPerMs;
    let a = a0, b = b0;
    if (mode === "move") {
      const w = b0 - a0;
      a = clamp(a0 + d, 0, total - w);
      b = a + w;
    } else if (mode === "l") {
      a = clamp(a0 + d, 0, b0 - 200);
    } else {
      b = clamp(b0 + d, a0 + 200, total);
    }
    na = a;
    nb = b;
    el.style.left = a * pxPerMs + "px";
    el.style.width = Math.max(14, (b - a) * pxPerMs - 2) + "px";
  };
  el.onpointerup = (ev) => {
    el.onpointermove = el.onpointerup = null;
    try { el.releasePointerCapture(ev.pointerId); } catch {}
    if (!moved) return;
    const ns = outToSrc(na).src, ne = outToSrc(nb).src;
    change(null, () => {
      item.sourceStartMs = ns;
      item.sourceEndMs = Math.max(ns + 100, ne);
    }, { timeline: true, panel: false });
  };
}

/** Clip block: edge handles trim, dragging the body re-orders, a click selects + seeks. */
function beginClipDrag(e, s, el, pxPerMs) {
  e.stopPropagation();
  e.preventDefault();
  el.setPointerCapture(e.pointerId);
  S.selected = { type: "slice", id: s.id };
  document.querySelectorAll("#trackZoom .block.sel, #trackClip .block.sel").forEach((b) => b.classList.remove("sel"));
  el.classList.add("sel");
  hooks.renderPanel();
  hooks.syncControls();

  const mode = e.target.classList.contains("l") ? "l" : e.target.classList.contains("r") ? "r" : "move";
  const x0 = e.clientX;
  const rect = $("tlInner").getBoundingClientRect();
  const left0 = parseFloat(el.style.left), width0 = parseFloat(el.style.width);
  const total = P().media.durationMs;
  const start0 = s.sourceStartMs, end0 = s.sourceEndMs;
  let moved = false;
  let ns = start0, ne = end0, drop = -1;

  el.onpointermove = (ev) => {
    const dx = ev.clientX - x0;
    if (!moved && Math.abs(dx) < 4) return;
    moved = true;
    if (mode === "move") {
      el.classList.add("dragging");
      el.style.transform = `translateX(${dx}px)`;
      // drop slot = number of other blocks whose centre is left of the pointer
      const px = ev.clientX - rect.left;
      let idx = 0;
      for (const b of $("trackClip").children) {
        if (b === el) continue;
        const c = parseFloat(b.style.left) + parseFloat(b.style.width) / 2;
        if (c < px) idx++;
      }
      drop = idx;
    } else if (mode === "l") {
      const dSrc = (dx / pxPerMs) * s.timeScale;
      ns = clamp(start0 + dSrc, 0, end0 - 200);
      const w = (end0 - ns) / s.timeScale * pxPerMs;
      el.style.left = left0 + width0 - w + "px";
      el.style.width = Math.max(8, w) + "px";
    } else {
      const dSrc = (dx / pxPerMs) * s.timeScale;
      ne = clamp(end0 + dSrc, start0 + 200, total);
      el.style.width = Math.max(8, ((ne - start0) / s.timeScale) * pxPerMs) + "px";
    }
  };
  el.onpointerup = (ev) => {
    el.onpointermove = el.onpointerup = null;
    try { el.releasePointerCapture(ev.pointerId); } catch {}
    el.classList.remove("dragging");
    el.style.transform = "";
    if (!moved) {
      requestSeek((ev.clientX - rect.left) / pxPerMs);
      return;
    }
    if (mode === "move") {
      const from = P().scene.slices.findIndex((x) => x.id === s.id);
      if (drop < 0 || drop === from) return renderTimeline();
      change(null, () => {
        const [it] = P().scene.slices.splice(from, 1);
        P().scene.slices.splice(drop, 0, it);
      }, { timeline: true });
    } else {
      change(null, () => { s.sourceStartMs = ns; s.sourceEndMs = ne; }, { timeline: true });
    }
  };
}

function beginZoomDrag(e, z, el, pxPerMs, a0, b0) {
  e.stopPropagation();
  e.preventDefault();
  S.tab = "zoom";
  // Shift / ⌘ / Ctrl-click adds or removes a region from the selection (no drag).
  if (e.shiftKey || e.metaKey || e.ctrlKey) {
    const ids = selectedZoomIds().slice();
    const at = ids.indexOf(z.id);
    if (at >= 0) ids.splice(at, 1);
    else ids.push(z.id);
    S.selected = ids.length ? { type: "zoom", id: ids[0] } : null;
    S.multi = ids.length > 1 ? ids : null;
    renderTimeline();
    hooks.renderPanel();
    hooks.syncControls();
    return;
  }
  const wasInGroup = isZoomSel(z.id) && S.multi?.length > 1;
  if (!wasInGroup) {
    S.selected = { type: "zoom", id: z.id };
    S.multi = null;
  }
  el.setPointerCapture(e.pointerId);
  document.querySelectorAll("#trackZoom .block.sel, #trackClip .block.sel").forEach((b) => b.classList.remove("sel"));
  el.classList.add("sel");
  hooks.renderPanel();
  hooks.syncControls();

  const mode = e.target.classList.contains("l") ? "l" : e.target.classList.contains("r") ? "r" : "move";
  const x0 = e.clientX;
  const total = outDuration();
  let na = a0, nb = b0, moved = false;

  el.onpointermove = (ev) => {
    if (Math.abs(ev.clientX - x0) < 3 && !moved) return;
    moved = true;
    const d = (ev.clientX - x0) / pxPerMs;
    let a = a0, b = b0;
    if (mode === "move") {
      const w = b0 - a0;
      a = clamp(a0 + d, 0, total - w);
      b = a + w;
    } else if (mode === "l") {
      a = clamp(a0 + d, 0, b0 - 300);
    } else {
      b = clamp(b0 + d, a0 + 300, total);
    }
    na = a; nb = b;
    el.style.left = a * pxPerMs + "px";
    el.style.width = Math.max(14, (b - a) * pxPerMs - 2) + "px";
    // the rest of the selection follows the drag
    if (wasInGroup && mode === "move") {
      const shift = (a - a0) * pxPerMs;
      for (const id of S.multi || []) {
        if (id === z.id) continue;
        document.querySelectorAll(`#trackZoom .block[data-zid="${id}"]`).forEach((b2) => (b2.style.transform = `translateX(${shift}px)`));
      }
    }
  };
  el.onpointerup = (ev) => {
    el.onpointermove = el.onpointerup = null;
    try { el.releasePointerCapture(ev.pointerId); } catch {}
    if (moved && wasInGroup && mode === "move") {
      // move every selected region by the same amount of recording time
      const delta = outToSrc(na).src - outToSrc(a0).src;
      const ids = (S.multi || []).slice();
      const total = P().media.durationMs;
      change("zoom-group-" + z.id, () => {
        for (const id of ids) {
          const r = P().scene.zooms.find((x) => x.id === id);
          if (!r) continue;
          const len = r.sourceEndMs - r.sourceStartMs;
          const start = Math.min(Math.max(0, r.sourceStartMs + delta), Math.max(0, total - len));
          r.sourceStartMs = start;
          r.sourceEndMs = start + len;
        }
      }, { timeline: true });
      return;
    }
    if (!moved) {
      if (wasInGroup) {
        S.selected = { type: "zoom", id: z.id };
        S.multi = null;
        renderTimeline();
        hooks.renderPanel();
      }
      return;
    }
    S.selected = { type: "zoom", id: z.id };
    S.multi = null;
    const ns = outToSrc(na).src, ne = outToSrc(nb).src;
    change("zoom-drag-" + z.id, () => {
      const spans = spansOut(z.sourceStartMs, z.sourceEndMs);
      // the 1 ms eases a split leaves behind only make sense while the halves touch
      if (z.easeInMs === 1) z.easeInMs = null;
      if (z.easeOutMs === 1) z.easeOutMs = null;
      if (spans.length <= 1) {
        z.sourceStartMs = ns;
        z.sourceEndMs = ne;
        return;
      }
      // The region shows up in several places (clips were re-ordered): detach the stretch that
      // was dragged and keep every other stretch as its own region, so nothing disappears.
      const k = Math.max(0, spans.findIndex((sp) => Math.abs(sp[0] - a0) < 2 && Math.abs(sp[1] - b0) < 2));
      const copies = spans.map((sp, j) => (j === k ? null : { ...JSON.parse(JSON.stringify(z)), id: uid(), sourceStartMs: sp[2], sourceEndMs: sp[3] }));
      const zs = P().scene.zooms;
      const at = zs.indexOf(z);
      z.sourceStartMs = ns;
      z.sourceEndMs = ne;
      zs.splice(at + 1, 0, ...copies.filter(Boolean));
    }, { timeline: true });
  };
}

/** Music clip: body moves it freely, the ends trim it. */
function beginMusicDrag(e, c, el, pxPerMs) {
  e.stopPropagation();
  e.preventDefault();
  el.setPointerCapture(e.pointerId);
  S.selected = { type: "music", id: c.id };
  S.tab = "audio";
  document.querySelectorAll(".block.sel").forEach((b) => b.classList.remove("sel"));
  el.classList.add("sel");
  hooks.renderPanel();

  const mode = e.target.classList.contains("l") ? "l" : e.target.classList.contains("r") ? "r" : "move";
  const x0 = e.clientX;
  const total = outDuration();
  const { offsetMs: o0, inMs: i0, durMs: d0, fileDurMs: fd } = c;
  const cv = el.querySelector("canvas.wave");
  let v = { offsetMs: o0, inMs: i0, durMs: d0 };
  let moved = false;
  el.onpointermove = (ev) => {
    if (!moved && Math.abs(ev.clientX - x0) < 3) return;
    moved = true;
    const d = (ev.clientX - x0) / pxPerMs;
    if (mode === "move") {
      v = { offsetMs: clamp(o0 + d, 0, Math.max(0, total - 200)), inMs: i0, durMs: d0 };
    } else if (mode === "l") {
      const delta = clamp(d, Math.max(-i0, -o0), d0 - 200);
      v = { offsetMs: o0 + delta, inMs: i0 + delta, durMs: d0 - delta };
    } else {
      v = { offsetMs: o0, inMs: i0, durMs: clamp(d0 + d, 200, Math.max(200, (fd || d0 + i0) - i0)) };
    }
    el.style.left = v.offsetMs * pxPerMs + "px";
    el.style.width = Math.max(14, v.durMs * pxPerMs - 2) + "px";
    // keep the waveform where it was while an end is being trimmed
    if (cv) {
      cv.style.width = d0 * pxPerMs - 2 + "px";
      cv.style.left = (mode === "l" ? (o0 - v.offsetMs) * pxPerMs : 0) + "px";
    }
  };
  el.onpointerup = (ev) => {
    el.onpointermove = el.onpointerup = null;
    try { el.releasePointerCapture(ev.pointerId); } catch {}
    if (!moved) return;
    change(null, () => {
      c.offsetMs = Math.round(v.offsetMs);
      c.inMs = Math.round(v.inMs);
      c.durMs = Math.round(v.durMs);
    }, { track: false, timeline: true });
  };
}

// ---- scrubbing on ruler / empty track space
export function bindScrub() {
  const inner = $("tlInner");
  inner.addEventListener("pointerdown", (e) => {
    if (e.target.closest(".block, .add-music")) return;
    S.selected = null;
    S.multi = null;
    document.querySelectorAll(".block.sel").forEach((b) => b.classList.remove("sel"));
    hooks.renderPanel();
    const wasPlaying = S.playing;
    if (wasPlaying) pause();
    S.scrubbing = true;
    inner.setPointerCapture(e.pointerId);
    const seekTo = (ev, fast) => {
      const r = inner.getBoundingClientRect();
      requestSeek(clamp(((ev.clientX - r.left) / r.width) * outDuration(), 0, outDuration()), fast);
    };
    seekTo(e, true);
    const mv = (ev) => seekTo(ev, true);
    const up = (ev) => {
      inner.removeEventListener("pointermove", mv);
      inner.removeEventListener("pointerup", up);
      try { inner.releasePointerCapture(ev.pointerId); } catch {}
      S.scrubbing = false;
      seekTo(ev, false); // exact frame + full-resolution redraw
      if (wasPlaying) play();
    };
    inner.addEventListener("pointermove", mv);
    inner.addEventListener("pointerup", up);
  });
}

// ---- edit operations
/**
 * Split at the playhead. Nothing selected: everything (clips and zoom regions) is cut.
 * A clip or zoom selected: only that one.
 */
export function splitAtPlayhead() {
  const sel = S.selected;
  const { src, idx } = outToSrc(S.tOut);
  if (sel?.type === "layout" || sel?.type === "annot" || sel?.type === "caption") {
    const list = regionList(sel.type);
    const it = list.find((x) => x.id === sel.id);
    if (!it || src <= it.sourceStartMs + 50 || src >= it.sourceEndMs - 50) return;
    change(null, () => {
      const right = { ...JSON.parse(JSON.stringify(it)), id: uid(), sourceStartMs: src };
      it.sourceEndMs = src;
      list.splice(list.indexOf(it) + 1, 0, right);
      S.selected = { type: sel.type, id: right.id };
    }, { panel: true, timeline: true });
    return;
  }
  const slices = P().scene.slices;
  const doClips = !sel || sel.type === "slice";
  const doZooms = !sel || sel.type === "zoom";

  // the clip to cut: the selected one if the playhead is on it, else the one under the playhead
  let clipIdx = -1;
  if (doClips) {
    if (sel?.type === "slice") {
      const si = slices.findIndex((x) => x.id === sel.id);
      if (si === idx) clipIdx = si;
    } else clipIdx = idx;
    if (clipIdx >= 0) {
      const s = slices[clipIdx];
      if (src <= s.sourceStartMs + 50 || src >= s.sourceEndMs - 50) clipIdx = -1;
    }
  }
  const doMusic = !sel || sel.type === "music";
  const mclips = doMusic
    ? musicClips().filter((c) => (sel ? c.id === sel.id : true) && S.tOut > c.offsetMs + 50 && S.tOut < c.offsetMs + c.durMs - 50)
    : [];
  const zoomIds = selectedZoomIds();
  const zooms = doZooms
    ? P().scene.zooms.filter((z) => (sel ? zoomIds.includes(z.id) : true) && src > z.sourceStartMs + 50 && src < z.sourceEndMs - 50)
    : [];
  if (clipIdx < 0 && !zooms.length && !mclips.length) return;

  change(null, () => {
    if (clipIdx >= 0) {
      const s = slices[clipIdx];
      const right = { ...s, id: uid(), sourceStartMs: src };
      s.sourceEndMs = src;
      slices.splice(clipIdx + 1, 0, right);
      if (sel?.type === "slice") S.selected = { type: "slice", id: right.id };
    }
    for (const c of mclips) {
      const cut = S.tOut - c.offsetMs;
      const right = { ...c, id: uid(), offsetMs: S.tOut, inMs: c.inMs + cut, durMs: c.durMs - cut };
      c.durMs = cut;
      const list = musicClips();
      list.splice(list.indexOf(c) + 1, 0, right);
      if (sel?.type === "music") S.selected = { type: "music", id: right.id };
    }
    const rights = [];
    for (const z of zooms) {
      const right = { ...JSON.parse(JSON.stringify(z)), id: uid(), sourceStartMs: src };
      rights.push(right.id);
      // tiny eases at the cut so the camera doesn't dip between the two halves
      z.sourceEndMs = src;
      z.easeOutMs = 1;
      right.easeInMs = 1;
      const at = P().scene.zooms.indexOf(z);
      P().scene.zooms.splice(at + 1, 0, right);
    }
    if (sel?.type === "zoom" && rights.length) {
      S.selected = { type: "zoom", id: rights[0] };
      S.multi = rights.length > 1 ? rights : null;
    }
  }, { panel: true, timeline: true });
}

const REGION_KEYS = { layout: "cameraLayouts", annot: "annotations", caption: "subtitles" };
const regionList = (type) => (P().scene[REGION_KEYS[type]] ??= []);

export function addCaptionAtPlayhead() {
  const { start, end } = regionAtPlayhead(2500);
  const c = { id: uid(), sourceStartMs: start, sourceEndMs: end, text: "Your caption" };
  change(null, () => {
    regionList("caption").push(c);
    S.selected = { type: "caption", id: c.id };
    S.tab = "captions";
  }, { panel: true, timeline: true, track: false });
}

export function duplicateSelected() {
  const sel = S.selected;
  if (sel?.type === "annot") {
    const a = P().scene.annotations.find((x) => x.id === sel.id);
    if (!a) return;
    change(null, () => {
      const copy = { ...JSON.parse(JSON.stringify(a)), id: uid(), x: Math.min(0.9, a.x + 0.04), y: Math.min(0.9, a.y + 0.05) };
      P().scene.annotations.push(copy);
      S.selected = { type: "annot", id: copy.id };
    }, { panel: true, timeline: true });
    return;
  }
  if (!sel || sel.type !== "slice") return;
  const idx = P().scene.slices.findIndex((x) => x.id === sel.id);
  if (idx < 0) return;
  change(null, () => {
    const copy = { ...P().scene.slices[idx], id: uid() };
    P().scene.slices.splice(idx + 1, 0, copy);
    S.selected = { type: "slice", id: copy.id };
  }, { panel: true, timeline: true });
}

export function deleteSelected() {
  const sel = S.selected;
  if (!sel) return;
  if (sel.type === "music") {
    removeMusicClip(sel.id);
  } else if (sel.type === "layout" || sel.type === "annot" || sel.type === "caption") {
    const key = REGION_KEYS[sel.type];
    change(null, () => { P().scene[key] = P().scene[key].filter((x) => x.id !== sel.id); S.selected = null; }, { panel: true, timeline: true });
  } else if (sel.type === "zoom") {
    const ids = selectedZoomIds();
    change(null, () => { P().scene.zooms = P().scene.zooms.filter((z) => !ids.includes(z.id)); S.selected = null; S.multi = null; }, { panel: true, timeline: true });
  } else if (P().scene.slices.length > 1) {
    change(null, () => { P().scene.slices = P().scene.slices.filter((s) => s.id !== sel.id); S.selected = null; }, { panel: true, timeline: true });
    requestSeek(Math.min(S.tOut, outDuration()));
  }
}

/** Select every zoom region (⌘/Ctrl + A while editing zooms). */
export function selectAllZooms() {
  const ids = P().scene.zooms.map((z) => z.id);
  if (!ids.length) return;
  S.selected = { type: "zoom", id: ids[0] };
  S.multi = ids.length > 1 ? ids : null;
  S.tab = "zoom";
  renderTimeline();
  hooks.renderPanel();
}

/** Where new regions go: from the playhead, up to `len` ms long, inside the current clip. */
function regionAtPlayhead(len) {
  const { src, idx } = outToSrc(S.tOut);
  const sl = P().scene.slices[idx];
  const start = Math.min(src, Math.max(sl.sourceStartMs, sl.sourceEndMs - 400));
  return { start, end: Math.min(start + len, sl.sourceEndMs) };
}

export function addLayoutAtPlayhead(layout = "side-right", len = 4000) {
  const { start, end } = regionAtPlayhead(len);
  const r = { id: uid(), sourceStartMs: start, sourceEndMs: end, layout };
  change(null, () => {
    P().scene.cameraLayouts = P().scene.cameraLayouts || [];
    P().scene.cameraLayouts.push(r);
    S.selected = { type: "layout", id: r.id };
    S.tab = "camera";
  }, { panel: true, timeline: true });
}

const ANNOT_DEFAULTS = {
  text: { space: "screen", x: 0.1, y: 0.8, w: 0.6, h: 0.1, text: "Your text", fontSize: 48, color: "#ffffff", fill: "#000000b3" },
  box: { space: "content", x: 0.35, y: 0.35, w: 0.3, h: 0.2, color: "#ffd60a", thickness: 5 },
  ellipse: { space: "content", x: 0.4, y: 0.35, w: 0.2, h: 0.2, color: "#ffd60a", thickness: 5 },
  highlight: { space: "content", x: 0.3, y: 0.4, w: 0.4, h: 0.06, color: "#ffe14a" },
  arrow: { space: "content", x: 0.3, y: 0.3, x2: 0.5, y2: 0.5, color: "#ff453a", thickness: 6 },
  blur: { space: "content", x: 0.3, y: 0.4, w: 0.3, h: 0.1, strength: 0.7, anim: "none" },
  pixelate: { space: "content", x: 0.3, y: 0.4, w: 0.3, h: 0.1, strength: 0.6, anim: "none" },
  spotlight: { space: "content", x: 0.3, y: 0.3, w: 0.4, h: 0.3, strength: 0.65 },
};

export function addAnnotationAtPlayhead(kind = "text") {
  const { start, end } = regionAtPlayhead(kind === "blur" || kind === "pixelate" ? 6000 : 3500);
  const base = {
    space: "content", x: 0.4, y: 0.4, w: 0.3, h: 0.1, x2: 0.6, y2: 0.6, text: "", color: "#ffffff", fill: "",
    fontSize: 44, thickness: 5, strength: 0.6, anim: "fade", animMs: 250,
  };
  const a = { ...base, ...ANNOT_DEFAULTS[kind], id: uid(), kind, sourceStartMs: start, sourceEndMs: end };
  change(null, () => {
    P().scene.annotations = P().scene.annotations || [];
    P().scene.annotations.push(a);
    S.selected = { type: "annot", id: a.id };
    S.tab = "annotate";
  }, { panel: true, timeline: true, track: false });
}

export function addZoomAtPlayhead() {
  const { src, idx } = outToSrc(S.tOut);
  const sl = P().scene.slices[idx];
  const end = Math.min(src + 2500, sl.sourceEndMs);
  if (end - src < 400) return;
  const z = {
    id: uid(), zoom: P().config.animations.defaultZoom, kind: "follow-cursor", manualTargetPoint: { x: 0.5, y: 0.5 },
    isDisabled: false, sourceStartMs: src, sourceEndMs: end, instant: false,
  };
  change(null, () => { P().scene.zooms.push(z); S.selected = { type: "zoom", id: z.id }; S.tab = "zoom"; }, { panel: true, timeline: true });
}

export async function autoZoom() {
  const zooms = await invoke("auto_zoom", { dir, project: P(), zoom: P().config.animations.defaultZoom });
  change(null, () => { P().scene.zooms = zooms.map((z) => ({ ...z, id: uid() })); S.selected = null; }, { panel: true, timeline: true });
}
