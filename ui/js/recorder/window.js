// Toolbar window geometry. The bar always stays anchored at its bottom-centre,
// so wherever the user drags it, menus open upward and the HUD collapses in place.
import { currentWindow, invoke } from "../tauri.js";
import { BAR_W, BAR_H } from "./state.js";

export const win = currentWindow();

/** One native setFrame call (bottom-centre anchored) so the bar never jumps between steps. */
export async function resizeBar(w, h) {
  await invoke("resize_bar", { w, h });
}

export const showBar = () => resizeBar(BAR_W, BAR_H);

/** Drag the toolbar / HUD by any non-interactive spot. */
export function enableDragging() {
  document.addEventListener("mousedown", (e) => {
    if (e.button !== 0) return;
    if (e.target.closest("button, canvas, input, select, .menu, .err")) return;
    win.startDragging();
  });
}
