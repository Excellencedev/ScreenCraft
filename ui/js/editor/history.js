// Undo/redo + debounced autosave. Every edit goes through `change()`.
import { $, S, P, dir, hooks } from "./state.js";
import { invoke } from "../tauri.js";

const snapshot = () => JSON.stringify({ config: P().config, scene: P().scene });
function restore(json) {
  const o = JSON.parse(json);
  P().config = o.config;
  P().scene = o.scene;
}

/** Edits are kept in memory until saved (⌘S); the title shows * while unsaved. */
export function scheduleSave() {
  S.dirty = true;
  hooks.updateTitle?.();
}

export async function saveNow() {
  await invoke("save_project", { dir, project: P() });
  S.dirty = false;
  hooks.updateTitle?.();
}

export function updateUndo() {
  $("btnUndo").disabled = !S.history.length;
  $("btnRedo").disabled = !S.future.length;
}

/**
 * Apply a mutation with undo support.
 * `key` coalesces rapid edits (slider drags) into one undo step.
 * opts: track (recompute camera path), panel (rebuild side panel), timeline (rebuild timeline DOM).
 */
export function change(key, mutate, { track = true, panel = false, timeline = false } = {}) {
  const now = performance.now();
  if (!(key && key === S.lastKey && now - S.lastT < 1200)) {
    S.history.push(snapshot());
    if (S.history.length > 100) S.history.shift();
    S.future = [];
  }
  S.lastKey = key;
  S.lastT = now;
  mutate();
  scheduleSave();
  updateUndo();
  if (track) hooks.scheduleTrack();
  else hooks.draw();
  if (timeline) hooks.renderTimeline();
  if (panel) hooks.renderPanel();
}

function after() {
  hooks.syncMusic?.();
  scheduleSave();
  updateUndo();
  hooks.scheduleTrack();
  hooks.renderTimeline();
  hooks.renderPanel();
  hooks.syncControls();
}
export function undo() {
  if (!S.history.length) return;
  S.future.push(snapshot());
  restore(S.history.pop());
  S.lastKey = null;
  after();
}
export function redo() {
  if (!S.future.length) return;
  S.history.push(snapshot());
  restore(S.future.pop());
  S.lastKey = null;
  after();
}
