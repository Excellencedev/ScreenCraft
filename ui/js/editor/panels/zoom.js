import { S, P } from "../state.js";
import { h3, el, slider, toggle, segmented } from "../widgets.js";
import { addZoomAtPlayhead, autoZoom, deleteSelected, selectedZoomIds } from "../timeline.js";

export function panelZoom(p) {
  const sel = S.selected;
  const z = sel?.type === "zoom" && P().scene.zooms.find((x) => x.id === sel.id);
  if (!z) {
    p.append(h3("Zoom"));
    p.append(el('<div class="muted">Zooms are created automatically from your clicks and typing. Select a zoom block on the timeline to edit it, or add one at the playhead.</div>'));
    p.append(slider("Default zoom level", 1.2, 4, 0.1, () => P().config.animations.defaultZoom, (v) => (P().config.animations.defaultZoom = v), { fmt: (v) => v.toFixed(1) + "×", track: false }));
    const an = P().config.animations;
    p.append(slider("Default ease in", 0, 1.5, 0.05, () => an.easeInMs / 1000, (v) => (an.easeInMs = v * 1000), { fmt: (v) => v.toFixed(2) + " s", track: true, key: "ease-in-default" }));
    p.append(slider("Default ease out", 0, 1.5, 0.05, () => an.easeOutMs / 1000, (v) => (an.easeOutMs = v * 1000), { fmt: (v) => v.toFixed(2) + " s", track: true, key: "ease-out-default" }));
    const add = el('<button class="btn-line">Add zoom at playhead</button>');
    add.onclick = addZoomAtPlayhead;
    const auto = el('<button class="btn-line">Regenerate from clicks & typing</button>');
    auto.onclick = autoZoom;
    p.append(add, auto);
    return;
  }
  // Several regions selected: the panel shows the first one's settings, and moving any
  // control applies that value to every selected region (the others are untouched until then).
  const group = selectedZoomIds().map((id) => P().scene.zooms.find((x) => x.id === id)).filter(Boolean);
  const all = (fn) => group.forEach(fn);
  p.append(h3(group.length > 1 ? `Zoom regions · ${group.length} selected` : "Zoom region"));
  if (group.length > 1) p.append(el('<div class="hint muted">Showing the first region’s settings. Changing a control applies it to all selected regions.</div>'));
  else p.append(el('<div class="hint muted">Shift-click more zoom blocks to edit several at once.</div>'));
  p.append(slider("Zoom level", 1.1, 5, 0.05, () => z.zoom, (v) => all((g) => (g.zoom = v)), { track: true, timeline: true, key: "zl-" + z.id, fmt: (v) => v.toFixed(2) + "×" }));
  const an = P().config.animations;
  p.append(slider("Ease in", 0, 1.5, 0.05, () => (z.easeInMs ?? an.easeInMs) / 1000, (v) => all((g) => (g.easeInMs = v * 1000)), { fmt: (v) => v.toFixed(2) + " s", track: true, key: "ei-" + z.id }));
  p.append(slider("Ease out", 0, 1.5, 0.05, () => (z.easeOutMs ?? an.easeOutMs) / 1000, (v) => all((g) => (g.easeOutMs = v * 1000)), { fmt: (v) => v.toFixed(2) + " s", track: true, key: "eo-" + z.id }));
  p.append(segmented("Zoom position", [["follow-cursor", "Auto"], ["manual", "Manual"]], () => z.kind, (v) => all((g) => (g.kind = v)), { track: true, timeline: true }));
  p.append(el(`<div class="hint muted">${z.kind === "manual" ? "Click the preview to choose where the camera looks." : "Camera keeps the cursor centred and follows the text caret while you type."}</div>`));
  p.append(toggle("Instant animation", "Apply the zoom immediately without easing.", () => z.instant, (v) => all((g) => (g.instant = v)), { track: true }));
  p.append(toggle("Disabled", null, () => z.isDisabled, (v) => all((g) => (g.isDisabled = v)), { track: true }));
  const del = el(`<button class="btn-line danger">${group.length > 1 ? `Delete ${group.length} zooms` : "Delete zoom"}</button>`);
  del.onclick = deleteSelected;
  p.append(del);
}
