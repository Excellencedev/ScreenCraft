// Start / stop recording (screen via Rust, camera via MediaRecorder).
import { invoke } from "../tauri.js";
import { $, state, QUALITY, showError, hooks } from "./state.js";
import { showBar, resizeBar } from "./window.js";
import { closeMenu, isMenuOpen } from "./menu.js";
import { getPreview, openCameraStream, startDrawLoop, syncPreview } from "./preview.js";
import { syncMeters } from "./meters.js";

let cameraRec = null;
let cameraStartedMs = null;
let timerHandle = null;
let writeChain = Promise.resolve();

async function startCamera() {
  if (!window.MediaRecorder) throw new Error("this web view cannot record camera video");
  const shared = getPreview();
  const stream = shared || (await openCameraStream());
  const candidates = [
    ["video/mp4;codecs=avc1", "mp4"],
    ["video/mp4", "mp4"],
    ["video/webm;codecs=vp9", "webm"],
    ["video/webm", "webm"],
  ];
  const pick = candidates.find(([m]) => MediaRecorder.isTypeSupported(m)) || ["", "webm"];
  let rec;
  try {
    rec = new MediaRecorder(stream, pick[0] ? { mimeType: pick[0], videoBitsPerSecond: 6_000_000 } : {});
  } catch (e) {
    if (!shared) stream.getTracks().forEach((t) => t.stop());
    throw e;
  }
  return { stream, rec, ext: pick[1], shared: !!shared };
}

async function ensurePermissions() {
  const p = await invoke("permission_status");
  if (!p.screen) {
    await invoke("show_permissions");
    return false;
  }
  if (state.camera && ["denied", "restricted"].includes(p.camera)) {
    showError("Camera access is off. Open Permissions from the menu-bar icon to fix it.");
    return false;
  }
  if (state.mic && ["denied", "restricted"].includes(p.microphone)) {
    showError("Microphone access is off. Open Permissions from the menu-bar icon to fix it.");
    return false;
  }
  return true;
}

export async function beginRecording() {
  if (state.phase !== "idle") return;
  if (!(await ensurePermissions())) return;
  if (isMenuOpen()) await closeMenu();
  if (state.mode === "window" && !state.windowId) return hooks.chooseMode("window");
  if (state.mode === "area" && !state.area) return hooks.chooseMode("area");

  let cam = null;
  let cameraNote = "";
  if (state.camera) {
    try {
      cam = await startCamera();
    } catch (e) {
      cameraNote = `Camera unavailable (${e?.message || e}) — recording without it.`;
    }
  }

  state.phase = "countdown";
  document.body.classList.add("countdown");
  $("hudLeft").style.display = "none";
  $("btnStop").style.display = "none";
  $("count").style.display = "block";
  await resizeBar(220, 60);
  for (let i = 3; i > 0; i--) {
    $("count").textContent = String(i);
    await new Promise((r) => setTimeout(r, 1000));
    if (state.phase !== "countdown") {
      if (cam && !cam.shared) cam.stream.getTracks().forEach((t) => t.stop());
      return;
    }
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
        microphoneId: state.mic ? state.micId : null,
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
  } else {
    $("hudCam").classList.remove("on");
  }

  state.phase = "recording";
  if (cameraNote) showError(cameraNote);
  document.body.classList.remove("countdown");
  document.body.classList.add("hud");
  $("hudLeft").style.display = "flex";
  $("btnStop").style.display = "block";
  await resizeBar(cam ? 330 : 290, 60);
  const t0 = performance.now();
  timerHandle = setInterval(() => {
    const s = Math.floor((performance.now() - t0) / 1000);
    $("timer").textContent = `${Math.floor(s / 60)}:${String(s % 60).padStart(2, "0")}`;
  }, 250);
}

export async function finishRecording() {
  if (state.phase === "countdown") {
    state.phase = "idle";
    return resetUI();
  }
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
  await showBar();
  hooks.render();
  syncPreview();
  syncMeters();
}
