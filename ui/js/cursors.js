// Vector cursors drawn on the canvas. Index order matches CursorKind::ALL
// in the Rust core: arrow, iBeam, pointer, crosshair, openHand, closedHand,
// resizeLeftRight, resizeUpDown, notAllowed.

const ARROW = [[0, 0], [0, 0.86], [0.19, 0.69], [0.31, 0.97], [0.43, 0.92], [0.31, 0.64], [0.56, 0.64]];

function poly(ctx, pts, s) {
  ctx.beginPath();
  pts.forEach(([x, y], i) => (i ? ctx.lineTo(x * s, y * s) : ctx.moveTo(x * s, y * s)));
  ctx.closePath();
}

const styles = {
  macos: { fill: "#111", stroke: "#fff" },
  light: { fill: "#fff", stroke: "#111" },
  accent: { fill: "#5b4cf5", stroke: "#fff" },
};

/**
 * Draw a cursor whose hotspot is at (x, y); `h` is the cursor height in px.
 */
export function drawCursor(ctx, kindIndex, x, y, h, style = "macos", opts = {}) {
  const { alpha = 1, scale = 1, rotation = 0 } = opts;
  if (alpha <= 0.001) return;
  const st = styles[style] || styles.macos;
  ctx.save();
  ctx.globalAlpha *= alpha;
  ctx.translate(x, y);
  ctx.rotate(rotation);
  ctx.scale(scale, scale);
  ctx.shadowColor = "rgba(0,0,0,.35)";
  ctx.shadowBlur = h * 0.18;
  ctx.shadowOffsetY = h * 0.06;
  ctx.fillStyle = st.fill;
  ctx.strokeStyle = st.stroke;
  ctx.lineWidth = Math.max(1, h * 0.065);
  ctx.lineJoin = "round";
  ctx.lineCap = "round";
  const s = h;

  if (style === "dot") {
    ctx.beginPath();
    ctx.arc(0, 0, h * 0.28, 0, Math.PI * 2);
    ctx.fillStyle = "rgba(255,255,255,.9)";
    ctx.fill();
    ctx.strokeStyle = "rgba(0,0,0,.5)";
    ctx.stroke();
    ctx.restore();
    return;
  }

  switch (kindIndex) {
    case 1: { // I-beam
      ctx.translate(0, -s * 0.5);
      const w = s * 0.18;
      ctx.beginPath();
      ctx.moveTo(-w, 0); ctx.lineTo(w, 0);
      ctx.moveTo(0, 0); ctx.lineTo(0, s);
      ctx.moveTo(-w, s); ctx.lineTo(w, s);
      ctx.lineWidth = Math.max(2, h * 0.16);
      ctx.strokeStyle = st.stroke;
      ctx.stroke();
      ctx.lineWidth = Math.max(1, h * 0.08);
      ctx.strokeStyle = st.fill;
      ctx.stroke();
      break;
    }
    case 2: case 4: case 5: { // pointing / open / closed hand
      ctx.translate(-s * 0.2, -s * 0.05);
      ctx.beginPath();
      const f = kindIndex === 2;
      if (f) {
        ctx.moveTo(0.18 * s, 0); ctx.quadraticCurveTo(0.26 * s, -0.02 * s, 0.26 * s, 0.1 * s);
        ctx.lineTo(0.26 * s, 0.38 * s);
        ctx.lineTo(0.62 * s, 0.42 * s);
        ctx.quadraticCurveTo(0.78 * s, 0.46 * s, 0.76 * s, 0.62 * s);
        ctx.lineTo(0.7 * s, 0.9 * s);
        ctx.lineTo(0.22 * s, 0.9 * s);
        ctx.lineTo(0.0 * s, 0.56 * s);
        ctx.quadraticCurveTo(0.0 * s, 0.48 * s, 0.1 * s, 0.5 * s);
        ctx.lineTo(0.18 * s, 0.6 * s);
        ctx.lineTo(0.1 * s, 0.1 * s);
        ctx.quadraticCurveTo(0.1 * s, -0.02 * s, 0.18 * s, 0);
      } else {
        ctx.roundRect(0.05 * s, 0.25 * s, 0.7 * s, 0.6 * s, 0.2 * s);
      }
      ctx.closePath();
      ctx.fillStyle = "#fff";
      ctx.strokeStyle = "#111";
      ctx.fill();
      ctx.stroke();
      break;
    }
    case 3: { // crosshair
      const r = s * 0.45;
      ctx.beginPath();
      ctx.moveTo(-r, 0); ctx.lineTo(r, 0); ctx.moveTo(0, -r); ctx.lineTo(0, r);
      ctx.lineWidth = Math.max(2, h * 0.14);
      ctx.strokeStyle = st.stroke; ctx.stroke();
      ctx.lineWidth = Math.max(1, h * 0.06);
      ctx.strokeStyle = st.fill; ctx.stroke();
      break;
    }
    case 6: case 7: { // resize
      ctx.rotate(kindIndex === 7 ? Math.PI / 2 : 0);
      const r = s * 0.5, a = s * 0.18;
      ctx.beginPath();
      ctx.moveTo(-r, 0); ctx.lineTo(-r + a, -a); ctx.lineTo(-r + a, a); ctx.closePath();
      ctx.moveTo(r, 0); ctx.lineTo(r - a, -a); ctx.lineTo(r - a, a); ctx.closePath();
      ctx.moveTo(-r + a, 0); ctx.lineTo(r - a, 0);
      ctx.stroke();
      ctx.fill();
      break;
    }
    case 8: { // not allowed
      ctx.beginPath();
      ctx.arc(0, 0, s * 0.4, 0, Math.PI * 2);
      ctx.moveTo(-s * 0.28, s * 0.28); ctx.lineTo(s * 0.28, -s * 0.28);
      ctx.lineWidth = Math.max(2, h * 0.14);
      ctx.strokeStyle = "#e5484d";
      ctx.stroke();
      break;
    }
    default: { // arrow
      poly(ctx, ARROW, s);
      ctx.stroke();
      ctx.fill();
    }
  }
  ctx.restore();
}
