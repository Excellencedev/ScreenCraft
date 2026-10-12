import { drawBackground } from "./wallpapers.js";
import { drawCursor } from "./cursors.js";

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
function staticLayer(p, W, H, src) {
  const st = p.config.styles;
  const bg = st.background;
  const key = JSON.stringify([W, H, contentAspect(p), st.paddingRatio, st.cornerRadius, st.shadow, { ...bg, image: bg.image ? bg.image.length : 0 }]);
  const cache = (src.cache ??= {});
  if (cache.key === key && cache.canvas) return cache.canvas;
  const c = cache.canvas && cache.canvas.width === W && cache.canvas.height === H ? cache.canvas : document.createElement("canvas");
  c.width = W;
  c.height = H;
  const ctx = c.getContext("2d");
  const s = H / 1080;
  ctx.save();
  if (bg.blur > 0 && "filter" in ctx) ctx.filter = `blur(${bg.blur * s}px)`;
  drawBackground(ctx, W, H, bg, src.bgImage);
  ctx.restore();
  if (st.shadow.intensity > 0) {
    const r = contentRect(p, W, H);
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

/**
 * Paint one output frame.
 * `cam` = { zoom, cx, cy, mx, my, kind, cscale, calpha, crot }.
 * `ctx` may be any 2D context sized W×H.
 */
export function paintFrame(ctx, W, H, p, cam, tOutMs, clicks, src) {
  const s = H / 1080;
  const st = p.config.styles;
  // background + shadow (cached layer)
  ctx.drawImage(staticLayer(p, W, H, src), 0, 0);

  const r = contentRect(p, W, H);
  const radius = st.cornerRadius * s;

  // screen
  const crop = p.config.crop;
  const vwN = crop.width / cam.zoom, vhN = crop.height / cam.zoom;
  const vx = cam.cx - vwN / 2, vy = cam.cy - vhN / 2;
  ctx.save();
  rr(ctx, r.x, r.y, r.w, r.h, radius);
  ctx.clip();
  ctx.fillStyle = "#000";
  ctx.fillRect(r.x, r.y, r.w, r.h);
  const v = src.screen;
  if (v && v.videoWidth) {
    ctx.imageSmoothingQuality = "high";
    ctx.drawImage(v, vx * v.videoWidth, vy * v.videoHeight, vwN * v.videoWidth, vhN * v.videoHeight, r.x, r.y, r.w, r.h);
  }

  // click effects + cursor live inside the clipped screen
  const ptsToPx = (r.h / (crop.height * p.media.bounds.height)) * cam.zoom;
  const toX = (nx) => r.x + ((nx - vx) / vwN) * r.w;
  const toY = (ny) => r.y + ((ny - vy) / vhN) * r.h;
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
    const kind = cfg.alwaysUsePointer ? 0 : cam.kind;
    drawCursor(ctx, kind, toX(cam.mx), toY(cam.my), cfg.size * ptsToPx, cfg.style, {
      alpha: cam.calpha,
      scale: cam.cscale,
      rotation: cam.crot,
    });
  }
  ctx.restore();

  // inset border
  if (st.inset.enabled) {
    ctx.save();
    ctx.lineWidth = st.inset.size * s;
    ctx.strokeStyle = st.inset.color;
    ctx.globalAlpha = st.inset.alpha;
    rr(ctx, r.x + ctx.lineWidth / 2, r.y + ctx.lineWidth / 2, r.w - ctx.lineWidth, r.h - ctx.lineWidth, Math.max(0, radius - ctx.lineWidth / 2));
    ctx.stroke();
    ctx.restore();
  }

  // camera bubble
  const cc = p.config.camera;
  const cv = src.camera;
  if (cv && cv.videoWidth && !cc.hide) {
    const base = cc.sizeRatio * Math.min(W, H) * camScale(p, cam.zoom);
    const a = cc.aspectRatio || 1;
    const cw = a >= 1 ? base * a : base;
    const ch = a >= 1 ? base : base / a;
    const pad = cc.paddingRatio * 0.08 * Math.min(W, H) + 8 * s;
    const x = pad + cc.position.x * (W - cw - 2 * pad);
    const y = pad + cc.position.y * (H - ch - 2 * pad);
    const cr = cc.cornerRadiusRatio * Math.min(cw, ch);
    ctx.save();
    if (cc.shadow) {
      ctx.shadowColor = "rgba(0,0,0,.45)";
      ctx.shadowBlur = 28 * s;
      ctx.shadowOffsetY = 8 * s;
    }
    ctx.fillStyle = "#000";
    rr(ctx, x, y, cw, ch, cr);
    ctx.fill();
    ctx.restore();
    ctx.save();
    rr(ctx, x, y, cw, ch, cr);
    ctx.clip();
    const va = cv.videoWidth / cv.videoHeight;
    let sw = cv.videoWidth, sh = cv.videoHeight;
    if (va > a) sw = sh * a; else sh = sw / a;
    const sx = (cv.videoWidth - sw) / 2, sy = (cv.videoHeight - sh) / 2;
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
      ctx.lineWidth = 3 * s;
      ctx.strokeStyle = "rgba(255,255,255,.85)";
      rr(ctx, x, y, cw, ch, cr);
      ctx.stroke();
      ctx.restore();
    }
  }
}

/** Camera/cursor state at output frame `fi` from the Rust render track. */
export function camAt(track, fi) {
  const i = Math.max(0, Math.min(track.count - 1, fi));
  return {
    zoom: track.zoom[i], cx: track.cx[i], cy: track.cy[i], mx: track.mx[i], my: track.my[i],
    kind: track.cursorKind[i], cscale: track.cursorScale[i], calpha: track.cursorAlpha[i], crot: track.cursorRotation[i],
  };
}

export function lerpCam(a, b, f) {
  const l = (x, y) => x + (y - x) * f;
  return { ...a, zoom: l(a.zoom, b.zoom), cx: l(a.cx, b.cx), cy: l(a.cy, b.cy), mx: l(a.mx, b.mx), my: l(a.my, b.my) };
}
