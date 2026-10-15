// Serve the installers from the site itself: copy them from the repo root into public/downloads.
import { copyFileSync, mkdirSync, existsSync, readFileSync, writeFileSync } from 'node:fs';
import { fileURLToPath } from 'node:url';
import { join, dirname } from 'node:path';

const root = join(dirname(fileURLToPath(import.meta.url)), '..', '..');
const out = join(dirname(fileURLToPath(import.meta.url)), '..', 'public', 'downloads');
mkdirSync(out, { recursive: true });
import { readdirSync, rmSync } from 'node:fs';
for (const f of ['ScreenCraft.dmg', 'ScreenCraft-Setup.exe']) {
  if (!existsSync(join(root, f))) throw new Error(`missing ${f} in the repository root`);
  copyFileSync(join(root, f), join(out, f));
}

// latest.json: what the app's notify-only update check reads. The version comes from the app's
// own config, the notes from the matching section of CHANGELOG.md (bold bullet titles).
const conf = { version: readFileSync(join(root, 'Cargo.toml'), 'utf8').match(/^version = "([^"]+)"/m)[1] };
const log = readFileSync(join(root, 'CHANGELOG.md'), 'utf8');
const section = log.split(/\n(?=## )/).find((b) => b.startsWith(`## ${conf.version}`)) || '';
const notes = [...section.matchAll(/^- \*\*(.+?)\*\*/gm)].map((m) => m[1].replace(/[.:]$/, '')).slice(0, 5);
// A version in the file name is a new URL for every release, so no browser or edge cache can
// hand out an older installer under the same address.
const dmgName = `ScreenCraft-${conf.version}.dmg`;
copyFileSync(join(root, 'ScreenCraft.dmg'), join(out, dmgName));
for (const f of readdirSync(out)) if (/^ScreenCraft-\d+\.\d+\.\d+\.dmg$/.test(f) && f !== dmgName) rmSync(join(out, f));
const base = 'https://screencraft.mdnlabs.dev';
const manifest = {
  version: conf.version,
  notes,
  mac: `${base}/downloads/${dmgName}`,
  windows: `${base}/downloads/ScreenCraft-Setup.exe`,
  page: `${base}/download`,
};
writeFileSync(join(dirname(fileURLToPath(import.meta.url)), '..', 'public', 'latest.json'), JSON.stringify(manifest, null, 2) + '\n');
