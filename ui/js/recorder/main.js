import { invoke, listen } from "../tauri.js";
import { $, state, QUALITY, savePrefs, showError, hooks } from "./state.js";
import { win, showBar, enableDragging } from "./window.js";
import { openMenu, closeMenu, isMenuOpen } from "./menu.js";
import { syncPreview } from "./preview.js";
import { syncMeters } from "./meters.js";
import { cameraMenu, micMenu, systemMenu } from "./devices.js";
import { beginRecording, finishRecording } from "./capture.js";

function render() {
  document.querySelectorAll(".tool").forEach((b) => b.classList.toggle("active", b.dataset.mode === state.mode));
  const set = (id, on, text) => {
    const el = $(id);
    el.classList.toggle("off", !on);
    el.classList.toggle("on", on);
    el.querySelector("span.t").textContent = text;
  };
  set("optCamera", state.camera, state.camera ? state.cameraLabel || "Camera" : "No camera");
  set("optMic", state.mic, state.mic ? state.micName || "Microphone" : "No microphone");
  set("optSys", state.sys, state.sys ? "System audio" : "No system audio");
  $("qualLabel").textContent = QUALITY[state.quality].label;
  $("recLabel").textContent = {
    display: "Record display",
    window: state.windowTitle ? "Record window" : "Choose window",
    area: state.area ? "Record area" : "Select area",
  }[state.mode];
  syncPreview();
  syncMeters();
}
hooks.render = render;

async function refreshSources() {
  try {
    state.sources = await invoke("list_sources");
    if (state.displayId == null && state.sources.displays.length) {
      state.displayId = (state.sources.displays.find((d) => d.isMain) || state.sources.displays[0]).id;
    }
  } catch (e) {
    showError(String(e));
  }
}

async function chooseMode(mode) {
  if (isMenuOpen()) await closeMenu();
  state.mode = mode;
  savePrefs();
  render();
  await refreshSources();
  const anchor = document.querySelector(`.tool[data-mode="${mode}"]`);
  if (mode === "window" && state.sources) {
    const items = [{ header: "Windows" }];
    for (const w of state.sources.windows) {
      items.push({
        label: `${w.app}${w.title ? " — " + w.title : ""}`,
        sub: `${Math.round(w.width)} × ${Math.round(w.height)}`,
        checked: state.windowId === w.id,
        onPick: () => { state.windowId = w.id; state.windowTitle = w.title || w.app; render(); },
      });
    }
    if (items.length === 1) items.push({ label: "No windows found", onPick() {} });
    await openMenu(items, anchor);
  } else if (mode === "display" && state.sources && state.sources.displays.length > 1) {
    await openMenu(
      [
        { header: "Displays" },
        ...state.sources.displays.map((d) => ({
          label: d.name, sub: `${d.width} × ${d.height}`, checked: state.displayId === d.id,
          onPick: () => { state.displayId = d.id; render(); },
        })),
      ],
      anchor,
    );
  } else if (mode === "area") {
    await invoke("open_area_picker", { displayId: state.displayId });
  }
}
hooks.chooseMode = chooseMode;

async function toggleMenu(fn) {
  if (isMenuOpen()) return closeMenu();
  await fn();
}

async function folderMenu() {
  const [projects, settings] = await Promise.all([invoke("recent_projects"), invoke("get_settings")]);
  const items = [{ header: "Recent recordings" }];
  for (const p of projects.slice(0, 8)) {
    items.push({ label: p.name, sub: new Date(p.modified * 1000).toLocaleString(), onPick: () => invoke("open_editor", { dir: p.dir }) });
  }
  if (items.length === 1) items.push({ label: "No recordings yet", onPick() {} });
  items.push(
    { header: "Raw recordings folder" },
    { label: "Open recordings folder", sub: settings.projectsDir, onPick: () => invoke("reveal_in_finder", { path: settings.projectsDir }) },
    {
      label: "Choose recordings folder…",
      onPick: async () => {
        const dir = await window.__TAURI__.dialog.open({ directory: true, defaultPath: settings.projectsDir });
        if (dir) await invoke("set_projects_dir", { path: Array.isArray(dir) ? dir[0] : dir });
      },
    },
  );
  await openMenu(items, $("optFolder"));
}

async function qualityMenu() {
  await openMenu(
    [
      { header: "Capture quality" },
      ...Object.entries(QUALITY).map(([k, q]) => ({
        label: q.label, sub: q.sub, checked: state.quality === k,
        onPick: () => { state.quality = k; savePrefs(); render(); },
      })),
    ],
    $("optQuality"),
  );
}

document.querySelectorAll(".tool").forEach((b) => (b.onclick = () => chooseMode(b.dataset.mode)));
$("optCamera").onclick = () => toggleMenu(cameraMenu);
$("optMic").onclick = () => toggleMenu(micMenu);
$("optSys").onclick = () => toggleMenu(systemMenu);
$("optQuality").onclick = () => toggleMenu(qualityMenu);
$("optFolder").onclick = () => toggleMenu(folderMenu);
$("btnRec").onclick = beginRecording;
$("btnStop").onclick = finishRecording;
$("btnClose").onclick = () => win.hide();

await listen("area-selected", (e) => { state.area = e.payload; state.mode = "area"; savePrefs(); render(); });
await listen("toggle-record", () => (state.phase === "recording" ? finishRecording() : beginRecording()));
await listen("stop-record", () => finishRecording());
addEventListener("keydown", (e) => { if (e.key === "Escape" && isMenuOpen()) closeMenu(); });

enableDragging();
render();
if ((await invoke("permission_status")).screen) refreshSources();
else await invoke("show_permissions");
