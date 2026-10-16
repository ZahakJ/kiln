import { shiftAddress } from '@kiln-ref/formula';
import { describe, expect, it } from 'vitest';
import {
  type CellRange,
  type DrawingAnchor,
  MAX_COLS,
  MAX_ROWS,
  ModelError,
  parseRange,
  type StructuralEdit,
  shiftBreaks,
  shiftCell,
  shiftDrawing,
  shiftFrozenCount,
  shiftMerges,
  shiftRange,
  shiftTable,
  type Table,
} from '../src/index.ts';
import { int, rng } from './helpers.ts';

const ins = (at: number, count = 1): StructuralEdit => ({ kind: 'insertRows', at, count });
const del = (at: number, count = 1): StructuralEdit => ({ kind: 'deleteRows', at, count });
const insC = (at: number, count = 1): StructuralEdit => ({ kind: 'insertCols', at, count });
const delC = (at: number, count = 1): StructuralEdit => ({ kind: 'deleteCols', at, count });

const R = (a1: string): CellRange => parseRange(a1) as CellRange;

/** Range shifting matrix: [range, edit, expected (A1 or null)]. Rows are 1-based in A1 text. */
const RANGE_CASES: [string, StructuralEdit, string | null][] = [
  // Insert rows (0-based `at`: 4 = before row 5).
  ['B2:C3', ins(4), 'B2:C3'], // edit below the range
  ['B5:C8', ins(4), 'B6:C9'], // insert at the first row moves the range
  ['B5:C8', ins(4, 3), 'B8:C11'],
  ['B5:C8', ins(5), 'B5:C9'], // inside: grows
  ['B5:C8', ins(7, 2), 'B5:C10'], // at the last row: grows
  ['B5:C8', ins(8), 'B5:C8'], // just after: unchanged
  ['B5:C8', ins(0, 10), 'B15:C18'],
  ['A1', ins(0), 'A2'],
  ['A1', ins(1), 'A1'],
  ['D10', ins(9, 5), 'D15'],
  ['A:A', ins(3), 'A:A'], // whole columns never change for row edits
  ['3:5', ins(3), '3:6'],
  ['3:5', ins(1), '4:6'],
  // Delete rows.
  ['B5:C8', del(0, 2), 'B3:C6'], // above
  ['B5:C8', del(4), 'B5:C7'], // first row
  ['B5:C8', del(5, 2), 'B5:C6'], // middle
  ['B5:C8', del(7), 'B5:C7'], // last row
  ['B5:C8', del(4, 4), null], // exactly the range
  ['B5:C8', del(2, 10), null], // covers the range
  ['B5:C8', del(2, 4), 'B3:C4'], // overlaps the top
  ['B5:C8', del(6, 10), 'B5:C6'], // overlaps the bottom
  ['B5:C8', del(8, 3), 'B5:C8'], // below
  ['A1', del(0), null],
  ['A2', del(0), 'A1'],
  ['A:A', del(0, 100), 'A:A'],
  ['3:5', del(3), '3:4'],
  // Insert columns.
  ['C2:E4', insC(2), 'D2:F4'],
  ['C2:E4', insC(3), 'C2:F4'],
  ['C2:E4', insC(5), 'C2:E4'],
  ['C2:E4', insC(0, 3), 'F2:H4'],
  ['1:1', insC(0), '1:1'],
  ['B:C', insC(1), 'C:D'],
  ['B:C', insC(2, 2), 'B:E'],
  // Delete columns.
  ['C2:E4', delC(0), 'B2:D4'],
  ['C2:E4', delC(2), 'C2:D4'],
  ['C2:E4', delC(3), 'C2:D4'],
  ['C2:E4', delC(2, 3), null],
  ['C2:E4', delC(1, 3), 'B2:B4'],
  ['C2:E4', delC(4, 5), 'C2:D4'],
  ['C2:E4', delC(5, 5), 'C2:E4'],
  ['B:C', delC(1), 'B:B'],
  ['B:C', delC(1, 2), null],
  ['2:2', delC(0, 20), '2:2'],
];

const fmt = (r: CellRange | null) => {
  if (!r) return null;
  if (r.r0 === 0 && r.r1 === MAX_ROWS - 1) return `${col(r.c0)}:${col(r.c1)}`;
  if (r.c0 === 0 && r.c1 === MAX_COLS - 1) return `${r.r0 + 1}:${r.r1 + 1}`;
  const a = `${col(r.c0)}${r.r0 + 1}`;
  const b = `${col(r.c1)}${r.r1 + 1}`;
  return a === b ? a : `${a}:${b}`;
};
const col = (c: number) => String.fromCharCode(65 + c);

describe('range shifting matrix', () => {
  it.each(RANGE_CASES)('%s with %o → %s', (a1, edit, expected) => {
    expect(fmt(shiftRange(R(a1), edit))).toBe(expected);
  });
});

describe('agreement with the formula engine', () => {
  it('matches shiftAddress for random areas and edits', () => {
    const r = rng(2026);
    const kinds: StructuralEdit['kind'][] = ['insertRows', 'deleteRows', 'insertCols', 'deleteCols'];
    for (let i = 0; i < 2000; i++) {
      const r0 = int(r, 0, 30);
      const c0 = int(r, 0, 30);
      const range = { r0, c0, r1: r0 + int(r, 0, 10), c1: c0 + int(r, 0, 10) };
      const edit: StructuralEdit = {
        kind: kinds[int(r, 0, 3)] as StructuralEdit['kind'],
        at: int(r, 0, 40),
        count: int(r, 1, 6),
      };
      const ours = shiftRange(range, edit);
      const theirs = shiftAddress(
        { kind: 'area', ...range, r0Abs: false, c0Abs: false, r1Abs: false, c1Abs: false },
        edit,
      ) as { r0: number; c0: number; r1: number; c1: number } | null;
      expect(ours).toEqual(theirs ? { r0: theirs.r0, c0: theirs.c0, r1: theirs.r1, c1: theirs.c1 } : null);
    }
  });
});

describe('cells', () => {
  it.each([
    [4, 2, ins(4), { row: 5, col: 2 }],
    [3, 2, ins(4), { row: 3, col: 2 }],
    [4, 2, del(4), null],
    [6, 2, del(4, 2), { row: 4, col: 2 }],
    [6, 2, insC(2), { row: 6, col: 3 }],
    [6, 2, delC(0, 2), { row: 6, col: 0 }],
    [6, 2, delC(2), null],
  ] as const)('(%i,%i) %o → %o', (row, c, edit, expected) => {
    expect(shiftCell(row, c, edit)).toEqual(expected);
  });
});

describe('merges', () => {
  it.each([
    ['B2:C3', ins(2), ['B2:C4']], // inside: grows
    ['B2:C3', ins(1), ['B3:C4']], // at the first row: moves
    ['B2:C3', del(1), ['B2:C2']], // one row of two cells is still a merge
    ['B2:B3', del(1), []], // collapses to a single cell: dropped
    ['B2:C3', delC(1), ['B2:B3']],
    ['B2:C2', delC(1), []],
    ['B2:D3', delC(2), ['B2:C3']],
  ])('%s %o → %o', (a1, edit, expected) => {
    expect(shiftMerges([R(a1)], edit as StructuralEdit).map(fmt)).toEqual(expected);
  });
});

describe('frozen panes and page breaks', () => {
  it.each([
    [3, ins(0), 4],
    [3, ins(2, 2), 5],
    [3, ins(3), 3],
    [3, del(0), 2],
    [3, del(1, 5), 1],
    [3, del(3), 3],
    [0, ins(0), 0],
  ] as const)('freeze %i %o → %i', (n, edit, expected) => {
    expect(shiftFrozenCount(n, edit)).toBe(expected);
  });

  it('shifts and drops page breaks', () => {
    expect(shiftBreaks([5, 10, 20], ins(8, 2))).toEqual([5, 12, 22]);
    expect(shiftBreaks([5, 10, 20], del(9, 3))).toEqual([5, 17]);
    expect(shiftBreaks([1], del(0))).toEqual([]);
  });
});

describe('tables', () => {
  const table = (): Table => ({
    id: 1,
    name: 'Sales',
    range: R('B2:D6'),
    headerRow: true,
    totalsRow: true,
    columns: [{ name: 'Region' }, { name: 'Qty' }, { name: 'Price' }],
  });

  it('grows with rows inserted inside and moves with rows inserted above', () => {
    expect(fmt(shiftTable(table(), ins(3, 2))?.range ?? null)).toBe('B2:D8');
    expect(fmt(shiftTable(table(), ins(1))?.range ?? null)).toBe('B3:D7');
  });

  it('refuses to delete the header row alone', () => {
    expect(() => shiftTable(table(), del(1))).toThrow(ModelError);
  });

  it('turns the totals row off when it is deleted', () => {
    const t = shiftTable(table(), del(5)) as Table;
    expect(t.totalsRow).toBe(false);
    expect(fmt(t.range)).toBe('B2:D5');
  });

  it('disappears when all its rows are deleted', () => {
    expect(shiftTable(table(), del(0, 10))).toBeNull();
  });

  it('keeps at least one data row', () => {
    const t = shiftTable(table(), del(2, 3)) as Table;
    expect(fmt(t.range)).toBe('B2:D4');
    expect(t.totalsRow).toBe(true);
  });

  it('names inserted columns and drops deleted ones', () => {
    const t = shiftTable(table(), insC(2)) as Table;
    expect(t.columns.map((c) => c.name)).toEqual(['Region', 'Column1', 'Qty', 'Price']);
    expect(fmt(t.range)).toBe('B2:E6');
    const d = shiftTable(table(), delC(2)) as Table;
    expect(d.columns.map((c) => c.name)).toEqual(['Region', 'Price']);
    expect(shiftTable(table(), delC(1, 3))).toBeNull();
  });

  it('re-indexes its filter columns', () => {
    const t = {
      ...table(),
      autoFilter: { columns: [{ colId: 2, filter: { type: 'values' as const, values: ['1'] } }] },
    };
    expect(shiftTable(t, delC(2))?.autoFilter?.columns[0]?.colId).toBe(1);
    expect(shiftTable(t, delC(3))?.autoFilter?.columns).toEqual([]);
  });
});

describe('drawings', () => {
  const pic = (editAs?: 'twoCell' | 'oneCell' | 'absolute'): DrawingAnchor => ({
    id: 'p1',
    kind: 'picture',
    ref: 'image1',
    anchor: {
      type: 'twoCell',
      from: { row: 4, col: 1, rowOff: 100, colOff: 0 },
      to: { row: 8, col: 3, rowOff: 0, colOff: 0 },
      ...(editAs ? { editAs } : {}),
    },
  });

  it('moves and sizes with cells by default', () => {
    const a = shiftDrawing(pic(), ins(6, 2)).anchor;
    expect(a.type === 'twoCell' && [a.from.row, a.to.row]).toEqual([4, 10]);
    const b = shiftDrawing(pic(), ins(0, 2)).anchor;
    expect(b.type === 'twoCell' && [b.from.row, b.to.row]).toEqual([6, 10]);
  });

  it('moves without sizing for oneCell anchors', () => {
    const a = shiftDrawing(pic('oneCell'), ins(6, 2)).anchor;
    expect(a.type === 'twoCell' && [a.from.row, a.to.row]).toEqual([4, 8]);
    const b = shiftDrawing(pic('oneCell'), ins(2, 3)).anchor;
    expect(b.type === 'twoCell' && [b.from.row, b.to.row]).toEqual([7, 11]);
  });

  it('never moves absolute anchors', () => {
    expect(shiftDrawing(pic('absolute'), ins(0, 5))).toEqual(pic('absolute'));
  });

  it('collapses onto the deletion point', () => {
    const a = shiftDrawing(pic(), del(3, 3)).anchor;
    expect(a.type === 'twoCell' && [a.from.row, a.from.rowOff, a.to.row]).toEqual([3, 0, 5]);
  });
});
