import { P } from "../state.js";
import { change } from "../history.js";
import { h3, el, slider } from "../widgets.js";

export function panelCrop(p) {
  const c = P().config.crop;
  p.append(h3("Crop"));
  const v = { l: c.x * 100, t: c.y * 100, r: (1 - c.x - c.width) * 100, b: (1 - c.y - c.height) * 100 };
  const apply = () => {
    const k = (n) => Math.min(80, Math.max(0, n)) / 100;
    const l = k(v.l), t = k(v.t), r = k(v.r), b = k(v.b);
    c.x = l; c.y = t; c.width = Math.max(0.1, 1 - l - r); c.height = Math.max(0.1, 1 - t - b);
  };
  for (const [key, name] of [["l", "Left"], ["t", "Top"], ["r", "Right"], ["b", "Bottom"]]) {
    p.append(slider(name, 0, 60, 0.5, () => v[key], (n) => { v[key] = n; apply(); }, { track: true, key: "crop-" + key, fmt: (n) => n.toFixed(1) + "%" }));
  }
  const reset = el('<button class="btn-line">Reset crop</button>');
  reset.onclick = () => change(null, () => Object.assign(c, { x: 0, y: 0, width: 1, height: 1 }), { panel: true });
  p.append(reset);
}
