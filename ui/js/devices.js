// Device frames drawn around the screen rectangle: laptop, monitor, tablet, phone, browser window.
// Everything is vector-drawn so ScreenCraft ships no image assets. Insets are fractions of the
// screen's height; `contentRect` uses them to leave room for the frame.

export const DEVICES = [
  ["none", "None"], ["laptop", "Laptop"], ["monitor", "Monitor"], ["tablet", "Tablet"], ["phone", "Phone"], ["browser", "Browser"],
];

const INSETS = {
  laptop: { side: 0.035, top: 0.05, bottom: 0.1 },
  monitor: { side: 0.028, top: 0.028, bottom: 0.17 },
  tablet: { side: 0.06, top: 0.06, bottom: 0.06 },
  phone: { side: 0.04, top: 0.05, bottom: 0.05 },
  browser: { side: 0, top: 0.07, bottom: 0 },
};
const NONE = { side: 0, top: 0, bottom: 0 };

export function deviceInsets(kind) {
  return INSETS[kind] || NONE;
}

function rr(ctx, x, y, w, h, r) {
  ctx.beginPath();
  ctx.roundRect(x, y, w, h, Math.max(0, Math.min(r, w / 2, h / 2)));
}

/** Paint the frame around the screen rectangle (x, y, w, h). `rad` is the screen's corner radius. */
export function drawDevice(ctx, kind, x, y, w, h, rad, s) {
  const k = INSETS[kind];
  if (!k) return;
  const side = k.side * h, top = k.top * h, bot = k.bottom * h;
  const X = x - side, Y = y - top, TW = w + side * 2;
  ctx.save();
  const body = (px, py, pw, ph, r, fill) => {
    ctx.fillStyle = fill;
    rr(ctx, px, py, pw, ph, r);
    ctx.fill();
    ctx.strokeStyle = "rgba(255,255,255,.14)";
    ctx.lineWidth = Math.max(1, s);
    ctx.stroke();
  };
  const metal = (py, ph) => {
    const g = ctx.createLinearGradient(0, py, 0, py + ph);
    g.addColorStop(0, "#d9dde5");
    g.addColorStop(1, "#8b93a3");
    return g;
  };
  const grad = (c0, c1, py, ph) => {
    const g = ctx.createLinearGradient(0, py, 0, py + ph);
    g.addColorStop(0, c0);
    g.addColorStop(1, c1);
    return g;
  };
  if (kind === "laptop") {
    const lidH = top + h + side;
    body(X, Y, TW, lidH, rad + side * 0.8, "#0c0e13");
    // camera dot in the top bezel
    ctx.fillStyle = "#1f2430";
    ctx.beginPath();
    ctx.arc(x + w / 2, Y + top / 2, Math.max(2, top * 0.14), 0, Math.PI * 2);
    ctx.fill();
    // base
    const by = Y + lidH, bh = bot - side;
    const lip = side * 0.9;
    ctx.fillStyle = metal(by, bh);
    ctx.beginPath();
    ctx.roundRect(X - lip, by, TW + lip * 2, bh, [0, 0, bh * 0.9, bh * 0.9]);
    ctx.fill();
    ctx.fillStyle = "rgba(0,0,0,.22)";
    rr(ctx, x + w / 2 - TW * 0.09, by, TW * 0.18, bh * 0.45, bh * 0.3);
    ctx.fill();
  } else if (kind === "monitor") {
    const bezelH = top + h + side * 1.2;
    body(X, Y, TW, bezelH + h * 0.045, rad + side * 0.8, "#0d0f14");
    // chin + stand
    const standTop = Y + bezelH + h * 0.045;
    const neckW = TW * 0.1, neckH = h * 0.06;
    ctx.fillStyle = grad("#9aa2b2", "#6f7686", standTop, neckH);
    ctx.fillRect(x + w / 2 - neckW / 2, standTop - 1, neckW, neckH + 1);
    const footW = TW * 0.28, footH = h * 0.035;
    ctx.fillStyle = metal(standTop + neckH, footH);
    rr(ctx, x + w / 2 - footW / 2, standTop + neckH, footW, footH, footH * 0.5);
    ctx.fill();
  } else if (kind === "tablet") {
    body(X, Y, TW, top + h + bot, rad + side * 0.9, "#0b0d11");
  } else if (kind === "phone") {
    body(X, Y, TW, top + h + bot, rad + side * 1.6, "#08090c");
    ctx.fillStyle = "#1a1d26";
    rr(ctx, x + w / 2 - w * 0.09, Y + top * 0.35, w * 0.18, top * 0.3, top * 0.15);
    ctx.fill();
  } else if (kind === "browser") {
    body(X, Y, TW, top + h, rad, "#1a1e29");
    const cy = Y + top / 2, r0 = top * 0.17;
    ["#ff5f57", "#febc2e", "#28c840"].forEach((c, i) => {
      ctx.fillStyle = c;
      ctx.beginPath();
      ctx.arc(X + top * 0.55 + i * r0 * 3, cy, r0, 0, Math.PI * 2);
      ctx.fill();
    });
    ctx.fillStyle = "#0e1118";
    const uw = Math.min(TW * 0.5, TW - top * 3.5);
    rr(ctx, X + TW / 2 - uw / 2, cy - top * 0.26, uw, top * 0.52, top * 0.26);
    ctx.fill();
  }
  ctx.restore();
}
