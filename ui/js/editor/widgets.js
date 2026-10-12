// Small DOM helpers for the side-panel controls.
import { change } from "./history.js";

export const el = (html) => {
  const t = document.createElement("template");
  t.innerHTML = html.trim();
  return t.content.firstChild;
};
export const h3 = (t) => el(`<h3>${t}</h3>`);

export function slider(label, min, max, step, get, set, { fmt = (v) => v, track = false, timeline = false, key } = {}) {
  const w = el(`<div class="field"><label><span>${label}</span><span class="val"></span></label><input type="range" min="${min}" max="${max}" step="${step}"></div>`);
  const inp = w.querySelector("input"), val = w.querySelector(".val");
  inp.value = get();
  val.textContent = fmt(get());
  inp.oninput = () => {
    const v = parseFloat(inp.value);
    val.textContent = fmt(v);
    change(key || label, () => set(v), { track, timeline });
  };
  return w;
}

export function toggle(label, hint, get, set, { track = false } = {}) {
  const w = el(`<div class="row-t"><div>${label}${hint ? `<small>${hint}</small>` : ""}</div><div class="toggle"></div></div>`);
  const t = w.querySelector(".toggle");
  t.classList.toggle("on", !!get());
  t.onclick = () => change(null, () => set(!get()), { track, panel: true });
  return w;
}

export function segmented(label, options, get, set, { track = false, timeline = false } = {}) {
  const w = el(`<div class="field"><label><span>${label}</span></label><div class="seg"></div></div>`);
  const seg = w.querySelector(".seg");
  for (const [v, text] of options) {
    const b = el(`<button class="${get() === v ? "on" : ""}">${text}</button>`);
    b.onclick = () => change(null, () => set(v), { track, panel: true, timeline });
    seg.appendChild(b);
  }
  return w;
}

export function colorField(label, get, set) {
  const w = el(`<div class="row-t"><div>${label}</div><input type="color"></div>`);
  const i = w.querySelector("input");
  i.value = get();
  i.oninput = () => change(label, () => set(i.value), { track: false });
  return w;
}
