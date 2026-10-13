import { P } from "./state.js";
import { change } from "./history.js";
import { showMenu } from "./menus.js";
import { el } from "./widgets.js";

// Every preset keeps the video padding at 2%.
const PAD = 0.02;
const BUILTIN = {
  Clean: { styles: { paddingRatio: PAD, cornerRadius: 14, background: { kind: "wallpaper", wallpaper: "sunset" } } },
  Minimal: { styles: { paddingRatio: PAD, cornerRadius: 10, background: { kind: "color", color: "#0b0c12" }, shadow: { intensity: 0.15 } } },
  Vivid: { styles: { paddingRatio: PAD, cornerRadius: 24, background: { kind: "gradient", gradientFrom: "#ff6ad5", gradientTo: "#3b82c4", gradientAngle: 135 } }, cursor: { clickEffect: "ripple", size: 34 } },
  Sunset: { styles: { paddingRatio: PAD, cornerRadius: 18, background: { kind: "wallpaper", wallpaper: "sunset" } } },
};
const userPresets = () => {
  try { return JSON.parse(localStorage.getItem("sc-presets") || "{}"); } catch { return {}; }
};
const saveUserPresets = (all) => {
  try { localStorage.setItem("sc-presets", JSON.stringify(all)); } catch {}
};

function deepMerge(t, s) {
  for (const k of Object.keys(s)) {
    if (s[k] && typeof s[k] === "object" && !Array.isArray(s[k])) deepMerge((t[k] ??= {}), s[k]);
    else t[k] = s[k];
  }
}

function apply(pr) {
  change(null, () => {
    const c = P().config;
    for (const key of ["styles", "cursor", "camera"]) {
      if (pr[key]) deepMerge(c[key], JSON.parse(JSON.stringify(pr[key])));
    }
    c.styles.paddingRatio = PAD;
  }, { panel: true });
}

const ICON_GEAR = '<svg viewBox="0 0 24 24" width="16" height="16" fill="none" stroke="currentColor" stroke-width="1.8" stroke-linecap="round" stroke-linejoin="round"><circle cx="12" cy="12" r="3"/><path d="M19.4 15a1.7 1.7 0 00.3 1.9l.1.1a2 2 0 11-2.8 2.8l-.1-.1a1.7 1.7 0 00-1.9-.3 1.7 1.7 0 00-1 1.5V21a2 2 0 11-4 0v-.1a1.7 1.7 0 00-1.1-1.5 1.7 1.7 0 00-1.9.3l-.1.1a2 2 0 11-2.8-2.8l.1-.1a1.7 1.7 0 00.3-1.9 1.7 1.7 0 00-1.5-1H3a2 2 0 110-4h.1a1.7 1.7 0 001.5-1.1 1.7 1.7 0 00-.3-1.9l-.1-.1a2 2 0 112.8-2.8l.1.1a1.7 1.7 0 001.9.3h.1a1.7 1.7 0 001-1.5V3a2 2 0 114 0v.1a1.7 1.7 0 001 1.5 1.7 1.7 0 001.9-.3l.1-.1a2 2 0 112.8 2.8l-.1.1a1.7 1.7 0 00-.3 1.9v.1a1.7 1.7 0 001.5 1H21a2 2 0 110 4h-.1a1.7 1.7 0 00-1.5 1z"/></svg>';
const ICON_TRASH = '<svg viewBox="0 0 24 24" width="15" height="15" fill="none" stroke="currentColor" stroke-width="1.8" stroke-linecap="round" stroke-linejoin="round"><path d="M4 7h16M10 11v6M14 11v6M6 7l1 13h10l1-13M9 7V4h6v3"/></svg>';

/** The Presets popover: built-in presets, your own, and a Save Preset row with a settings button. */
export function presetsMenu(anchor, manage = false, focusName = null) {
  if (manage) return manageMenu(anchor, focusName);
  const items = Object.keys(BUILTIN).map((n) => ({ label: n, run: () => apply(BUILTIN[n]) }));
  const mine = userPresets();
  const names = Object.keys(mine);
  if (names.length) {
    items.push("-");
    names.forEach((n) => items.push({ label: n, run: () => apply(mine[n]) }));
  }
  items.push("-");
  const row = el('<div class="preset-save"><button class="save">Save Preset</button><button class="gear" title="Rename or delete your presets"></button></div>');
  row.querySelector(".gear").innerHTML = ICON_GEAR;
  row.querySelector(".save").onclick = () => {
    const all = userPresets();
    let n = Object.keys(all).length + 1;
    while (all["My preset " + n]) n++;
    const name = "My preset " + n;
    all[name] = JSON.parse(JSON.stringify({ styles: P().config.styles, cursor: P().config.cursor, camera: P().config.camera }));
    saveUserPresets(all);
    manageMenu(anchor, name); // straight into renaming the new preset
  };
  row.querySelector(".gear").onclick = () => manageMenu(anchor);
  items.push({ node: row });
  showMenu(anchor, items);
}

function manageMenu(anchor, focusName = null) {
  const items = [{ header: "Your presets" }];
  const all = userPresets();
  const names = Object.keys(all);
  if (!names.length) items.push({ node: el('<div class="preset-empty">No saved presets yet. Use Save Preset first.</div>') });
  for (const name of names) {
    const row = el('<div class="preset-row"><input type="text" spellcheck="false" maxlength="40"><button class="del" title="Delete preset"></button></div>');
    const input = row.querySelector("input");
    input.value = name;
    row.querySelector(".del").innerHTML = ICON_TRASH;
    const commit = () => {
      const next = input.value.trim();
      if (!next || next === name) { input.value = name; return; }
      const cur = userPresets();
      if (cur[next]) { input.value = name; return; } // keep names unique
      // rebuild so the order is kept
      const out = {};
      for (const k of Object.keys(cur)) out[k === name ? next : k] = cur[k];
      saveUserPresets(out);
      manageMenu(anchor);
    };
    input.onkeydown = (e) => {
      e.stopPropagation();
      if (e.key === "Enter") { e.preventDefault(); commit(); }
      else if (e.key === "Escape") { input.value = name; input.blur(); }
    };
    input.onblur = commit;
    row.querySelector(".del").onmousedown = (e) => e.preventDefault(); // don't commit a rename first
    row.querySelector(".del").onclick = () => {
      const cur = userPresets();
      delete cur[name];
      saveUserPresets(cur);
      manageMenu(anchor);
    };
    items.push({ node: row });
    if (name === focusName) setTimeout(() => { input.focus(); input.select(); }, 30);
  }
  items.push("-");
  const done = el('<div class="preset-save"><button class="save">Done</button></div>');
  done.querySelector(".save").onclick = () => presetsMenu(anchor);
  items.push({ node: done });
  showMenu(anchor, items);
}
