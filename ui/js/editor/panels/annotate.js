import { S, P, hooks } from "../state.js";
import { change } from "../history.js";
import { h3, el, slider, toggle, segmented, colorField } from "../widgets.js";
import { addAnnotationAtPlayhead, deleteSelected, NOTE_NAMES } from "../timeline.js";

const ADD = [
  ["text", "Text", '<path d="M5 6h14M12 6v13M9 19h6"/>'],
  ["arrow", "Arrow", '<path d="M5 19L19 5M10 5h9v9"/>'],
  ["box", "Box", '<rect x="4" y="6" width="16" height="12" rx="2"/>'],
  ["ellipse", "Circle", '<ellipse cx="12" cy="12" rx="8" ry="6"/>'],
  ["highlight", "Highlight", '<path d="M4 15l9-9 5 5-9 9H4zM12 18h8"/>'],
  ["blur", "Blur", '<circle cx="12" cy="12" r="3"/><circle cx="12" cy="12" r="7" stroke-dasharray="2 3"/>'],
  ["pixelate", "Pixelate", '<path d="M4 4h6v6H4zM14 4h6v6h-6zM4 14h6v6H4zM14 14h6v6h-6z"/>'],
  ["spotlight", "Spotlight", '<path d="M4 4h16v16H4z"/><circle cx="12" cy="12" r="4"/>'],
];

const hex6 = (c) => (c && /^#[0-9a-f]{6}/i.test(c) ? c.slice(0, 7) : "#000000");
const alphaOf = (c) => (c && /^#[0-9a-f]{8}$/i.test(c) ? parseInt(c.slice(7), 16) / 255 : 1);
const withAlpha = (rgb, a) => rgb + Math.round(Math.max(0, Math.min(1, a)) * 255).toString(16).padStart(2, "0");

export function panelAnnotate(p) {
  p.append(h3("Annotate"));
  p.append(el('<div class="muted">Add something at the playhead, then drag it on the preview. Blur and spotlight stay on the same spot of the recording as the camera zooms.</div>'));
  const grid = el('<div class="add-grid"></div>');
  for (const [kind, name, icon] of ADD) {
    const b = el(`<button class="add-tile" title="Add ${name.toLowerCase()} at the playhead"><svg viewBox="0 0 24 24">${icon}</svg><span>${name}</span></button>`);
    b.onclick = () => addAnnotationAtPlayhead(kind);
    grid.appendChild(b);
  }
  p.append(grid);

  const list = P().scene.annotations || [];
  const a = S.selected?.type === "annot" && list.find((x) => x.id === S.selected.id);
  if (a) editAnnotation(p, a);
  else if (list.length) {
    p.append(h3("On the timeline"));
    for (const it of list) {
      const row = el('<button class="note-row"></button>');
      row.textContent = it.kind === "text" ? `Text · ${(it.text || "").slice(0, 26)}` : NOTE_NAMES[it.kind] || it.kind;
      row.onclick = () => {
        S.selected = { type: "annot", id: it.id };
        hooks.renderTimeline();
        hooks.renderPanel();
        hooks.draw();
      };
      p.append(row);
    }
  }

  // shortcut display
  const ks = P().config.keystrokes;
  p.append(h3("Shortcut display"));
  p.append(toggle("Show shortcuts", "Pops up keys like ⌘C or Esc as you press them. Only recordings made after this feature was added carry shortcut data.", () => ks.enabled, (v) => (ks.enabled = v), { track: false }));
  if (ks.enabled) {
    p.append(segmented("Position", [["bottom-center", "Bottom"], ["bottom-left", "Left"], ["bottom-right", "Right"], ["top-center", "Top"]], () => ks.position, (v) => (ks.position = v), { track: false }));
    p.append(slider("Size", 16, 64, 1, () => ks.size, (v) => (ks.size = v), { fmt: (v) => v + " px", track: false }));
    p.append(slider("Stays on screen", 600, 3000, 100, () => ks.holdMs, (v) => (ks.holdMs = v), { fmt: (v) => (v / 1000).toFixed(1) + " s", track: false }));
  }
}

function editAnnotation(p, a) {
  const k = a.kind;
  p.append(h3(NOTE_NAMES[k] || "Annotation"));
  const opt = { track: false };
  if (k === "text") {
    const w = el('<div class="field"><label><span>Text</span></label><textarea rows="3" spellcheck="false"></textarea></div>');
    const ta = w.querySelector("textarea");
    ta.value = a.text || "";
    ta.oninput = () => change("an-text-" + a.id, () => (a.text = ta.value), { ...opt, timeline: true });
    ta.onkeydown = (e) => e.stopPropagation();
    p.append(w);
    p.append(slider("Size", 16, 140, 1, () => a.fontSize, (v) => (a.fontSize = v), { fmt: (v) => v + " px", ...opt, key: "an-fs-" + a.id }));
    p.append(colorField("Text color", () => hex6(a.color), (v) => (a.color = v)));
    const hasFill = a.fill && a.fill !== "none";
    p.append(toggle("Background", null, () => !!hasFill, (v) => (a.fill = v ? "#000000b3" : ""), opt));
    if (hasFill) {
      p.append(colorField("Background color", () => hex6(a.fill), (v) => (a.fill = withAlpha(v, alphaOf(a.fill)))));
      p.append(slider("Background opacity", 0.1, 1, 0.05, () => alphaOf(a.fill), (v) => (a.fill = withAlpha(hex6(a.fill), v)), { fmt: (v) => Math.round(v * 100) + "%", ...opt, key: "an-fo-" + a.id }));
    }
  } else if (k === "box" || k === "ellipse" || k === "arrow" || k === "highlight") {
    p.append(colorField("Color", () => hex6(a.color), (v) => (a.color = v)));
    if (k !== "highlight") p.append(slider("Thickness", 1, 24, 1, () => a.thickness, (v) => (a.thickness = v), { fmt: (v) => v + " px", ...opt, key: "an-th-" + a.id }));
  } else if (k === "blur" || k === "pixelate") {
    p.append(slider("Strength", 0.1, 1, 0.05, () => a.strength, (v) => (a.strength = v), { fmt: (v) => Math.round(v * 100) + "%", ...opt, key: "an-st-" + a.id }));
  } else if (k === "spotlight") {
    p.append(slider("Dim outside", 0.2, 0.95, 0.05, () => a.strength, (v) => (a.strength = v), { fmt: (v) => Math.round(v * 100) + "%", ...opt, key: "an-st-" + a.id }));
  }
  if (k === "text" || k === "arrow" || k === "box" || k === "ellipse" || k === "highlight") {
    p.append(segmented("Appears", [["fade", "Fade"], ["pop", "Pop"], ["none", "Instant"]], () => a.anim, (v) => (a.anim = v), opt));
    p.append(segmented("Stays", [["content", "On the recording"], ["screen", "On the frame"]], () => a.space, (v) => (a.space = v), opt));
  }
  const dur = ((a.sourceEndMs - a.sourceStartMs) / 1000).toFixed(1);
  p.append(el(`<div class="hint muted">${dur} s long. Drag the block on the Notes track to move it, its ends to trim. Drag on the preview to position${k === "text" ? "" : " and resize"}.</div>`));
  const del = el('<button class="btn-line danger">Delete</button>');
  del.onclick = deleteSelected;
  p.append(del);
}
