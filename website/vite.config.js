import { defineConfig } from 'vite';
import { readFileSync, readdirSync, statSync } from 'node:fs';
import { resolve, join } from 'node:path';
import { marked } from 'marked';

const root = import.meta.dirname;
const appVersion = JSON.parse(readFileSync(join(root, '../src-tauri/tauri.conf.json'), 'utf8')).version;
const partial = (n) => readFileSync(join(root, 'src/partials', n + '.html'), 'utf8');

// Every .html file (outside node_modules/dist) becomes a page.
function pages(dir = root, out = {}) {
  for (const f of readdirSync(dir)) {
    if (['node_modules', 'dist', 'src', 'public'].includes(f)) continue;
    const p = join(dir, f);
    if (statSync(p).isDirectory()) pages(p, out);
    else if (f.endsWith('.html')) out[p.slice(root.length + 1).replace(/\.html$/, '')] = p;
  }
  return out;
}

// Shared header / footer / docs sidebar, injected at build and dev time.
const partials = {
  name: 'partials',
  transformIndexHtml: {
    order: 'pre',
    handler(html) {
      return html
        .replaceAll('/downloads/ScreenCraft.dmg', `/downloads/ScreenCraft-${appVersion}.dmg`)
        .replace('<!--@header-->', partial('header'))
        .replace('<!--@footer-->', partial('footer'))
        .replace('<!--@docs-nav-->', partial('docs-nav'))
        .replace('<!--@changelog-->', () => marked.parse(readFileSync(join(root, '../CHANGELOG.md'), 'utf8').replace(/^# Changelog\s*/, '')))
        .replace('<!--@whatsnew-->', () => marked.parse(readFileSync(join(root, '../CHANGELOG.md'), 'utf8').split(/\n(?=## )/).slice(1, 3).join('\n').replace(/^## /gm, '### ')))
        .replace('<!--@notices-->', () => marked.parse(readFileSync(join(root, '../THIRD-PARTY-NOTICES.md'), 'utf8')));
    },
  },
};

export default defineConfig({
  plugins: [partials],
  build: { rollupOptions: { input: pages() } },
});
