/**
 * Conformance of the keymaps with docs/SHORTCUTS.md (PLAN.md §1.7): every row of the document's
 * tables maps, in each app that uses it, to declared command ids, with exactly the keys the row names.
 */
import { readFileSync } from 'node:fs';
import { describe, expect, it } from 'vitest';
import { isDeclared } from './catalog/index.ts';
import type { KeyBinding } from './keymap.ts';
import { commonKeymap, deckKeymap, gridKeymap, SLIDE_SHOW_ROW, writeKeymap } from './keymaps/index.ts';
import { chordId, parseShortcut } from './keys.ts';
import { cellShortcuts, expandShortcutExpression, isNoneCell, parseShortcutDoc } from './shortcutsDoc.ts';

const markdown = readFileSync(new URL('../../../docs/SHORTCUTS.md', import.meta.url), 'utf8');
const doc = parseShortcutDoc(markdown);
const APPS = { write: writeKeymap, grid: gridKeymap, deck: deckKeymap } as const;
const id = (k: string) => chordId(parseShortcut(k, 'other'));

describe('docs/SHORTCUTS.md parsing', () => {
  it('finds both tables', () => {
    expect(doc.rows.length).toBeGreaterThanOrEqual(55);
    expect(doc.slideShow.length).toBe(7);
    expect(doc.rows[0]?.shortcut).toBe('Ctrl+N / O / S / P / W');
  });

  it('expands shortcut expressions', () => {
    expect(expandShortcutExpression('Ctrl+N / O / S / P / W')).toEqual([
      'Ctrl+N',
      'Ctrl+O',
      'Ctrl+S',
      'Ctrl+P',
      'Ctrl+W',
    ]);
    expect(expandShortcutExpression('Ctrl+Z / Ctrl+Y (also Ctrl+Shift+Z)')).toEqual([
      'Ctrl+Z',
      'Ctrl+Y',
      'Ctrl+Shift+Z',
    ]);
    expect(expandShortcutExpression('Ctrl+Shift+> / <, Ctrl+] / [')).toEqual([
      'Ctrl+Shift+>',
      'Ctrl+Shift+<',
      'Ctrl+]',
      'Ctrl+[',
    ]);
    expect(expandShortcutExpression('Alt or F10')).toEqual(['Alt', 'F10']);
    expect(expandShortcutExpression('Ctrl+/ ★')).toEqual(['Ctrl+/']);
    expect(expandShortcutExpression('Alt+Shift+Arrows')).toHaveLength(4);
    expect(expandShortcutExpression('number, then Enter')).toHaveLength(11);
    expect(expandShortcutExpression('Shift+Arrows / Ctrl+Arrows (object selected)')).toHaveLength(8);
    expect(cellShortcuts('New comment (Shift+F2: note)')).toEqual(['Shift+F2']);
    expect(cellShortcuts('Go To (also F5)')).toEqual(['F5']);
    expect(cellShortcuts('New slide (in the thumbnail rail)')).toEqual([]);
  });
});

describe('every shortcut row of docs/SHORTCUTS.md maps to declared command ids', () => {
  for (const row of doc.rows) {
    it(row.shortcut, () => {
      const expected = new Set(expandShortcutExpression(row.shortcut).map(id));
      const covered = new Set<string>();
      const commonRow = commonKeymap.filter((b) => b.row === row.shortcut);
      for (const [app, keymap] of Object.entries(APPS)) {
        const cell = row[app as keyof typeof APPS];
        const bindings: KeyBinding[] = [...commonRow, ...keymap.filter((b) => b.row === row.shortcut)];
        if (isNoneCell(cell)) {
          expect(bindings, `${app} should have no binding for "${row.shortcut}"`).toEqual([]);
          continue;
        }
        expect(bindings.length, `${app} has no binding for "${row.shortcut}" (${cell})`).toBeGreaterThan(0);
        const allowed = new Set([...expected, ...cellShortcuts(cell).map(id)]);
        for (const b of bindings) {
          expect(isDeclared(b.command), `${b.command} is not declared`).toBe(true);
          expect(allowed.has(id(b.key)), `${app}: ${b.key} is not part of "${row.shortcut}"`).toBe(true);
          covered.add(id(b.key));
        }
      }
      for (const k of expected) expect(covered.has(k), `no app binds ${k} of "${row.shortcut}"`).toBe(true);
    });
  }

  for (const row of doc.slideShow) {
    it(`${SLIDE_SHOW_ROW}${row.key}`, () => {
      const bindings = deckKeymap.filter((b) => b.row === `${SLIDE_SHOW_ROW}${row.key}`);
      const expected = new Set(expandShortcutExpression(row.key).map(id));
      expect(new Set(bindings.map((b) => id(b.key)))).toEqual(expected);
      for (const b of bindings) {
        expect(isDeclared(b.command)).toBe(true);
        expect(b.scope).toBe('slideshow');
      }
    });
  }

  it('no binding points at a row that is not in the document', () => {
    const rows = new Set([
      ...doc.rows.map((r) => r.shortcut),
      ...doc.slideShow.map((r) => `${SLIDE_SHOW_ROW}${r.key}`),
    ]);
    for (const b of [...commonKeymap, ...writeKeymap, ...gridKeymap, ...deckKeymap]) {
      expect(rows.has(b.row ?? ''), `${b.key} → ${b.command}: row "${b.row}"`).toBe(true);
    }
  });
});
