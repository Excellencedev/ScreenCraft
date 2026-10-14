// Decorative three.js scenes: a floating "recording" in the hero, a slow lens in the closer.
// Loaded lazily, skipped for reduced motion / no WebGL, paused while off screen.
import {
  WebGLRenderer, Scene, PerspectiveCamera, Group, Mesh, Color, Vector2,
  AmbientLight, DirectionalLight, PointLight, Shape, ExtrudeGeometry, TorusGeometry,
  SphereGeometry, BoxGeometry, EdgesGeometry, LineSegments, LineBasicMaterial,
  MeshStandardMaterial, MeshBasicMaterial, BufferGeometry, Float32BufferAttribute,
  Points, PointsMaterial, TorusKnotGeometry, MathUtils,
} from 'three';

const css = (name) => getComputedStyle(document.documentElement).getPropertyValue(name).trim() || '#3b82c4';
const isLight = () => document.documentElement.dataset.theme === 'light';

function cursorGeometry() {
  // the classic pointer outline, extruded a little so it catches the light
  const s = new Shape();
  s.moveTo(0, 0); s.lineTo(0, -1.6); s.lineTo(0.42, -1.25); s.lineTo(0.72, -1.95);
  s.lineTo(1.02, -1.82); s.lineTo(0.74, -1.14); s.lineTo(1.25, -1.1); s.closePath();
  const g = new ExtrudeGeometry(s, { depth: 0.18, bevelEnabled: true, bevelSize: 0.04, bevelThickness: 0.04, bevelSegments: 2 });
  g.center();
  return g;
}

function particles(count, spread) {
  const pos = new Float32Array(count * 3);
  for (let i = 0; i < count; i++) {
    pos[i * 3] = (Math.random() - 0.5) * spread;
    pos[i * 3 + 1] = (Math.random() - 0.5) * spread * 0.7;
    pos[i * 3 + 2] = (Math.random() - 0.5) * spread * 0.6;
  }
  const g = new BufferGeometry();
  g.setAttribute('position', new Float32BufferAttribute(pos, 3));
  return g;
}

function build(kind, small) {
  const scene = new Scene();
  const camera = new PerspectiveCamera(38, 1, 0.1, 100);
  camera.position.set(0, 0, 9);
  scene.add(new AmbientLight(0xffffff, 0.7));
  const key = new DirectionalLight(0xffffff, 2.2); key.position.set(4, 5, 6); scene.add(key);
  const glow = new PointLight(0x3b82c4, 40, 30); glow.position.set(-4, -1, 4); scene.add(glow);

  const group = new Group();
  scene.add(group);
  const parts = {};

  if (kind === 'hero') {
    parts.cursor = new Mesh(cursorGeometry(), new MeshStandardMaterial({ roughness: 0.28, metalness: 0.15 }));
    parts.cursor.scale.setScalar(1.05);
    parts.cursor.position.set(0.9, 0.2, 1.4);
    parts.ring = new Mesh(new TorusGeometry(1.9, 0.045, 24, 120), new MeshBasicMaterial());
    parts.ring.position.set(0.9, 0.2, 0);
    // the "screen": a thin slab outlined with edges, so it reads as a window, not a box
    const slab = new BoxGeometry(5.2, 3.2, 0.12);
    parts.screen = new Mesh(slab, new MeshStandardMaterial({ roughness: 0.6, metalness: 0.1, transparent: true, opacity: 0.55 }));
    parts.screen.position.set(1.1, -0.2, -1.4);
    parts.screen.rotation.set(-0.12, -0.32, 0.04);
    parts.edges = new LineSegments(new EdgesGeometry(slab), new LineBasicMaterial({ transparent: true, opacity: 0.8 }));
    parts.edges.position.copy(parts.screen.position);
    parts.edges.rotation.copy(parts.screen.rotation);
    parts.bubble = new Mesh(new SphereGeometry(0.62, 48, 32), new MeshStandardMaterial({ roughness: 0.22, metalness: 0.05 }));
    parts.bubble.position.set(3.1, -2.0, 0.6);
    group.add(parts.screen, parts.edges, parts.ring, parts.cursor, parts.bubble);
  } else {
    parts.knot = new Mesh(new TorusKnotGeometry(1.5, 0.34, small ? 120 : 220, 20, 2, 3),
      new MeshStandardMaterial({ roughness: 0.25, metalness: 0.3, wireframe: false }));
    parts.ring = new Mesh(new TorusGeometry(2.7, 0.03, 16, 120), new MeshBasicMaterial({ transparent: true, opacity: 0.9 }));
    parts.ring.rotation.x = 1.15;
    group.add(parts.knot, parts.ring);
  }
  parts.dust = new Points(particles(small ? 70 : 160, 14), new PointsMaterial({ size: 0.045, transparent: true, opacity: 0.8, sizeAttenuation: true }));
  scene.add(parts.dust);

  const recolor = () => {
    const accent = new Color(css('--accent')), light = isLight();
    const body = new Color(light ? '#0e1621' : '#eaeff7');
    if (parts.cursor) parts.cursor.material.color = body;
    if (parts.ring) parts.ring.material.color = accent;
    if (parts.screen) parts.screen.material.color = new Color(light ? '#dfe6f0' : '#18202e');
    if (parts.edges) parts.edges.material.color = accent;
    if (parts.bubble) parts.bubble.material.color = accent;
    if (parts.knot) parts.knot.material.color = accent;
    parts.dust.material.color = new Color(light ? '#1f6cb3' : '#9fcbf2');
    glow.color = accent;
    key.intensity = light ? 2.8 : 2.2;
  };
  recolor();
  return { scene, camera, group, parts, recolor };
}

export function mountScene(canvas, kind) {
  if (matchMedia('(prefers-reduced-motion: reduce)').matches) return;
  let renderer;
  try { renderer = new WebGLRenderer({ canvas, alpha: true, antialias: true, powerPreference: 'low-power' }); } catch { return; }
  const small = matchMedia('(max-width: 880px)').matches;
  renderer.setPixelRatio(Math.min(devicePixelRatio || 1, small ? 1.25 : 1.75));
  renderer.setClearColor(0x000000, 0);
  const { scene, camera, group, parts, recolor } = build(kind, small);

  const size = () => {
    const r = canvas.getBoundingClientRect();
    if (!r.width || !r.height) return;
    renderer.setSize(r.width, r.height, false);
    camera.aspect = r.width / r.height;
    camera.updateProjectionMatrix();
  };
  size();
  addEventListener('resize', size);

  const pointer = new Vector2(0, 0), eased = new Vector2(0, 0);
  addEventListener('pointermove', (e) => {
    pointer.set((e.clientX / innerWidth) * 2 - 1, -((e.clientY / innerHeight) * 2 - 1));
  }, { passive: true });
  new MutationObserver(recolor).observe(document.documentElement, { attributes: true, attributeFilter: ['data-theme'] });

  let visible = true, raf = 0, t0 = performance.now();
  new IntersectionObserver(([e]) => { visible = e.isIntersecting; if (visible && !raf) raf = requestAnimationFrame(frame); }).observe(canvas);
  document.addEventListener('visibilitychange', () => { if (!document.hidden && visible && !raf) raf = requestAnimationFrame(frame); });

  function frame(now) {
    raf = 0;
    if (!visible || document.hidden) return;
    const t = (now - t0) / 1000;
    eased.lerp(pointer, 0.05);
    group.rotation.y = eased.x * 0.28;
    group.rotation.x = -eased.y * 0.18;
    if (kind === 'hero') {
      parts.cursor.position.y = 0.2 + Math.sin(t * 0.9) * 0.18;
      parts.cursor.position.x = 0.9 + Math.cos(t * 0.6) * 0.25 + eased.x * 0.5;
      parts.cursor.rotation.set(0.1 + Math.sin(t * 0.7) * 0.08, -0.35 + Math.sin(t * 0.5) * 0.18, 0.18);
      // the zoom ring breathes, like a zoom region easing in and out
      const k = 0.9 + 0.14 * Math.sin(t * 0.8);
      parts.ring.scale.setScalar(k);
      parts.ring.rotation.set(Math.sin(t * 0.4) * 0.35, Math.cos(t * 0.35) * 0.35, 0);
      parts.ring.position.set(parts.cursor.position.x, parts.cursor.position.y, 0);
      parts.screen.position.y = -0.2 + Math.sin(t * 0.5) * 0.12;
      parts.edges.position.y = parts.screen.position.y;
      parts.bubble.position.y = -2.0 + Math.sin(t * 1.1 + 1) * 0.16;
    } else {
      parts.knot.rotation.set(t * 0.18, t * 0.26, 0);
      parts.ring.rotation.z = t * 0.12;
    }
    parts.dust.rotation.y = t * 0.02;
    parts.dust.position.y = Math.sin(t * 0.3) * 0.2;
    renderer.render(scene, camera);
    raf = requestAnimationFrame(frame);
  }
  raf = requestAnimationFrame(frame);
  canvas.classList.add('live');
}
