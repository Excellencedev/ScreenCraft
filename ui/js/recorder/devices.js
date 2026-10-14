// Camera / microphone / system-audio dropdowns.
import { invoke, IS_WINDOWS } from "../tauri.js";
import { $, state, savePrefs, hooks, showError } from "./state.js";
import { openMenu } from "./menu.js";
import { guardAudio, refreshRoute } from "./audioguard.js";

export async function cameraMenu() {
  try {
    const tmp = await navigator.mediaDevices.getUserMedia({ video: true });
    tmp.getTracks().forEach((t) => t.stop());
  } catch (e) {
    return showError("Camera access was denied: " + e);
  }
  const devs = (await navigator.mediaDevices.enumerateDevices()).filter((d) => d.kind === "videoinput");
  await openMenu(
    [
      { header: "Camera" },
      { label: "No camera", checked: !state.camera, onPick: () => { state.camera = false; savePrefs(); hooks.render(); } },
      ...devs.map((d) => ({
        label: d.label || "Camera",
        checked: state.camera && state.cameraId === d.deviceId,
        onPick: () => {
          Object.assign(state, { camera: true, cameraId: d.deviceId, cameraLabel: d.label });
          savePrefs();
          hooks.render();
        },
      })),
    ],
    $("optCamera"),
  );
}

/** Microphones as the web view sees them (used where there is no native list). */
async function webAudioInputs() {
  try {
    const tmp = await navigator.mediaDevices.getUserMedia({ audio: true });
    tmp.getTracks().forEach((t) => t.stop());
  } catch {
    return [];
  }
  return (await navigator.mediaDevices.enumerateDevices())
    .filter((d) => d.kind === "audioinput" && !["default", "communications"].includes(d.deviceId))
    .map((d) => ({ id: d.deviceId, name: d.label || "Microphone" }));
}

export async function micMenu() {
  let devs = [];
  const r = await refreshRoute();
  if (r?.inputs?.length) {
    // built-in first: it is the one that keeps Bluetooth headphones in stereo
    const rank = (d) => (d.transport === "builtin" ? 0 : d.transport === "bluetooth" ? 2 : 1);
    devs = r.inputs
      .slice()
      .sort((a, b) => rank(a) - rank(b))
      .map((d) => ({
        id: d.uid,
        name: d.name,
        sub:
          d.transport === "bluetooth"
            ? "Bluetooth: headphones drop to call quality while this is on"
            : d.transport === "builtin" && r.defaultOutputBluetooth
              ? "Recommended: keeps your Bluetooth headphones in stereo"
              : "",
        bluetooth: d.transport === "bluetooth",
      }));
  } else {
    try { devs = await invoke("list_audio_inputs"); } catch {}
    if (!devs.length) devs = await webAudioInputs();
  }
  await openMenu(
    [
      { header: "Microphone" },
      { label: "No microphone", checked: !state.mic, onPick: () => { state.mic = false; savePrefs(); hooks.render(); guardAudio(); } },
      ...devs.map((d) => ({
        label: d.name,
        sub: d.sub,
        checked: state.mic && state.micId === d.id,
        onPick: () => {
          Object.assign(state, { mic: true, micId: d.id, micName: d.name, micBtOk: !!d.bluetooth });
          savePrefs();
          hooks.render();
          guardAudio({ force: true });
        },
      })),
    ],
    $("optMic"),
  );
}

export async function systemMenu() {
  await openMenu(
    [
      { header: "Device audio" },
      { label: "Off", checked: !state.sys, onPick: () => { state.sys = false; savePrefs(); hooks.render(); } },
      {
        label: "System audio",
        sub: `Everything your ${IS_WINDOWS ? "PC" : "Mac"} is playing (all apps)`,
        checked: state.sys,
        onPick: () => { state.sys = true; savePrefs(); hooks.render(); },
      },
    ],
    $("optSys"),
  );
}
