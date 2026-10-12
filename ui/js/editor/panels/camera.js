import { P } from "../state.js";
import { change } from "../history.js";
import { h3, el, slider, toggle, segmented } from "../widgets.js";

export function panelCamera(p) {
  const c = P().config.camera;
  p.append(h3("Camera"));
  if (!P().media.camera) {
    p.append(el('<div class="muted">This recording has no camera track. Enable a camera in the recorder toolbar before recording.</div>'));
    return;
  }
  p.append(toggle("Hide camera", null, () => c.hide, (v) => (c.hide = v)));
  p.append(toggle("Mirror", null, () => c.mirror, (v) => (c.mirror = v)));
  p.append(slider("Size", 0.1, 0.5, 0.01, () => c.sizeRatio, (v) => (c.sizeRatio = v), { fmt: (v) => Math.round(v * 100) + "%" }));
  p.append(slider("Roundness", 0, 0.5, 0.01, () => c.cornerRadiusRatio, (v) => (c.cornerRadiusRatio = v), { fmt: (v) => Math.round(v * 200) + "%" }));
  p.append(slider("Padding", 0, 2, 0.05, () => c.paddingRatio, (v) => (c.paddingRatio = v)));
  p.append(slider("Scale while zoomed", 0.4, 1, 0.01, () => c.zoomedScale, (v) => (c.zoomedScale = v), { fmt: (v) => Math.round(v * 100) + "%" }));
  p.append(segmented("Shape", [[1, "Square"], [4 / 3, "4:3"], [16 / 9, "16:9"], [3 / 4, "3:4"]], () => c.aspectRatio, (v) => (c.aspectRatio = v)));
  const pos = el('<div class="field"><label><span>Position</span></label><div class="seg"></div></div>');
  for (const [name, x, y] of [["↖", 0, 0], ["↗", 1, 0], ["↙", 0, 1], ["↘", 1, 1]]) {
    const b = el(`<button class="${c.position.x === x && c.position.y === y ? "on" : ""}">${name}</button>`);
    b.onclick = () => change(null, () => (c.position = { x, y }), { panel: true, track: false });
    pos.querySelector(".seg").appendChild(b);
  }
  p.append(pos);
  p.append(toggle("Shadow", null, () => c.shadow, (v) => (c.shadow = v)));
  p.append(toggle("Border", null, () => c.border, (v) => (c.border = v)));
}
