// Shared recorder state + persisted preferences.
export const $ = (id) => document.getElementById(id);

export const BAR_W = 1040;
export const BAR_H = 76;

export const QUALITY = {
  native: { label: "Retina", sub: "Full native resolution · 60 fps · heaviest", fps: 60 },
  balanced: { label: "Balanced", sub: "Up to 2560 px · 30 fps · recommended, smooth to edit", fps: 30 },
  light: { label: "Light", sub: "1× resolution · 30 fps · smoothest on older Macs", fps: 30 },
};

const prefs = (() => {
  try { return JSON.parse(localStorage.getItem("sc-recorder") || "{}"); } catch { return {}; }
})();

export const state = {
  mode: prefs.mode || "display",
  displayId: null,
  windowId: null,
  windowTitle: "",
  area: null,
  camera: !!prefs.camera,
  cameraId: prefs.cameraId || null,
  cameraLabel: prefs.cameraLabel || "",
  mic: !!prefs.mic,
  micId: prefs.micId || null,
  micName: prefs.micName || "",
  sys: !!prefs.sys,
  quality: prefs.quality || "balanced",
  phase: "idle", // idle | countdown | recording | finishing
  sources: null,
};

export function savePrefs() {
  try {
    const { mode, camera, cameraId, cameraLabel, mic, micId, micName, sys, quality } = state;
    localStorage.setItem("sc-recorder", JSON.stringify({ mode, camera, cameraId, cameraLabel, mic, micId, micName, sys, quality }));
  } catch {}
}

let errTimer = null;
export function showError(msg) {
  const e = $("err");
  e.textContent = msg ? msg + "  (click to dismiss)" : "";
  e.style.display = msg ? "flex" : "none";
  e.onclick = () => (e.style.display = "none");
  clearTimeout(errTimer);
  if (msg) errTimer = setTimeout(() => (e.style.display = "none"), 9000);
}

/** Hooks registered by main.js so modules can trigger a re-render without importing each other. */
export const hooks = { render: () => {} };
