import { S, P } from "../state.js";
import { addLayoutAtPlayhead, deleteSelected, LAYOUT_NAMES } from "../timeline.js";
import { change } from "../history.js";
import { h3, el, slider, toggle, segmented, colorField } from "../widgets.js";

export function panelCamera(p) {
  const c = P().config.camera;
  p.append(h3("Camera"));
  if (!P().media.camera) {
    p.append(el('<div class="muted">This recording has no camera track. Enable a camera in the recorder toolbar before recording.</div>'));
    return;
  }
  // layouts over time: a region on the Camera track changes how the camera is shown
  const lay = (P().scene.cameraLayouts || []).find((x) => S.selected?.type === "layout" && x.id === S.selected.id);
  p.append(h3("Layout over time"));
  if (lay) {
    p.append(segmented("Layout", Object.entries(LAYOUT_NAMES), () => lay.layout, (v) => (lay.layout = v), { track: true, timeline: true }));
    p.append(el('<div class="hint muted">Drag the purple block on the Camera track to move it, its ends to trim.</div>'));
    const del = el('<button class="btn-line danger">Remove this layout</button>');
    del.onclick = deleteSelected;
    p.append(del);
  } else {
    p.append(el('<div class="muted">Show the camera differently for part of the video. Adds a block at the playhead on the Camera track.</div>'));
    const grid = el('<div class="add-grid two"></div>');
    for (const k of ["side-right", "side-left", "camera", "large", "hidden"]) {
      const b = el(`<button class="add-tile"><span>${LAYOUT_NAMES[k]}</span></button>`);
      b.onclick = () => addLayoutAtPlayhead(k);
      grid.appendChild(b);
    }
    p.append(grid);
  }
  p.append(h3("Bubble"));
  p.append(toggle("Hide camera", null, () => c.hide, (v) => (c.hide = v)));
  p.append(toggle("Mirror", null, () => c.mirror, (v) => (c.mirror = v)));
  p.append(slider("Size", 0.1, 0.5, 0.01, () => c.sizeRatio, (v) => (c.sizeRatio = v), { fmt: (v) => Math.round(v * 100) + "%" }));
  p.append(slider("Roundness", 0, 0.5, 0.01, () => c.cornerRadiusRatio, (v) => (c.cornerRadiusRatio = v), { fmt: (v) => Math.round(v * 200) + "%" }));
  p.append(slider("Scale while zoomed", 0.4, 1, 0.01, () => c.zoomedScale, (v) => (c.zoomedScale = v), { fmt: (v) => Math.round(v * 100) + "%" }));
  p.append(segmented("Shape", [[1, "Square"], [4 / 3, "4:3"], [16 / 9, "16:9"], [3 / 4, "3:4"]], () => c.aspectRatio, (v) => (c.aspectRatio = v)));
  // nine anchor points; "flush" additionally drops the padding so the camera touches the edge
  const pos = el('<div class="field"><label><span>Position</span></label><div class="pos-grid"></div></div>');
  const grid = pos.querySelector(".pos-grid");
  const spots = [[0, 0, "↖"], [0.5, 0, "↑"], [1, 0, "↗"], [0, 0.5, "←"], [0.5, 0.5, "•"], [1, 0.5, "→"], [0, 1, "↙"], [0.5, 1, "↓"], [1, 1, "↘"]];
  for (const [x, y, name] of spots) {
    const on = Math.abs(c.position.x - x) < 0.01 && Math.abs(c.position.y - y) < 0.01;
    const b = el(`<button class="${on ? "on" : ""}">${name}</button>`);
    b.onclick = () => change(null, () => (c.position = { x, y }), { panel: true, track: false });
    grid.appendChild(b);
  }
  p.append(pos);
  p.append(toggle("Flush to the frame edge", "Ignore the padding and sit right on the edge of the video.", () => !!c.edgeFlush, (v) => (c.edgeFlush = v), { track: false }));
  if (!c.edgeFlush) p.append(slider("Padding", 0, 2, 0.05, () => c.paddingRatio, (v) => (c.paddingRatio = v), { track: false }));
  p.append(toggle("Shadow", null, () => c.shadow, (v) => (c.shadow = v)));
  p.append(toggle("Border", null, () => c.border, (v) => (c.border = v)));
  if (c.border) p.append(colorField("Border color", () => c.borderColor || "#ffffff", (v) => (c.borderColor = v)));
}
