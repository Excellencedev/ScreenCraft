import { P, hooks } from "../state.js";
import { invoke, dialog } from "../../tauri.js";
import { change } from "../history.js";
import { h3, el, slider, toggle, segmented, colorField } from "../widgets.js";
import { WALLPAPERS, drawWallpaper } from "../../wallpapers.js";

function swatch(name) {
  const c = document.createElement("canvas");
  c.width = 120;
  c.height = 90;
  drawWallpaper(c.getContext("2d"), 120, 90, name);
  return c;
}

export function panelAppearance(p) {
  const st = P().config.styles, bg = st.background;
  p.append(h3("Background"));
  p.append(segmented("Type", [["wallpaper", "Wallpaper"], ["gradient", "Gradient"], ["color", "Color"], ["image", "Image"]], () => bg.kind, (v) => (bg.kind = v), { track: false }));
  if (bg.kind === "wallpaper") {
    const grid = el('<div class="swatches"></div>');
    for (const name of WALLPAPERS) {
      const cv = swatch(name);
      if (bg.wallpaper === name) cv.classList.add("on");
      cv.onclick = () => change(null, () => (bg.wallpaper = name), { panel: true, track: false });
      grid.appendChild(cv);
    }
    p.append(grid);
    const rnd = el('<button class="btn-line">Pick random wallpaper</button>');
    rnd.onclick = () => change(null, () => (bg.wallpaper = WALLPAPERS[Math.floor(Math.random() * WALLPAPERS.length)]), { panel: true, track: false });
    p.append(rnd);
  } else if (bg.kind === "gradient") {
    p.append(colorField("From", () => bg.gradientFrom, (v) => (bg.gradientFrom = v)));
    p.append(colorField("To", () => bg.gradientTo, (v) => (bg.gradientTo = v)));
    p.append(slider("Angle", 0, 360, 1, () => bg.gradientAngle, (v) => (bg.gradientAngle = v), { fmt: (v) => v + "°" }));
  } else if (bg.kind === "color") {
    p.append(colorField("Color", () => bg.color, (v) => (bg.color = v)));
  } else {
    const b = el('<button class="btn-line">Choose image…</button>');
    b.onclick = async () => {
      // the system file picker + a Rust read: <input type=file> doesn't open in this web view
      const picked = await dialog.open({
        multiple: false,
        filters: [{ name: "Images", extensions: ["png", "jpg", "jpeg", "gif", "webp", "heic", "bmp", "tiff"] }],
      });
      const path = Array.isArray(picked) ? picked[0] : picked;
      if (!path) return;
      try {
        const url = await invoke("read_image_data_url", { path });
        change(null, () => (bg.image = url), { panel: true, track: false });
        hooks.syncBgImage?.();
      } catch (err) {
        alert("Could not open that image: " + err);
      }
    };
    p.append(b);
  }
  if (bg.kind !== "color") p.append(slider("Background blur", 0, 40, 1, () => bg.blur, (v) => (bg.blur = v), {}));
  p.append(slider("Padding", 0, 0.3, 0.005, () => st.paddingRatio, (v) => (st.paddingRatio = v), { fmt: (v) => Math.round(v * 100) + "%" }));
  p.append(slider("Screen Corners", 0, 60, 1, () => st.cornerRadius, (v) => (st.cornerRadius = v)));
  p.append(slider("Shadow", 0, 1, 0.01, () => st.shadow.intensity, (v) => (st.shadow.intensity = v), { fmt: (v) => Math.round(v * 100) + "%" }));
  p.append(segmented("Shadow style", [[false, "Classic"], [true, "Directional"]], () => st.shadow.directional, (v) => (st.shadow.directional = v), { track: false }));
  p.append(slider("Shadow distance", 0, 80, 1, () => st.shadow.distance, (v) => (st.shadow.distance = v)));
  p.append(slider("Shadow blur", 0, 60, 1, () => st.shadow.blur, (v) => (st.shadow.blur = v)));
  p.append(slider("Shadow angle", 0, 360, 1, () => st.shadow.angle, (v) => (st.shadow.angle = v), { fmt: (v) => v + "°" }));

  // watermark: a logo over the whole video
  const wm = P().config.watermark;
  p.append(h3("Watermark"));
  const choose = el(`<button class="btn-line">${wm.image ? "Change image…" : "Choose image…"}</button>`);
  choose.onclick = async () => {
    const picked = await dialog.open({
      multiple: false,
      filters: [{ name: "Images", extensions: ["png", "jpg", "jpeg", "gif", "webp", "heic", "bmp", "tiff", "svg"] }],
    });
    const path = Array.isArray(picked) ? picked[0] : picked;
    if (!path) return;
    try {
      const url = await downscale(await invoke("read_image_data_url", { path }), 1024);
      change(null, () => { wm.image = url; wm.enabled = true; }, { panel: true, track: false });
      hooks.syncWatermark?.();
    } catch (err) {
      alert("Could not open that image: " + err);
    }
  };
  p.append(choose);
  if (wm.image) {
    p.append(toggle("Show watermark", null, () => wm.enabled, (v) => { wm.enabled = v; }, { track: false }));
    const pos = el('<div class="field"><label><span>Position</span></label><div class="pos-grid"></div></div>');
    const grid = pos.querySelector(".pos-grid");
    for (const [x, y, name] of [[0, 0, "↖"], [0.5, 0, "↑"], [1, 0, "↗"], [0, 0.5, "←"], [0.5, 0.5, "•"], [1, 0.5, "→"], [0, 1, "↙"], [0.5, 1, "↓"], [1, 1, "↘"]]) {
      const on = Math.abs(wm.x - x) < 0.01 && Math.abs(wm.y - y) < 0.01;
      const b = el(`<button class="${on ? "on" : ""}">${name}</button>`);
      b.onclick = () => change(null, () => { wm.x = x; wm.y = y; }, { panel: true, track: false });
      grid.appendChild(b);
    }
    p.append(pos);
    p.append(slider("Size", 0.03, 0.6, 0.01, () => wm.size, (v) => (wm.size = v), { fmt: (v) => Math.round(v * 100) + "%", track: false, key: "wm-size" }));
    p.append(slider("Opacity", 0.05, 1, 0.01, () => wm.opacity, (v) => (wm.opacity = v), { fmt: (v) => Math.round(v * 100) + "%", track: false, key: "wm-op" }));
    const rm = el('<button class="btn-line danger">Remove watermark</button>');
    rm.onclick = () => {
      change(null, () => { wm.image = ""; }, { panel: true, track: false });
      hooks.syncWatermark?.();
    };
    p.append(rm);
  }
}

/** Shrink a data URL image to at most `max` px wide (keeps the project file small). */
function downscale(url, max) {
  return new Promise((res) => {
    const img = new Image();
    img.onload = () => {
      const k = Math.min(1, max / (img.naturalWidth || max));
      if (k >= 1) return res(url);
      const c = document.createElement("canvas");
      c.width = Math.round(img.naturalWidth * k);
      c.height = Math.round(img.naturalHeight * k);
      c.getContext("2d").drawImage(img, 0, 0, c.width, c.height);
      res(c.toDataURL("image/png"));
    };
    img.onerror = () => res(url);
    img.src = url;
  });
}
