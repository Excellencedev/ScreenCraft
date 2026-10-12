// Dropdown shown in its own window above the recorder bar (see windows.rs).
import { invoke, listen, emit, currentWindow } from "./tauri.js";

const card = document.getElementById("card");

function render(items) {
  card.replaceChildren();
  for (const it of items || []) {
    if (it.header) {
      const h = document.createElement("div");
      h.className = "hd";
      h.textContent = it.header;
      card.appendChild(h);
      continue;
    }
    const b = document.createElement("button");
    const chk = document.createElement("span");
    chk.className = "chk";
    chk.textContent = it.checked ? "✓" : "";
    b.appendChild(chk);
    b.appendChild(document.createTextNode(it.label));
    if (it.sub) {
      const s = document.createElement("div");
      s.className = "small";
      s.textContent = it.sub;
      b.appendChild(s);
    }
    b.onclick = async () => {
      await emit("menu-pick", { id: it.id });
      invoke("close_menu_window");
    };
    card.appendChild(b);
  }
  // offsetWidth/Height force layout, so the measured size is already final
  invoke("menu_show", { width: Math.ceil(card.offsetWidth), height: Math.ceil(card.offsetHeight) });
}

await listen("menu-render", (e) => render(e.payload));
const pending = await invoke("menu_take_items");
if (pending) render(pending);

// closing: click elsewhere (focus lost) or Esc
const dismiss = () => {
  invoke("close_menu_window");
  emit("menu-closed", {});
};
currentWindow().onFocusChanged?.(({ payload: focused }) => { if (!focused) dismiss(); });
addEventListener("keydown", (e) => { if (e.key === "Escape") dismiss(); });
