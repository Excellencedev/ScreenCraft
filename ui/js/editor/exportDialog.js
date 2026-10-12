import { invoke, dialog } from "../tauri.js";
import { $, P, dir } from "./state.js";
import { pause } from "./playback.js";
import { exportMp4, exportGif } from "../export.js";

const exp = { format: "mp4", res: 1080, fps: 60, blur: 1, codec: "avc", quality: 55, running: false, cancel: false };

function bindSeg(id, key, conv = (v) => v) {
  const seg = $(id);
  seg.querySelectorAll("button").forEach((b) => {
    b.onclick = () => {
      seg.querySelectorAll("button").forEach((x) => x.classList.remove("on"));
      b.classList.add("on");
      exp[key] = conv(b.dataset.v);
    };
  });
}

export function openExport() {
  $("modal").hidden = false;
}

function close() {
  if (exp.running) {
    exp.cancel = true;
    return;
  }
  $("modal").hidden = true;
  $("expProgress").hidden = true;
  $("expBar").style.width = "0";
}

async function run() {
  if (exp.running) return;
  const ext = exp.format;
  const base = (P().name || "ScreenCraft").replace(/[\\/:]/g, "-");
  const path = await dialog.save({ defaultPath: `${base}.${ext}`, filters: [{ name: ext.toUpperCase(), extensions: [ext] }] });
  if (!path) return;
  exp.running = true;
  exp.cancel = false;
  pause();
  $("expProgress").hidden = false;
  $("expGo").disabled = true;
  const onProgress = (f, text) => {
    $("expBar").style.width = Math.round(f * 100) + "%";
    $("expText").textContent = text;
  };
  try {
    const common = { project: JSON.parse(JSON.stringify(P())), dir, path, resolution: exp.res, onProgress, shouldCancel: () => exp.cancel };
    if (ext === "gif") await exportGif(common);
    else await exportMp4({ ...common, fps: exp.fps, motionBlur: exp.blur ? 0.8 : 0, codec: exp.codec, quality: exp.quality });
    $("expText").textContent = "Saved! Revealing in Finder…";
    invoke("reveal_in_finder", { path });
    setTimeout(close, 900);
  } catch (e) {
    $("expText").textContent = exp.cancel ? "Cancelled" : "Export failed: " + (e?.message || e);
  } finally {
    exp.running = false;
    $("expGo").disabled = false;
  }
}

export function bindExport() {
  $("expCancel").onclick = close;
  $("expGo").onclick = run;
  bindSeg("expFormat", "format");
  bindSeg("expRes", "res", (v) => parseInt(v, 10));
  bindSeg("expFps", "fps", (v) => parseInt(v, 10));
  bindSeg("expBlur", "blur", (v) => parseInt(v, 10));
  bindSeg("expCodec", "codec");
  bindSeg("expQuality", "quality", (v) => parseInt(v, 10));
}
