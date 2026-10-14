// Serve the installers from the site itself: copy them from the repo root into public/downloads.
import { copyFileSync, mkdirSync, existsSync } from 'node:fs';
import { fileURLToPath } from 'node:url';
import { join, dirname } from 'node:path';

const root = join(dirname(fileURLToPath(import.meta.url)), '..', '..');
const out = join(dirname(fileURLToPath(import.meta.url)), '..', 'public', 'downloads');
mkdirSync(out, { recursive: true });
for (const f of ['ScreenCraft.dmg', 'ScreenCraft-Setup.exe']) {
  if (!existsSync(join(root, f))) throw new Error(`missing ${f} in the repository root`);
  copyFileSync(join(root, f), join(out, f));
}
