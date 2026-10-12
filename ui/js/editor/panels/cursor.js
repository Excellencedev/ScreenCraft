import { P } from "../state.js";
import { change } from "../history.js";
import { h3, el, slider, toggle, segmented, colorField } from "../widgets.js";
import { drawCursor } from "../../cursors.js";

export function panelCursor(p) {
  const c = P().config.cursor;
  const a = P().config.animations;
  p.append(h3("Cursor"));
  p.append(slider("Size", 10, 80, 1, () => c.size, (v) => (c.size = v), { fmt: (v) => v + " pt" }));
  const row = el('<div class="field"><label><span>Cursor style</span></label><div class="seg"></div></div>');
  for (const [v, name] of [["macos", "Dark"], ["light", "Light"], ["accent", "Accent"], ["dot", "Dot"]]) {
    const cv = document.createElement("canvas");
    cv.width = 72;
    cv.height = 48;
    const g = cv.getContext("2d");
    g.fillStyle = "#2a2c3d";
    g.fillRect(0, 0, 72, 48);
    drawCursor(g, 0, 28, 8, 30, v);
    const b = el(`<button class="${c.style === v ? "on" : ""}" title="${name}" style="padding:0;overflow:hidden"></button>`);
    b.appendChild(cv);
    b.onclick = () => change(null, () => (c.style = v), { panel: true, track: false });
    row.querySelector(".seg").appendChild(b);
  }
  p.append(row);
  p.append(toggle("Always use pointer cursor", "Don't change cursor, even if selecting text, etc.", () => c.alwaysUsePointer, (v) => (c.alwaysUsePointer = v)));
  p.append(toggle("Hide cursor if not moving", null, () => c.hideNotMovingAfterMs != null, (v) => (c.hideNotMovingAfterMs = v ? 2000 : null), { track: true }));
  p.append(toggle("Loop cursor position", "Near the end of the video, cursor will move back to its initial position.", () => c.loopPositionBeforeEndMs != null, (v) => (c.loopPositionBeforeEndMs = v ? 1200 : null), { track: true }));
  p.append(toggle("Tilt with movement", "Rotate the cursor slightly in the direction it moves.", () => c.rotateWithVelocity, (v) => (c.rotateWithVelocity = v), { track: true }));
  p.append(toggle("Hide cursor", null, () => c.hide, (v) => (c.hide = v), { track: true }));
  p.append(segmented("Click effect", [["none", "None"], ["circle", "Circle"], ["ripple", "Ripple"]], () => c.clickEffect, (v) => (c.clickEffect = v), { track: false }));
  if (c.clickEffect !== "none") p.append(colorField("Effect color", () => c.clickEffectColor, (v) => (c.clickEffectColor = v)));

  p.append(h3("Camera motion"));
  p.append(segmented("Follow", [["center", "Keep centered"], ["edge", "Only near edges"]], () => a.follow, (v) => (a.follow = v), { track: true }));
  p.append(el('<div class="hint muted">Zoomed views keep the cursor in the middle of the screen. While you type, the camera moves to the text caret (grant Accessibility in Permissions).</div>'));
  p.append(slider("Cursor smoothing", 40, 400, 5, () => a.mouseSpring.stiffness, (v) => (a.mouseSpring.stiffness = v), { track: true, key: "mouse-stiff" }));
  p.append(slider("Camera stiffness", 40, 400, 5, () => a.screenSpring.stiffness, (v) => (a.screenSpring.stiffness = v), { track: true, key: "screen-stiff" }));
  p.append(slider("Camera damping", 4, 40, 0.5, () => a.screenSpring.damping, (v) => (a.screenSpring.damping = v), { track: true, key: "screen-damp" }));
}
