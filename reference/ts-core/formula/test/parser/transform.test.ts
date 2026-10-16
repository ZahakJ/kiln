import { describe, expect, it } from 'vitest';
import { parse } from '../../src/parser/parser.ts';
import { print } from '../../src/parser/printer.ts';
import { columnToLetters, formatCell } from '../../src/parser/references.ts';
import {
  copyFormula,
  cycleAbsolute,
  fromR1C1,
  moveFormula,
  removeSheetInFormula,
  renameSheetInFormula,
  shiftAddress,
  shiftFormula,
  toggleAbsolute,
  toR1C1,
} from '../../src/parser/transform.ts';
import { randInt, rng } from '../engine/_support.ts';

/** Canonical text (areas normalised, no spaces). */
const canon = (f: string) => print(parse(f), { leadingEquals: true });

const ins = (at: number, count = 1) => ({ kind: 'insertRows' as const, sheet: 'S', at, count });
const del = (at: number, count = 1) => ({ kind: 'deleteRows' as const, sheet: 'S', at, count });
const insC = (at: number, count = 1) => ({ kind: 'insertCols' as const, sheet: 'S', at, count });
const delC = (at: number, count = 1) => ({ kind: 'deleteCols' as const, sheet: 'S', at, count });

describe('shiftFormula: insert/delete rows and columns', () => {
  it.each([
    ['=A5', ins(4), '=A6'],
    ['=A5', ins(5), '=A5'],
    ['=A5', ins(0, 3), '=A8'],
    ['=$A$5', ins(2), '=$A$6'],
    ['=SUM(A1:A10)', ins(4, 2), '=SUM(A1:A12)'],
    ['=SUM(A1:A10)', ins(0), '=SUM(A2:A11)'],
    ['=SUM(A1:A10)', ins(10), '=SUM(A1:A10)'],
    ['=SUM(A:A)', ins(3), '=SUM(A:A)'],
    ['=SUM(3:5)', ins(2), '=SUM(4:6)'],
    ['=SUM(3:5)', ins(3), '=SUM(3:6)'],
    ['=A5', del(4), '=#REF!'],
    ['=A5+1', del(0, 2), '=A3+1'],
    ['=SUM(A1:A10)', del(2, 3), '=SUM(A1:A7)'],
    ['=SUM(A3:A10)', del(0, 5), '=SUM(A1:A5)'],
    ['=SUM(A3:A5)', del(2, 3), '=SUM(#REF!)'],
    ['=SUM(A1:A3)', del(2, 5), '=SUM(A1:A2)'],
    ['=C1', insC(2), '=D1'],
    ['=C1', insC(3), '=C1'],
    ['=SUM(B:D)', insC(2), '=SUM(B:E)'],
    ['=SUM(B:D)', delC(1), '=SUM(B:C)'],
    ['=C1', delC(2), '=#REF!'],
    ['=SUM(1:1)', insC(0), '=SUM(1:1)'],
  ])('%s with %o → %s', (src, change, out) => {
    expect(shiftFormula(src, change, 'S')).toBe(out);
  });

  it('only shifts references to the changed sheet', () => {
    expect(shiftFormula('=A5+Other!A5+S!A5', ins(0), 'S')).toBe('=A6+Other!A5+S!A6');
    expect(shiftFormula('=A5+S!A5', ins(0), 'Other')).toBe('=A5+S!A6');
    expect(shiftFormula("='s'!A5", ins(0), 'X')).toBe('=s!A6');
  });

  it('keeps the user formatting outside changed references', () => {
    expect(shiftFormula('= SUM( A5 ,  b7 )\n+ 1', ins(0), 'S')).toBe('= SUM( A6 ,  B8 )\n+ 1');
  });

  it('keeps Sheet!#REF! qualified on deletion', () => {
    expect(shiftFormula('=S!A5', del(4), 'X')).toBe('=S!#REF!');
  });

  it('leaves 3-D references alone', () => {
    expect(shiftFormula('=SUM(S:T!A5)', ins(0), 'S')).toBe('=SUM(S:T!A5)');
  });

  it('clamps areas that reach the last row', () => {
    expect(shiftFormula('=SUM(A2:A1048576)', ins(5), 'S')).toBe('=SUM(A2:A1048576)');
    expect(shiftFormula('=A1048576', ins(5), 'S')).toBe('=#REF!');
  });

  it('property: a shifted cell reference still names the same logical cell', () => {
    const r = rng(7);
    for (let i = 0; i < 300; i++) {
      const row = randInt(r, 0, 200);
      const col = randInt(r, 0, 50);
      const at = randInt(r, 0, 200);
      const count = randInt(r, 1, 10);
      const rowsOp = r() < 0.5;
      const insert = r() < 0.5;
      const change = {
        kind: (insert ? (rowsOp ? 'insertRows' : 'insertCols') : rowsOp ? 'deleteRows' : 'deleteCols') as
          | 'insertRows'
          | 'insertCols'
          | 'deleteRows'
          | 'deleteCols',
        at,
        count,
      };
      const addr = { kind: 'cell' as const, row, col, rowAbs: r() < 0.5, colAbs: r() < 0.5 };
      const out = shiftAddress(addr, change);
      const idx = rowsOp ? row : col;
      if (insert) {
        const expected = idx >= at ? idx + count : idx;
        expect(out && (rowsOp ? (out as typeof addr).row : (out as typeof addr).col)).toBe(expected);
      } else if (idx >= at && idx < at + count) {
        expect(out).toBeNull();
      } else {
        const expected = idx >= at + count ? idx - count : idx;
        expect(rowsOp ? (out as typeof addr).row : (out as typeof addr).col).toBe(expected);
      }
    }
  });

  it('property: insert then delete of the same rows restores every reference', () => {
    const r = rng(11);
    for (let i = 0; i < 200; i++) {
      const a = `${columnToLetters(randInt(r, 0, 20))}${randInt(r, 1, 300)}`;
      const b = `${columnToLetters(randInt(r, 0, 20))}${randInt(r, 1, 300)}`;
      const f = `=SUM(${a}:${b})+${a}*2`;
      const norm = canon(f);
      const at = randInt(r, 0, 300);
      const n = randInt(r, 1, 20);
      const there = shiftFormula(f, ins(at, n), 'S');
      const back = shiftFormula(there, del(at, n), 'S');
      expect(canon(back)).toBe(norm);
    }
  });
});

describe('copyFormula: relative adjustment', () => {
  it.each([
    ['=A1+B2', 1, 0, '=A2+B3'],
    ['=$A1+A$1+$A$1', 2, 3, '=$A3+D$1+$A$1'],
    ['=SUM(A1:B2)', 0, 2, '=SUM(C1:D2)'],
    ['=SUM(A:A)', 5, 1, '=SUM(B:B)'],
    ['=SUM(1:2)', 1, 5, '=SUM(2:3)'],
    ['=A1', -1, 0, '=#REF!'],
    ['=Sheet2!A1', 1, 1, '=Sheet2!B2'],
    ['=A1', 0, 0, '=A1'],
  ])('%s moved by (%i,%i) → %s', (src, dr, dc, out) => {
    expect(copyFormula(src, dr, dc)).toBe(out);
  });

  it('property: copying there and back is the identity while in bounds', () => {
    const r = rng(3);
    for (let i = 0; i < 200; i++) {
      const cell = (lo: number, hi: number) =>
        formatCell(randInt(r, lo, hi), randInt(r, lo, hi), r() < 0.3, r() < 0.3);
      // Area corners far enough apart that shifting never swaps them (Excel normalises swapped areas).
      const f = `=${cell(50, 500)}+SUM(${cell(50, 150)}:${cell(250, 400)})`;
      const dr = randInt(r, -40, 40);
      const dc = randInt(r, -25, 25);
      expect(canon(copyFormula(copyFormula(f, dr, dc), -dr, -dc))).toBe(canon(f));
    }
  });
});

describe('moveFormula: cut and paste', () => {
  const move = { sheet: 'S', r0: 0, c0: 0, r1: 1, c1: 1, dRow: 5, dCol: 2 };
  it('retargets references inside the moved block, absolute or not', () => {
    expect(moveFormula('=A1+$B$2', move, 'S')).toBe('=C6+$D$7');
    expect(moveFormula('=SUM(A1:B2)', move, 'S')).toBe('=SUM(C6:D7)');
  });

  it('leaves references that only partly overlap the block', () => {
    expect(moveFormula('=SUM(A1:A5)', move, 'S')).toBe('=SUM(A1:A5)');
  });

  it('turns references to the overwritten destination into #REF!', () => {
    expect(moveFormula('=C6', move, 'S')).toBe('=#REF!');
  });

  it('adds a sheet prefix when moving to another sheet', () => {
    expect(moveFormula('=A1', { ...move, toSheet: 'T' }, 'S')).toBe('=T!C6');
    expect(moveFormula('=S!A1', { ...move, toSheet: 'T' }, 'T')).toBe('=C6');
  });
});

describe('sheet renames and removal', () => {
  it('renames qualifiers, adding quotes when needed', () => {
    expect(renameSheetInFormula('=Sheet1!A1+sheet1!B2+Other!C3', 'Sheet1', 'My Data')).toBe(
      "='My Data'!A1+'My Data'!B2+Other!C3",
    );
    expect(renameSheetInFormula('=SUM(Jan:Mar!A1)', 'Mar', 'Dec')).toBe('=SUM(Jan:Dec!A1)');
    expect(renameSheetInFormula('=Sheet1!Total', 'Sheet1', 'S2')).toBe("='S2'!Total"); // S2 looks like a cell
  });

  it('turns references to a removed sheet into #REF!', () => {
    expect(removeSheetInFormula('=Gone!A1+A1', 'Gone')).toBe('=#REF!+A1');
  });
});

describe('A1 ↔ R1C1', () => {
  it('converts relative, absolute and mixed references', () => {
    expect(toR1C1('=A1+$B$2+C$3+$D4', 2, 2)).toBe('=R[-2]C[-2]+R2C2+R3C+R[1]C4');
    expect(toR1C1('=SUM(A:A)+SUM(3:3)', 4, 0)).toBe('=SUM(C)+SUM(R[-2])');
    expect(toR1C1('=SUM($A:$B)', 0, 5)).toBe('=SUM(C1:C2)');
    expect(fromR1C1('=R[-2]C[-2]+R2C2+RC', 2, 2)).toBe('=A1+$B$2+C3');
  });

  it('property: A1 → R1C1 → A1 is the identity', () => {
    const r = rng(5);
    for (let i = 0; i < 200; i++) {
      const cell = () => formatCell(randInt(r, 0, 2000), randInt(r, 0, 400), r() < 0.5, r() < 0.5);
      const f = `=${cell()}*SUM(${cell()}:${cell()})`;
      const row = randInt(r, 0, 3000);
      const col = randInt(r, 0, 500);
      expect(canon(fromR1C1(toR1C1(f, row, col), row, col))).toBe(canon(f));
    }
  });

  it('parses R1C1 formulas into the same AST as A1', () => {
    const a = JSON.stringify(parse('=$A$1+B2', {}), (k, v) => (k === 'span' ? undefined : v));
    const b = JSON.stringify(
      parse('=R1C1+R[1]C[1]', { style: 'R1C1', anchor: { row: 0, col: 0 } }),
      (k, v) => (k === 'span' ? undefined : v),
    );
    expect(b).toBe(a);
  });
});

describe('F4: toggling absolute references', () => {
  it('cycles A1 → $A$1 → A$1 → $A1 → A1', () => {
    let t = '=A1';
    const seen: string[] = [];
    for (let i = 0; i < 4; i++) {
      t = toggleAbsolute(t, 2).text;
      seen.push(t);
    }
    expect(seen).toEqual(['=$A$1', '=A$1', '=$A1', '=A1']);
  });

  it('toggles a whole area and selects it', () => {
    expect(toggleAbsolute('=SUM(A1:B2)', 6)).toEqual({
      text: '=SUM($A$1:$B$2)',
      selectionStart: 5,
      selectionEnd: 14,
    });
  });

  it('toggles every reference in a selection and keeps sheet prefixes', () => {
    expect(toggleAbsolute("=A1+'My S'!B2", 0, 13).text).toBe("=$A$1+'My S'!$B$2");
  });

  it('works on unfinished formulas and leaves non-references alone', () => {
    expect(toggleAbsolute('=SUM(A1', 7).text).toBe('=SUM($A$1');
    expect(toggleAbsolute('=SUM(1,2)', 3)).toEqual({ text: '=SUM(1,2)', selectionStart: 3, selectionEnd: 3 });
  });

  it('cycles whole rows/columns between relative and absolute', () => {
    expect(cycleAbsolute({ kind: 'cols', c0: 0, c1: 0, c0Abs: false, c1Abs: false })).toMatchObject({
      c0Abs: true,
    });
    expect(toggleAbsolute('=A:A', 2).text).toBe('=$A:$A');
  });
});
