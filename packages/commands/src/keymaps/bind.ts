import type { KeyBinding } from '../keymap.ts';

/** `bind('Mod+B', 'suite.format.bold', 'Ctrl+B / I / U')` — the row is the first column of docs/SHORTCUTS.md. */
export function bind(
  key: string,
  command: string,
  row: string,
  extra: Omit<KeyBinding, 'key' | 'command' | 'row'> = {},
): KeyBinding {
  return { key, command, row, ...extra };
}

/** Prefix of rows from the "During a slide show" table of docs/SHORTCUTS.md. */
export const SLIDE_SHOW_ROW = 'During a slide show: ';
