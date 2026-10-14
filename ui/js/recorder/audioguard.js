// Keep Bluetooth headphones out of call-quality mode.
//
// Opening a Bluetooth headset's microphone makes macOS switch it from stereo (A2DP) to the
// hands-free profile (16 kHz mono): music turns tinny and so does anything recorded. OBS and
// Audacity have the same problem and their answer is "use a different microphone". So: prefer the
// Mac's own microphone, and while the app has a microphone open, pin the system default input to
// it, then put the default back.
import { invoke, IS_WINDOWS } from "../tauri.js";
import { state, savePrefs, hooks } from "./state.js";

export let route = null;
let lastSig = "";

export async function refreshRoute() {
  if (IS_WINDOWS) return null;
  try { route = await invoke("audio_route"); } catch { route = null; }
  return route;
}

/** Is `uid` an input that would put headphones into call mode? */
export const isBluetoothInput = (uid) => !!route?.inputs?.find((i) => i.uid === uid && i.transport === "bluetooth");

/**
 * Call before anything opens the microphone, and whenever the choice changes. Cheap when nothing
 * relevant changed.
 */
export async function guardAudio({ force = false } = {}) {
  if (IS_WINDOWS) return;
  const sig = [state.mic, state.micId, state.micBtOk].join("|");
  if (!force && sig === lastSig && route) return;
  lastSig = sig;
  const r = await refreshRoute();
  if (!r) return;
  const chosen = r.inputs.find((i) => i.uid === state.micId);
  const builtin = r.inputs.find((i) => i.uid === r.builtinInputUid);
  // a Bluetooth input saved from an earlier session is replaced by the Mac's own microphone,
  // unless the person picked it on purpose
  if (state.mic && !state.micBtOk && chosen?.transport === "bluetooth" && builtin) {
    Object.assign(state, { micId: builtin.uid, micName: builtin.name });
    lastSig = [state.mic, state.micId, state.micBtOk].join("|");
    savePrefs();
    hooks.render?.();
  }
  const usingBluetoothMic = state.micBtOk && (state.micId ? isBluetoothInput(state.micId) : r.defaultInputBluetooth);
  if (state.mic && !usingBluetoothMic) {
    // no device chosen yet means "the default", which may be the headset: move it
    await invoke("audio_pin_builtin_input").catch(() => {});
  } else {
    await invoke("audio_restore_input").catch(() => {});
  }
  await refreshRoute();
}

document.addEventListener("visibilitychange", () => { if (!document.hidden) guardAudio({ force: true }); });
