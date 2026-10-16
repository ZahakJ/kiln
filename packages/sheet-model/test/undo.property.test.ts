/**
 * Property tests for the command log: random command sequences (seeded, reproducible) must satisfy
 * - every successful command keeps the engine's formulas identical to the store's;
 * - a command that throws changes nothing;
 * - undoing everything restores the initial state exactly, and redoing everything restores the final
 *   state exactly (both checked on the full snapshot, cells and metadata);
 * - after any undo/redo the computed values equal those of a freshly built calculation.
 *
 * Stress runs: PROP_SEEDS=400 PROP_STEPS=80 PROP_BASE=77 pnpm test -- test/undo.property.test.ts
 */
import { describe, expect, it } from 'vitest';
import {
  type CellRange,
  type Command,
  CommandLog,
  type EncodedValue,
  ModelError,
  restoreWorkbook,
  snapshotWorkbook,
  Workbook,
  WorkbookCalc,
} from '../src/index.ts';
import { expectEngineInSync, int, pick, rng, stateOf } from './helpers.ts';

const N = 12; // coordinates stay in a 12×12 corner so commands interact

const FORMULAS = [
  '=A1+B2',
  '=SUM(A1:C5)',
  '=A1*2',
  '=COUNT(A:A)',
  '=SUM($B$2:B4)',
  '=IFERROR(C3/D4,0)',
  '=Rate*3',
  '=LEN(B1)',
  '=MAX(A1:L12)',
  '=Two!A1+1',
  '=SUM(Two!B1:B5)',
  '=SEQUENCE(2,2)',
  '=SUM(Tab[Val])',
  '=ROWS(Tab[#All])',
  '=INDEX(A1:C3,2,2)',
  '=E5&"x"',
];

function randRange(r: () => number): CellRange {
  const r0 = int(r, 0, N - 1);
  const c0 = int(r, 0, N - 1);
  return { r0, c0, r1: Math.min(N + 3, r0 + int(r, 0, 3)), c1: Math.min(N + 3, c0 + int(r, 0, 3)) };
}

function randValue(r: () => number): EncodedValue {
  const k = int(r, 0, 5);
  if (k === 0) return null;
  if (k === 1) return pick(r, ['x', 'Pen', 'apple', '']);
  if (k === 2) return r() < 0.5;
  if (k === 3) return { e: '#N/A' };
  return int(r, -50, 50) / (k === 4 ? 1 : 4);
}

function randCommand(r: () => number, wb: Workbook): Command {
  const sheet = pick(r, wb.sheets);
  const s = sheet.id;
  const other = pick(r, wb.sheets).id;
  switch (int(r, 0, 27)) {
    case 0:
    case 1:
    case 2:
      return {
        type: 'setCells',
        sheet: s,
        cells: Array.from({ length: int(r, 1, 3) }, () => ({
          row: int(r, 0, N),
          col: int(r, 0, N),
          value: randValue(r),
        })),
      };
    case 3:
    case 4:
      return {
        type: 'setCells',
        sheet: s,
        cells: [{ row: int(r, 0, N), col: int(r, 0, N), formula: pick(r, FORMULAS) }],
      };
    case 5:
      return {
        type: 'setRange',
        sheet: s,
        row: int(r, 0, N),
        col: int(r, 0, N),
        values: Array.from({ length: int(r, 1, 3) }, () =>
          Array.from({ length: int(r, 1, 3) }, () => randValue(r)),
        ),
      };
    case 6:
      return {
        type: 'clearRange',
        sheet: s,
        ranges: [randRange(r)],
        what: pick(r, ['all', 'contents', 'formats', 'notes', 'hyperlinks'] as const),
      };
    case 7:
      return {
        type: 'setStyle',
        sheet: s,
        ranges: [randRange(r)],
        patch: pick(r, [
          { font: { bold: true } },
          { numFmt: '0.00' },
          { fill: { type: 'pattern', pattern: 'solid', fg: { rgb: 'FFFFFF00' } } },
          { alignment: { horizontal: 'center' } },
        ] as const),
      };
    case 8:
    case 9:
      return {
        type: pick(r, ['insertRows', 'insertCols'] as const),
        sheet: s,
        at: int(r, 0, N),
        count: int(r, 1, 3),
      };
    case 10:
    case 11:
      return {
        type: pick(r, ['deleteRows', 'deleteCols'] as const),
        sheet: s,
        at: int(r, 0, N),
        count: int(r, 1, 3),
      };
    case 12:
      return {
        type: 'moveRange',
        sheet: s,
        range: randRange(r),
        to: { sheet: other, row: int(r, 0, N), col: int(r, 0, N) },
      };
    case 13:
      return {
        type: 'copyRange',
        sheet: s,
        range: randRange(r),
        to: { sheet: other, range: randRange(r) },
        what: pick(r, ['all', 'values', 'formulas', 'formats'] as const),
        transpose: r() < 0.2,
      };
    case 14: {
      const range = randRange(r);
      return {
        type: 'sort',
        sheet: s,
        range,
        keys: [{ index: range.c0, descending: r() < 0.5 }],
        header: r() < 0.3,
      };
    }
    case 15:
      return r() < 0.5 || wb.sheets.length < 2
        ? { type: 'addSheet', index: int(r, 0, wb.sheets.length) }
        : { type: 'removeSheet', sheet: s };
    case 16:
      return { type: 'renameSheet', sheet: s, name: pick(r, ['Two', 'Data', 'My Sheet', 'Q1', sheet.name]) };
    case 17:
      return { type: 'moveSheet', sheet: s, index: int(r, 0, wb.sheets.length - 1) };
    case 18:
      return {
        type: 'setRowProps',
        sheet: s,
        at: int(r, 0, N),
        count: int(r, 1, 3),
        props: pick(r, [{ hidden: true }, { hidden: null }, { height: 30 }, { outlineLevel: 1 }] as const),
      };
    case 19:
      return {
        type: 'setColProps',
        sheet: s,
        at: int(r, 0, N),
        count: int(r, 1, 3),
        props: pick(r, [{ hidden: true }, { width: 20 }, { width: null }] as const),
      };
    case 20:
      return {
        type: 'merge',
        sheet: s,
        range: randRange(r),
        mode: pick(r, ['merge', 'center', 'across'] as const),
      };
    case 21:
      return { type: 'unmerge', sheet: s, range: randRange(r) };
    case 22:
      return r() < 0.7
        ? {
            type: 'defineName',
            name: 'Rate',
            formula: pick(r, ['0.5', `'${sheet.name}'!$A$1`, 'SUM(Two!$A$1:$A$3)']),
            scope: r() < 0.7 ? null : s,
          }
        : { type: 'deleteName', name: 'Rate', scope: null };
    case 23: {
      const r0 = int(r, 0, N - 3);
      const c0 = int(r, 0, N - 2);
      return {
        type: 'addTable',
        sheet: s,
        table: {
          name: 'Tab',
          range: { r0, c0, r1: r0 + 2, c1: c0 + 1 },
          headerRow: true,
          totalsRow: r() < 0.3,
          columns: [{ name: 'Key' }, { name: 'Val' }],
        },
      };
    }
    case 24:
      return r() < 0.5
        ? { type: 'removeTable', name: 'Tab' }
        : { type: 'updateTable', name: 'Tab', patch: { name: pick(r, ['Tab', 'Tab2']) } };
    case 25:
      return { type: 'setFreeze', sheet: s, rows: int(r, 0, 3), cols: int(r, 0, 3) };
    case 26: {
      const key = pick(r, ['notes', 'hyperlinks', 'validations'] as const);
      const row = int(r, 0, N);
      const col = int(r, 0, N);
      const value =
        r() < 0.2
          ? []
          : key === 'notes'
            ? [{ row, col, author: 'A', text: 'n' }]
            : key === 'hyperlinks'
              ? [{ ref: { r0: row, c0: col, r1: row, c1: col }, target: 'https://example.org' }]
              : [{ id: 1, ranges: [randRange(r)], type: 'list', formula1: pick(r, ['"a,b"', '$A$1:$A$3']) }];
      return { type: 'setSheetMeta', sheet: s, key, value };
    }
    default:
      return {
        type: 'setSheetMeta',
        sheet: s,
        key: 'cf',
        value: [
          {
            ranges: [randRange(r)],
            rules: [{ id: 1, priority: 1, type: 'expression', formula: 'A1>0', dxf: 0 }],
          },
        ],
      };
  }
}

function valueKey(v: unknown): unknown {
  return v !== null && typeof v === 'object' && 'code' in (v as object)
    ? `#${(v as { code: string }).code}`
    : v;
}

/** Computed values of every formula cell, compared against a calculation built from scratch. */
function expectValuesFresh(wb: Workbook, calc: WorkbookCalc): void {
  const fresh = new WorkbookCalc(restoreWorkbook(snapshotWorkbook(wb)));
  for (const s of wb.sheets) {
    const used = s.cells.getUsedRange();
    if (!used) continue;
    for (let r = used.r0; r <= Math.min(used.r1, used.r0 + 30); r++)
      for (let c = used.c0; c <= Math.min(used.c1, used.c0 + 30); c++)
        expect(valueKey(calc.getValue(s.id, r, c)), `${s.name}!R${r}C${c}`).toEqual(
          valueKey(fresh.getValue(s.id, r, c)),
        );
  }
  fresh.dispose();
}

function seedWorkbook(): Workbook {
  const wb = new Workbook({ sheets: ['One', 'Two'] });
  const log = new CommandLog(wb);
  const [a, b] = wb.sheets.map((s) => s.id) as [number, number];
  log.exec({
    type: 'setRange',
    sheet: a,
    row: 0,
    col: 0,
    values: [
      [1, 2, 3],
      [4, 5, 6],
      [7, 8, 9],
    ],
  });
  log.exec({ type: 'setRange', sheet: b, row: 0, col: 0, values: [[10], [20], [30]] });
  log.exec({
    type: 'setCells',
    sheet: a,
    cells: [
      { row: 4, col: 4, formula: '=SUM(A1:C3)' },
      { row: 5, col: 4, formula: '=Two!A1+E5' },
    ],
  });
  return wb;
}

describe('undo/redo invariants (property tests)', () => {
  const seeds = Array.from(
    { length: Number(process.env.PROP_SEEDS ?? 30) },
    (_, i) => Number(process.env.PROP_BASE ?? 1000) + i * 7919,
  );

  it.each(seeds)('random sequence with seed %i', (seed) => {
    const r = rng(seed);
    const wb = seedWorkbook();
    const calc = new WorkbookCalc(wb);
    const log = new CommandLog(wb);
    const initial = stateOf(wb);
    let applied = 0;
    for (let i = 0; i < Number(process.env.PROP_STEPS ?? 25); i++) {
      const cmd = randCommand(r, wb);
      const before = stateOf(wb);
      try {
        log.exec(cmd);
        applied++;
      } catch (e) {
        if (!(e instanceof ModelError)) throw new Error(`${JSON.stringify(cmd)} threw ${(e as Error).stack}`);
        expect(stateOf(wb), `failed ${cmd.type} must not change the workbook`).toEqual(before);
      }
      expectEngineInSync(wb, calc);
    }
    const final = stateOf(wb);
    expectValuesFresh(wb, calc);
    while (log.undo()) expectEngineInSync(wb, calc);
    expect(stateOf(wb)).toEqual(initial);
    expectValuesFresh(wb, calc);
    let redone = 0;
    while (log.redo()) redone++;
    expect(redone).toBe(Math.min(applied, 1000));
    expect(stateOf(wb)).toEqual(final);
    expectEngineInSync(wb, calc);
    expectValuesFresh(wb, calc);
  });

  it.each(seeds.slice(0, 10))('interleaved undo/redo with seed %i', (seed) => {
    const r = rng(seed ^ 0x5bd1e995);
    const wb = seedWorkbook();
    const calc = new WorkbookCalc(wb);
    const log = new CommandLog(wb);
    // Shadow history of states: states[k] is the state after k undoable steps.
    const states: unknown[] = [stateOf(wb)];
    let pos = 0;
    for (let i = 0; i < 40; i++) {
      const roll = r();
      if (roll < 0.25 && pos > 0) {
        log.undo();
        pos--;
        expect(stateOf(wb)).toEqual(states[pos]);
      } else if (roll < 0.4 && pos < states.length - 1) {
        log.redo();
        pos++;
        expect(stateOf(wb)).toEqual(states[pos]);
      } else {
        try {
          log.exec(randCommand(r, wb));
          pos++;
          states.length = pos;
          states.push(stateOf(wb));
        } catch (e) {
          if (!(e instanceof ModelError)) throw e;
        }
      }
      expectEngineInSync(wb, calc);
    }
    expectValuesFresh(wb, calc);
  });
});
