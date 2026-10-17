/**
 * Writes docs/SHORTCUTS.md from the §1.7 table and the keymaps (`pnpm --filter @kiln/commands
 * shortcuts:doc`). `--check` exits 1 when the committed file differs (used by the unit test too).
 */
import { readFileSync, writeFileSync } from 'node:fs';
import { shortcutsDocText } from '../src/keymaps/doc.ts';

const target = new URL('../../../docs/SHORTCUTS.md', import.meta.url);
const text = shortcutsDocText();
if (process.argv.includes('--check')) {
  const current = readFileSync(target, 'utf8');
  if (current !== text) {
    console.error('docs/SHORTCUTS.md is out of date: run `pnpm --filter @kiln/commands shortcuts:doc`');
    process.exit(1);
  }
  console.log('docs/SHORTCUTS.md is up to date');
} else {
  writeFileSync(target, text);
  console.log('wrote docs/SHORTCUTS.md');
}
