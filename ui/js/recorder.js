import { invoke, listen, currentWindow, LogicalSize, LogicalPosition } from "./tauri.js";

const $ = (id) => document.getElementById(id);
const win = currentWindow();
const BAR_W = 860, BAR_H = 76;

const prefs = (() => {
  try { return JSON.parse(localStorage.getItem("sc-recorder") || "{}"); } catch { return {}; }
})();
const state = {
  mode: prefs.mode || "display",
  displayId: null,
  windowId: null,
  windowTitle: "",
  area: null,
  camera: !!prefs.camera,
  cameraId: prefs.cameraId || null,
  cameraLabel: prefs.cameraLabel || "",
  mic: !!prefs.mic,
  sys: !!prefs.sys,
  phase: "idle", // idle | countdown | recording
  sources: null,
};
const save = () => {
  try {
    localStorage.setItem("sc-recorder", JSON.stringify({
      mode: state.mode, camera: state.camera, cameraId: state.cameraId, cameraLabel: state.cameraLabel, mic: state.mic, sys: state.sys,
    }));
  } catch {}
};

// ---------------------------------------------------------------- window geometry
let base = { x: 0, y: 0 };
async function initGeometry() {
  const sf = await win.scaleFactor();
  const p = (await win.outerPosition()).toLogical(sf);
  base = { x: p.x, y: p.y };
}
async function setBar(w, h, extraUp = 0) {
  const x = base.x + (BAR_W - w) / 2;
  const y = base.y + (BAR_H - h) - extraUp + (extraUp ? 0 : 0);
  await win.setSize(new LogicalSize(w, h + extraUp));
  await win.setPosition(new LogicalPosition(x, y));
}

// ---------------------------------------------------------------- UI
function showError(msg) {
  const e = $("err");
  e.textContent = msg;
  e.style.display = msg ? "block" : "none";
  if (msg) setTimeout(() => (e.style.display = "none"), 8000);
}

function render() {
  document.querySelectorAll(".tool").forEach((b) => b.classList.toggle("active", b.dataset.mode === state.mode));
  const set = (id, on, text) => {
    const el = $(id);
    el.classList.toggle("off", !on);
    el.classList.toggle("on", on);
    el.querySelector("span").textContent = text;
  };
  set("optCamera", state.camera, state.camera ? state.cameraLabel || "Camera" : "No camera");
  set("optMic", state.mic, state.mic ? "Microphone" : "No microphone");
  set("optSys", state.sys, state.sys ? "System audio" : "No system audio");
  const label = { display: "Record display", window: state.windowTitle ? "Record window" : "Choose window", area: state.area ? "Record area" : "Select area" }[state.mode];
  $("recLabel").textContent = label;
}

let menuOpen = false;
async function openMenu(items) {
  const m = $("menu");
  m.innerHTML = "";
  for (const it of items) {
    if (it.header) {
      const h = document.createElement("div");
      h.className = "hd";
      h.textContent = it.header;
      m.appendChild(h);
      continue;
    }
    const b = document.createElement("button");
    b.innerHTML = `${it.label}${it.sub ? `<div class="small">${it.sub}</div>` : ""}`;
    b.onclick = async () => { await closeMenu(); it.onPick(); };
    m.appendChild(b);
  }
  const h = Math.min(320, items.length * 40 + 20);
  m.style.left = "12px";
  m.style.bottom = BAR_H + 8 + "px";
  m.style.maxHeight = h + "px";
  m.classList.add("show");
  menuOpen = true;
  await setBar(BAR_W, BAR_H, h + 16);
}
async function closeMenu() {
  if (!menuOpen) return;
  $("menu").classList.remove("show");
  menuOpen = false;
  await setBar(BAR_W, BAR_H, 0);
}

// ---------------------------------------------------------------- sources
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
  if (menuOpen) await closeMenu();
  state.mode = mode;
  save();
  render();
  await refreshSources();
  if (mode === "window" && state.sources) {
    const items = [{ header: "Windows" }];
    for (const w of state.sources.windows) {
      items.push({
        label: `${w.app}${w.title ? " — " + w.title : ""}`,
        sub: `${Math.round(w.width)} × ${Math.round(w.height)}`,
        onPick: () => { state.windowId = w.id; state.windowTitle = w.title || w.app; render(); },
      });
    }
    if (items.length === 1) items.push({ label: "No windows found", onPick() {} });
    await openMenu(items);
  } else if (mode === "display" && state.sources && state.sources.displays.length > 1) {
    await openMenu([
      { header: "Displays" },
      ...state.sources.displays.map((d) => ({
        label: d.name, sub: `${d.width} × ${d.height}`, onPick: () => { state.displayId = d.id; render(); },
      })),
    ]);
  } else if (mode === "area") {
    await invoke("open_area_picker", { displayId: state.displayId });
  }
}

async function cameraMenu() {
  if (menuOpen) return closeMenu();
  try {
    const tmp = await navigator.mediaDevices.getUserMedia({ video: true });
    tmp.getTracks().forEach((t) => t.stop());
  } catch (e) {
    return showError("Camera access was denied: " + e);
  }
  const devs = (await navigator.mediaDevices.enumerateDevices()).filter((d) => d.kind === "videoinput");
  await openMenu([
    { header: "Camera" },
    { label: "No camera", onPick: () => { state.camera = false; save(); render(); } },
    ...devs.map((d) => ({
      label: d.label || "Camera",
      onPick: () => { state.camera = true; state.cameraId = d.deviceId; state.cameraLabel = d.label; save(); render(); },
    })),
  ]);
}

// ---------------------------------------------------------------- recording
let cameraRec = null;
let cameraStartedMs = null;
let timerHandle = null;
let writeChain = Promise.resolve();

async function startCamera() {
  const stream = await navigator.mediaDevices.getUserMedia({
    video: { deviceId: state.cameraId ? { exact: state.cameraId } : undefined, width: { ideal: 1280 }, height: { ideal: 720 }, frameRate: { ideal: 30 } },
    audio: false,
  });
  const candidates = [
    ["video/mp4;codecs=avc1", "mp4"],
    ["video/mp4", "mp4"],
    ["video/webm;codecs=vp9", "webm"],
    ["video/webm", "webm"],
  ];
  const pick = candidates.find(([m]) => window.MediaRecorder && MediaRecorder.isTypeSupported(m)) || ["", "webm"];
  const rec = new MediaRecorder(stream, pick[0] ? { mimeType: pick[0], videoBitsPerSecond: 6_000_000 } : { videoBitsPerSecond: 6_000_000 });
  return { stream, rec, ext: pick[1] };
}

async function beginRecording() {
  if (state.phase !== "idle") return;
  if (menuOpen) await closeMenu();
  if (state.mode === "window" && !state.windowId) return chooseMode("window");
  if (state.mode === "area" && !state.area) return chooseMode("area");

  let cam = null;
  if (state.camera) {
    try { cam = await startCamera(); } catch (e) { return showError("Camera error: " + e); }
  }

  // countdown inside the HUD
  state.phase = "countdown";
  document.body.classList.add("countdown");
  $("hudLeft").style.display = "none";
  $("btnStop").style.display = "none";
  $("count").style.display = "block";
  await setBar(220, 60);
  for (let i = 3; i > 0; i--) {
    $("count").textContent = String(i);
    await new Promise((r) => setTimeout(r, 1000));
    if (state.phase !== "countdown") { cam?.stream.getTracks().forEach((t) => t.stop()); return; }
  }
  $("count").style.display = "none";

  try {
    await invoke("start_recording", {
      args: {
        mode: state.mode,
        displayId: state.displayId,
        windowId: state.mode === "window" ? state.windowId : null,
        area: state.mode === "area" ? state.area : null,
        systemAudio: state.sys,
        microphone: state.mic,
        fps: 60,
      },
    });
  } catch (e) {
    cam?.stream.getTracks().forEach((t) => t.stop());
    showError(String(e));
    return resetUI();
  }

  if (cam) {
    await invoke("camera_begin", { ext: cam.ext });
    cam.rec.ondataavailable = (ev) => {
      if (!ev.data || !ev.data.size) return;
      writeChain = writeChain.then(async () => invoke("camera_chunk", await ev.data.arrayBuffer()));
    };
    cameraStartedMs = Date.now();
    cam.rec.start(500);
    cameraRec = cam;
  }

  state.phase = "recording";
  document.body.classList.remove("countdown");
  document.body.classList.add("hud");
  $("hudLeft").style.display = "flex";
  $("btnStop").style.display = "block";
  const t0 = performance.now();
  timerHandle = setInterval(() => {
    const s = Math.floor((performance.now() - t0) / 1000);
    $("timer").textContent = `${Math.floor(s / 60)}:${String(s % 60).padStart(2, "0")}`;
  }, 250);
}

async function finishRecording() {
  if (state.phase === "countdown") { state.phase = "idle"; return resetUI(); }
  if (state.phase !== "recording") return;
  state.phase = "finishing";
  clearInterval(timerHandle);
  $("timer").textContent = "Saving…";
  if (cameraRec) {
    await new Promise((res) => {
      cameraRec.rec.onstop = res;
      cameraRec.rec.stop();
    });
    cameraRec.stream.getTracks().forEach((t) => t.stop());
    await writeChain;
  }
  try {
    const dir = await invoke("stop_recording", { cameraStartedUnixMs: cameraRec ? cameraStartedMs : null });
    await invoke("open_editor", { dir });
  } catch (e) {
    showError(String(e));
  }
  cameraRec = null;
  resetUI();
}

async function resetUI() {
  state.phase = "idle";
  document.body.classList.remove("hud", "countdown");
  $("timer").textContent = "0:00";
  $("hudLeft").style.display = "flex";
  $("btnStop").style.display = "block";
  $("count").style.display = "none";
  await setBar(BAR_W, BAR_H);
  render();
}

// ---------------------------------------------------------------- wiring
document.querySelectorAll(".tool").forEach((b) => (b.onclick = () => chooseMode(b.dataset.mode)));
$("optCamera").onclick = cameraMenu;
$("optMic").onclick = () => { state.mic = !state.mic; save(); render(); };
$("optSys").onclick = () => { state.sys = !state.sys; save(); render(); };
$("btnRec").onclick = beginRecording;
$("btnStop").onclick = finishRecording;
$("btnClose").onclick = () => win.hide();
$("optFolder").onclick = async () => {
  const projects = await invoke("recent_projects");
  if (menuOpen) return closeMenu();
  const items = [{ header: "Recent projects" }];
  for (const p of projects.slice(0, 12)) {
    items.push({ label: p.name, sub: new Date(p.modified * 1000).toLocaleString(), onPick: () => invoke("open_editor", { dir: p.dir }) });
  }
  if (items.length === 1) items.push({ label: "No recordings yet", onPick() {} });
  await openMenu(items);
};

await listen("area-selected", (e) => { state.area = e.payload; state.mode = "area"; save(); render(); });
await listen("toggle-record", () => (state.phase === "recording" ? finishRecording() : beginRecording()));
await listen("stop-record", () => finishRecording());
addEventListener("keydown", (e) => { if (e.key === "Escape" && menuOpen) closeMenu(); });

await initGeometry();
render();
refreshSources();
