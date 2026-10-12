import { $, BAR_W, BAR_H } from "./state.js";
import { resizeBar } from "./window.js";

let open = false;
export const isMenuOpen = () => open;

/**
 * Show a dropdown above the bar, aligned with `anchor`.
 * items: {header} | {label, sub?, checked?, onPick}
 */
export async function openMenu(items, anchor) {
  const m = $("menu");
  m.innerHTML = "";
  for (const it of items) {
    if (it.header) {
      const h = document.createElement("div");
      h.className = "hd";
      h.textContent = it.header;
      m.appendChild(h);
      continue;
    }
    const b = document.createElement("button");
    b.innerHTML = `<span class="chk">${it.checked ? "✓" : ""}</span>${it.label}${it.sub ? `<div class="small">${it.sub}</div>` : ""}`;
    b.onclick = async () => {
      await closeMenu();
      it.onPick();
    };
    m.appendChild(b);
  }
  const rows = items.reduce((n, it) => n + (it.header ? 0.7 : it.sub ? 1.5 : 1), 0);
  const h = Math.min(500, Math.round(rows * 38) + 20);
  const left = anchor ? Math.max(8, Math.min(BAR_W - 270, anchor.getBoundingClientRect().left)) : 12;
  m.style.left = left + "px";
  m.style.bottom = BAR_H + 6 + "px";
  m.style.maxHeight = h + "px";
  m.classList.add("show");
  open = true;
  await resizeBar(BAR_W, BAR_H + h + 14);
}

export async function closeMenu() {
  if (!open) return;
  $("menu").classList.remove("show");
  open = false;
  await resizeBar(BAR_W, BAR_H);
}
