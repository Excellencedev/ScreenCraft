import { defineConfig } from 'vite';
import { readFileSync, readdirSync, statSync } from 'node:fs';
import { resolve, join } from 'node:path';

const root = import.meta.dirname;
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
        .replace('<!--@header-->', partial('header'))
        .replace('<!--@footer-->', partial('footer'))
        .replace('<!--@docs-nav-->', partial('docs-nav'));
    },
  },
};

export default defineConfig({
  plugins: [partials],
  build: { rollupOptions: { input: pages() } },
});
