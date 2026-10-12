import { Muxer, ArrayBufferTarget } from "../vendor/mp4-muxer.mjs";
import { GIFEncoder, quantize, applyPalette } from "../vendor/gifenc.esm.js";
import { invoke, convertFileSrc } from "./tauri.js";
import { outputSize, paintFrame, camAt, lerpCam } from "./render.js";

const sleep = (ms) => new Promise((r) => setTimeout(r, ms));

function makeVideo(url) {
  const v = document.createElement("video");
  v.muted = true;
  v.preload = "auto";
  v.playsInline = true;
  v.src = url;
  return new Promise((res, rej) => {
    v.onloadeddata = () => res(v);
    v.onerror = () => rej(new Error("could not load " + url));
  });
}

function seek(v, t) {
  return new Promise((res) => {
    t = Math.max(0, Math.min(t, (v.duration || t) - 0.001));
    if (Math.abs(v.currentTime - t) < 0.0005 && v.readyState >= 2) return res();
    const h = () => { v.removeEventListener("seeked", h); res(); };
    v.addEventListener("seeked", h);
    v.currentTime = t;
  });
}

async function writeFile(path, bytes, onProgress) {
  await invoke("export_begin", { path });
  const CH = 4 * 1024 * 1024;
  for (let pos = 0; pos < bytes.byteLength; pos += CH) {
    const part = bytes.subarray(pos, Math.min(bytes.byteLength, pos + CH));
    await invoke("export_write", part, { headers: { "x-position": String(pos) } });
    onProgress?.(Math.min(1, (pos + CH) / bytes.byteLength));
  }
  await invoke("export_end");
}

/** Build the exported audio timeline (slices re-timed, volumes applied). */
async function renderAudio(project, dir, outDurSec) {
  const url = convertFileSrc(`${dir}/${project.media.screen}`);
  const bytes = await (await fetch(url)).arrayBuffer();
  const ctx = new (window.AudioContext || window.webkitAudioContext)();
  let buf;
  try {
    buf = await ctx.decodeAudioData(bytes);
  } catch {
    return null;
  } finally {
    ctx.close?.();
  }
  if (!buf || !buf.length) return null;
  const sr = 48000;
  const off = new OfflineAudioContext(2, Math.ceil(outDurSec * sr), sr);
  let at = 0;
  const master = project.config.audio.volume;
  for (const s of project.scene.slices) {
    const srcDur = (s.sourceEndMs - s.sourceStartMs) / 1000;
    const outDur = srcDur / s.timeScale;
    const node = off.createBufferSource();
    node.buffer = buf;
    node.playbackRate.value = s.timeScale;
    const g = off.createGain();
    g.gain.value = s.volume * master;
    node.connect(g).connect(off.destination);
    node.start(at, s.sourceStartMs / 1000, srcDur);
    at += outDur;
  }
  return off.startRendering();
}

async function pickCodec(W, H, fps, bitrate, want) {
  const hevc = ["hvc1.1.6.L153.B0", "hvc1.1.6.L123.B0", "hvc1.1.6.L93.B0"];
  const avc = ["avc1.640034", "avc1.64002A", "avc1.640028", "avc1.4d0028", "avc1.42001f"];
  const groups = want === "hevc" ? [["hevc", hevc], ["avc", avc]] : [["avc", avc]];
  for (const [kind, list] of groups) {
    for (const codec of list) {
      const cfg = { codec, width: W, height: H, bitrate, framerate: fps };
      try {
        const sup = await VideoEncoder.isConfigSupported(cfg);
        if (sup.supported) return { cfg, kind };
      } catch {}
    }
  }
  throw new Error("This system cannot encode video in the web view.");
}

/** Bits-per-pixel model shared with the Rust compressor. */
function qualityBitrate(W, H, fps, quality, hevc) {
  const q = Math.min(1, Math.max(0, quality / 100));
  let bpp = 0.015 + q * q * 0.2;
  if (hevc) bpp *= 0.62;
  return Math.round(Math.min(100e6, Math.max(250e3, W * H * fps * bpp)));
}

export async function exportMp4({ project, dir, path, resolution, fps, motionBlur, codec = "avc", quality = 55, onProgress, shouldCancel }) {
  if (typeof VideoEncoder === "undefined") throw new Error("WebCodecs is not available in this web view.");
  const { W, H } = outputSize(project, resolution);
  onProgress(0, "Computing camera path…");
  const track = await invoke("compute_render_track", { dir, project, fps });
  const screen = await makeVideo(convertFileSrc(`${dir}/${project.media.screen}`));
  const camera = project.media.camera ? await makeVideo(convertFileSrc(`${dir}/${project.media.camera}`)).catch(() => null) : null;

  const canvas = document.createElement("canvas");
  canvas.width = W; canvas.height = H;
  const ctx = canvas.getContext("2d", { willReadFrequently: false });
  const tmp = document.createElement("canvas");
  tmp.width = W; tmp.height = H;
  const tctx = tmp.getContext("2d");

  let bgImage = null;
  if (project.config.styles.background.kind === "image" && project.config.styles.background.image) {
    bgImage = new Image();
    bgImage.src = project.config.styles.background.image;
    await bgImage.decode().catch(() => {});
  }

  const bitrate = qualityBitrate(W, H, fps, quality, codec === "hevc");
  const { cfg: vcfg, kind: vkind } = await pickCodec(W, H, fps, bitrate, codec);

  onProgress(0.02, "Preparing audio…");
  const outDurSec = track.durationMs / 1000;
  let audio = null;
  try { audio = await renderAudio(project, dir, outDurSec); } catch { audio = null; }

  const target = new ArrayBufferTarget();
  const muxer = new Muxer({
    target,
    video: { codec: vkind, width: W, height: H, frameRate: fps },
    audio: audio ? { codec: "aac", sampleRate: audio.sampleRate, numberOfChannels: audio.numberOfChannels } : undefined,
    fastStart: "in-memory",
  });
  let encError = null;
  const venc = new VideoEncoder({
    output: (chunk, meta) => muxer.addVideoChunk(chunk, meta),
    error: (e) => (encError = e),
  });
  venc.configure(vcfg);

  const total = track.count;
  const src = { screen, camera, bgImage, cache: {} };
  const srcRatio = (i) => Math.max(0, Math.min(total - 1, i));
  for (let i = 0; i < total; i++) {
    if (shouldCancel?.()) { venc.close(); throw new Error("cancelled"); }
    if (encError) throw encError;
    const t = srcRatio(i);
    const tOut = (i * 1000) / fps;
    await seek(screen, track.tSrc[t] / 1000);
    if (camera) await seek(camera, Math.max(0, (track.tSrc[t] + project.media.cameraOffsetMs) / 1000));
    const cam = camAt(track, i);
    const prev = camAt(track, srcRatio(i - 1)), next = camAt(track, srcRatio(i + 1));
    const motion = Math.abs(next.zoom - prev.zoom) * 4 + Math.abs(next.cx - prev.cx) * 12 + Math.abs(next.cy - prev.cy) * 12;
    if (motionBlur > 0 && motion > 0.01) {
      const N = Math.min(6, 2 + Math.round(motion * 40));
      ctx.clearRect(0, 0, W, H);
      for (let k = 0; k < N; k++) {
        const f = N === 1 ? 0 : (k / (N - 1) - 0.5) * motionBlur;
        const c = f < 0 ? lerpCam(cam, prev, -f) : lerpCam(cam, next, f);
        paintFrame(tctx, W, H, project, c, tOut, track.clicks, src);
        ctx.globalAlpha = 1 / (k + 1);
        ctx.drawImage(tmp, 0, 0);
      }
      ctx.globalAlpha = 1;
    } else {
      paintFrame(ctx, W, H, project, cam, tOut, track.clicks, src);
    }
    const frame = new VideoFrame(canvas, { timestamp: Math.round((i * 1e6) / fps), duration: Math.round(1e6 / fps) });
    venc.encode(frame, { keyFrame: i % (fps * 2) === 0 });
    frame.close();
    while (venc.encodeQueueSize > 6) await sleep(2);
    if (i % 4 === 0) onProgress(0.05 + 0.85 * (i / total), `Rendering frame ${i + 1} of ${total}`);
  }
  await venc.flush();
  venc.close();

  if (audio) {
    onProgress(0.92, "Encoding audio…");
    const aenc = new AudioEncoder({
      output: (chunk, meta) => muxer.addAudioChunk(chunk, meta),
      error: (e) => (encError = e),
    });
    aenc.configure({ codec: "mp4a.40.2", sampleRate: audio.sampleRate, numberOfChannels: audio.numberOfChannels, bitrate: 160000 });
    const STEP = 4096 * 4;
    const nch = audio.numberOfChannels;
    for (let pos = 0; pos < audio.length; pos += STEP) {
      const n = Math.min(STEP, audio.length - pos);
      const data = new Float32Array(n * nch);
      for (let c = 0; c < nch; c++) data.set(audio.getChannelData(c).subarray(pos, pos + n), c * n);
      const ad = new AudioData({
        format: "f32-planar", sampleRate: audio.sampleRate, numberOfFrames: n, numberOfChannels: nch,
        timestamp: Math.round((pos / audio.sampleRate) * 1e6), data,
      });
      aenc.encode(ad);
      ad.close();
      while (aenc.encodeQueueSize > 8) await sleep(2);
    }
    await aenc.flush();
    aenc.close();
  }
  if (encError) throw encError;
  muxer.finalize();
  onProgress(0.95, "Writing file…");
  await writeFile(path, new Uint8Array(target.buffer), (f) => onProgress(0.95 + 0.05 * f, "Writing file…"));
  onProgress(1, "Done");
}

export async function exportGif({ project, dir, path, resolution, onProgress, shouldCancel }) {
  const fps = 15;
  const res = Math.min(resolution, 540);
  const { W, H } = outputSize(project, res);
  onProgress(0, "Computing camera path…");
  const track = await invoke("compute_render_track", { dir, project, fps });
  const screen = await makeVideo(convertFileSrc(`${dir}/${project.media.screen}`));
  const camera = project.media.camera ? await makeVideo(convertFileSrc(`${dir}/${project.media.camera}`)).catch(() => null) : null;
  let bgImage = null;
  if (project.config.styles.background.kind === "image" && project.config.styles.background.image) {
    bgImage = new Image();
    bgImage.src = project.config.styles.background.image;
    await bgImage.decode().catch(() => {});
  }
  const canvas = document.createElement("canvas");
  canvas.width = W; canvas.height = H;
  const ctx = canvas.getContext("2d", { willReadFrequently: true });
  const gifCache = {};
  const gif = GIFEncoder();
  const delay = Math.round(1000 / fps);
  for (let i = 0; i < track.count; i++) {
    if (shouldCancel?.()) throw new Error("cancelled");
    await seek(screen, track.tSrc[i] / 1000);
    if (camera) await seek(camera, Math.max(0, (track.tSrc[i] + project.media.cameraOffsetMs) / 1000));
    paintFrame(ctx, W, H, project, camAt(track, i), (i * 1000) / fps, track.clicks, { screen, camera, bgImage, cache: gifCache });
    const { data } = ctx.getImageData(0, 0, W, H);
    const palette = quantize(data, 256, { format: "rgb565" });
    const index = applyPalette(data, palette, "rgb565");
    gif.writeFrame(index, W, H, { palette, delay, repeat: 0 });
    if (i % 3 === 0) onProgress(0.9 * (i / track.count), `Encoding frame ${i + 1} of ${track.count}`);
    if (i % 8 === 0) await sleep(0);
  }
  gif.finish();
  onProgress(0.95, "Writing file…");
  await writeFile(path, gif.bytes());
  onProgress(1, "Done");
}
