// A small, working version of the editor for the landing page. The preview is three.js: a
// fake screen is mapped onto a rounded plane (zooming slides the texture's UV window, like the
// real compositor), with a cursor, click ripple, captions, device frame and wallpaper.
// The timeline is DOM: drag and resize zoom blocks, click an empty spot to add one.
import {
  WebGLRenderer, Scene, OrthographicCamera, Mesh, PlaneGeometry, CanvasTexture, MeshBasicMaterial, SRGBColorSpace, LinearFilter,
} from 'three';

const D = 12;                                    // seconds in the demo
const WORLD_W = 16, WORLD_H = 10;
const MIN_LEN = 0.8, MAX_ZOOMS = 6;
const SCREEN_W = 1600, SCREEN_H = 900;

const WALLPAPERS = {
  aurora: ['#0b1b3a', '#2a4a8f', '#7b4bd1', '#d25aa6'],
  dusk: ['#1b1030', '#6b2c7f', '#e0566b', '#ffb36b'],
  ocean: ['#04202e', '#0b5a73', '#1aa6a1', '#9be3c8'],
  paper: ['#e9e4d8', '#f4efe3', '#dcd3c0', '#cfc4ad'],
};

const KEYS = [                                   // scripted cursor: time, x, y (0..1), click
  { t: 0, x: .56, y: .52 }, { t: 1.6, x: .15, y: .34, click: 1 }, { t: 3.4, x: .42, y: .40 },
  { t: 6.0, x: .46, y: .40 }, { t: 7.4, x: .84, y: .085, click: 1 }, { t: 9.0, x: .56, y: .78 },
  { t: 11.0, x: .70, y: .83 }, { t: 12, x: .56, y: .52 },
];
const WORDS = [['Zooms', .6], ['follow', 1.1], ['what', 1.7], ['you', 2.1], ['do.', 2.5], ['Typing', 4.5], ['is', 5.0], ['tracked', 5.3], ['too.', 5.9], ['Export', 8.4], ['in', 9.0], ['one', 9.3], ['click.', 9.7]];

const clamp = (v, a, b) => Math.min(b, Math.max(a, v));
const smooth = (a, b, v) => { const t = clamp((v - a) / (b - a), 0, 1); return t * t * (3 - 2 * t); };
const lerp = (a, b, t) => a + (b - a) * t;

function cssVar(name, fallback) {
  const v = getComputedStyle(document.documentElement).getPropertyValue(name).trim();
  return v || fallback;
}

/** The fake screen being "recorded": a code editor with a sidebar, toolbar and output panel. */
const TYPED = [['const ', '#c586c0'], ['tax', '#9cdcfe'], [' = ', '#d4d4d4'], ['total', '#9cdcfe'], [' * ', '#d4d4d4'], ['0.08', '#b5cea8'], [';', '#d4d4d4']];
const TYPED_LEN = TYPED.reduce((n, [t]) => n + t.length, 0);
const TYPE_FROM = 3.4, TYPE_TO = 5.6;

/** The first `n` characters of the line being typed, as colour runs. */
function typedRuns(n) {
  const out = []; let left = n;
  for (const [t, c] of TYPED) { if (left <= 0) break; out.push([t.slice(0, left), c]); left -= t.length; }
  return out;
}

function drawScreen(g, typed = 0, caret = false) {
  const W = SCREEN_W, H = SCREEN_H;
  g.fillStyle = '#0d1320'; g.fillRect(0, 0, W, H);
  // title bar
  g.fillStyle = '#131b2b'; g.fillRect(0, 0, W, 54);
  ['#ff5f57', '#febc2e', '#28c840'].forEach((c, i) => { g.fillStyle = c; g.beginPath(); g.arc(28 + i * 26, 27, 8, 0, 7); g.fill(); });
  g.fillStyle = '#8d9ab3'; g.font = '600 20px system-ui, sans-serif'; g.textBaseline = 'middle';
  g.fillText('checkout.ts  -  demo-app', W / 2 - 110, 28);
  // sidebar
  g.fillStyle = '#101826'; g.fillRect(0, 54, W * .2, H - 54);
  g.font = '500 22px system-ui, sans-serif';
  ['src', 'api', 'checkout.ts', 'cart.ts', 'theme.css', 'README.md'].forEach((n, i) => {
    const y = 110 + i * 56;
    if (i === 2) { g.fillStyle = '#1b2a45'; g.fillRect(0, y - 24, W * .2, 48); g.fillStyle = '#3b82c4'; g.fillRect(0, y - 24, 4, 48); }
    g.fillStyle = i === 2 ? '#e8eefc' : '#7f8ca6';
    g.fillText((i < 2 ? '▸ ' : '   ') + n, 28, y);
  });
  // toolbar + run button
  g.fillStyle = '#111a2a'; g.fillRect(W * .2, 54, W * .8, 70);
  g.fillStyle = '#3b82c4'; g.beginPath(); g.roundRect(W * .79, 68, 170, 44, 10); g.fill();
  g.fillStyle = '#fff'; g.font = '700 22px system-ui, sans-serif'; g.fillText('▶  Run', W * .79 + 44, 91);
  g.fillStyle = '#8d9ab3'; g.font = '500 20px system-ui, sans-serif'; g.fillText('main', W * .22, 91);
  // code
  const code = [
    [['const ', '#c586c0'], ['total', '#9cdcfe'], [' = ', '#d4d4d4'], ['cart', '#9cdcfe'], ['.items', '#d4d4d4']],
    [['  .map(', '#d4d4d4'], ['i', '#9cdcfe'], [' => ', '#c586c0'], ['i.price', '#9cdcfe'], [' * ', '#d4d4d4'], ['i.qty', '#9cdcfe'], [')', '#d4d4d4']],
    [['  .reduce(', '#d4d4d4'], ['(a, b)', '#9cdcfe'], [' => ', '#c586c0'], ['a + b', '#9cdcfe'], [', ', '#d4d4d4'], ['0', '#b5cea8'], [')', '#d4d4d4']],
    typedRuns(typed),
    [['export async function ', '#c586c0'], ['checkout', '#dcdcaa'], ['() {', '#d4d4d4']],
    [['  const ', '#c586c0'], ['order', '#9cdcfe'], [' = await ', '#c586c0'], ['api', '#9cdcfe'], ['.create({ total })', '#d4d4d4']],
    [['  return ', '#c586c0'], ['order', '#9cdcfe'], ['.id', '#d4d4d4']],
    [['}', '#d4d4d4']],
  ];
  g.font = '500 28px ui-monospace, Menlo, Consolas, monospace';
  code.forEach((ln, i) => {
    const y = 190 + i * 46;
    g.fillStyle = '#3a4660'; g.font = '500 22px ui-monospace, Menlo, monospace'; g.fillText(String(i + 1).padStart(2, ' '), W * .22, y);
    let x = W * .22 + 54;
    g.font = '500 28px ui-monospace, Menlo, Consolas, monospace';
    ln.forEach(([t, c]) => { g.fillStyle = c; g.fillText(t, x, y); x += g.measureText(t).width; });
    if (i === 3 && caret) { g.fillStyle = '#e8eefc'; g.fillRect(x + 2, y - 18, 3, 36); }
  });
  // output
  g.fillStyle = '#0a101b'; g.fillRect(W * .2, H * .64, W * .8, H * .36);
  g.fillStyle = '#1d2738'; g.fillRect(W * .2, H * .64, W * .8, 2);
  g.font = '600 20px system-ui, sans-serif'; g.fillStyle = '#7f8ca6'; g.fillText('OUTPUT', W * .22, H * .64 + 34);
  g.font = '500 26px ui-monospace, Menlo, Consolas, monospace';
  [['✓ built in 412 ms', '#5fd38d'], ['✓ 12 tests passed', '#5fd38d'], ['→ order #4821 created', '#8ec5ff'], ['  total: $128.40', '#d4d4d4']].forEach(([t, c], i) => {
    g.fillStyle = c; g.fillText(t, W * .22, H * .64 + 84 + i * 40);
  });
}

function roundRectTex(w, h, r, fill, blur = 0, pad = 0) {
  const c = document.createElement('canvas');
  c.width = w + pad * 2; c.height = h + pad * 2;
  const g = c.getContext('2d');
  if (blur) g.filter = `blur(${blur}px)`;
  g.fillStyle = fill;
  g.beginPath(); g.roundRect(pad, pad, w, h, r); g.fill();
  return c;
}

export function mountPlayground(root) {
  const canvas = root.querySelector('canvas');
  const $ = (s) => root.querySelector(s);
  let renderer;
  try {
    renderer = new WebGLRenderer({ canvas, antialias: true, alpha: false });
  } catch { return false; }
  renderer.setPixelRatio(Math.min(2, window.devicePixelRatio || 1));

  const scene = new Scene();
  const cam = new OrthographicCamera(-WORLD_W / 2, WORLD_W / 2, WORLD_H / 2, -WORLD_H / 2, -10, 10);
  const tex = (c) => { const t = new CanvasTexture(c); t.colorSpace = SRGBColorSpace; t.minFilter = LinearFilter; t.generateMipmaps = false; return t; };
  const mat = (o) => new MeshBasicMaterial({ toneMapped: false, transparent: true, ...o });

  // ---- state ----
  const S = {
    t: 0, playing: !matchMedia('(prefers-reduced-motion: reduce)').matches,
    bg: 'aurora', pad: 0.06, radius: 18, frame: false, captions: true, smooth: true, level: 2.2,
    zooms: [{ id: 1, a: 1.2, b: 4.2 }, { id: 2, a: 4.4, b: 7.0 }, { id: 3, a: 8.2, b: 11.0 }],
    sel: null, nextId: 4,
  };

  // ---- meshes ----
  const bgCanvas = document.createElement('canvas'); bgCanvas.width = 1280; bgCanvas.height = 800;
  const bg = new Mesh(new PlaneGeometry(WORLD_W * 1.06, WORLD_H * 1.06), mat({ map: tex(bgCanvas), transparent: false }));
  bg.position.z = -2; scene.add(bg);

  const shadow = new Mesh(new PlaneGeometry(1, 1), mat({}));
  shadow.position.z = 0; scene.add(shadow);
  const bezel = new Mesh(new PlaneGeometry(1, 1), mat({}));
  bezel.position.z = 0.1; scene.add(bezel);
  const base = new Mesh(new PlaneGeometry(1, 1), mat({}));
  base.position.z = 0.05; scene.add(base);

  const screenCanvas = document.createElement('canvas'); screenCanvas.width = SCREEN_W; screenCanvas.height = SCREEN_H;
  drawScreen(screenCanvas.getContext('2d'));
  let typedKey = '0';
  const screenTex = tex(screenCanvas);
  const maskCanvas = document.createElement('canvas');
  const maskTex = new CanvasTexture(maskCanvas);
  const win = new Mesh(new PlaneGeometry(1, 1), mat({ map: screenTex, alphaMap: maskTex, alphaTest: 0.5 }));
  win.position.z = 0.2; scene.add(win);

  const curCanvas = document.createElement('canvas'); curCanvas.width = curCanvas.height = 96;
  {
    const g = curCanvas.getContext('2d');
    g.translate(18, 8); g.scale(2.1, 2.1);
    const p = new Path2D('M0 0 L0 17 L4.4 13.2 L7.4 20 L10 18.8 L7.1 12.2 L12.6 12.2 Z');
    g.shadowColor = 'rgba(0,0,0,.45)'; g.shadowBlur = 5; g.shadowOffsetY = 1.5;
    g.fillStyle = '#fff'; g.fill(p); g.shadowColor = 'transparent';
    g.lineWidth = 1.4; g.strokeStyle = '#111'; g.stroke(p);
  }
  const cursor = new Mesh(new PlaneGeometry(0.62, 0.62), mat({ map: tex(curCanvas) }));
  cursor.position.z = 0.4; scene.add(cursor);

  const ringCanvas = document.createElement('canvas'); ringCanvas.width = ringCanvas.height = 128;
  {
    const g = ringCanvas.getContext('2d');
    g.strokeStyle = 'rgba(120,180,255,.95)'; g.lineWidth = 7; g.beginPath(); g.arc(64, 64, 52, 0, 7); g.stroke();
  }
  const ripple = new Mesh(new PlaneGeometry(1, 1), mat({ map: tex(ringCanvas), opacity: 0 }));
  ripple.position.z = 0.35; scene.add(ripple);

  const capCanvas = document.createElement('canvas'); capCanvas.width = 1400; capCanvas.height = 200;
  const capTex = tex(capCanvas);
  const capMesh = new Mesh(new PlaneGeometry(1, 1), mat({ map: capTex }));
  capMesh.position.z = 0.5; scene.add(capMesh);

  // ---- geometry that depends on the controls ----
  const geo = { cx: 0, cy: 0, w: 0, h: 0 };
  function layout() {
    const padU = S.pad * WORLD_W;
    const bz = S.frame ? 0.22 : 0;
    const w = Math.min(WORLD_W - 2 * padU - 2 * bz, (WORLD_H - 2 * padU - 2 * bz - (S.frame ? 0.5 : 0)) * 16 / 9);
    const h = w * 9 / 16;
    geo.w = w; geo.h = h; geo.cx = 0; geo.cy = S.frame ? 0.25 : 0;
    win.scale.set(w, h, 1); win.position.x = geo.cx; win.position.y = geo.cy;
    // rounded mask in the window's own pixel space
    const mw = 1024, mh = Math.round(mw * 9 / 16);
    maskCanvas.width = mw; maskCanvas.height = mh;
    const mg = maskCanvas.getContext('2d');
    mg.fillStyle = '#000'; mg.fillRect(0, 0, mw, mh);
    mg.fillStyle = '#fff'; mg.beginPath(); mg.roundRect(0, 0, mw, mh, S.radius * 1.1); mg.fill();
    maskTex.needsUpdate = true;
    // shadow
    const sw = w + 1.4, sh = h + 1.4;
    const sc = roundRectTex(256, Math.round(256 * sh / sw), 40, 'rgba(0,0,0,.55)', 22, 30);
    shadow.material.map = tex(sc); shadow.material.needsUpdate = true;
    shadow.scale.set(sw + 0.9, sh + 0.9, 1); shadow.position.set(geo.cx, geo.cy - 0.18, 0);
    // device frame
    bezel.visible = base.visible = S.frame;
    if (S.frame) {
      const bw = w + bz * 2, bh = h + bz * 2;
      bezel.material.map = tex(roundRectTex(512, Math.round(512 * bh / bw), 34, '#10131a'));
      bezel.scale.set(bw, bh, 1); bezel.position.set(geo.cx, geo.cy, 0.1);
      const baseW = bw * 1.12, baseH = 0.26;
      base.material.map = tex(roundRectTex(512, 28, 14, '#aab2c0'));
      base.scale.set(baseW, baseH, 1); base.position.set(geo.cx, geo.cy - bh / 2 - baseH / 2 + 0.02, 0.05);
    }
    capMesh.scale.set(Math.min(w * 0.86, 9.6), Math.min(w * 0.86, 9.6) * 200 / 1400, 1);
    capMesh.position.set(geo.cx, geo.cy - h / 2 + h * 0.17, 0.5);
    drawBg();
  }

  function drawBg() {
    const g = bgCanvas.getContext('2d');
    const [c0, c1, c2, c3] = WALLPAPERS[S.bg];
    const grd = g.createLinearGradient(0, 0, 1280, 800);
    grd.addColorStop(0, c0); grd.addColorStop(.45, c1); grd.addColorStop(.8, c2); grd.addColorStop(1, c3);
    g.fillStyle = grd; g.fillRect(0, 0, 1280, 800);
    const glow = (x, y, r, c, a) => { const rg = g.createRadialGradient(x, y, 0, x, y, r); rg.addColorStop(0, c); rg.addColorStop(1, 'transparent'); g.globalAlpha = a; g.fillStyle = rg; g.fillRect(0, 0, 1280, 800); g.globalAlpha = 1; };
    glow(250, 160, 420, c3, .55); glow(1060, 660, 520, c1, .6); glow(700, 760, 360, c2, .4);
    bg.material.map.needsUpdate = true;
  }

  // ---- time-dependent pieces ----
  function cursorAt(t) {
    let i = 0;
    while (i < KEYS.length - 2 && t > KEYS[i + 1].t) i++;
    const a = KEYS[i], b = KEYS[i + 1];
    const u = clamp((t - a.t) / (b.t - a.t), 0, 1);
    const e = S.smooth ? u * u * (3 - 2 * u) : u;
    let x = lerp(a.x, b.x, e), y = lerp(a.y, b.y, e);
    if (!S.smooth) {                              // raw mouse: hand tremor and stepping
      x += Math.sin(t * 37) * 0.004 + Math.sin(t * 91) * 0.0025;
      y += Math.cos(t * 43) * 0.004 + Math.sin(t * 77) * 0.0025;
    }
    return { x, y };
  }

  function zoomAmount(t) {
    let m = 0;
    for (const z of S.zooms) m = Math.max(m, smooth(z.a, z.a + 0.55, t) * (1 - smooth(z.b - 0.55, z.b, t)));
    return m;
  }

  let capKey = '';
  function drawCaptions(t) {
    // chunk the words into lines of up to 3, ending at sentence ends
    const chunks = []; let cur = [];
    WORDS.forEach((w) => { cur.push(w); if (cur.length >= 3 || /[.?!]$/.test(w[0])) { chunks.push(cur); cur = []; } });
    if (cur.length) chunks.push(cur);
    let ch = null, act = -1;
    for (const c of chunks) {
      const end = c[c.length - 1][1] + 0.75;
      if (t >= c[0][1] && t < end) { ch = c; c.forEach((w, i) => { if (w[1] <= t) act = i; }); break; }
    }
    const key = S.captions && ch ? ch.map((w) => w[0]).join(' ') + act : '';
    if (key === capKey) return;
    capKey = key;
    const g = capCanvas.getContext('2d');
    g.clearRect(0, 0, 1400, 200);
    capMesh.visible = !!key;
    if (!key) { capTex.needsUpdate = true; return; }
    g.font = '800 112px "Geist Variable", Geist, system-ui, sans-serif';
    g.textBaseline = 'middle'; g.lineJoin = 'round'; g.textAlign = 'left';
    const words = ch.map((w) => w[0].toUpperCase());
    const widths = words.map((w) => g.measureText(w).width);
    const gap = g.measureText(' ').width;
    let x = (1400 - (widths.reduce((a, b) => a + b, 0) + gap * (words.length - 1))) / 2;
    g.lineWidth = 18; g.strokeStyle = '#000';
    const xs = [];
    words.forEach((w, i) => { xs.push(x); g.strokeText(w, x, 104); x += widths[i] + gap; });
    words.forEach((w, i) => { g.fillStyle = i === act ? '#ffe14a' : '#fff'; g.fillText(w, xs[i], 104); });
    capTex.needsUpdate = true;
  }

  function frame() {
    const t = S.t;
    // typing: the line in the editor fills in while the second zoom is on it
    const n = t < TYPE_FROM ? 0 : Math.min(TYPED_LEN, Math.floor((t - TYPE_FROM) / (TYPE_TO - TYPE_FROM) * TYPED_LEN));
    const caret = t > TYPE_FROM - 0.4 && t < TYPE_TO + 1.2 && (n < TYPED_LEN || Math.floor(t * 2) % 2 === 0);
    const tk = n + (caret ? 'c' : '');
    if (tk !== typedKey) { typedKey = tk; drawScreen(screenCanvas.getContext('2d'), n, caret); screenTex.needsUpdate = true; }
    const amt = zoomAmount(t);
    const z = 1 + (S.level - 1) * amt;
    const c = cursorAt(t);
    // the view window follows the cursor, clamped inside the screen
    const half = 0.5 / z;
    const fx = clamp(c.x, half, 1 - half), fy = clamp(c.y, half, 1 - half);
    const ox = fx - half, oy = fy - half;
    screenTex.repeat.set(1 / z, 1 / z);
    screenTex.offset.set(ox, 1 - oy - 1 / z);
    // cursor in the window
    const vx = (c.x - ox) * z, vy = (c.y - oy) * z;
    const inside = vx >= 0 && vx <= 1 && vy >= 0 && vy <= 1;
    cursor.visible = inside;
    cursor.position.set(geo.cx + (vx - 0.5) * geo.w + 0.2, geo.cy - (vy - 0.5) * geo.h - 0.2, 0.4);
    const s = geo.w / 12;
    cursor.scale.set(s, s, 1);
    // click ripple
    let rp = 0, rx = 0, ry = 0;
    for (const k of KEYS) if (k.click) { const dt = t - k.t; if (dt >= 0 && dt < 0.6) { rp = dt / 0.6; rx = k.x; ry = k.y; } }
    ripple.visible = rp > 0 && inside;
    if (rp > 0) {
      const px = geo.cx + ((rx - ox) * z - 0.5) * geo.w, py = geo.cy - ((ry - oy) * z - 0.5) * geo.h;
      ripple.position.set(px, py, 0.35);
      const rs = (0.3 + rp * 1.1) * s;
      ripple.scale.set(rs, rs, 1);
      ripple.material.opacity = 1 - rp;
    }
    drawCaptions(t);
    renderer.render(scene, cam);
  }

  // ---- controls ----
  const on = (sel, ev, fn) => $(sel)?.addEventListener(ev, fn);
  root.querySelectorAll('[data-bg]').forEach((b) => b.addEventListener('click', () => {
    S.bg = b.dataset.bg; root.querySelectorAll('[data-bg]').forEach((x) => x.setAttribute('aria-pressed', String(x === b))); drawBg(); frame();
  }));
  on('[data-ctl=pad]', 'input', (e) => { S.pad = Number(e.target.value) / 100; layout(); frame(); });
  on('[data-ctl=radius]', 'input', (e) => { S.radius = Number(e.target.value); layout(); frame(); });
  on('[data-ctl=level]', 'input', (e) => { S.level = Number(e.target.value); frame(); });
  const toggle = (name, key) => on(`[data-ctl=${name}]`, 'change', (e) => { S[key] = e.target.checked; if (name !== 'smooth') layout(); capKey = '~'; frame(); });
  toggle('frame', 'frame'); toggle('captions', 'captions'); toggle('smooth', 'smooth');

  // ---- timeline ----
  const track = $('[data-track=zoom]');
  const ruler = $('[data-ruler]');
  const head = $('[data-head]');
  const timeEl = $('[data-time]');
  const playBtn = $('[data-play]');
  const rm = $('[data-remove]');
  const pct = (t) => (t / D * 100) + '%';

  function renderBlocks() {
    track.querySelectorAll('.pg-block').forEach((n) => n.remove());
    S.zooms.forEach((z) => {
      const el = document.createElement('div');
      el.className = 'pg-block' + (S.sel === z.id ? ' sel' : '');
      el.tabIndex = 0;
      el.dataset.id = z.id;
      el.style.left = pct(z.a); el.style.width = pct(z.b - z.a);
      el.setAttribute('role', 'button');
      el.setAttribute('aria-label', `Zoom ${z.a.toFixed(1)} to ${z.b.toFixed(1)} seconds`);
      el.innerHTML = '<i class="h l"></i><span>Zoom</span><i class="h r"></i>';
      track.appendChild(el);
    });
    rm.disabled = S.sel == null;
  }

  function xToT(clientX) {
    const r = track.getBoundingClientRect();
    return clamp((clientX - r.left) / r.width, 0, 1) * D;
  }

  track.addEventListener('pointerdown', (e) => {
    const blk = e.target.closest('.pg-block');
    if (!blk) {
      // empty spot: add a zoom there
      if (S.zooms.length >= MAX_ZOOMS) return;
      const t = xToT(e.clientX);
      // fit the new block into the free gap around the click
      const lo = Math.max(0, ...S.zooms.filter((o) => o.b <= t).map((o) => o.b));
      const hi = Math.min(D, ...S.zooms.filter((o) => o.a >= t).map((o) => o.a));
      if (S.zooms.some((o) => t >= o.a && t <= o.b) || hi - lo < MIN_LEN) return;
      const len = Math.min(1.8, hi - lo);
      const a = clamp(t - len / 2, lo, hi - len);
      const z = { id: S.nextId++, a, b: a + len };
      S.zooms.push(z); S.sel = z.id; renderBlocks(); frame();
      return;
    }
    e.preventDefault();
    const z = S.zooms.find((q) => String(q.id) === blk.dataset.id);
    S.sel = z.id; renderBlocks();
    const el = track.querySelector(`.pg-block[data-id="${z.id}"]`);
    el.focus({ preventScroll: true });
    const mode = e.target.classList.contains('l') ? 'l' : e.target.classList.contains('r') ? 'r' : 'move';
    const t0 = xToT(e.clientX), a0 = z.a, b0 = z.b;
    const others = S.zooms.filter((q) => q !== z);
    const lo = (x) => Math.max(0, ...others.filter((o) => o.b <= a0 + 1e-6).map((o) => o.b), 0);
    const hi = Math.min(D, ...others.filter((o) => o.a >= b0 - 1e-6).map((o) => o.a), D);
    const move = (ev) => {
      const d = xToT(ev.clientX) - t0;
      if (mode === 'move') { const len = b0 - a0; z.a = clamp(a0 + d, lo(0), hi - len); z.b = z.a + len; }
      else if (mode === 'l') z.a = clamp(a0 + d, lo(0), b0 - MIN_LEN);
      else z.b = clamp(b0 + d, a0 + MIN_LEN, hi);
      el.style.left = pct(z.a); el.style.width = pct(z.b - z.a);
      el.setAttribute('aria-label', `Zoom ${z.a.toFixed(1)} to ${z.b.toFixed(1)} seconds`);
      frame();
    };
    const up = () => { removeEventListener('pointermove', move); removeEventListener('pointerup', up); };
    addEventListener('pointermove', move); addEventListener('pointerup', up);
  });

  track.addEventListener('keydown', (e) => {
    const blk = e.target.closest('.pg-block');
    if (!blk) return;
    const z = S.zooms.find((q) => String(q.id) === blk.dataset.id);
    const step = e.shiftKey ? 0.5 : 0.1;
    if (e.key === 'Delete' || e.key === 'Backspace') { e.preventDefault(); removeSel(z.id); return; }
    if (e.key !== 'ArrowLeft' && e.key !== 'ArrowRight') return;
    e.preventDefault();
    const d = e.key === 'ArrowLeft' ? -step : step, len = z.b - z.a;
    const na = clamp(z.a + d, 0, D - len);
    if (S.zooms.some((o) => o !== z && na < o.b && na + len > o.a)) return;
    z.a = na; z.b = na + len; S.sel = z.id; renderBlocks();
    track.querySelector(`.pg-block[data-id="${z.id}"]`)?.focus({ preventScroll: true });
    frame();
  });

  function removeSel(id = S.sel) {
    if (id == null) return;
    S.zooms = S.zooms.filter((z) => z.id !== id); S.sel = null; renderBlocks(); frame();
  }
  rm.addEventListener('click', () => removeSel());
  $('[data-reset]')?.addEventListener('click', () => {
    S.zooms = [{ id: 1, a: 1.2, b: 4.2 }, { id: 2, a: 4.4, b: 7.0 }, { id: 3, a: 8.2, b: 11.0 }]; S.nextId = 4; S.sel = null; renderBlocks(); frame();
  });

  // scrubbing the ruler
  let scrubbing = false;
  const scrubTo = (clientX) => { S.t = xToT(clientX); frame(); updateHead(); };
  ruler.addEventListener('pointerdown', (e) => { scrubbing = true; ruler.setPointerCapture(e.pointerId); scrubTo(e.clientX); });
  ruler.addEventListener('pointermove', (e) => { if (scrubbing) scrubTo(e.clientX); });
  ruler.addEventListener('pointerup', () => { scrubbing = false; });
  ruler.addEventListener('keydown', (e) => {
    if (e.key === 'ArrowLeft') { S.t = clamp(S.t - 0.25, 0, D); } else if (e.key === 'ArrowRight') { S.t = clamp(S.t + 0.25, 0, D); } else return;
    e.preventDefault(); frame(); updateHead();
  });

  const fmt = (t) => `0:${String(Math.floor(t)).padStart(2, '0')}.${Math.floor((t % 1) * 10)}`;
  function updateHead() {
    head.style.left = pct(S.t);
    timeEl.textContent = fmt(S.t);
    ruler.setAttribute('aria-valuenow', S.t.toFixed(1));
  }
  const syncPlay = () => { playBtn.dataset.state = S.playing ? 'playing' : 'paused'; playBtn.setAttribute('aria-label', S.playing ? 'Pause' : 'Play'); };
  playBtn.addEventListener('click', () => { S.playing = !S.playing; syncPlay(); });

  // ---- size + loop (drawn only while on screen) ----
  let w0 = 0;
  function resize() {
    const w = Math.max(1, canvas.clientWidth);
    if (w === w0) return;
    w0 = w;
    renderer.setSize(w, Math.round(w * WORLD_H / WORLD_W), false);
  }
  const onScreen = () => { const r = root.getBoundingClientRect(); return r.bottom > -100 && r.top < innerHeight + 100; };
  let last = performance.now();
  function loop(now) {
    requestAnimationFrame(loop);
    const dt = Math.min(0.1, (now - last) / 1000); last = now;
    if (!onScreen()) return;
    resize();
    if (S.playing && !scrubbing) { S.t += dt; if (S.t >= D) S.t = 0; updateHead(); frame(); }
  }

  root.classList.add('is-live');                // the canvas must be displayed before it can be measured
  layout(); renderBlocks(); updateHead(); syncPlay(); resize(); frame();
  // the first frame needs a late redraw once the web font and the layout have settled
  setTimeout(() => { resize(); frame(); }, 400);
  addEventListener('resize', () => { resize(); frame(); });
  requestAnimationFrame(loop);
  return true;
}
