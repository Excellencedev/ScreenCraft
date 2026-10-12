// Popover menu used by Presets and Raw files.
import { $ } from "./state.js";

export function showMenu(anchor, items) {
  const m = $("menu");
  m.replaceChildren();
  for (const it of items) {
    if (it === "-") {
      m.appendChild(document.createElement("hr"));
      continue;
    }
    if (it.header) {
      const h = document.createElement("div");
      h.className = "mh";
      h.textContent = it.header;
      m.appendChild(h);
      continue;
    }
    const b = document.createElement("button");
    b.textContent = it.label;
    b.onclick = () => {
      m.hidden = true;
      it.run();
    };
    m.appendChild(b);
  }
  const r = anchor.getBoundingClientRect();
  m.hidden = false;
  m.style.top = r.bottom + 6 + "px";
  m.style.left = Math.max(8, r.right - m.offsetWidth) + "px";
  const close = (e) => {
    if (!m.contains(e.target)) {
      m.hidden = true;
      removeEventListener("pointerdown", close, true);
    }
  };
  setTimeout(() => addEventListener("pointerdown", close, true), 0);
}
