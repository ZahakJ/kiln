/** docs/SHORTCUTS.md as generated from the table and keymaps of this directory. */
import { formatChord, parseShortcut } from '../keys.ts';
import { renderShortcutsDoc } from '../shortcutsDoc.ts';
import { commonKeymap } from './common.ts';
import { deckKeymap } from './deck.ts';
import { gridKeymap } from './grid.ts';
import { SHORTCUT_TABLE, SLIDE_SHOW_TABLE } from './table.ts';
import { writeKeymap } from './write.ts';

/** Display label of a keymap key on Windows/Linux (`Mod+Shift+L` → `Ctrl+Shift+L`). */
function label(key: string): string {
  return formatChord(parseShortcut(key, 'other'), 'other');
}

export function shortcutsDocText(): string {
  return renderShortcutsDoc({
    rows: SHORTCUT_TABLE,
    slideShow: SLIDE_SHOW_TABLE,
    layers: { common: commonKeymap, write: writeKeymap, grid: gridKeymap, deck: deckKeymap },
    label,
  });
}
