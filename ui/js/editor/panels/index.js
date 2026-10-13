import { $, S, P } from "../state.js";
import { panelAppearance } from "./appearance.js";
import { panelCursor } from "./cursor.js";
import { panelCamera } from "./camera.js";
import { panelZoom } from "./zoom.js";
import { panelAudio } from "./audio.js";
import { panelCrop } from "./crop.js";
import { panelSoon } from "./soon.js";
import { panelAnnotate } from "./annotate.js";
import { panelPace } from "./pace.js";
import { panelCaptions } from "./captions.js";
import { el } from "../widgets.js";

const TABS = [
  ["background", "Appearance", '<path d="M4 5h16v14H4zM4 15l5-5 4 4 3-3 4 4"/>', panelAppearance],
  ["cursor", "Cursor & motion", '<path d="M5 3l14 8-6 2-2 6z"/>', panelCursor],
  ["camera", "Camera", '<rect x="3" y="7" width="13" height="10" rx="2"/><path d="M16 11l5-3v8l-5-3z"/>', panelCamera],
  ["zoom", "Zoom", '<circle cx="11" cy="11" r="6"/><path d="M21 21l-5-5M11 8v6M8 11h6"/>', panelZoom],
  ["audio", "Audio", '<path d="M4 10v4h4l5 4V6L8 10z"/><path d="M16 9a4 4 0 010 6"/>', panelAudio],
  ["crop", "Crop", '<path d="M6 2v14a2 2 0 002 2h14M2 6h14a2 2 0 012 2v14"/>', panelCrop],
  ["annotate", "Annotate", '<path d="M5 6h14M12 6v13M9 19h6"/>', panelAnnotate],
  ["captions", "Captions", '<rect x="3" y="5" width="18" height="14" rx="2.5"/><path d="M7 12h3M12 12h5M7 15.5h6"/>', panelCaptions],
  ["pace", "Pace & transitions", '<path d="M13 3L5 14h6l-1 7 8-11h-6z"/>', panelPace],
  ["soon", "Coming soon", '<path d="M12 3l2.4 5.2 5.6.7-4.1 3.9 1 5.6L12 15.7 7.1 18.4l1-5.6L4 8.9l5.6-.7z"/>', panelSoon],
];

function renderRail() {
  const rail = $("rail");
  rail.replaceChildren();
  for (const [id, title, path] of TABS) {
    const b = el(`<button title="${title}" class="${S.tab === id ? "on" : ""}"><svg viewBox="0 0 24 24">${path}</svg></button>`);
    b.onclick = () => {
      S.tab = id;
      renderPanel();
    };
    rail.appendChild(b);
  }
}

export function renderPanel() {
  renderRail();
  const p = $("panel");
  p.replaceChildren();
  if (!P()) return;
  const tab = TABS.find((t) => t[0] === S.tab) || TABS[0];
  tab[3](p);
}
