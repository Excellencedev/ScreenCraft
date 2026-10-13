import { S, P, hooks, uid, fmt } from "../state.js";
import { invoke, dialog, baseName } from "../../tauri.js";
import { change } from "../history.js";
import { h3, el, slider, toggle, segmented, colorField } from "../widgets.js";
import { addCaptionAtPlayhead, deleteSelected } from "../timeline.js";
import { outToSrc, srcToOut } from "../timemap.js";
import { parseSubtitles, writeSrt } from "../srt.js";

/** Style presets; position and width are left as the user set them. */
export const SUBTITLE_PRESETS = {
  tiktok: { style: "tiktok", size: 56, color: "#ffffff", outlineColor: "#000000", outlineWidth: 9, background: "", uppercase: false, pop: true },
  "tiktok-red": { style: "tiktok-red", size: 56, color: "#000000", outlineColor: "#fe2c55", outlineWidth: 9, background: "", uppercase: false, pop: true },
  highlight: { style: "highlight", size: 56, color: "#ffe14a", outlineColor: "#000000", outlineWidth: 9, background: "", uppercase: true, pop: true },
  classic: { style: "classic", size: 46, color: "#ffffff", outlineColor: "#000000", outlineWidth: 0, background: "#000000b3", uppercase: false, pop: false },
  minimal: { style: "minimal", size: 42, color: "#ffffff", outlineColor: "#000000", outlineWidth: 3, background: "", uppercase: false, pop: false },
};
const PRESET_NAMES = [["tiktok", "TikTok"], ["tiktok-red", "TikTok red"], ["highlight", "Highlight"], ["classic", "Classic"], ["minimal", "Minimal"]];

async function importSrt() {
  const picked = await dialog.open({ multiple: false, filters: [{ name: "Subtitles", extensions: ["srt", "vtt"] }] });
  const path = Array.isArray(picked) ? picked[0] : picked;
  if (!path) return;
  let items;
  try {
    items = parseSubtitles(await invoke("read_text_file", { path }));
  } catch (e) {
    alert("Could not read that file: " + (e?.message || e));
    return;
  }
  if (!items.length) {
    alert("No captions found in that file.");
    return;
  }
  // SRT times describe the finished video, so map them back onto the recording
  change(null, () => {
    P().scene.subtitles = P().scene.subtitles || [];
    for (const it of items) {
      P().scene.subtitles.push({
        id: uid(), text: it.text,
        sourceStartMs: outToSrc(it.start).src,
        sourceEndMs: Math.max(outToSrc(it.start).src + 200, outToSrc(it.end).src),
      });
    }
    S.selected = null;
  }, { panel: true, timeline: true, track: false });
}

async function exportSrt() {
  const items = (P().scene.subtitles || [])
    .map((c) => ({ start: srcToOut(c.sourceStartMs), end: srcToOut(c.sourceEndMs), text: c.text }))
    .filter((c) => c.start != null && c.end != null && (c.text || "").trim())
    .sort((a, b) => a.start - b.start);
  if (!items.length) {
    alert("There are no captions to export.");
    return;
  }
  const base = (P().name || "captions").replace(/[\\/:]/g, "-");
  const path = await dialog.save({ defaultPath: `${base}.srt`, filters: [{ name: "SRT", extensions: ["srt"] }] });
  if (!path) return;
  const bytes = new TextEncoder().encode(writeSrt(items));
  await invoke("export_begin", { path });
  await invoke("export_write", bytes, { headers: { "x-position": "0" } });
  await invoke("export_end");
}

export function panelCaptions(p) {
  const st = P().config.subtitles;
  const list = (P().scene.subtitles || []).slice().sort((a, b) => a.sourceStartMs - b.sourceStartMs);
  p.append(h3("Captions"));
  p.append(el('<div class="muted">Type captions or import an SRT file. They sit in the middle of the bottom third of the video, like TikTok.</div>'));
  const row = el('<div class="btn-row"></div>');
  const add = el('<button class="btn-line">Add caption</button>');
  add.onclick = addCaptionAtPlayhead;
  const imp = el('<button class="btn-line">Import SRT…</button>');
  imp.onclick = importSrt;
  const exp = el('<button class="btn-line">Export SRT…</button>');
  exp.onclick = exportSrt;
  row.append(add, imp, exp);
  p.append(row);

  const cap = S.selected?.type === "caption" && list.find((c) => c.id === S.selected.id);
  if (cap) {
    p.append(h3("Selected caption"));
    const w = el('<div class="field"><label><span>Text</span></label><textarea rows="3" spellcheck="false"></textarea></div>');
    const ta = w.querySelector("textarea");
    ta.value = cap.text || "";
    ta.oninput = () => change("cap-" + cap.id, () => (cap.text = ta.value), { track: false, timeline: true });
    ta.onkeydown = (e) => e.stopPropagation();
    p.append(w);
    const len = ((cap.sourceEndMs - cap.sourceStartMs) / 1000).toFixed(1);
    p.append(el(`<div class="hint muted">${len} s. Drag the block on the Captions track to move it, its ends to trim.</div>`));
    const del = el('<button class="btn-line danger">Delete caption</button>');
    del.onclick = deleteSelected;
    p.append(del);
  } else if (list.length) {
    p.append(h3(`${list.length} caption${list.length === 1 ? "" : "s"}`));
    for (const c of list.slice(0, 60)) {
      const r = el('<button class="note-row"></button>');
      const t0 = srcToOut(c.sourceStartMs);
      r.textContent = `${t0 != null ? fmt(t0) : "—"}  ${(c.text || "").replace(/\s+/g, " ").slice(0, 34) || "(empty)"}`;
      r.onclick = () => {
        S.selected = { type: "caption", id: c.id };
        hooks.seek(t0 ?? 0);
        hooks.renderTimeline();
        hooks.renderPanel();
      };
      p.append(r);
    }
  }

  p.append(h3("Style"));
  p.append(segmented("Preset", PRESET_NAMES, () => st.style, (v) => Object.assign(st, SUBTITLE_PRESETS[v]), { track: false }));
  const opt = { track: false };
  p.append(slider("Size", 24, 120, 1, () => st.size, (v) => (st.size = v), { fmt: (v) => v + " px", ...opt, key: "sub-size" }));
  p.append(slider("Height", 0.15, 0.95, 0.01, () => st.y, (v) => (st.y = v), { fmt: (v) => Math.round(v * 100) + "%", ...opt, key: "sub-y" }));
  p.append(slider("Width", 0.2, 0.95, 0.01, () => st.width, (v) => (st.width = v), { fmt: (v) => Math.round(v * 100) + "%", ...opt, key: "sub-w" }));
  p.append(colorField("Text color", () => st.color, (v) => (st.color = v)));
  p.append(slider("Outline", 0, 20, 1, () => st.outlineWidth, (v) => (st.outlineWidth = v), { fmt: (v) => v + " px", ...opt, key: "sub-ow" }));
  if (st.outlineWidth > 0) p.append(colorField("Outline color", () => st.outlineColor, (v) => (st.outlineColor = v)));
  p.append(toggle("Capital letters", null, () => st.uppercase, (v) => (st.uppercase = v), opt));
  p.append(toggle("Pop in", "A quick bounce when each caption appears.", () => st.pop, (v) => (st.pop = v), opt));
  const hasBox = !!st.background;
  p.append(toggle("Box behind text", null, () => hasBox, (v) => (st.background = v ? "#000000b3" : ""), opt));
  void baseName;
}
