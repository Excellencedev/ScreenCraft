// Dragging annotations on the preview canvas: body moves, the corner resizes, arrow ends
// move individually. Positions are stored normalised (to the recording or the frame).
import { S, P, clamp } from "./state.js";
import { change } from "./history.js";
import { displayedRect, drawSrc } from "./draw.js";
import { $ } from "./state.js";

/** Returns true when the pointer press was used to edit the selected annotation. */
export function beginAnnotDrag(e) {
  if (S.selected?.type !== "annot") return false;
  const a = (P().scene.annotations || []).find((x) => x.id === S.selected.id);
  const b = a && drawSrc.bounds.get(a.id);
  if (!a || !b) return false;
  const c = $("preview");
  const rect = displayedRect();
  const toCanvas = (ev) => ({
    x: ((ev.clientX - rect.left) / rect.width) * c.width,
    y: ((ev.clientY - rect.top) / rect.height) * c.height,
  });
  const u = c.height / 1080;
  const p0 = toCanvas(e);
  const near = (x, y, r) => Math.hypot(p0.x - x, p0.y - y) <= r * u;

  let mode = null;
  if (b.arrow) {
    const [x1, y1, x2, y2] = b.arrow;
    if (near(x2, y2, 18)) mode = "end";
    else if (near(x1, y1, 18)) mode = "start";
    else {
      // anywhere along the shaft
      const dx = x2 - x1, dy = y2 - y1, len2 = dx * dx + dy * dy || 1;
      const t = clamp(((p0.x - x1) * dx + (p0.y - y1) * dy) / len2, 0, 1);
      if (Math.hypot(p0.x - (x1 + dx * t), p0.y - (y1 + dy * t)) <= 14 * u) mode = "move";
    }
  } else {
    const pad = 10 * u;
    const inside = p0.x >= b.x - pad && p0.x <= b.x + b.w + pad && p0.y >= b.y - pad && p0.y <= b.y + b.h + pad;
    if (inside) mode = a.kind !== "text" && near(b.x + b.w, b.y + b.h, 18) ? "resize" : "move";
  }
  if (!mode) return false;

  const g = drawSrc.geom;
  const content = a.space === "content" && g.sr;
  // canvas px -> annotation units
  const kx = content ? g.vwN / g.sr.w : 1 / c.width;
  const ky = content ? g.vhN / g.sr.h : 1 / c.height;
  const start = { x: a.x, y: a.y, w: a.w, h: a.h, x2: a.x2, y2: a.y2 };
  const key = "annot-drag-" + a.id;
  e.preventDefault();
  const move = (ev) => {
    const p = toCanvas(ev);
    const dx = (p.x - p0.x) * kx, dy = (p.y - p0.y) * ky;
    change(key, () => {
      if (mode === "move") {
        a.x = start.x + dx;
        a.y = start.y + dy;
        if (a.kind === "arrow") {
          a.x2 = start.x2 + dx;
          a.y2 = start.y2 + dy;
        }
      } else if (mode === "resize") {
        a.w = Math.max(0.01, start.w + dx);
        a.h = Math.max(0.01, start.h + dy);
      } else if (mode === "start") {
        a.x = start.x + dx;
        a.y = start.y + dy;
      } else if (mode === "end") {
        a.x2 = start.x2 + dx;
        a.y2 = start.y2 + dy;
      }
    }, { track: false });
  };
  const up = () => {
    removeEventListener("pointermove", move);
    removeEventListener("pointerup", up);
  };
  addEventListener("pointermove", move);
  addEventListener("pointerup", up);
  return true;
}
