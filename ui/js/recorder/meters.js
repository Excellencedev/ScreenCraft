// Live input level meters: microphone (WebAudio) and system audio (Rust).
import { invoke, listen } from "../tauri.js";
import { $, state, setLoading } from "./state.js";
import { guardAudio } from "./audioguard.js";

let micStream = null, micCtx = null, analyser = null, micBuf = null, micLoop = 0;
let micLevel = 0, sysLevel = 0;
let micOpening = false, sysOpening = false;

/** Map a linear peak (0..1) to a pleasant 0..1 bar position (−60..0 dBFS). */
const toBar = (v) => Math.max(0, Math.min(1, (20 * Math.log10(Math.max(v, 1e-5)) + 60) / 60));

function paint() {
  for (const [id, lvl] of [["micMeter", micLevel], ["sysMeter", sysLevel], ["hudMic", micLevel], ["hudSys", sysLevel]]) {
    const el = $(id);
    if (el) el.firstElementChild.style.width = Math.round(lvl * 100) + "%";
  }
}

function pollMic(id) {
  if (id !== micLoop || !analyser) return;
  analyser.getFloatTimeDomainData(micBuf);
  let peak = 0;
  for (let i = 0; i < micBuf.length; i++) peak = Math.max(peak, Math.abs(micBuf[i]));
  const target = toBar(peak);
  micLevel = target > micLevel ? target : micLevel * 0.82 + target * 0.18;
  paint();
  setTimeout(() => pollMic(id), 40);
}

export async function startMicMeter() {
  if (micStream || micOpening || !state.mic || document.hidden) return;
  micOpening = true;
  setLoading("mic", true);
  try {
    await guardAudio(); // never open a Bluetooth headset's microphone by accident
    // match the chosen device by name (WebKit device ids differ from AVFoundation ids)
    let deviceId;
    try {
      const devs = (await navigator.mediaDevices.enumerateDevices()).filter((d) => d.kind === "audioinput");
      deviceId = devs.find((d) => state.micName && d.label && d.label.includes(state.micName.slice(0, 10)))?.deviceId;
    } catch {}
    micStream = await navigator.mediaDevices.getUserMedia({
      audio: deviceId ? { deviceId: { ideal: deviceId } } : true,
      video: false,
    });
    micCtx = new AudioContext();
    const src = micCtx.createMediaStreamSource(micStream);
    analyser = micCtx.createAnalyser();
    analyser.fftSize = 1024;
    micBuf = new Float32Array(analyser.fftSize);
    src.connect(analyser);
    pollMic(++micLoop);
  } catch {
    stopMicMeter();
  } finally {
    micOpening = false;
    setLoading("mic", false);
    // the toggle may have changed while the stream was opening
    if (micStream && !state.mic) stopMicMeter();
    document.querySelector("#optMic")?.classList.toggle("listening", !!micStream);
  }
}

export function stopMicMeter() {
  micLoop++;
  micStream?.getTracks().forEach((t) => t.stop());
  micStream = null;
  micCtx?.close?.().catch?.(() => {});
  micCtx = null;
  analyser = null;
  micLevel = 0;
  paint();
}

let sysRunning = false;
export async function startSysMeter() {
  if (sysRunning || sysOpening || !state.sys) return;
  sysOpening = true;
  setLoading("sys", true);
  try {
    await invoke("meter_start");
    sysRunning = true;
  } catch {
  } finally {
    sysOpening = false;
    setLoading("sys", false);
  }
}
export async function stopSysMeter() {
  if (!sysRunning) return;
  sysRunning = false;
  sysLevel = 0;
  paint();
  try { await invoke("meter_stop"); } catch {}
}

/** Re-sync meters with the current toggles / visibility. */
export function syncMeters() {
  const visible = !document.hidden;
  guardAudio(); // pins/restores the default input when the microphone choice changed
  if (state.mic && visible) startMicMeter(); else stopMicMeter();
  if (state.sys && visible) startSysMeter(); else stopSysMeter();
  document.querySelector("#optMic")?.classList.toggle("listening", !!micStream);
}

listen("system-level", (e) => {
  const target = toBar(e.payload || 0);
  sysLevel = target > sysLevel ? target : sysLevel * 0.82 + target * 0.18;
  paint();
});
document.addEventListener("visibilitychange", syncMeters);
