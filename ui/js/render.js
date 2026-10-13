import { drawBackground } from "./wallpapers.js";
import { drawCursor } from "./cursors.js";
import { drawAnnotations, drawKeys, applyTransition } from "./overlays.js";

export const ASPECTS = {
  "16:9": 16 / 9, "9:16": 9 / 16, "1:1": 1, "4:3": 4 / 3, "3:4": 3 / 4, "4:5": 4 / 5, "21:9": 21 / 9,
};

const even = (n) => Math.max(2, Math.round(n / 2) * 2);

export function contentAspect(p) {
  const c = p.config.crop;
  return (c.width * p.media.width) / (c.height * p.media.height);
}

/** Output pixel size for a given "resolution" (length of the shorter side). */
export function outputSize(p, resolution) {
  const a = p.config.output.aspectRatio === "auto" ? contentAspect(p) : ASPECTS[p.config.output.aspectRatio] || 16 / 9;
  return a >= 1
    ? { W: even(resolution * a), H: even(resolution) }
    : { W: even(resolution), H: even(resolution / a) };
}

export function contentRect(p, W, H) {
  const pad = p.config.styles.paddingRatio * Math.min(W, H);
  const aw = W - 2 * pad, ah = H - 2 * pad;
  const a = contentAspect(p);
  let w = aw, h = aw / a;
  if (h > ah) { h = ah; w = ah * a; }
  return { x: (W - w) / 2, y: (H - h) / 2, w, h };
}

/** Approximate gaussian blur: shrink in halves, then grow back with smoothing. */
function blurInto(dst, srcCanvas, radius, W, H) {
  const levels = Math.max(1, Math.min(6, Math.round(Math.log2(Math.max(2, radius * 0.9)))));
  let cur = srcCanvas;
  const sizes = [[W, H]];
  for (let i = 1; i <= levels; i++) sizes.push([Math.max(2, Math.round(W / 2 ** i)), Math.max(2, Math.round(H / 2 ** i))]);
  for (let i = 1; i <= levels; i++) {
    const c = document.createElement("canvas");
    c.width = sizes[i][0];
    c.height = sizes[i][1];
    const g = c.getContext("2d");
    g.imageSmoothingQuality = "high";
    g.drawImage(cur, 0, 0, c.width, c.height);
    cur = c;
  }
  for (let i = levels - 1; i >= 0; i--) {
    const c = document.createElement("canvas");
    c.width = sizes[i][0];
    c.height = sizes[i][1];
    const g = c.getContext("2d");
    g.imageSmoothingQuality = "high";
    g.drawImage(cur, 0, 0, c.width, c.height);
    cur = c;
  }
  dst.drawImage(cur, 0, 0, W, H);
}

function rr(ctx, x, y, w, h, r) {
  ctx.beginPath();
  ctx.roundRect(x, y, w, h, Math.min(r, w / 2, h / 2));
}

function camScale(p, zoom) {
  const maxZ = Math.max(1.0001, ...p.scene.zooms.filter((z) => !z.isDisabled).map((z) => z.zoom));
  const t = Math.min(1, Math.max(0, (zoom - 1) / (maxZ - 1)));
  const f = t * t * (3 - 2 * t); // smooth, follows the eased zoom so the bubble shrinks gradually
  return 1 + (p.config.camera.zoomedScale - 1) * f;
}

/**
 * Background + drop shadow only change when the style or output size does, so
 * they are rendered once into an offscreen layer and blitted every frame.
 */
function staticLayer(p, W, H, src, shadowRect) {
  const st = p.config.styles;
  const bg = st.background;
  const sh = shadowRect ? [shadowRect.x, shadowRect.y, shadowRect.w, shadowRect.h].map((v) => Math.round(v)) : null;
  const key = JSON.stringify([W, H, contentAspect(p), st.paddingRatio, st.cornerRadius, st.shadow, sh, { ...bg, image: bg.image ? bg.image.length : 0 }]);
  const cache = (src.cache ??= {});
  if (cache.key === key && cache.canvas) return cache.canvas;
  const c = cache.canvas && cache.canvas.width === W && cache.canvas.height === H ? cache.canvas : document.createElement("canvas");
  c.width = W;
  c.height = H;
  const ctx = c.getContext("2d");
  const s = H / 1080;
  if (bg.blur > 0) {
    // WebKit canvases ignore ctx.filter, so blur by repeated down/up-sampling
    const tmp = document.createElement("canvas");
    tmp.width = W;
    tmp.height = H;
    drawBackground(tmp.getContext("2d"), W, H, bg, src.bgImage);
    blurInto(ctx, tmp, bg.blur * s, W, H);
  } else {
    drawBackground(ctx, W, H, bg, src.bgImage);
  }
  if (st.shadow.intensity > 0) {
    const r = shadowRect || contentRect(p, W, H);
    ctx.save();
    const ang = (st.shadow.angle * Math.PI) / 180;
    const d = st.shadow.distance * s;
    ctx.shadowColor = `rgba(0,0,0,${Math.min(1, st.shadow.intensity * 1.6)})`;
    ctx.shadowBlur = st.shadow.blur * 2 * s;
    ctx.shadowOffsetX = st.shadow.directional ? Math.cos(ang) * d * 0.5 : 0;
    ctx.shadowOffsetY = st.shadow.directional ? Math.sin(ang) * d * 0.5 : d * 0.3;
    ctx.fillStyle = "#000";
    rr(ctx, r.x, r.y, r.w, r.h, st.cornerRadius * s);
    ctx.fill();
    ctx.restore();
  }
  cache.key = key;
  cache.canvas = c;
  return c;
}

const lerp = (a, b, t) => a + (b - a) * t;
const easeK = (t) => t * t * (3 - 2 * t);

/** The camera bubble's box for the project's camera settings (`mul` scales its size). */
function bubbleBox(p, W, H, r, zoom, mul = 1) {
  const cc = p.config.camera;
  const s = H / 1080;
  const a = cc.aspectRatio || 1;
  let base = cc.sizeRatio * Math.min(W, H) * camScale(p, zoom) * mul;
  base = Math.min(base, Math.min(W, H) * 0.8);
  const w = a >= 1 ? base * a : base;
  const h = a >= 1 ? base : base / a;
  const pad = cc.paddingRatio * 0.08 * Math.min(W, H) + 8 * s;
  return {
    x: pad + cc.position.x * (W - w - 2 * pad),
    y: pad + cc.position.y * (H - h - 2 * pad),
    w, h,
    rad: cc.cornerRadiusRatio * Math.min(w, h),
  };
}

/**
 * Where the screen and camera go for a camera layout id:
 * 0 bubble, 1 hidden, 2 large, 3 camera only, 4 side by side (camera right), 5 (camera left).
 */
function layoutGeom(id, p, W, H, r, zoom, hasCam) {
  const s = H / 1080;
  const rad = p.config.styles.cornerRadius * s;
  if (!hasCam || id === 0 || id === 1) {
    const b = bubbleBox(p, W, H, r, zoom);
    return { sr: r, sa: 1, cr: b, ca: hasCam && id === 0 && !p.config.camera.hide ? 1 : 0, rad: b.rad };
  }
  if (id === 2) {
    const b = bubbleBox(p, W, H, r, zoom, 1.7);
    return { sr: r, sa: 1, cr: b, ca: p.config.camera.hide ? 0 : 1, rad: b.rad };
  }
  if (id === 3) return { sr: r, sa: 0, cr: { x: r.x, y: r.y, w: r.w, h: r.h }, ca: 1, rad };
  const gap = r.w * 0.025;
  const sw = r.w * 0.64, cw = r.w - sw - gap;
  const asp = contentAspect(p);
  const fw = Math.min(sw, r.h * asp), fh = fw / asp;
  const left = id === 5;
  const screenX = left ? r.x + cw + gap : r.x;
  const sr = { x: screenX + (sw - fw) / 2, y: r.y + (r.h - fh) / 2, w: fw, h: fh };
  const cr = { x: left ? r.x : r.x + sw + gap, y: r.y, w: cw, h: r.h };
  return { sr, sa: 1, cr, ca: 1, rad };
}

function mixGeom(a, b, t) {
  const m = (u, v) => ({ x: lerp(u.x, v.x, t), y: lerp(u.y, v.y, t), w: lerp(u.w, v.w, t), h: lerp(u.h, v.h, t) });
  return { sr: m(a.sr, b.sr), sa: lerp(a.sa, b.sa, t), cr: m(a.cr, b.cr), ca: lerp(a.ca, b.ca, t), rad: lerp(a.rad, b.rad, t) };
}

/**
 * Paint one output frame.
 * `cam` = { zoom, cx, cy, mx, my, kind, cscale, calpha, crot, tSrc, transKind, transP, layCur, layPrev, layMix }.
 * `ctx` may be any 2D context sized W×H. `src.combos` carries the shortcut marks and
 * `src.bounds` (a Map) collects annotation boxes for the editor's handles.
 */
export function paintFrame(ctx, W, H, p, cam, tOutMs, clicks, src) {
  const s = H / 1080;
  const st = p.config.styles;
  const r = contentRect(p, W, H);
  const radius = st.cornerRadius * s;

  // camera layout (screen rectangle and camera box), eased between layouts
  const v = src.screen;
  const vW = v && (v.videoWidth || v.width), vH = v && (v.videoHeight || v.height);
  const cv = src.camera;
  const cW = cv && (cv.videoWidth || cv.width), cH = cv && (cv.videoHeight || cv.height);
  const hasCam = !!(cv && cW);
  const gCur = layoutGeom(cam.layCur | 0, p, W, H, r, cam.zoom, hasCam);
  const mixT = cam.layMix == null ? 1 : easeK(Math.max(0, Math.min(1, cam.layMix)));
  const geo = mixT >= 1 || (cam.layPrev | 0) === (cam.layCur | 0)
    ? gCur
    : mixGeom(layoutGeom(cam.layPrev | 0, p, W, H, r, cam.zoom, hasCam), gCur, mixT);
  const sr = geo.sr;

  // background + shadow (cached layer; the shadow follows the screen's rectangle)
  ctx.drawImage(staticLayer(p, W, H, src, gCur.sr === r ? null : gCur.sr), 0, 0);
  src.bounds?.clear();

  // screen
  const crop = p.config.crop;
  const vwN = crop.width / cam.zoom, vhN = crop.height / cam.zoom;
  const vx = cam.cx - vwN / 2, vy = cam.cy - vhN / 2;
  const toX = (nx) => sr.x + ((nx - vx) / vwN) * sr.w;
  const toY = (ny) => sr.y + ((ny - vy) / vhN) * sr.h;
  if (src.geom) Object.assign(src.geom, { sr: { ...sr }, vwN, vhN, W, H });
  if (geo.sa > 0.01) {
    ctx.save();
    ctx.globalAlpha *= geo.sa;
    rr(ctx, sr.x, sr.y, sr.w, sr.h, radius);
    ctx.clip();
    ctx.fillStyle = "#000";
    ctx.fillRect(sr.x, sr.y, sr.w, sr.h);
    if (v && vW) {
      ctx.imageSmoothingQuality = "high";
      // Keep the source rectangle inside the image. While the camera is pushed against the
      // edge during a zoom, rounding can leave it a hair outside, and WebKit then skips the
      // whole draw (a black frame). Clamp it and shrink the destination to match.
      const sx = vx * vW, sy = vy * vH, sw = vwN * vW, sh = vhN * vH;
      const x0 = Math.max(0, sx), y0 = Math.max(0, sy);
      const x1 = Math.min(vW, sx + sw), y1 = Math.min(vH, sy + sh);
      if (x1 - x0 > 0.5 && y1 - y0 > 0.5) {
        ctx.drawImage(
          v, x0, y0, x1 - x0, y1 - y0,
          sr.x + ((x0 - sx) / sw) * sr.w, sr.y + ((y0 - sy) / sh) * sr.h,
          ((x1 - x0) / sw) * sr.w, ((y1 - y0) / sh) * sr.h,
        );
      }
    }

    // annotations pinned to the recording (blur, spotlight, arrows, boxes ...)
    const g = {
      p, tSrc: cam.tSrc ?? 0, W, H, s, zoomK: cam.zoom * (sr.w / r.w),
      toX, toY, wPx: (n) => (n / vwN) * sr.w, hPx: (n) => (n / vhN) * sr.h,
      img: v, imgW: vW, imgH: vH, bounds: src.bounds, cache: (src.cache ??= {}),
      clipX: sr.x, clipY: sr.y, clipW: sr.w, clipH: sr.h,
    };
    drawAnnotations(ctx, g, "content");

    // click effects + cursor live inside the clipped screen
    const ptsToPx = (sr.h / (crop.height * p.media.bounds.height)) * cam.zoom;
    const cfg = p.config.cursor;
    if (cfg.clickEffect !== "none") {
      for (const c of clicks) {
        const age = tOutMs - c.tOutMs;
        if (age < 0 || age > 650) continue;
        const k = age / 650;
        const x = toX(c.x), y = toY(c.y);
        ctx.save();
        ctx.strokeStyle = cfg.clickEffectColor;
        ctx.fillStyle = cfg.clickEffectColor;
        if (cfg.clickEffect === "ripple") {
          for (let i = 0; i < 2; i++) {
            const kk = Math.max(0, k - i * 0.18);
            if (kk <= 0) continue;
            ctx.globalAlpha = (1 - kk) * 0.7;
            ctx.lineWidth = 3 * s * ptsToPx / 1.5;
            ctx.beginPath();
            ctx.arc(x, y, (8 + 46 * kk) * ptsToPx, 0, Math.PI * 2);
            ctx.stroke();
          }
        } else {
          ctx.globalAlpha = (1 - k) * 0.5;
          ctx.beginPath();
          ctx.arc(x, y, (26 - 14 * k) * ptsToPx, 0, Math.PI * 2);
          ctx.fill();
        }
        ctx.restore();
      }
    }
    if (!cfg.hide && cam.calpha > 0.01) {
      drawCursor(ctx, cam.kind, toX(cam.mx), toY(cam.my), cfg.size * ptsToPx, cfg.style, {
        alpha: cam.calpha,
        scale: cam.cscale,
        rotation: cam.crot,
      });
    }
    ctx.restore();
  }

  // camera
  const cc = p.config.camera;
  if (hasCam && geo.ca > 0.01) {
    const { cr } = geo;
    const x = cr.x, y = cr.y, cw = cr.w, ch = cr.h;
    ctx.save();
    ctx.globalAlpha *= geo.ca;
    if (cc.shadow) {
      ctx.shadowColor = "rgba(0,0,0,.45)";
      ctx.shadowBlur = 28 * s;
      ctx.shadowOffsetY = 8 * s;
    }
    ctx.fillStyle = "#000";
    rr(ctx, x, y, cw, ch, geo.rad);
    ctx.fill();
    ctx.restore();
    ctx.save();
    ctx.globalAlpha *= geo.ca;
    rr(ctx, x, y, cw, ch, geo.rad);
    ctx.clip();
    const a = cw / ch;
    const va = cW / cH;
    let sw = cW, sh = cH;
    if (va > a) sw = sh * a; else sh = sw / a;
    const sx = (cW - sw) / 2, sy = (cH - sh) / 2;
    if (cc.mirror) {
      ctx.translate(x + cw, y);
      ctx.scale(-1, 1);
      ctx.drawImage(cv, sx, sy, sw, sh, 0, 0, cw, ch);
    } else {
      ctx.drawImage(cv, sx, sy, sw, sh, x, y, cw, ch);
    }
    ctx.restore();
    if (cc.border) {
      ctx.save();
      ctx.globalAlpha *= geo.ca;
      ctx.lineWidth = 3 * s;
      ctx.strokeStyle = cc.borderColor || "#ffffff";
      rr(ctx, x, y, cw, ch, geo.rad);
      ctx.stroke();
      ctx.restore();
    }
  }

  // titles / callouts fixed on the frame, then shortcuts, then the cut transition
  drawAnnotations(ctx, { p, tSrc: cam.tSrc ?? 0, W, H, s, zoomK: 1, bounds: src.bounds }, "screen");
  drawKeys(ctx, W, H, p.config.keystrokes, src.combos, tOutMs, s);
  if (cam.transKind) {
    applyTransition(ctx, W, H, cam.transKind, cam.transP, (src.cache ??= {}), () =>
      ctx.drawImage(staticLayer(p, W, H, src, gCur.sr === r ? null : gCur.sr), 0, 0));
  }
}

/** Camera/cursor state at output frame `fi` from the Rust render track. */
export function camAt(track, fi) {
  const i = Math.max(0, Math.min(track.count - 1, fi));
  return {
    zoom: track.zoom[i], cx: track.cx[i], cy: track.cy[i], mx: track.mx[i], my: track.my[i],
    kind: track.cursorKind[i], cscale: track.cursorScale[i], calpha: track.cursorAlpha[i], crot: track.cursorRotation[i],
    tSrc: track.tSrc[i],
    transKind: track.transKind ? track.transKind[i] : 0,
    transP: track.transP ? track.transP[i] : 0,
    layCur: track.layoutCur ? track.layoutCur[i] : 0,
    layPrev: track.layoutPrev ? track.layoutPrev[i] : 0,
    layMix: track.layoutMix ? track.layoutMix[i] : 1,
  };
}

export function lerpCam(a, b, f) {
  const l = (x, y) => x + (y - x) * f;
  return { ...a, zoom: l(a.zoom, b.zoom), cx: l(a.cx, b.cx), cy: l(a.cy, b.cy), mx: l(a.mx, b.mx), my: l(a.my, b.my) };
}
