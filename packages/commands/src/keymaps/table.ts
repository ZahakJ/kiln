/**
 * PLAN.md §1.7, the keyboard shortcut table, as data: one row per line of the plan's table, with what
 * the shortcut does in each app (`—` = nothing). `docs/SHORTCUTS.md` is generated from this table and the
 * keymaps (`pnpm --filter @kiln/commands shortcuts:doc`), and `keymaps.test.ts` fails unless every
 * row is bound in every app that uses it, so the table, the keymaps and the document cannot drift.
 */

export interface ShortcutRow {
  /** The shortcut column, verbatim (`Ctrl+N / O / S / P / W`). */
  shortcut: string;
  write: string;
  grid: string;
  deck: string;
}

export interface SlideShowRow {
  key: string;
  action: string;
}

/** The main table. */
export const SHORTCUT_TABLE: readonly ShortcutRow[] = [
  {
    shortcut: 'Ctrl+N / O / S / P / W',
    write: 'New / Open / Save / Print / Close',
    grid: 'same',
    deck: 'same',
  },
  { shortcut: 'F12 or Ctrl+Shift+S', write: 'Save As', grid: 'Save As', deck: 'Save As' },
  { shortcut: 'Ctrl+Z / Ctrl+Y (also Ctrl+Shift+Z)', write: 'Undo / Redo', grid: 'same', deck: 'same' },
  {
    shortcut: 'Ctrl+X / C / V, Ctrl+Alt+V',
    write: 'Cut / Copy / Paste, Paste Special',
    grid: 'same',
    deck: 'same',
  },
  {
    shortcut: 'Ctrl+Shift+C / Ctrl+Shift+V',
    write: 'Copy / Paste formatting (Format Painter)',
    grid: '—',
    deck: 'Copy / Paste formatting',
  },
  {
    shortcut: 'Ctrl+A',
    write: 'Select all',
    grid: 'Select region, then sheet',
    deck: 'Select all objects or text',
  },
  {
    shortcut: 'Ctrl+F / Ctrl+H / Ctrl+G',
    write: 'Find / Replace / Go To',
    grid: 'Find / Replace / Go To (also F5)',
    deck: 'Find / Replace / —',
  },
  { shortcut: 'Ctrl+B / I / U', write: 'Bold / Italic / Underline', grid: 'same', deck: 'same' },
  { shortcut: 'Ctrl+Shift+D', write: 'Double underline', grid: '—', deck: '—' },
  {
    shortcut: 'Ctrl+= / Ctrl+Shift+=',
    write: 'Subscript / Superscript',
    grid: '—',
    deck: 'Subscript / Superscript',
  },
  {
    shortcut: 'Ctrl+Shift+> / <, Ctrl+] / [',
    write: 'Grow / shrink font',
    grid: '—',
    deck: 'Grow / shrink font',
  },
  { shortcut: 'Ctrl+Shift+A', write: 'All caps', grid: '—', deck: '—' },
  { shortcut: 'Ctrl+Shift+L', write: 'Bullets', grid: 'Toggle AutoFilter', deck: 'Bullets' },
  {
    shortcut: 'Ctrl+E / L / R / J',
    write: 'Centre / Left / Right / Justify',
    grid: 'Flash Fill (Ctrl+E)',
    deck: 'Centre / Left / Right / Justify',
  },
  { shortcut: 'Ctrl+1 / 2 / 5', write: 'Line spacing 1 / 2 / 1.5', grid: 'Format Cells (Ctrl+1)', deck: '—' },
  { shortcut: 'Ctrl+M / Ctrl+Shift+M', write: 'Indent / Outdent', grid: '—', deck: 'New slide (Ctrl+M)' },
  {
    shortcut: 'Ctrl+Enter',
    write: 'Page break',
    grid: 'Fill the selection with the entry',
    deck: 'New slide (in the thumbnail rail)',
  },
  { shortcut: 'Ctrl+Shift+Enter', write: 'Column break', grid: 'Array formula (legacy)', deck: '—' },
  { shortcut: 'Ctrl+K', write: 'Insert link', grid: 'Insert link', deck: 'Insert link' },
  { shortcut: 'Ctrl+Shift+F', write: 'Font dialog', grid: '—', deck: '—' },
  { shortcut: 'Ctrl+D', write: 'Font dialog', grid: 'Fill down', deck: 'Duplicate' },
  { shortcut: 'Ctrl+Shift+N', write: 'Normal style', grid: '—', deck: '—' },
  { shortcut: 'Ctrl+Alt+1 / 2 / 3', write: 'Heading 1 / 2 / 3', grid: '—', deck: '—' },
  { shortcut: 'Ctrl+Space', write: 'Clear character formatting', grid: 'Select column', deck: '—' },
  { shortcut: 'Ctrl+Q', write: 'Clear paragraph formatting', grid: '—', deck: '—' },
  { shortcut: 'Shift+F3', write: 'Change case', grid: '—', deck: 'Change case' },
  { shortcut: 'F7 / Shift+F7', write: 'Spelling / Thesaurus', grid: 'Spelling', deck: 'Spelling' },
  { shortcut: 'Ctrl+Shift+E', write: 'Track changes', grid: '—', deck: '—' },
  { shortcut: 'Ctrl+Alt+M', write: 'New comment', grid: 'New comment (Shift+F2: note)', deck: 'New comment' },
  { shortcut: 'Ctrl+Shift+8', write: 'Show ¶', grid: 'Outline symbols', deck: '—' },
  {
    shortcut: 'Alt+Shift+Arrows',
    write: 'Move paragraph / promote and demote',
    grid: 'Group / ungroup rows and columns',
    deck: 'Promote / demote',
  },
  { shortcut: 'Ctrl+F1', write: 'Collapse or expand the ribbon', grid: 'same', deck: 'same' },
  { shortcut: 'Alt or F10', write: 'Key Tips', grid: 'same', deck: 'same' },
  { shortcut: 'Alt+Q, Ctrl+Shift+P ★', write: 'Search / command palette', grid: 'same', deck: 'same' },
  { shortcut: 'Ctrl+/ ★', write: 'Shortcut cheat sheet', grid: 'same', deck: 'same' },
  { shortcut: 'Ctrl+Shift+V', write: '—', grid: 'Paste values only ★', deck: '—' },
  {
    shortcut: 'F2',
    write: '—',
    grid: 'Edit the cell in place',
    deck: 'Edit the text of the selected object',
  },
  {
    shortcut: 'F4',
    write: 'Repeat last action',
    grid: 'Toggle `$` absolute and relative references; repeat',
    deck: 'Repeat',
  },
  { shortcut: 'F9 / Shift+F9', write: 'Update fields', grid: 'Recalculate workbook / sheet', deck: '—' },
  {
    shortcut: 'F5 / Shift+F5',
    write: 'Go To',
    grid: 'Go To',
    deck: 'Slide show from the start / from the current slide',
  },
  { shortcut: 'Ctrl+`', write: '—', grid: 'Show formulas', deck: '—' },
  { shortcut: 'Ctrl+; / Ctrl+Shift+;', write: 'Insert date / time', grid: 'Insert date / time', deck: '—' },
  { shortcut: 'Ctrl+T or Ctrl+L', write: '—', grid: 'Format as Table', deck: '—' },
  { shortcut: 'Ctrl+Shift+1 / 4 / 5', write: '—', grid: 'Number / Currency / Percent format', deck: '—' },
  { shortcut: 'Ctrl+Shift+& / _', write: '—', grid: 'Outline border / remove borders', deck: '—' },
  {
    shortcut: 'Ctrl+9 / Ctrl+0, Ctrl+Shift+9 / 0',
    write: '—',
    grid: 'Hide rows / columns; unhide',
    deck: '—',
  },
  {
    shortcut: 'Ctrl+Page Up / Page Down',
    write: '—',
    grid: 'Previous / next sheet',
    deck: 'Previous / next slide',
  },
  {
    shortcut: 'Ctrl+Arrows, Ctrl+Shift+Arrows, Ctrl+Home / End',
    write: 'Move by word or paragraph',
    grid: 'Jump to the edge of data, extend, A1, last cell',
    deck: '—',
  },
  { shortcut: 'Shift+Space', write: '—', grid: 'Select row', deck: '—' },
  { shortcut: 'Alt+=', write: '—', grid: 'AutoSum', deck: '—' },
  { shortcut: 'Alt+Enter', write: '—', grid: 'New line in the cell', deck: '—' },
  { shortcut: 'Alt+Down', write: '—', grid: 'Open the filter or autocomplete list', deck: '—' },
  { shortcut: 'Ctrl+- / Ctrl+Shift+=', write: '—', grid: 'Delete / insert cells', deck: '—' },
  { shortcut: 'Shift+F11', write: '—', grid: 'New sheet', deck: '—' },
  { shortcut: 'Ctrl+Shift+G / Ctrl+Shift+H', write: '—', grid: '—', deck: 'Group / ungroup' },
  {
    shortcut: 'Shift+Arrows / Ctrl+Arrows (object selected)',
    write: '—',
    grid: '—',
    deck: 'Resize / nudge the shape',
  },
  {
    shortcut: 'Esc',
    write: 'Cancel, close the pane',
    grid: 'Cancel the edit',
    deck: 'Deselect, end the show',
  },
];

/** Keys during a slide show (Deck, scope `slideshow`). */
export const SLIDE_SHOW_TABLE: readonly SlideShowRow[] = [
  { key: 'N, Space, Right, Page Down', action: 'Next' },
  { key: 'P, Backspace, Left, Page Up', action: 'Previous' },
  { key: 'Home / End', action: 'First / last slide' },
  { key: 'number, then Enter', action: 'Go to that slide' },
  { key: 'B / W', action: 'Black / white screen' },
  { key: 'Ctrl+L / Ctrl+P / Ctrl+I', action: 'Laser pointer / pen / highlighter' },
  { key: 'Esc', action: 'End the show' },
];

/** Number of rows of the main table (asserted by the tests: a dropped row fails them). */
export const SHORTCUT_TABLE_ROWS = 57;
