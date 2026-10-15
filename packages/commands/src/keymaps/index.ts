/**
 * The single source of truth for shortcuts (PLAN.md §1.7): the editors, Electron menu accelerators,
 * Key Tips, Options ▸ Keyboard and the shortcut cheat sheet all read these tables. Every binding
 * names the docs/SHORTCUTS.md row it implements; `keymaps.test.ts` checks the document against them.
 */
import { type KeyBinding, Keymap, type KeymapLayerId, type KeymapOptions } from '../keymap.ts';
import { commonKeymap } from './common.ts';
import { deckKeymap } from './deck.ts';
import { gridKeymap } from './grid.ts';
import { writeKeymap } from './write.ts';

export { SLIDE_SHOW_ROW } from './bind.ts';
export { commonKeymap, deckKeymap, gridKeymap, writeKeymap };

export const DEFAULT_KEYMAP_LAYERS: Readonly<Partial<Record<KeymapLayerId, readonly KeyBinding[]>>> =
  Object.freeze({
    common: commonKeymap,
    write: writeKeymap,
    grid: gridKeymap,
    deck: deckKeymap,
    notes: [],
    hub: [],
  });

/** A keymap with the Office default shortcuts and the user's saved remapping. */
export function createDefaultKeymap(options: Omit<KeymapOptions, 'layers'> = {}): Keymap {
  return new Keymap({ ...options, layers: DEFAULT_KEYMAP_LAYERS });
}
