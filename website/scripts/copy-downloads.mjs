// Serve the installers from the site itself: copy them from the repo root into public/downloads.
// Installers are named after the version (0.4.0 -> screencraft_0_4_0.dmg / .exe), so every release
// is a new URL and no browser or edge cache can hand out an older installer.
import { copyFileSync, mkdirSync, existsSync, readFileSync, writeFileSync, readdirSync, rmSync } from 'node:fs';
import { fileURLToPath } from 'node:url';
import { join, dirname } from 'node:path';

const here = dirname(fileURLToPath(import.meta.url));
const root = join(here, '..', '..');
const out = join(here, '..', 'public', 'downloads');
mkdirSync(out, { recursive: true });

const version = readFileSync(join(root, 'Cargo.toml'), 'utf8').match(/^version = "([^"]+)"/m)[1];
const stem = `screencraft_${version.replaceAll('.', '_')}`;
const names = [`${stem}.dmg`, `${stem}.exe`];
for (const f of names) {
  if (!existsSync(join(root, f))) throw new Error(`missing ${f} in the repository root`);
  copyFileSync(join(root, f), join(out, f));
}
// drop installers of other versions
for (const f of readdirSync(out)) if (!names.includes(f) && /\.(dmg|exe)$/.test(f)) rmSync(join(out, f));

// latest.json: what the app's notify-only update check reads. The version comes from the app's
// own config, the notes from the matching section of CHANGELOG.md (bold bullet titles).
const log = readFileSync(join(root, 'CHANGELOG.md'), 'utf8');
const section = log.split(/\n(?=## )/).find((b) => b.startsWith(`## ${version}`)) || '';
const notes = [...section.matchAll(/^- \*\*(.+?)\*\*/gm)].map((m) => m[1].replace(/[.:]$/, '')).slice(0, 5);
const base = 'https://screencraft.mdnlabs.dev';
const manifest = {
  version,
  notes,
  mac: `${base}/downloads/${names[0]}`,
  windows: `${base}/downloads/${names[1]}`,
  page: `${base}/download`,
};
writeFileSync(join(here, '..', 'public', 'latest.json'), JSON.stringify(manifest, null, 2) + '\n');
