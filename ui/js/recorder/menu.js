import { $, BAR_W } from "./state.js";
import { invoke, listen } from "../tauri.js";

let open = false;
let handlers = new Map();
let hovers = new Map();
let onLeave = null;
let lastClosed = { at: 0, anchor: null };
let currentAnchor = null;
export const isMenuOpen = () => open;

await listen("menu-hover", (e) => hovers.get(e.payload?.id)?.());
await listen("menu-pick", (e) => {
  const fn = handlers.get(e.payload?.id);
  onLeave?.();
  open = false;
  lastClosed = { at: performance.now(), anchor: currentAnchor };
  if (fn) fn();
});
await listen("menu-closed", () => {
  onLeave?.();
  if (open) lastClosed = { at: performance.now(), anchor: currentAnchor };
  open = false;
});

/**
 * Show a dropdown above the bar, aligned with `anchor`, in its own window.
 * items: {header} | {label, sub?, checked?, onPick}
 */
export async function openMenu(items, anchor, opts = {}) {
  // the click that dismissed this very menu also lands on its button: don't reopen
  if (anchor && lastClosed.anchor === anchor && performance.now() - lastClosed.at < 350) return;
  handlers = new Map();
  hovers = new Map();
  onLeave = opts.onLeave || null;
  const payload = items.map((it, i) => {
    if (it.header) return { header: it.header };
    handlers.set(i, it.onPick);
    if (it.onHover) hovers.set(i, it.onHover);
    return { id: i, label: it.label, sub: it.sub || "", checked: !!it.checked };
  });
  const left = anchor ? Math.max(8, Math.min(BAR_W - 320, anchor.getBoundingClientRect().left)) : 12;
  currentAnchor = anchor;
  open = true;
  await invoke("open_menu_window", { items: payload, left });
}

export async function closeMenu() {
  if (!open) return;
  onLeave?.();
  open = false;
  await invoke("close_menu_window");
}
