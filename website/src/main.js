import './style.css';
import { STRINGS, LANGS, detect } from './i18n.js';

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

const store = {
  get(k) { try { return localStorage.getItem(k); } catch { return null; } },
  set(k, v) { try { localStorage.setItem(k, v); } catch { /* private mode: fine */ } },
};

// theme: dark unless the visitor chose otherwise (the choice is kept only in this browser)
const root = document.documentElement;
document.querySelectorAll('.theme-btn').forEach((b) => b.addEventListener('click', () => {
  const next = root.dataset.theme === 'light' ? 'dark' : 'light';
  root.dataset.theme = next;
  store.set('sc-theme', next);
}));

// language
const isMac = !/Windows/i.test(navigator.userAgent);
function applyLang(lang) {
  const dict = { ...STRINGS.en, ...(STRINGS[lang] || {}) };
  root.lang = lang === 'zh-CN' ? 'zh-Hans' : lang === 'zh-TW' ? 'zh-Hant' : lang;
  document.querySelectorAll('[data-i18n]').forEach((el) => {
    const v = dict[el.dataset.i18n];
    if (v != null) el.innerHTML = v;
  });
  document.querySelectorAll('[data-i18n-os]').forEach((el) => {
    el.textContent = dict[isMac ? 'dl.mac' : 'dl.win'];
  });
  document.querySelectorAll('[data-lang-select]').forEach((sel) => {
    sel.value = lang;
    // size the select to the chosen language so the caret sits right beside the text
    const probe = document.createElement('span');
    probe.style.cssText = 'position:absolute;visibility:hidden;white-space:nowrap;font:500 15px var(--font)';
    probe.textContent = sel.options[sel.selectedIndex].text;
    document.body.append(probe);
    sel.style.width = Math.ceil(probe.getBoundingClientRect().width) + 22 + 'px';
    probe.remove();
  });
}
const saved = store.get('sc-lang');
let lang = LANGS.includes(saved) ? saved : detect();
document.querySelectorAll('[data-lang-select]').forEach((sel) => sel.addEventListener('change', () => {
  lang = sel.value;
  store.set('sc-lang', lang);
  applyLang(lang);
}));
applyLang(lang);

// the hero button downloads the installer for the visitor's system
document.querySelectorAll('[data-os-cta]').forEach((a) => {
  a.href = isMac ? a.dataset.mac : a.dataset.win;
});
