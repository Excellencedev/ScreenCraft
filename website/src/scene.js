// One decorative three.js scene: a film reel turning slowly behind the closing call to action.
// Loaded lazily, skipped for reduced motion / no WebGL, paused while off screen.
import {
  WebGLRenderer, Scene, PerspectiveCamera, Group, Mesh, Color, Vector2,
  AmbientLight, DirectionalLight, PointLight, Shape, Path, ExtrudeGeometry, TorusGeometry, ConeGeometry,
  MeshStandardMaterial, MeshBasicMaterial, BufferGeometry, Float32BufferAttribute, Points, PointsMaterial,
} from 'three';

const cssVar = (n, fallback) => getComputedStyle(document.documentElement).getPropertyValue(n).trim() || fallback;
const isLight = () => document.documentElement.dataset.theme === 'light';

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

function build(small) {
  const scene = new Scene();
  const camera = new PerspectiveCamera(34, 1, 0.1, 100);
  camera.position.set(0, 0, 9);
  scene.add(new AmbientLight(0xffffff, 0.8));
  const key = new DirectionalLight(0xffffff, 2.4); key.position.set(3, 6, 8); scene.add(key);
  const glow = new PointLight(0x3b82c4, 60, 40); glow.position.set(-5, -2, 5); scene.add(glow);

  const accent = new MeshStandardMaterial({ roughness: 0.35, metalness: 0.1 });
  const head = new MeshBasicMaterial({ color: 0xffffff });
  const ringMat = new MeshBasicMaterial({ transparent: true, opacity: 0.8 });
  const reel = new Mesh(reelGeometry(), accent);
  const play = new Mesh(new ConeGeometry(0.55, 0.9, 3), head);
  play.rotation.z = -Math.PI / 2;
  play.position.z = 0.35;
  const ring = new Mesh(new TorusGeometry(3.1, 0.025, 12, 140), ringMat);
  const reelGroup = new Group();
  reelGroup.add(reel, play);
  reelGroup.rotation.x = -0.35;
  const group = new Group();
  group.add(reelGroup, ring);
  scene.add(group);
  const motes = new Points(dust(small ? 60 : 130, 16), new PointsMaterial({ size: 0.05, transparent: true, opacity: 0.7 }));
  scene.add(motes);

  const recolor = () => {
    const c = new Color(cssVar('--accent', '#3b82c4')), light = isLight();
    accent.color = c;
    ringMat.color = c;
    head.color = new Color(light ? '#0e1621' : '#ffffff');
    motes.material.color = new Color(light ? '#1f6cb3' : '#9fcbf2');
    glow.color = c;
    key.intensity = light ? 3 : 2.4;
  };
  recolor();
  return { scene, camera, group, reelGroup, ring, motes, recolor };
}

export function mountScene(canvas) {
  if (matchMedia('(prefers-reduced-motion: reduce)').matches) return;
  let renderer;
  try { renderer = new WebGLRenderer({ canvas, alpha: true, antialias: true, powerPreference: 'low-power' }); } catch { return; }
  const small = matchMedia('(max-width: 880px)').matches;
  renderer.setPixelRatio(Math.min(devicePixelRatio || 1, small ? 1.25 : 1.75));
  renderer.setClearColor(0x000000, 0);
  const { scene, camera, group, reelGroup, ring, motes, recolor } = build(small);

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

  let visible = true, raf = 0;
  const t0 = performance.now();
  new IntersectionObserver(([e]) => { visible = e.isIntersecting; if (visible && !raf) raf = requestAnimationFrame(frame); }).observe(canvas);
  document.addEventListener('visibilitychange', () => { if (!document.hidden && visible && !raf) raf = requestAnimationFrame(frame); });

  function frame(now) {
    raf = 0;
    if (!visible || document.hidden) return;
    const t = (now - t0) / 1000;
    eased.lerp(pointer, 0.05);
    group.rotation.y = eased.x * 0.22;
    group.rotation.x = -eased.y * 0.12;
    reelGroup.rotation.z = -t * 0.45;
    ring.rotation.set(1.1, 0, t * 0.1);
    motes.rotation.y = t * 0.02;
    renderer.render(scene, camera);
    raf = requestAnimationFrame(frame);
  }
  raf = requestAnimationFrame(frame);
  canvas.classList.add('live');
}
