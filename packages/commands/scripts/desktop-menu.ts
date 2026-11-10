/**
 * Writes apps/desktop/src-tauri/menu.json, the desktop shell's native menus with their labels in
 * every interface language (`pnpm --filter @kiln/commands desktop:menu`; the shell's build runs it
 * first). `--check` exits 1 when the committed file differs (the unit test runs the same check).
 */
import { readFileSync, writeFileSync } from 'node:fs';
import { desktopMenuText } from '../src/desktop-menu-file.ts';

const target = new URL('../../../apps/desktop/src-tauri/menu.json', import.meta.url);
const text = desktopMenuText();
if (process.argv.includes('--check')) {
  let current = '';
  try {
    current = readFileSync(target, 'utf8');
  } catch {
    // Missing: reported below.
  }
  if (current !== text) {
    console.error(
      'apps/desktop/src-tauri/menu.json is out of date: run `pnpm --filter @kiln/commands desktop:menu`',
    );
    process.exit(1);
  }
  console.log('apps/desktop/src-tauri/menu.json is up to date');
} else {
  writeFileSync(target, text);
  console.log('wrote apps/desktop/src-tauri/menu.json');
}
