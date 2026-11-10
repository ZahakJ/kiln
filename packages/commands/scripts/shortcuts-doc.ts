/**
 * Writes docs/SHORTCUTS.md and docs/manual/shortcuts.md from the shortcut table and the keymaps
 * (`pnpm --filter @kiln/commands shortcuts:doc`). `--check` exits 1 when a committed file differs
 * (used by the unit test too).
 */
import { readFileSync, writeFileSync } from 'node:fs';
import { manualShortcutsText, shortcutsDocText } from '../src/keymaps/doc.ts';

const targets = [
  ['docs/SHORTCUTS.md', shortcutsDocText()],
  ['docs/manual/shortcuts.md', manualShortcutsText()],
] as const;
let stale = false;
for (const [path, text] of targets) {
  const target = new URL(`../../../${path}`, import.meta.url);
  if (process.argv.includes('--check')) {
    if (readFileSync(target, 'utf8') !== text) {
      console.error(`${path} is out of date: run \`pnpm --filter @kiln/commands shortcuts:doc\``);
      stale = true;
    } else console.log(`${path} is up to date`);
  } else {
    writeFileSync(target, text);
    console.log(`wrote ${path}`);
  }
}
if (stale) process.exit(1);
