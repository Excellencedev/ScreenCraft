// The recorder bar from the app, drawn to a canvas from the same measurements as ui/recorder.html
// and floated in three.js so it tilts toward the pointer. Clicking Record goes to the downloads page.
import {
  WebGLRenderer, Scene, PerspectiveCamera, Mesh, Shape, ExtrudeGeometry, CanvasTexture, MeshBasicMaterial,
  MeshStandardMaterial, AmbientLight, DirectionalLight, SRGBColorSpace, Raycaster, Vector2, LinearFilter,
} from 'three';

const W = 1100, H = 76, S = 2;               // css px of the real bar, and the texture scale
const C = {
  bar: 'rgba(11,16,26,.96)', line: '#1d2738', line2: '#2a374d', panel2: '#141c2a', text: '#b9c3d6',
  muted: '#8693aa', accent: '#3b82c4', accent2: '#6aaee8', white: '#ffffff',
};
const FONT = 'ui-rounded, "SF Pro Rounded", -apple-system, BlinkMacSystemFont, "Segoe UI", system-ui, sans-serif';

const ICON = {
  display: ['M3 13h18'], window: ['M3 9h18'], area: [],
  camera: ['M16 11l5-3v8l-5-3z'], mic: ['M5 11a7 7 0 0014 0M12 18v3'], sys: ['M4 10v4h4l5 4V6L8 10z', 'M16 9a4 4 0 010 6'],
  quality: ['M4 18V9M10 18V5M16 18v-7M22 18H2'], chev: ['M6 9l6 6 6-6'],
  folder: ['M3 7a2 2 0 012-2h4l2 2h8a2 2 0 012 2v8a2 2 0 01-2 2H5a2 2 0 01-2-2z'],
};
const RECT = { display: [3, 4, 18, 13, 2.5], window: [3, 5, 18, 14, 2.5], camera: [3, 7, 13, 10, 2], mic: [9, 3, 6, 11, 3] };

function rr(g, x, y, w, h, r) { g.beginPath(); g.roundRect(x, y, w, h, r); }

function icon(g, name, x, y, size, color, widthK = 1.7) {
  const k = size / 24;
  g.save();
  g.translate(x, y); g.scale(k, k);
  g.strokeStyle = color; g.lineWidth = widthK; g.lineCap = 'round'; g.lineJoin = 'round';
  const r = RECT[name];
  if (r) { rr(g, r[0], r[1], r[2], r[3], r[4]); g.stroke(); }
  if (name === 'area') { g.setLineDash([3, 3]); g.beginPath(); g.rect(5, 5, 14, 14); g.stroke(); g.setLineDash([]); }
  for (const d of ICON[name] || []) g.stroke(new Path2D(d));
  g.restore();
}

function fit(g, text, max) {
  if (g.measureText(text).width <= max) return text;
  let t = text;
  while (t.length > 1 && g.measureText(t + '…').width > max) t = t.slice(0, -1);
  return t + '…';
}

/** Draw the bar. `t` seconds drives the level meters; `hot` is the Record button hover state. */
function draw(g, t, hot) {
  g.setTransform(S, 0, 0, S, 0, 0);
  g.clearRect(0, 0, W, H);
  // bar
  rr(g, 0.5, 0.5, W - 1, H - 1, 14); g.fillStyle = C.bar; g.fill();
  g.strokeStyle = C.line2; g.lineWidth = 1; g.stroke();
  g.textBaseline = 'middle';
  const cy = H / 2;
  let x = 13;

  // close
  rr(g, x + 0.5, cy - 15 + 0.5, 29, 29, 9); g.strokeStyle = C.line2; g.stroke();
  g.save(); g.translate(x + 15 - 6.5, cy - 6.5); g.strokeStyle = C.text; g.lineWidth = 2.2; g.lineCap = 'round';
  g.stroke(new Path2D('M3 3l10 10M13 3L3 13')); g.restore();
  x += 30 + 10;
  const sep = () => { x += 2; g.fillStyle = C.line2; g.fillRect(x, cy - 20, 1, 40); x += 1 + 2 + 10; };
  const cap = (label, cx) => {
    g.font = `500 9.5px ${FONT}`; g.fillStyle = C.muted;
    if ('letterSpacing' in g) g.letterSpacing = '1.1px';
    g.fillText(label.toUpperCase(), cx + 2, cy - 25.5 + 6 + 0.5);
    if ('letterSpacing' in g) g.letterSpacing = '0px';
  };
  const top = cy - 25.5 + 17;           // top of the 34px controls

  sep();
  // capture
  cap('Capture', x);
  g.font = `500 12.5px ${FONT}`;
  const tools = [['display', 'Display', true], ['window', 'Window'], ['area', 'Area']];
  const tw = tools.map(([, l]) => 10 + 17 + 7 + g.measureText(l).width + 10);
  const segW = tw.reduce((a, b) => a + b, 0);
  g.save();
  rr(g, x + 0.5, top + 0.5, segW, 33, 9); g.strokeStyle = C.line2; g.stroke(); g.clip();
  let tx = x + 0.5;
  tools.forEach(([id, label, on], i) => {
    if (on) { g.fillStyle = C.accent; g.fillRect(tx, top + 0.5, tw[i], 33); }
    icon(g, id, tx + 10, top + 17 - 8.5, 17, on ? C.white : C.text);
    g.fillStyle = on ? C.white : C.text; g.font = `500 12.5px ${FONT}`;
    g.fillText(label, tx + 10 + 17 + 7, top + 17 + 0.5);
    tx += tw[i];
    if (i < tools.length - 1) { g.fillStyle = C.line2; g.fillRect(tx - 1, top + 0.5, 1, 33); }
  });
  g.restore();
  x += segW + 1 + 10;

  sep();
  // inputs
  cap('Inputs', x);
  const opts = [
    { id: 'camera', text: 'FaceTime HD Camera', on: true, cam: true },
    { id: 'mic', text: 'MacBook Pro Microphone', on: true, meter: 0.62 },
    { id: 'sys', text: 'System audio', on: true, meter: 0.4 },
  ];
  opts.forEach((o, i) => {
    const w = 108;
    rr(g, x + 0.5, top + 0.5, w - 1, 33, 9);
    g.fillStyle = C.panel2; g.fill();
    g.strokeStyle = o.on ? 'rgba(106,174,232,.55)' : C.line; g.stroke();
    let ix = x + 10;
    if (o.cam) { rr(g, ix, top + 4, 26, 26, 7); g.fillStyle = '#000'; g.fill(); ix += 26; }
    else { icon(g, o.id, ix, top + 17 - 8.5, 17, C.white); ix += 17; }
    g.font = `500 12.5px ${FONT}`; g.fillStyle = o.on ? C.white : C.text;
    g.fillText(fit(g, o.text, w - 10 - (ix - x - 10) - 7 - 12), ix + 7, top + 17 + 0.5);
    if (o.meter != null) {
      const mx = x + 8, mw = w - 16, my = top + 34 - 3 - 2;
      rr(g, mx, my, mw, 2, 1); g.fillStyle = 'rgba(255,255,255,.1)'; g.fill();
      const lvl = Math.min(1, Math.max(0.05, o.meter * (0.55 + 0.45 * Math.sin(t * (2.2 + i * 1.1) + i) * Math.sin(t * 5.3 + i * 2))));
      const gr = g.createLinearGradient(mx, 0, mx + mw, 0);
      gr.addColorStop(0, '#3ddc97'); gr.addColorStop(.65, '#3ddc97'); gr.addColorStop(.8, '#f6c453'); gr.addColorStop(1, '#ff6b6b');
      g.save(); rr(g, mx, my, mw, 2, 1); g.clip(); g.fillStyle = gr; g.fillRect(mx, my, mw * lvl, 2); g.restore();
    }
    x += w + 6;
  });
  x += -6 + 10;

  sep();
  // output
  cap('Output', x);
  g.font = `500 12.5px ${FONT}`;
  const qw = 138;
  rr(g, x + 0.5, top + 0.5, qw - 1, 33, 9); g.fillStyle = C.panel2; g.fill(); g.strokeStyle = C.line; g.stroke();
  icon(g, 'quality', x + 10, top + 17 - 8.5, 17, C.text);
  g.fillStyle = C.text; g.fillText(fit(g, 'Native · 60 fps', qw - 10 - 17 - 7 - 11 - 7 - 10), x + 10 + 17 + 7, top + 17 + 0.5);
  icon(g, 'chev', x + qw - 10 - 11, top + 17 - 5.5, 11, C.text);
  x += qw + 6;
  const pw = 10 + g.measureText('Projects').width + 7 + 17 + 10;
  rr(g, x + 0.5, top + 0.5, pw - 1, 33, 9); g.fillStyle = C.panel2; g.fill(); g.strokeStyle = C.line; g.stroke();
  g.fillStyle = C.text; g.fillText('Projects', x + 10, top + 17 + 0.5);
  icon(g, 'folder', x + pw - 10 - 17, top + 17 - 8.5, 17, C.text);

  // record (right aligned)
  g.font = `600 13.5px ${FONT}`;
  const rw = 16 + 14 + 9 + g.measureText('Record').width + 16, rx = W - 13 - rw, ry = cy - 23;
  rr(g, rx, ry, rw, 46, 10); g.fillStyle = hot ? C.accent2 : C.accent; g.fill();
  rr(g, rx + 0.5, ry + 0.5, rw - 1, 45, 10); g.strokeStyle = 'rgba(255,255,255,.18)'; g.stroke();
  // the logo's four corner marks
  g.strokeStyle = 'rgba(255,255,255,.7)'; g.lineWidth = 1;
  const l = 7, ix0 = rx + 6, iy0 = ry + 8, ix1 = rx + rw - 6, iy1 = ry + 46 - 8;
  g.beginPath();
  g.moveTo(ix0, iy0 + l); g.lineTo(ix0, iy0); g.lineTo(ix0 + l, iy0);
  g.moveTo(ix1 - l, iy0); g.lineTo(ix1, iy0); g.lineTo(ix1, iy0 + l);
  g.moveTo(ix0, iy1 - l); g.lineTo(ix0, iy1); g.lineTo(ix0 + l, iy1);
  g.moveTo(ix1 - l, iy1); g.lineTo(ix1, iy1); g.lineTo(ix1, iy1 - l);
  g.stroke();
  g.beginPath(); g.arc(rx + 16 + 7, cy, 10, 0, Math.PI * 2); g.fillStyle = 'rgba(255,255,255,.28)'; g.fill();
  g.beginPath(); g.arc(rx + 16 + 7, cy, 7, 0, Math.PI * 2); g.fillStyle = C.white; g.fill();
  g.fillStyle = C.white; g.fillText('Record', rx + 16 + 14 + 9, cy + 0.5);
  return { x: rx, y: ry, w: rw, h: 46 };
}

export function mountRecBar(canvas) {
  const reduce = matchMedia('(prefers-reduced-motion: reduce)').matches;
  const tex = document.createElement('canvas');
  tex.width = W * S; tex.height = H * S;
  const g = tex.getContext('2d');
  let renderer;
  try { renderer = new WebGLRenderer({ canvas, alpha: true, antialias: true, powerPreference: 'low-power' }); } catch { return false; }
  renderer.setPixelRatio(Math.min(devicePixelRatio || 1, 2));
  renderer.setClearColor(0x000000, 0);

  const scene = new Scene();
  const camera = new PerspectiveCamera(22, 1, 1, 4000);
  scene.add(new AmbientLight(0xffffff, 1.2));
  const key = new DirectionalLight(0xffffff, 1.4); key.position.set(200, 400, 800); scene.add(key);

  // a rounded slab the size of the bar; its caps carry the texture, its edge is a dark rim
  const shape = new Shape();
  const r = 14;
  shape.moveTo(r, 0); shape.lineTo(W - r, 0); shape.quadraticCurveTo(W, 0, W, r); shape.lineTo(W, H - r);
  shape.quadraticCurveTo(W, H, W - r, H); shape.lineTo(r, H); shape.quadraticCurveTo(0, H, 0, H - r);
  shape.lineTo(0, r); shape.quadraticCurveTo(0, 0, r, 0);
  const geo = new ExtrudeGeometry(shape, { depth: 10, bevelEnabled: false, curveSegments: 12 });
  const pos = geo.attributes.position, uv = geo.attributes.uv;
  for (let i = 0; i < uv.count; i++) uv.setXY(i, pos.getX(i) / W, pos.getY(i) / H);
  geo.translate(-W / 2, -H / 2, -5);
  const texture = new CanvasTexture(tex);
  texture.colorSpace = SRGBColorSpace; texture.minFilter = LinearFilter; texture.anisotropy = 8;
  const face = new MeshBasicMaterial({ map: texture, transparent: true });
  const rim = new MeshStandardMaterial({ color: 0x0b101a, roughness: 0.5, metalness: 0.3 });
  const bar = new Mesh(geo, [face, rim]);
  scene.add(bar);

  const ray = new Raycaster(), ptr = new Vector2();
  let hot = false, hotBox = null;
  const state = { rx: 0, ry: 0, tx: 0, ty: 0 };

  const size = () => {
    const rc = canvas.getBoundingClientRect();
    if (!rc.width) return;
    renderer.setSize(rc.width, rc.height, false);
    camera.aspect = rc.width / rc.height;
    // fit the bar's width with a little margin
    const fov = (camera.fov * Math.PI) / 180;
    camera.position.z = (W * 1.06) / 2 / Math.tan(fov / 2) / camera.aspect;
    camera.updateProjectionMatrix();
  };
  size();
  addEventListener('resize', size);

  const onMove = (e) => {
    const rc = canvas.getBoundingClientRect();
    const nx = ((e.clientX - rc.left) / rc.width) * 2 - 1, ny = -(((e.clientY - rc.top) / rc.height) * 2 - 1);
    state.tx = Math.max(-1, Math.min(1, nx)) * 0.16;
    state.ty = Math.max(-1, Math.min(1, ny)) * 0.1;
    ptr.set(nx, ny);
    ray.setFromCamera(ptr, camera);
    const hit = ray.intersectObject(bar)[0];
    let over = false;
    if (hit && hotBox && hit.uv) {
      const px = hit.uv.x * W, py = (1 - hit.uv.y) * H;
      over = px >= hotBox.x && px <= hotBox.x + hotBox.w && py >= hotBox.y && py <= hotBox.y + hotBox.h;
    }
    hot = over;
    canvas.style.cursor = over ? 'pointer' : 'default';
  };
  canvas.addEventListener('pointermove', onMove);
  canvas.addEventListener('pointerleave', () => { hot = false; state.tx = 0; state.ty = 0; canvas.style.cursor = 'default'; });
  canvas.addEventListener('click', () => { if (hot) location.href = '/download'; });

  let raf = 0;
  const t0 = performance.now();
  // on screen? checked by geometry (cheap), polled while off screen, so it can never get stuck
  const onScreen = () => { const r = canvas.getBoundingClientRect(); return r.bottom > -100 && r.top < innerHeight + 100; };
  const wake = () => { if (document.hidden || !onScreen()) { setTimeout(wake, 300); return; } if (!raf) raf = requestAnimationFrame(frame); };
  document.addEventListener('visibilitychange', wake);
  function frame(now) {
    raf = 0;
    if (document.hidden || !onScreen()) { setTimeout(wake, 300); return; }
    const t = (now - t0) / 1000;
    if (renderer.domElement.width !== Math.round(canvas.getBoundingClientRect().width * renderer.getPixelRatio())) size();
    hotBox = draw(g, reduce ? 1 : t, hot);
    texture.needsUpdate = true;
    state.rx += (state.ty - state.rx) * 0.08;
    state.ry += (state.tx - state.ry) * 0.08;
    const idle = reduce ? 0 : 1;
    bar.rotation.x = -state.rx + Math.sin(t * 0.6) * 0.03 * idle - 0.06;
    bar.rotation.y = state.ry + Math.sin(t * 0.45) * 0.05 * idle;
    bar.position.y = Math.sin(t * 0.9) * 3 * idle;
    renderer.render(scene, camera);
    raf = requestAnimationFrame(frame);
  }
  raf = requestAnimationFrame(frame);
  canvas.classList.add('live');
  return true;
}
