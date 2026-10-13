// SRT / WebVTT parsing and writing (captions), in milliseconds.

const TS = /(\d{1,2}):(\d{2}):(\d{2})[,.](\d{1,3})/;
const toMs = (m) => ((+m[1] * 60 + +m[2]) * 60 + +m[3]) * 1000 + +m[4].padEnd(3, "0");

/** Parse SRT or WebVTT text into [{ start, end, text }]. */
export function parseSubtitles(text) {
  const out = [];
  const blocks = String(text).replace(/\r/g, "").split(/\n{2,}/);
  for (const b of blocks) {
    const lines = b.split("\n").filter((l) => l.length || false);
    const at = lines.findIndex((l) => l.includes("-->"));
    if (at < 0) continue;
    const [a, c] = lines[at].split("-->");
    const ma = a.match(TS), mc = c.match(TS);
    if (!ma || !mc) continue;
    const body = lines.slice(at + 1).join("\n").replace(/<[^>]+>/g, "").trim();
    if (body) out.push({ start: toMs(ma), end: toMs(mc), text: body });
  }
  return out;
}

const pad = (n, w = 2) => String(Math.floor(n)).padStart(w, "0");
function stamp(ms) {
  ms = Math.max(0, Math.round(ms));
  return `${pad(ms / 3600000)}:${pad((ms / 60000) % 60)}:${pad((ms / 1000) % 60)},${pad(ms % 1000, 3)}`;
}

/** [{ start, end, text }] -> SRT text. */
export function writeSrt(items) {
  return items
    .map((it, i) => `${i + 1}\n${stamp(it.start)} --> ${stamp(it.end)}\n${it.text}\n`)
    .join("\n");
}
