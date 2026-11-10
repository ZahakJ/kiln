/**
 * Formula conformance suite.
 *
 * Every `*.csv` in this directory holds rows `formula,expected,type,notes`:
 * - `curated-*.csv` — typed in from Microsoft's documented examples and edge cases, by family;
 * - `generated-*.csv` — produced by `tools/gen-conformance.mjs` from the LibreOffice oracle.
 *
 * Formulas are evaluated on a "Calc" sheet next to the shared "Data" sheet (`fixture.json`).
 * Comparison is type-aware: numbers and dates (serials) to 1e-9 relative (or to the displayed
 * precision when the notes say `rounded`), errors by name, text and logicals exactly, arrays element
 * by element. Rows typed `skip` are not evaluated; each must be explained in
 * docs/formula-deviations.md.
 */
import { readdirSync, readFileSync } from 'node:fs';
import { dirname, join } from 'node:path';
import { fileURLToPath } from 'node:url';
import { afterAll, describe, expect, it } from 'vitest';
import { Engine } from '../../src/engine/engine.ts';
import { parseCellAddress } from '../../src/parser/references.ts';
import { errorFromCode } from '../../src/values/errors.ts';
import { Array2D, type Scalar } from '../../src/values/types.ts';
import { compare, displayTolerance } from '../functions/_support.ts';

const DIR = dirname(fileURLToPath(import.meta.url));
const DEVIATIONS = readFileSync(join(DIR, '../../../../docs/formula-deviations.md'), 'utf8');

interface Row {
  file: string;
  line: number;
  formula: string;
  expected: string;
  type: string;
  notes: string;
}

function parseCsv(text: string): string[][] {
  const rows: string[][] = [];
  let row: string[] = [];
  let cur = '';
  let inQ = false;
  for (let i = 0; i < text.length; i++) {
    const ch = text[i] as string;
    if (inQ) {
      if (ch === '"') {
        if (text[i + 1] === '"') {
          cur += '"';
          i++;
        } else inQ = false;
      } else cur += ch;
    } else if (ch === '"') inQ = true;
    else if (ch === ',') {
      row.push(cur);
      cur = '';
    } else if (ch === '\n') {
      row.push(cur);
      rows.push(row);
      row = [];
      cur = '';
    } else if (ch !== '\r') cur += ch;
  }
  if (cur !== '' || row.length > 0) {
    row.push(cur);
    rows.push(row);
  }
  return rows;
}

function loadRows(): Row[] {
  const out: Row[] = [];
  for (const file of readdirSync(DIR)
    .filter((f) => /^(curated|generated)-.*\.csv$/.test(f))
    .sort()) {
    const rows = parseCsv(readFileSync(join(DIR, file), 'utf8'));
    const header = rows.shift();
    if (header?.join(',') !== 'formula,expected,type,notes') throw new Error(`${file}: bad header`);
    rows.forEach((r, i) => {
      if (r.length === 1 && r[0] === '') return;
      const [formula = '', expected = '', type = '', notes = ''] = r;
      out.push({ file, line: i + 2, formula, expected, type, notes });
    });
  }
  return out;
}

function makeEngine(): Engine {
  const fixture = JSON.parse(readFileSync(join(DIR, 'fixture.json'), 'utf8')) as {
    sheet: string;
    cells: Record<string, number | string | boolean>;
  };
  const e = new Engine({ now: () => new Date(2024, 0, 15, 12, 0, 0) });
  e.addSheet('Calc');
  e.addSheet(fixture.sheet);
  for (const [addr, v] of Object.entries(fixture.cells)) {
    const p = parseCellAddress(addr);
    if (!p) throw new Error(`bad fixture address ${addr}`);
    if (typeof v === 'string' && v.startsWith('=')) e.setCell(fixture.sheet, p.row, p.col, { formula: v });
    else if (typeof v === 'string' && v.startsWith("'"))
      e.setCell(fixture.sheet, p.row, p.col, { value: v.slice(1) });
    else e.setCell(fixture.sheet, p.row, p.col, { value: v as Scalar });
  }
  e.recalc();
  return e;
}

/** 1e-9 relative (1e-14 absolute near zero); `rounded` rows compare at their displayed precision. */
function tolerance(row: Row): (lit: string) => number {
  if (/\brounded\b/.test(row.notes)) return displayTolerance;
  return (lit) => Math.max(Math.abs(Number(lit)) * 1e-9, 1e-14);
}

const rows = loadRows();
const engine = makeEngine();
const stats = { total: 0, passed: 0, skipped: 0, byFile: new Map<string, number>() };

describe('formula conformance', () => {
  const evaluated = rows.filter((r) => r.type !== 'skip');
  const curated = evaluated.filter((r) => r.file.startsWith('curated-')).length;
  it(`has at least 1,500 cases (${evaluated.length}: ${curated} curated, ${evaluated.length - curated} LibreOffice-generated, ${rows.length - evaluated.length} skipped)`, () => {
    expect(rows.filter((r) => r.type !== 'skip').length).toBeGreaterThanOrEqual(1500);
  });

  it('documents every skipped row in docs/formula-deviations.md', () => {
    for (const r of rows.filter((x) => x.type === 'skip')) expect(DEVIATIONS, r.formula).toContain(r.formula);
  });

  for (const r of rows) {
    const name = `${r.file}:${r.line} ${r.formula}`;
    if (r.type === 'skip') {
      stats.skipped++;
      it.skip(`${name} — ${r.notes}`, () => {});
      continue;
    }
    it(name, () => {
      stats.total++;
      stats.byFile.set(r.file, (stats.byFile.get(r.file) ?? 0) + 1);
      expect(['number', 'date', 'text', 'boolean', 'error', 'array']).toContain(r.type);
      let actual = engine.evaluate(r.formula, { sheet: 'Calc', row: 0, col: 0 });
      // A single cell shows the top-left value of an array result (what the oracle recorded).
      if (r.type !== 'array' && actual instanceof Array2D) actual = actual.get(0, 0);
      let expected = r.expected;
      if (r.type === 'text') expected = `"${r.expected.replace(/"/g, '""')}"`;
      if (r.type === 'error') expect(errorFromCode(r.expected), 'known error code').toBeDefined();
      const mismatch = compare(actual, expected, tolerance(r));
      expect(mismatch).toBeUndefined();
      stats.passed++;
    });
  }

  afterAll(() => {
    const files = [...stats.byFile.entries()].map(([f, n]) => `${f} ${n}`).join(', ');
    process.stdout.write(
      `conformance: ${stats.total} cases evaluated, ${stats.passed} passed, ${stats.total - stats.passed} failed, ${stats.skipped} skipped (${files})\n`,
    );
  });
});
