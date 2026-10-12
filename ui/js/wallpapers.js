// Procedural wallpapers: a base gradient plus soft colour blobs, all
// generated at draw time so ScreenCraft ships no image assets.

const SPECS = {
  sunset:   { base: ["#2b1055", "#7597de"], blobs: [["#ff7e5f", .2, .85, .7], ["#feb47b", .75, .8, .6], ["#ff4e8a", .5, .2, .5]] },
  aurora:   { base: ["#0b1026", "#1d1b4b"], blobs: [["#00e0b0", .2, .25, .55], ["#7a5cff", .8, .3, .6], ["#ff6ad5", .55, .9, .5]] },
  ocean:    { base: ["#021b3a", "#0a4a8a"], blobs: [["#00c6ff", .15, .2, .6], ["#0072ff", .85, .7, .7], ["#4be1ec", .5, .95, .4]] },
  forest:   { base: ["#07201a", "#1c4a38"], blobs: [["#6fe3a0", .25, .3, .5], ["#2f9e6b", .8, .75, .7], ["#c9f27a", .6, .1, .35]] },
  midnight: { base: ["#05060d", "#15172e"], blobs: [["#3a3fbf", .2, .8, .6], ["#8a2be2", .85, .2, .5], ["#1f6feb", .6, .6, .4]] },
  candy:    { base: ["#ff9a9e", "#fad0c4"], blobs: [["#a18cd1", .2, .2, .7], ["#fbc2eb", .8, .8, .7], ["#ffd1ff", .6, .3, .5]] },
  dune:     { base: ["#4a2c1a", "#c98f5a"], blobs: [["#ffd29d", .8, .2, .6], ["#8c4a2f", .2, .8, .7], ["#ffb86b", .55, .6, .4]] },
  slate:    { base: ["#1b1f2a", "#3a4254"], blobs: [["#6b7a99", .2, .3, .6], ["#2a3140", .8, .8, .7], ["#9fb0d0", .7, .15, .35]] },
  lavender: { base: ["#2d1b69", "#b49cff"], blobs: [["#f093fb", .8, .25, .6], ["#4facfe", .2, .75, .6], ["#ffffff", .55, .5, .25]] },
  ember:    { base: ["#1a0505", "#6a1b0a"], blobs: [["#ff512f", .25, .75, .7], ["#f09819", .8, .35, .55], ["#dd2476", .5, .1, .5]] },
  mint:     { base: ["#0f3d3e", "#7fe0c3"], blobs: [["#c1ffd7", .75, .2, .55], ["#00b09b", .2, .8, .7], ["#96c93d", .6, .6, .35]] },
  rose:     { base: ["#3a0f2a", "#e8668f"], blobs: [["#ffb3c6", .8, .3, .6], ["#8e2de2", .15, .7, .6], ["#ff758f", .5, .95, .5]] },
};
export const WALLPAPERS = Object.keys(SPECS);

export function drawWallpaper(ctx, W, H, name) {
  const spec = SPECS[name] || SPECS.aurora;
  const g = ctx.createLinearGradient(0, 0, W, H);
  g.addColorStop(0, spec.base[0]);
  g.addColorStop(1, spec.base[1]);
  ctx.fillStyle = g;
  ctx.fillRect(0, 0, W, H);
  const m = Math.max(W, H);
  for (const [color, x, y, r] of spec.blobs) {
    const rg = ctx.createRadialGradient(x * W, y * H, 0, x * W, y * H, r * m);
    rg.addColorStop(0, color + "e6");
    rg.addColorStop(1, color + "00");
    ctx.fillStyle = rg;
    ctx.fillRect(0, 0, W, H);
  }
}

export function drawBackground(ctx, W, H, bg, imageEl) {
  switch (bg.kind) {
    case "color":
      ctx.fillStyle = bg.color;
      ctx.fillRect(0, 0, W, H);
      break;
    case "gradient": {
      const a = ((bg.gradientAngle ?? 135) * Math.PI) / 180;
      const dx = Math.sin(a), dy = -Math.cos(a);
      const len = Math.abs(W * dx) + Math.abs(H * dy);
      const cx = W / 2, cy = H / 2;
      const g = ctx.createLinearGradient(cx - (dx * len) / 2, cy - (dy * len) / 2, cx + (dx * len) / 2, cy + (dy * len) / 2);
      g.addColorStop(0, bg.gradientFrom);
      g.addColorStop(1, bg.gradientTo);
      ctx.fillStyle = g;
      ctx.fillRect(0, 0, W, H);
      break;
    }
    case "image":
      if (imageEl && imageEl.naturalWidth) {
        const s = Math.max(W / imageEl.naturalWidth, H / imageEl.naturalHeight);
        const w = imageEl.naturalWidth * s, h = imageEl.naturalHeight * s;
        ctx.drawImage(imageEl, (W - w) / 2, (H - h) / 2, w, h);
        break;
      }
      ctx.fillStyle = "#111";
      ctx.fillRect(0, 0, W, H);
      break;
    default:
      drawWallpaper(ctx, W, H, bg.wallpaper);
  }
}
