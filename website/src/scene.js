// Decorative three.js scenes, built from the things ScreenCraft actually does:
//  hero   - a tiny editor: preview with a cursor and zoom frame, a timeline with clips and zoom
//           blocks, a sweeping playhead, a waveform that lights up as it passes, a REC light
//  closer - a film reel
// Loaded lazily, skipped for reduced motion / no WebGL, paused while off screen.
import {
  WebGLRenderer, Scene, PerspectiveCamera, Group, Mesh, Color, Vector2,
  AmbientLight, DirectionalLight, PointLight, Shape, Path, ExtrudeGeometry, TorusGeometry,
  SphereGeometry, BoxGeometry, ConeGeometry, EdgesGeometry, LineSegments, LineBasicMaterial,
  MeshStandardMaterial, MeshBasicMaterial, BufferGeometry, Float32BufferAttribute, Points, PointsMaterial,
} from 'three';

const cssVar = (n, fallback) => getComputedStyle(document.documentElement).getPropertyValue(n).trim() || fallback;
const isLight = () => document.documentElement.dataset.theme === 'light';

function cursorGeometry() {
  const s = new Shape();
  s.moveTo(0, 0); s.lineTo(0, -1.6); s.lineTo(0.42, -1.25); s.lineTo(0.72, -1.95);
  s.lineTo(1.02, -1.82); s.lineTo(0.74, -1.14); s.lineTo(1.25, -1.1); s.closePath();
  const g = new ExtrudeGeometry(s, { depth: 0.2, bevelEnabled: true, bevelSize: 0.05, bevelThickness: 0.05, bevelSegments: 2 });
  g.center();
  return g;
}

function reelGeometry() {
  const s = new Shape();
  s.absarc(0, 0, 2.3, 0, Math.PI * 2, false);
  for (let i = 0; i < 6; i++) {
    const a = (i / 6) * Math.PI * 2, h = new Path();
    h.absarc(Math.cos(a) * 1.4, Math.sin(a) * 1.4, 0.5, 0, Math.PI * 2, true);
    s.holes.push(h);
  }
  const c = new Path();
  c.absarc(0, 0, 0.32, 0, Math.PI * 2, true);
  s.holes.push(c);
  const g = new ExtrudeGeometry(s, { depth: 0.35, bevelEnabled: true, bevelSize: 0.05, bevelThickness: 0.05, curveSegments: 40 });
  g.center();
  return g;
}

function dust(count, spread) {
  const pos = new Float32Array(count * 3);
  for (let i = 0; i < count; i++) {
    pos[i * 3] = (Math.random() - 0.5) * spread;
    pos[i * 3 + 1] = (Math.random() - 0.5) * spread * 0.55;
    pos[i * 3 + 2] = (Math.random() - 0.5) * spread * 0.5 - 1;
  }
  const g = new BufferGeometry();
  g.setAttribute('position', new Float32BufferAttribute(pos, 3));
  return g;
}

const box = (w, h, d, mat) => new Mesh(new BoxGeometry(w, h, d), mat);

function build(kind, small) {
  const scene = new Scene();
  const camera = new PerspectiveCamera(34, 1, 0.1, 100);
  camera.position.set(0, 0, kind === 'hero' ? 10.6 : 9);
  scene.add(new AmbientLight(0xffffff, 0.8));
  const key = new DirectionalLight(0xffffff, 2.4); key.position.set(3, 6, 8); scene.add(key);
  const glow = new PointLight(0x3b82c4, 60, 40); glow.position.set(-5, -2, 5); scene.add(glow);

  const group = new Group();
  scene.add(group);
  const m = {}; // materials and animated bits, so recolor() and frame() can reach them
  const mats = {
    accent: new MeshStandardMaterial({ roughness: 0.35, metalness: 0.1 }),
    clip: new MeshStandardMaterial({ roughness: 0.5, metalness: 0.05, color: new Color('#d9895e') }),
    plate: new MeshStandardMaterial({ roughness: 0.7, metalness: 0.05 }),
    body: new MeshStandardMaterial({ roughness: 0.3, metalness: 0.15 }),
    rec: new MeshBasicMaterial({ color: new Color('#ff5566') }),
    head: new MeshBasicMaterial({ color: new Color('#ffffff') }),
    line: new LineBasicMaterial({ transparent: true, opacity: 0.9 }),
    zoomLine: new LineBasicMaterial({ color: new Color('#ffe14a'), transparent: true, opacity: 0.95 }),
  };
  m.mats = mats;

  if (kind === 'hero') {
    // preview window
    const slabGeo = new BoxGeometry(4.6, 2.6, 0.12);
    const slab = new Mesh(slabGeo, mats.plate);
    slab.position.set(-1.6, 1.7, 0);
    const slabEdges = new LineSegments(new EdgesGeometry(slabGeo), mats.line);
    slabEdges.position.copy(slab.position);
    // page content inside it: a few "lines of text" the zoom frame can land on
    for (let i = 0; i < 4; i++) {
      const bar = box(1.6 + (i % 2) * 1.0, 0.1, 0.02, mats.body);
      bar.position.set(-2.6 + (1.6 + (i % 2) * 1.0) / 2 - 0.2, 2.4 - i * 0.5, 0.08);
      group.add(bar);
    }
    // cursor and the yellow zoom frame that follows it
    m.cursor = new Mesh(cursorGeometry(), mats.body);
    m.cursor.scale.setScalar(0.34);
    const frameGeo = new EdgesGeometry(new BoxGeometry(1.5, 0.9, 0.01));
    m.zoomFrame = new LineSegments(frameGeo, mats.zoomLine);
    // webcam bubble
    m.bubble = new Mesh(new SphereGeometry(0.38, 40, 28), mats.accent);
    m.bubble.position.set(-3.45, 2.65, 0.3);
    // REC light
    m.rec = new Mesh(new SphereGeometry(0.16, 24, 16), mats.rec);
    m.rec.position.set(0.5, 2.75, 0.2);
    m.recRing = new Mesh(new TorusGeometry(0.28, 0.025, 12, 40), mats.rec);
    m.recRing.position.copy(m.rec.position);

    // timeline
    const plate = box(9.4, 3.1, 0.14, mats.plate);
    plate.position.set(0, -1.15, -0.2);
    const clips = [[-4.1, 2.2], [-1.7, 1.5], [0.0, 2.4], [2.6, 1.6]];
    clips.forEach(([x, w]) => {
      const c = box(w, 0.46, 0.18, mats.clip);
      c.position.set(x + w / 2, -0.35, 0);
      group.add(c);
    });
    m.zooms = [[-3.5, 1.2], [-0.6, 1.0], [1.5, 1.7]].map(([x, w]) => {
      const z = box(w, 0.36, 0.18, mats.accent);
      z.position.set(x + w / 2, -0.95, 0);
      z.userData = { x0: x, x1: x + w };
      group.add(z);
      return z;
    });
    // waveform
    m.bars = [];
    for (let i = 0; i < (small ? 36 : 56); i++) {
      const n = small ? 36 : 56;
      const b = box(0.08, 1, 0.1, mats.body.clone());
      b.position.set(-4.25 + (i / (n - 1)) * 8.5, -2.1, 0);
      b.userData = { x: b.position.x, seed: Math.sin(i * 12.9898) * 43758.5453 % 1 };
      group.add(b);
      m.bars.push(b);
    }
    // playhead
    m.playhead = new Group();
    const stem = box(0.045, 3.0, 0.05, mats.head);
    const tip = new Mesh(new ConeGeometry(0.14, 0.26, 16), mats.head);
    tip.rotation.z = Math.PI;
    tip.position.y = 1.62;
    m.playhead.add(stem, tip);
    m.playhead.position.set(-4.3, -1.2, 0.25);
    group.add(plate, slab, slabEdges, m.cursor, m.zoomFrame, m.bubble, m.rec, m.recRing, m.playhead);
    m.slab = slab;
    group.rotation.x = -0.18;
    group.position.y = -0.1;
  } else {
    m.reel = new Mesh(reelGeometry(), mats.accent);
    m.play = new Mesh(new ConeGeometry(0.55, 0.9, 3), mats.head);
    m.play.rotation.z = -Math.PI / 2;
    m.play.position.z = 0.35;
    m.ring = new Mesh(new TorusGeometry(3.1, 0.025, 12, 140), mats.line);
    m.ring.material = new MeshBasicMaterial({ transparent: true, opacity: 0.8 });
    m.reelGroup = new Group();
    m.reelGroup.add(m.reel, m.play);
    m.reelGroup.rotation.x = -0.35;
    group.add(m.reelGroup, m.ring);
    m.ringMat = m.ring.material;
  }
  m.dust = new Points(dust(small ? 60 : 130, 16), new PointsMaterial({ size: 0.05, transparent: true, opacity: 0.7 }));
  scene.add(m.dust);

  const recolor = () => {
    const accent = new Color(cssVar('--accent', '#3b82c4')), light = isLight();
    mats.accent.color = accent;
    mats.plate.color = new Color(light ? '#dfe6f0' : '#141b28');
    mats.body.color = new Color(light ? '#233042' : '#cfd8e8');
    mats.line.color = accent;
    mats.head.color = new Color(light ? '#0e1621' : '#ffffff');
    if (m.ringMat) m.ringMat.color = accent;
    if (m.bars) m.bars.forEach((b) => { b.material.color = new Color(light ? '#9aa8bd' : '#2b3649'); });
    m.dust.material.color = new Color(light ? '#1f6cb3' : '#9fcbf2');
    glow.color = accent;
    key.intensity = light ? 3 : 2.4;
  };
  recolor();
  return { scene, camera, group, m, recolor };
}

export function mountScene(canvas, kind) {
  if (matchMedia('(prefers-reduced-motion: reduce)').matches) return;
  let renderer;
  try { renderer = new WebGLRenderer({ canvas, alpha: true, antialias: true, powerPreference: 'low-power' }); } catch { return; }
  const small = matchMedia('(max-width: 880px)').matches;
  renderer.setPixelRatio(Math.min(devicePixelRatio || 1, small ? 1.25 : 1.75));
  renderer.setClearColor(0x000000, 0);
  const { scene, camera, group, m, recolor } = build(kind, small);

  const size = () => {
    const r = canvas.getBoundingClientRect();
    if (!r.width || !r.height) return;
    renderer.setSize(r.width, r.height, false);
    camera.aspect = r.width / r.height;
    // keep the whole timeline in view on narrow canvases
    if (kind === 'hero') camera.position.z = 10.6 * Math.max(1, 1.8 / camera.aspect);
    camera.updateProjectionMatrix();
  };
  size();
  addEventListener('resize', size);

  const pointer = new Vector2(0, 0), eased = new Vector2(0, 0);
  addEventListener('pointermove', (e) => {
    pointer.set((e.clientX / innerWidth) * 2 - 1, -((e.clientY / innerHeight) * 2 - 1));
  }, { passive: true });
  new MutationObserver(recolor).observe(document.documentElement, { attributes: true, attributeFilter: ['data-theme'] });

  let visible = true, raf = 0;
  const t0 = performance.now();
  new IntersectionObserver(([e]) => { visible = e.isIntersecting; if (visible && !raf) raf = requestAnimationFrame(frame); }).observe(canvas);
  document.addEventListener('visibilitychange', () => { if (!document.hidden && visible && !raf) raf = requestAnimationFrame(frame); });

  const baseRotX = group.rotation.x;
  function frame(now) {
    raf = 0;
    if (!visible || document.hidden) return;
    const t = (now - t0) / 1000;
    eased.lerp(pointer, 0.05);
    group.rotation.y = eased.x * 0.22;
    group.rotation.x = baseRotX - eased.y * 0.12;

    if (kind === 'hero') {
      const period = 9, p = (t % period) / period;       // 0..1 across the timeline
      const x = -4.3 + p * 8.6;
      m.playhead.position.x = x;
      // cursor wanders over the preview; the zoom frame follows it and "punches in" over zoom blocks
      const cx = -1.6 + Math.sin(t * 0.8) * 1.5, cy = 1.7 + Math.sin(t * 1.3 + 1) * 0.65;
      m.cursor.position.set(cx + 0.12, cy - 0.2, 0.2);
      m.cursor.rotation.set(0, 0, 0.1);
      const inZoom = m.zooms.some((z) => x > z.userData.x0 && x < z.userData.x1);
      const s = inZoom ? 1 : 1.9;
      m.zoomFrame.scale.x += (s - m.zoomFrame.scale.x) * 0.08;
      m.zoomFrame.scale.y = m.zoomFrame.scale.x;
      m.zoomFrame.position.set(cx, cy, 0.12);
      m.zooms.forEach((z) => {
        const on = x > z.userData.x0 && x < z.userData.x1;
        const k = z.scale.y + ((on ? 1.45 : 1) - z.scale.y) * 0.12;
        z.scale.y = k;
      });
      // waveform: lit to the left of the playhead, dim to the right
      m.bars.forEach((b, i) => {
        const h = 0.25 + Math.abs(Math.sin(i * 0.55 + t * 1.8) * 0.5 + Math.sin(i * 1.37 + t * 0.9) * 0.35) * 1.1;
        b.scale.y = h;
        b.position.y = -2.1 + (h - 1) * 0.5 + 0.5 - 0.5;
        const lit = b.userData.x <= x;
        b.material.emissive = b.material.emissive || new Color();
        b.material.color.set(lit ? m.mats.accent.color : (isLight() ? '#9aa8bd' : '#2b3649'));
      });
      // REC light blinks
      const blink = (Math.sin(t * 3.2) + 1) / 2;
      m.rec.scale.setScalar(0.8 + blink * 0.35);
      m.recRing.scale.setScalar(1 + blink * 0.5);
      m.recRing.material.opacity = 1;
      m.bubble.position.y = 2.65 + Math.sin(t * 1.1) * 0.05;
    } else {
      m.reelGroup.rotation.z = -t * 0.45;
      m.play.rotation.z = -Math.PI / 2;
      m.ring.rotation.set(1.1, 0, t * 0.1);
    }
    m.dust.rotation.y = t * 0.02;
    renderer.render(scene, camera);
    raf = requestAnimationFrame(frame);
  }
  raf = requestAnimationFrame(frame);
  canvas.classList.add('live');
}
