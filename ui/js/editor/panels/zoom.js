import { S, P } from "../state.js";
import { h3, el, slider, toggle, segmented } from "../widgets.js";
import { addZoomAtPlayhead, autoZoom, deleteSelected } from "../timeline.js";

export function panelZoom(p) {
  const sel = S.selected;
  const z = sel?.type === "zoom" && P().scene.zooms.find((x) => x.id === sel.id);
  if (!z) {
    p.append(h3("Zoom"));
    p.append(el('<div class="muted">Zooms are created automatically from your clicks and typing. Select a zoom block on the timeline to edit it, or add one at the playhead.</div>'));
    p.append(slider("Default zoom level", 1.2, 4, 0.1, () => P().config.animations.defaultZoom, (v) => (P().config.animations.defaultZoom = v), { fmt: (v) => v.toFixed(1) + "×", track: false }));
    const add = el('<button class="btn-line">Add zoom at playhead</button>');
    add.onclick = addZoomAtPlayhead;
    const auto = el('<button class="btn-line">Regenerate from clicks & typing</button>');
    auto.onclick = autoZoom;
    p.append(add, auto);
    return;
  }
  p.append(h3("Zoom region"));
  p.append(slider("Zoom level", 1.1, 5, 0.05, () => z.zoom, (v) => (z.zoom = v), { track: true, timeline: true, key: "zl-" + z.id, fmt: (v) => v.toFixed(2) + "×" }));
  p.append(segmented("Zoom position", [["follow-cursor", "Auto"], ["manual", "Manual"]], () => z.kind, (v) => (z.kind = v), { track: true, timeline: true }));
  p.append(el(`<div class="hint muted">${z.kind === "manual" ? "Click the preview to choose where the camera looks." : "Camera keeps the cursor centred and follows the text caret while you type."}</div>`));
  p.append(toggle("Instant animation", "Apply the zoom immediately without easing.", () => z.instant, (v) => (z.instant = v), { track: true }));
  p.append(toggle("Disabled", null, () => z.isDisabled, (v) => (z.isDisabled = v), { track: true }));
  const del = el('<button class="btn-line danger">Delete zoom</button>');
  del.onclick = deleteSelected;
  p.append(del);
}
