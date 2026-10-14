import './style.css';

const nav = document.getElementById('nav');
const onScroll = () => nav?.classList.toggle('scrolled', scrollY > 8);
addEventListener('scroll', onScroll, { passive: true }); onScroll();

document.getElementById('burger')?.addEventListener('click', (e) => {
  const open = nav.classList.toggle('open');
  e.currentTarget.setAttribute('aria-expanded', String(open));
});

// highlight the current page in nav + docs sidebar
const path = location.pathname.replace(/\/index\.html$/, '/').replace(/\.html$/, '');
document.querySelectorAll('.nav-links a, .docs-nav a').forEach((a) => {
  const p = new URL(a.href, location.origin);
  if (p.hash === '' && p.pathname === path) a.classList.add('active');
});

// scroll reveal
const io = new IntersectionObserver((es) => es.forEach((e) => { if (e.isIntersecting) { e.target.classList.add('in'); io.unobserve(e.target); } }), { threshold: 0.12 });
document.querySelectorAll('.reveal, .shot').forEach((el) => io.observe(el));

// feature tabs
const tabs = document.querySelectorAll('.tab');
tabs.forEach((t) => t.addEventListener('click', () => {
  tabs.forEach((x) => x.setAttribute('aria-selected', String(x === t)));
  document.querySelectorAll('.tabpanel').forEach((p) => p.classList.toggle('on', p.id === t.dataset.tab));
}));

// OS detection for the primary download button
const ua = navigator.userAgent;
const win = /Windows/i.test(ua);
document.querySelectorAll('[data-os-cta]').forEach((a) => {
  if (win) { a.textContent = 'Download for Windows'; a.href = a.dataset.win; }
  else { a.textContent = 'Download for macOS'; a.href = a.dataset.mac; }
});

// animated stage canvases (subtle moving backgrounds, same idea as the app's)
document.querySelectorAll('canvas[data-bg]').forEach((c) => {
  const ctx = c.getContext('2d'); let t = 0;
  const fit = () => { c.width = c.clientWidth * 1.5; c.height = c.clientHeight * 1.5; }; fit(); addEventListener('resize', fit);
  const blobs = [[.2, .3, 0, '59,130,196'], [.75, .25, 2, '130,90,230'], [.5, .8, 4, '40,200,170'], [.85, .75, 1, '59,130,196']];
  const reduce = matchMedia('(prefers-reduced-motion: reduce)').matches;
  const draw = () => {
    t += reduce ? 0 : 0.004; const { width: w, height: h } = c;
    ctx.fillStyle = '#0a0f18'; ctx.fillRect(0, 0, w, h);
    for (const [x, y, ph, rgb] of blobs) {
      const cx = (x + Math.sin(t + ph) * .08) * w, cy = (y + Math.cos(t * .8 + ph) * .1) * h, r = Math.max(w, h) * .45;
      const g = ctx.createRadialGradient(cx, cy, 0, cx, cy, r);
      g.addColorStop(0, `rgba(${rgb},.45)`); g.addColorStop(1, `rgba(${rgb},0)`);
      ctx.fillStyle = g; ctx.fillRect(0, 0, w, h);
    }
    if (!reduce) requestAnimationFrame(draw);
  };
  draw();
});
