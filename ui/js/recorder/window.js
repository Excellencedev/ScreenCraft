// Toolbar window geometry. The bar always stays anchored at its bottom-centre,
// so wherever the user drags it, menus open upward and the HUD collapses in place.
import { currentWindow, LogicalSize, LogicalPosition } from "../tauri.js";
import { BAR_W, BAR_H } from "./state.js";

export const win = currentWindow();

export async function resizeBar(w, h) {
  const sf = await win.scaleFactor();
  const pos = (await win.outerPosition()).toLogical(sf);
  const size = (await win.innerSize()).toLogical(sf);
  const cx = pos.x + size.width / 2;
  const bottom = pos.y + size.height;
  await win.setSize(new LogicalSize(w, h));
  await win.setPosition(new LogicalPosition(cx - w / 2, bottom - h));
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
