import { invoke, listen, currentWindow, LogicalSize, LogicalPosition } from "./tauri.js";

const $ = (id) => document.getElementById(id);
const win = currentWindow();
const BAR_W = 1000, BAR_H = 76;

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
  quality: prefs.quality || "balanced",
  phase: "idle", // idle | countdown | recording
  sources: null,
};
const save = () => {
  try {
    localStorage.setItem("sc-recorder", JSON.stringify({
      mode: state.mode, camera: state.camera, cameraId: state.cameraId, cameraLabel: state.cameraLabel, mic: state.mic, sys: state.sys, quality: state.quality,
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
let errTimer = null;
function showError(msg) {
  const e = $("err");
  e.textContent = msg ? msg + "  (click to dismiss)" : "";
  e.style.display = msg ? "flex" : "none";
  e.onclick = () => (e.style.display = "none");
  clearTimeout(errTimer);
  if (msg) errTimer = setTimeout(() => (e.style.display = "none"), 9000);
}

const QUALITY = {
  native: { label: "Retina", sub: "Full native resolution · 60 fps · heaviest", fps: 60 },
  balanced: { label: "Balanced", sub: "Up to 2560 px · 30 fps · recommended, smooth to edit", fps: 30 },
  light: { label: "Light", sub: "1× resolution · 30 fps · smoothest on older Macs", fps: 30 },
};

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
  $("qualLabel").textContent = { native: "Retina", balanced: "Balanced", light: "Light" }[state.quality];
  const label = { display: "Record display", window: state.windowTitle ? "Record window" : "Choose window", area: state.area ? "Record area" : "Select area" }[state.mode];
  $("recLabel").textContent = label;
  syncPreview();
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

const withTimeout = (p, ms, what) =>
  Promise.race([p, new Promise((_, rej) => setTimeout(() => rej(new Error(what + " timed out")), ms))]);

async function openCameraStream() {
  const base = { width: { ideal: 1280 }, height: { ideal: 720 }, frameRate: { ideal: 30 } };
  const attempts = [];
  if (state.cameraId) attempts.push({ ...base, deviceId: { ideal: state.cameraId } });
  attempts.push(base, true);
  let last;
  for (const video of attempts) {
    try {
      return await withTimeout(navigator.mediaDevices.getUserMedia({ video, audio: false }), 8000, "Camera");
    } catch (e) {
      last = e;
    }
  }
  throw last;
}

// ---- live camera preview inside the toolbar
let preview = null;
let drawLoop = 0;
function drawSquare(canvas, video) {
  const g = canvas.getContext("2d", { willReadFrequently: true });
  const vw = video.videoWidth, vh = video.videoHeight;
  if (!vw || !vh) return;
  const s = Math.min(vw, vh);
  g.drawImage(video, (vw - s) / 2, (vh - s) / 2, s, s, 0, 0, canvas.width, canvas.height);
}
function startDrawLoop() {
  const v = $("camSrc");
  const id = ++drawLoop;
  const tick = () => {
    if (id !== drawLoop) return;
    drawSquare($("camPreview"), v);
    if (state.phase === "recording") drawSquare($("hudCam"), v);
    setTimeout(tick, 50);
  };
  tick();
  // surface a frameless camera instead of showing a silent black circle
  setTimeout(() => {
    if (id !== drawLoop) return;
    if (!v.videoWidth) return showError("The camera is not delivering video. Check Camera access in Permissions.");
    try {
      const d = $("camPreview").getContext("2d", { willReadFrequently: true }).getImageData(0, 0, 80, 80).data;
      let sum = 0;
      for (let i = 0; i < d.length; i += 4) sum += d[i] + d[i + 1] + d[i + 2];
      if (sum < 200) showError(`Camera preview is black (${v.videoWidth}×${v.videoHeight}, paused=${v.paused}, ready=${v.readyState}). Is the camera covered or in use?`);
    } catch (e) {
      showError("Preview check failed: " + e);
    }
  }, 3000);
}
function stopPreview() {
  drawLoop++;
  if (preview) preview.getTracks().forEach((t) => t.stop());
  preview = null;
  $("camSrc").srcObject = null;
  $("optCamera").classList.remove("live");
}
async function syncPreview() {
  const want = state.camera && state.phase === "idle" && !document.hidden;
  if (!want) {
    if (state.phase === "idle") stopPreview();
    return;
  }
  if (preview) return;
  try {
    const s = await openCameraStream();
    if (!(state.camera && state.phase === "idle" && !document.hidden)) {
      s.getTracks().forEach((t) => t.stop());
      return;
    }
    preview = s;
    const v = $("camSrc");
    v.srcObject = s;
    await v.play().catch(() => {});
    startDrawLoop();
    $("optCamera").classList.add("live");
  } catch (e) {
    stopPreview();
    showError("Camera preview unavailable: " + (e?.message || e));
  }
}
document.addEventListener("visibilitychange", syncPreview);

async function startCamera() {
  if (!window.MediaRecorder) throw new Error("this web view cannot record camera video");
  const stream = preview || (await openCameraStream());
  const candidates = [
    ["video/mp4;codecs=avc1", "mp4"],
    ["video/mp4", "mp4"],
    ["video/webm;codecs=vp9", "webm"],
    ["video/webm", "webm"],
  ];
  const pick = candidates.find(([m]) => window.MediaRecorder && MediaRecorder.isTypeSupported(m)) || ["", "webm"];
  let rec;
  try {
    rec = new MediaRecorder(stream, pick[0] ? { mimeType: pick[0], videoBitsPerSecond: 6_000_000 } : {});
  } catch (e) {
    stream.getTracks().forEach((t) => t.stop());
    throw e;
  }
  return { stream, rec, ext: pick[1], shared: stream === preview };
}

async function ensurePermissions() {
  const p = await invoke("permission_status");
  if (!p.screen) {
    await invoke("show_permissions");
    return false;
  }
  if (state.camera && (p.camera === "denied" || p.camera === "restricted")) {
    showError("Camera access is off. Open Permissions from the menu-bar icon to fix it.");
    return false;
  }
  if (state.mic && (p.microphone === "denied" || p.microphone === "restricted")) {
    showError("Microphone access is off. Open Permissions from the menu-bar icon to fix it.");
    return false;
  }
  return true;
}

async function beginRecording() {
  if (state.phase !== "idle") return;
  if (!(await ensurePermissions())) return;
  if (menuOpen) await closeMenu();
  if (state.mode === "window" && !state.windowId) return chooseMode("window");
  if (state.mode === "area" && !state.area) return chooseMode("area");

  let cam = null;
  let cameraNote = "";
  if (state.camera) {
    try {
      cam = await startCamera();
    } catch (e) {
      cameraNote = `Camera unavailable (${e?.message || e}) — recording without it.`;
    }
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
    if (state.phase !== "countdown") { if (cam && !cam.shared) cam.stream.getTracks().forEach((t) => t.stop()); return; }
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
        fps: QUALITY[state.quality].fps,
        quality: state.quality,
      },
    });
  } catch (e) {
    if (cam && !cam.shared) cam.stream.getTracks().forEach((t) => t.stop());
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
    if (!cam.shared) {
      $("camSrc").srcObject = cam.stream;
      $("camSrc").play().catch(() => {});
      startDrawLoop();
    }
    $("hudCam").classList.add("on");
  }

  state.phase = "recording";
  if (cameraNote) showError(cameraNote);
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
    if (!cameraRec.shared) cameraRec.stream.getTracks().forEach((t) => t.stop());
    $("hudCam").classList.remove("on");
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
  syncPreview();
}

// ---------------------------------------------------------------- wiring
document.querySelectorAll(".tool").forEach((b) => (b.onclick = () => chooseMode(b.dataset.mode)));
$("optCamera").onclick = cameraMenu;
$("optQuality").onclick = async () => {
  if (menuOpen) return closeMenu();
  await openMenu([
    { header: "Capture quality" },
    ...Object.entries(QUALITY).map(([k, q]) => ({ label: (state.quality === k ? "✓ " : "") + q.label, sub: q.sub, onPick: () => { state.quality = k; save(); render(); } })),
  ]);
};
$("optMic").onclick = () => { state.mic = !state.mic; save(); render(); };
$("optSys").onclick = () => { state.sys = !state.sys; save(); render(); };
$("btnRec").onclick = beginRecording;
$("optCompress").onclick = () => invoke("show_compressor");
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
syncPreview();
if ((await invoke("permission_status")).screen) refreshSources();
else await invoke("show_permissions");
