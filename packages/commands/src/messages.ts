/**
 * The interface messages the command system holds as data: each declared command's title
 * (`cmd.<id>`) and ScreenTip description (`cmd.<id>.description`), Office names, categories, the
 * function categories of Formulas ▸ Function Library, every label of the ribbon definitions and the
 * desktop shell's menus and dialogs.
 * `@kiln/i18n`'s extractor merges them into the English locale; a unit test keeps them in it.
 */

import { GRID_FUNCTION_MENUS } from './catalog/grid-functions.ts';
import { COMMAND_CATALOG, DECK_SLIDE_MASTER_TAB, RIBBONS } from './catalog/index.ts';
import { DESKTOP_STRINGS } from './desktop-menu.ts';

/** Ribbon fields shown to people. `sample` is a typographic preview, never translated. */
const LABEL_FIELDS = new Set([
  'label',
  'description',
  'hint',
  'placeholder',
  'tooltip',
  'title',
  'group',
  'heading',
]);
const SKIP_FIELDS = new Set([
  'id',
  'command',
  'icon',
  'args',
  'value',
  'keyTip',
  'sample',
  'look',
  'context',
]);

/** A worksheet function's name, shown in the Function Library menus. */
const FUNCTION_NAME = /^[A-Z][A-Z0-9]*(\.[A-Z0-9]+)*$/;

function collectLabels(node: unknown, add: (text: string) => void): void {
  if (Array.isArray(node)) {
    for (const n of node) collectLabels(n, add);
    return;
  }
  if (!node || typeof node !== 'object') return;
  for (const [k, v] of Object.entries(node)) {
    if (SKIP_FIELDS.has(k)) continue;
    if (typeof v === 'string') {
      // Excel function names (SUM, BINOM.DIST) stay as Excel spells them.
      if (LABEL_FIELDS.has(k) && /\p{L}/u.test(v) && !FUNCTION_NAME.test(v)) add(v);
    } else if (v && typeof v === 'object') collectLabels(v, add);
  }
}

/** Every message of the command system: key → English. */
export function commandMessages(): Record<string, string> {
  const out: Record<string, string> = {};
  const text = (s: string) => {
    out[s] = s;
  };
  for (const d of COMMAND_CATALOG) {
    if (d.apps?.length && d.apps.every((a) => a === 'notes')) continue; // Kiln Notes is not in v0.1.
    out[`cmd.${d.id}`] = d.title;
    if (d.description) out[`cmd.${d.id}.description`] = d.description;
    if (d.officeName) text(d.officeName);
    if (d.category) for (const part of d.category.split(' ▸ ')) text(part);
  }
  for (const category of Object.keys(GRID_FUNCTION_MENUS)) text(category);
  for (const [app, ribbon] of Object.entries(RIBBONS)) {
    if (app === 'notes') continue; // Kiln Notes is not part of v0.1.
    collectLabels(ribbon, text);
  }
  collectLabels(DECK_SLIDE_MASTER_TAB, text);
  // The desktop shell's menus, dialogs and windows.
  for (const s of Object.values(DESKTOP_STRINGS)) text(s);
  return out;
}
