// Editor entry point: boot + event wiring. Everything else lives in sibling modules.
import { invoke, convertFileSrc, currentWindow } from "../tauri.js";
import { $, S, P, dir, hooks, media, clamp } from "./state.js";
import { outDuration } from "./timemap.js";
import { change, undo, redo, updateUndo } from "./history.js";
import { draw, frameIndex, displayedRect } from "./draw.js";
import { play, pause, requestSeek, scheduleTrack, computeTrack, refreshSliceAudio, syncMusic } from "./playback.js";
import {
  renderTimeline, bindScrub, splitAtPlayhead, deleteSelected, addZoomAtPlayhead, autoZoom, duplicateSelected,
} from "./timeline.js";
import { renderPanel } from "./panels/index.js";
import { presetsMenu } from "./presets.js";
import { showMenu } from "./menus.js";
import { rawMenu } from "./rawfiles.js";
import { openExport, bindExport } from "./exportDialog.js";
import { contentRect, camAt } from "../render.js";
import { buildScrubCache } from "./scrubcache.js";
import { loadMusicElement } from "./music.js";
import { autoGain } from "../audiolevel.js";

hooks.draw = draw;
hooks.syncMusic = syncMusic;
hooks.renderTimeline = renderTimeline;
hooks.renderPanel = renderPanel;
hooks.scheduleTrack = scheduleTrack;
hooks.seek = (t) => requestSeek(t);

function syncControls() {
  $("selAspect").value = P().config.output.aspectRatio;
  const sel = S.selected?.type === "slice" && P().scene.slices.find((s) => s.id === S.selected.id);
  const s = sel || P().scene.slices[S.curSlice] || P().scene.slices[0];
  $("selSpeed").value = String(s.timeScale);
  $("title").textContent = P().name;
}
hooks.syncControls = syncControls;

/** Click the title to rename the project (and its folder). */
function bindTitle() {
  const el = $("title");
  el.title = "Click to rename";
  el.onclick = () => {
    if (el.isContentEditable) return;
    el.contentEditable = "plaintext-only";
    el.classList.add("editing");
    el.focus();
    getSelection().selectAllChildren(el);
  };
  el.onkeydown = (e) => {
    e.stopPropagation();
    if (e.key === "Enter") { e.preventDefault(); el.blur(); }
    else if (e.key === "Escape") { el.textContent = P().name; el.blur(); }
  };
  el.onblur = async () => {
    el.contentEditable = "false";
    el.classList.remove("editing");
    const name = el.textContent.trim();
    if (!name || name === P().name) { el.textContent = P().name; return; }
    try {
      await invoke("save_project", { dir, project: P() });
      const next = await invoke("rename_project", { dir, name });
      location.replace(`editor.html?dir=${encodeURIComponent(next)}`);
    } catch (err) {
      el.textContent = P().name;
      alert(String(err));
    }
  };
}

function bindUI() {
  bindTitle();
  $("btnPlay").onclick = () => (S.playing ? pause() : play());
  $("btnStart").onclick = () => { pause(); requestSeek(0); };
  $("btnEnd").onclick = () => { pause(); requestSeek(outDuration()); };
  $("btnUndo").onclick = undo;
  $("btnRedo").onclick = redo;
  $("btnSplit").onclick = splitAtPlayhead;
  $("btnDelete").onclick = deleteSelected;
  $("btnZoomHere").onclick = addZoomAtPlayhead;
  $("btnDuplicate").onclick = duplicateSelected;
  $("btnAutoZoom").onclick = autoZoom;
  $("btnFolder").onclick = async (e) => {
    const list = await invoke("recent_projects");
    const items = [{ header: "Projects" }];
    for (const pr of list.slice(0, 10)) items.push({ label: pr.name, run: () => invoke("open_editor", { dir: pr.dir }) });
    items.push("-", { label: "Show this project in Finder", run: () => invoke("reveal_in_finder", { path: dir }) });
    showMenu(e.currentTarget, items);
  };
  $("btnCrop").onclick = () => { S.tab = "crop"; renderPanel(); };
  $("btnExport").onclick = openExport;
  $("btnRaw").onclick = (e) => rawMenu(e.currentTarget);
  $("btnPresets").onclick = (e) => presetsMenu(e.currentTarget);
  $("selAspect").onchange = (e) => change(null, () => (P().config.output.aspectRatio = e.target.value), { track: false });
  $("selPreview").onchange = (e) => { S.previewRes = parseInt(e.target.value, 10); draw(); };
  $("selSpeed").onchange = (e) => {
    const v = parseFloat(e.target.value);
    const target = S.selected?.type === "slice" ? P().scene.slices.find((s) => s.id === S.selected.id) : P().scene.slices[S.curSlice];
    if (!target) return;
    change(null, () => (target.timeScale = v), { timeline: true });
    if (S.playing) refreshSliceAudio();
  };

  // preview click: play/pause, or pick the manual zoom target
  $("preview").addEventListener("pointerdown", (e) => {
    const z = S.selected?.type === "zoom" && P().scene.zooms.find((x) => x.id === S.selected.id);
    if (!z || z.kind !== "manual") {
      S.playing ? pause() : play();
      return;
    }
    const c = $("preview"), rect = displayedRect();
    const px = ((e.clientX - rect.left) / rect.width) * c.width, py = ((e.clientY - rect.top) / rect.height) * c.height;
    const r = contentRect(P(), c.width, c.height);
    const cam = camAt(S.track, frameIndex());
    const crop = P().config.crop;
    const vwN = crop.width / cam.zoom, vhN = crop.height / cam.zoom;
    const nx = cam.cx - vwN / 2 + ((px - r.x) / r.w) * vwN, ny = cam.cy - vhN / 2 + ((py - r.y) / r.h) * vhN;
    change("manual-" + z.id, () => (z.manualTargetPoint = { x: clamp(nx, 0, 1), y: clamp(ny, 0, 1) }));
  });

  addEventListener("keydown", (e) => {
    if (["INPUT", "SELECT", "TEXTAREA"].includes(e.target.tagName)) return;
    const mod = e.metaKey || e.ctrlKey;
    if (e.code === "Space") { e.preventDefault(); S.playing ? pause() : play(); }
    else if (e.key === "s" && !mod) splitAtPlayhead();
    else if (e.key === "z" && !mod) addZoomAtPlayhead();
    else if (e.key === "d" && !mod) duplicateSelected();
    else if (e.key === "Backspace" || e.key === "Delete") { e.preventDefault(); deleteSelected(); }
    else if (e.key === "ArrowLeft") { pause(); requestSeek(S.tOut - (e.shiftKey ? 1000 : 1000 / 30)); }
    else if (e.key === "ArrowRight") { pause(); requestSeek(S.tOut + (e.shiftKey ? 1000 : 1000 / 30)); }
    else if (mod && e.key.toLowerCase() === "z") { e.preventDefault(); e.shiftKey ? redo() : undo(); }
    else if (mod && e.key.toLowerCase() === "e") { e.preventDefault(); openExport(); }
    else if (mod && e.key.toLowerCase() === "s") { e.preventDefault(); invoke("save_project", { dir, project: P() }); }
  });
  let rz = null;
  addEventListener("resize", () => {
    clearTimeout(rz);
    rz = setTimeout(() => { renderTimeline(); draw(); }, 80);
  });
  bindScrub();
  bindExport();
}

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

/** Measure the microphone once so it can be levelled automatically. */
async function levelMic(url) {
  try {
    const bytes = await (await fetch(url)).arrayBuffer();
    const ctx = new AudioContext();
    const buf = await ctx.decodeAudioData(bytes);
    ctx.close?.();
    media.micLevel = autoGain(buf);
    refreshSliceAudio();
  } catch (e) {
    console.warn("mic level", e);
  }
}

async function boot() {
  if (!dir) { $("loading").textContent = "No project specified."; return; }
  try {
    const loaded = await invoke("load_project", { dir });
    S.project = loaded.project;
    $("title").textContent = P().name;
    currentWindow().setTitle?.(P().name).catch?.(() => {});
    media.screen = await loadVideo(convertFileSrc(`${dir}/${P().media.screen}`));
    if (P().media.camera) media.camera = await loadVideo(convertFileSrc(`${dir}/${P().media.camera}`)).catch(() => null);
    if (media.camera) media.camera.muted = true;
    if (P().media.mic) {
      media.mic = await new Promise((res) => {
        const a = new Audio();
        a.crossOrigin = "anonymous";
        a.src = convertFileSrc(`${dir}/${P().media.mic}`);
        a.preload = "auto";
        a.onloadeddata = () => res(a);
        a.onerror = () => res(null);
      });
    }
    if (media.mic) levelMic(convertFileSrc(`${dir}/${P().media.mic}`));
    const v = media.screen;
    // trust the real file over the recorded numbers
    if (v.videoWidth) {
      P().media.width = v.videoWidth;
      P().media.height = v.videoHeight;
    }
    if (isFinite(v.duration) && v.duration > 0 && Math.abs(v.duration * 1000 - P().media.durationMs) > 250) {
      const d = v.duration * 1000;
      for (const s of P().scene.slices) s.sourceEndMs = Math.min(s.sourceEndMs || d, d);
      P().media.durationMs = d;
    }
    const bg = P().config.styles.background;
    if (bg.kind === "image" && bg.image) {
      media.bgImage = new Image();
      media.bgImage.src = bg.image;
    }
    bindUI();
    renderPanel();
    syncControls();
    updateUndo();
    await computeTrack();
    await loadMusicElement();
    // Pre-render frames into memory behind a spinner so the first scrub is already smooth.
    const prep = $("prep");
    prep.hidden = false;
    const done = () => { prep.hidden = true; };
    $("prepSkip").onclick = done;
    buildScrubCache({
      screen: media.screen.src,
      camera: media.camera ? media.camera.src : null,
    }, (f) => {
      $("prepBar").style.width = Math.round(f * 100) + "%";
      if (f >= 1) setTimeout(done, 150);
    });
    setTimeout(done, 45000); // never trap the user
    $("loading").hidden = true;
    requestSeek(0);
  } catch (e) {
    $("loading").textContent = "Could not open project: " + (e?.message || e);
    console.error(e);
  }
}
boot();
