// Background music: pick a file, copy it into the project, load it for playback.
import { invoke, dialog, convertFileSrc } from "../tauri.js";
import { S, P, dir, media } from "./state.js";
import { change } from "./history.js";

export function loadMusicElement() {
  const m = P().scene.music;
  media.music = null;
  if (!m) return Promise.resolve();
  return new Promise((res) => {
    const a = new Audio();
    a.crossOrigin = "anonymous";
    a.preload = "auto";
    a.src = convertFileSrc(`${dir}/${m.file}`);
    a.onloadeddata = () => { media.music = a; res(); };
    a.onerror = () => res();
  });
}

export async function addMusic() {
  const picked = await dialog.open({
    multiple: false,
    filters: [{ name: "Audio", extensions: ["mp3", "m4a", "aac", "wav", "aif", "aiff", "caf", "flac", "ogg"] }],
  });
  const path = Array.isArray(picked) ? picked[0] : picked;
  if (!path) return;
  const base = path.split("/").pop();
  const ext = (base.split(".").pop() || "mp3").toLowerCase();
  const file = `music-${Date.now()}.${ext}`;
  await invoke("copy_file", { src: path, dst: `${dir}/${file}` });
  // read the length from the file itself
  const durationMs = await new Promise((res) => {
    const a = new Audio(convertFileSrc(`${dir}/${file}`));
    a.preload = "metadata";
    a.onloadedmetadata = () => res(isFinite(a.duration) ? a.duration * 1000 : 0);
    a.onerror = () => res(0);
  });
  if (!durationMs) {
    alert("That audio file couldn't be read.");
    return;
  }
  change(null, () => {
    P().scene.music = { file, name: base.replace(/\.[^.]+$/, ""), offsetMs: 0, durationMs };
    S.selected = { type: "music" };
    S.tab = "audio";
  }, { panel: true, timeline: true, track: false });
  await loadMusicElement();
}

export function removeMusic() {
  change(null, () => {
    P().scene.music = null;
    if (S.selected?.type === "music") S.selected = null;
  }, { panel: true, timeline: true, track: false });
  media.music?.pause();
  media.music = null;
}
