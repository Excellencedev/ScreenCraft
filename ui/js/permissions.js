import { invoke, currentWindow } from "./tauri.js";

const $ = (id) => document.getElementById(id);
let startedGranted = null;
let screenNow = false;

function setRow(rowId, stateId, btnId, status, labels) {
  const row = $(rowId);
  const ok = status === "granted";
  row.classList.toggle("ok", ok);
  row.classList.toggle("bad", !ok && status !== "notDetermined");
  $(stateId).textContent = { granted: "Allowed", denied: "Denied", restricted: "Restricted", notDetermined: "Not set" }[status] || status;
  const b = $(btnId);
  b.hidden = ok;
  b.textContent = status === "denied" || status === "restricted" ? "Open Settings" : labels;
}

async function refresh() {
  const s = await invoke("permission_status");
  if (startedGranted === null) startedGranted = s.screen;
  screenNow = s.screen;
  setRow("rowScreen", "sScreen", "bScreen", s.screen ? "granted" : "notDetermined", "Grant");
  setRow("rowCamera", "sCamera", "bCamera", s.camera, "Allow");
  setRow("rowMic", "sMic", "bMic", s.microphone, "Allow");
  $("bContinue").disabled = !s.screen || !startedGranted;
  // Screen Recording only takes effect after a restart when it was granted mid-session.
  const needsRestart = s.screen && !startedGranted;
  $("note").classList.toggle("show", needsRestart);
  $("bRestart").hidden = !needsRestart;
  return s;
}

$("bScreen").onclick = async () => {
  const granted = await invoke("request_screen_access");
  if (!granted) await invoke("open_privacy_pane", { kind: "screen" });
  refresh();
};
async function media(kind, constraints, id) {
  const st = (await invoke("permission_status"))[kind === "camera" ? "camera" : "microphone"];
  if (st === "denied" || st === "restricted") return invoke("open_privacy_pane", { kind });
  try {
    const s = await navigator.mediaDevices.getUserMedia(constraints);
    s.getTracks().forEach((t) => t.stop());
  } catch {
    await invoke("open_privacy_pane", { kind });
  }
  refresh();
}
$("bCamera").onclick = () => media("camera", { video: true });
$("bMic").onclick = () => media("microphone", { audio: true });
$("bRecheck").onclick = refresh;
$("bRestart").onclick = () => invoke("restart_app");
$("bContinue").onclick = async () => {
  await invoke("show_recorder");
  currentWindow().close();
};

await refresh();
setInterval(refresh, 1500);
addEventListener("focus", refresh);
