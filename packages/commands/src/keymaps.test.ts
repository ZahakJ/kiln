/**
 * The §1.7 table is fully bound (PLAN.md §1.7): every row of `keymaps/table.ts` maps, in each app that
 * uses it, to declared command ids with exactly the keys the row names, and docs/SHORTCUTS.md is the
 * text generated from the table and the keymaps.
 */
import { readFileSync } from 'node:fs';
import { describe, expect, it } from 'vitest';
import { isDeclared } from './catalog/index.ts';
import type { KeyBinding } from './keymap.ts';
import { shortcutsDocText } from './keymaps/doc.ts';
import { commonKeymap, deckKeymap, gridKeymap, SLIDE_SHOW_ROW, writeKeymap } from './keymaps/index.ts';
import { SHORTCUT_TABLE, SHORTCUT_TABLE_ROWS, SLIDE_SHOW_TABLE } from './keymaps/table.ts';
import { chordId, parseShortcut } from './keys.ts';
import { cellShortcuts, expandShortcutExpression, isNoneCell, parseShortcutDoc } from './shortcutsDoc.ts';

const APPS = { write: writeKeymap, grid: gridKeymap, deck: deckKeymap } as const;
const id = (k: string) => chordId(parseShortcut(k, 'other'));

describe('shortcut expressions', () => {
  it('expands the forms used by the table', () => {
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

describe('§1.7 table fully bound', () => {
  it(`has all ${SHORTCUT_TABLE_ROWS} rows of the plan's table and the 7 slide-show keys`, () => {
    expect(SHORTCUT_TABLE).toHaveLength(57);
    expect(SHORTCUT_TABLE_ROWS).toBe(57);
    expect(SLIDE_SHOW_TABLE).toHaveLength(7);
    expect(new Set(SHORTCUT_TABLE.map((r) => r.shortcut)).size).toBe(SHORTCUT_TABLE.length);
  });

  for (const row of SHORTCUT_TABLE) {
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

  for (const row of SLIDE_SHOW_TABLE) {
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

  it('no binding points at a row that is not in the table', () => {
    const rows = new Set([
      ...SHORTCUT_TABLE.map((r) => r.shortcut),
      ...SLIDE_SHOW_TABLE.map((r) => `${SLIDE_SHOW_ROW}${r.key}`),
    ]);
    for (const b of [...commonKeymap, ...writeKeymap, ...gridKeymap, ...deckKeymap]) {
      expect(rows.has(b.row ?? ''), `${b.key} → ${b.command}: row "${b.row}"`).toBe(true);
    }
  });
});

describe('docs/SHORTCUTS.md', () => {
  const committed = readFileSync(new URL('../../../docs/SHORTCUTS.md', import.meta.url), 'utf8');

  it('is generated from the table and keymaps (run `pnpm --filter @kiln/commands shortcuts:doc`)', () => {
    expect(committed).toBe(shortcutsDocText());
  });

  it('round-trips through the parser', () => {
    const doc = parseShortcutDoc(committed);
    expect(doc.rows).toEqual(SHORTCUT_TABLE.map((r) => ({ ...r })));
    expect(doc.slideShow).toEqual(SLIDE_SHOW_TABLE.map((r) => ({ ...r })));
  });
});
