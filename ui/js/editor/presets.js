import { P } from "./state.js";
import { change } from "./history.js";
import { showMenu } from "./menus.js";

const BUILTIN = {
  Clean: { styles: { paddingRatio: 0.02, cornerRadius: 14, background: { kind: "wallpaper", wallpaper: "aurora" } } },
  Minimal: { styles: { paddingRatio: 0.04, cornerRadius: 10, background: { kind: "color", color: "#0b0c12" }, shadow: { intensity: 0.15 } } },
  Vivid: { styles: { paddingRatio: 0.12, cornerRadius: 24, background: { kind: "gradient", gradientFrom: "#ff6ad5", gradientTo: "#3b82c4", gradientAngle: 135 } }, cursor: { clickEffect: "ripple", size: 34 } },
  Sunset: { styles: { paddingRatio: 0.1, cornerRadius: 18, background: { kind: "wallpaper", wallpaper: "sunset" } } },
};
const userPresets = () => {
  try { return JSON.parse(localStorage.getItem("sc-presets") || "{}"); } catch { return {}; }
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
  }, { panel: true });
}

export function presetsMenu(anchor) {
  const items = Object.keys(BUILTIN).map((n) => ({ label: n, run: () => apply(BUILTIN[n]) }));
  const mine = userPresets();
  const names = Object.keys(mine);
  if (names.length) {
    items.push("-");
    names.forEach((n) => items.push({ label: n, run: () => apply(mine[n]) }));
  }
  items.push("-", {
    label: "Save current as preset",
    run: () => {
      const all = userPresets();
      all["My preset " + (Object.keys(all).length + 1)] = JSON.parse(
        JSON.stringify({ styles: P().config.styles, cursor: P().config.cursor, camera: P().config.camera }),
      );
      localStorage.setItem("sc-presets", JSON.stringify(all));
    },
  });
  showMenu(anchor, items);
}
