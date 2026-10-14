// Toolbar window geometry. The bar always stays anchored at its bottom-centre,
// so wherever the user drags it, menus open upward and the HUD collapses in place.
import { currentWindow, invoke } from "../tauri.js";
import { BAR_W, BAR_H, state } from "./state.js";

export const win = currentWindow();

/** One native setFrame call (bottom-centre anchored) so the bar never jumps between steps. */
export async function resizeBar(w, h) {
  await invoke("resize_bar", { w, h });
}

/** The record bar always lives bottom-centre on the main display. */
export const showBar = async () => {
  await invoke("dock_bar", { w: BAR_W, h: BAR_H });
};

/** Only the recording HUD (timer + stop) can be moved; the record bar is locked in place. */
export function enableDragging() {
  document.addEventListener("mousedown", (e) => {
    if (e.button !== 0) return;
    if (state.phase !== "recording") return;
    if (e.target.closest("button, canvas, input, select, .menu, .err")) return;
    win.startDragging();
  });
}
