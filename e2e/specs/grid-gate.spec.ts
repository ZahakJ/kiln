/**
 * The Kiln Grid integration gate: the Office file loop over real workbooks (open, type
 * values and formulas, format them, add a chart and a PivotTable, save, open again with
 * everything kept), LibreOffice recalculating the saved files to the values Kiln computed, the
 * defects the gate's review fixed, and the screenshots of the visual review (light and dark,
 * desktop and phone) checked against the reviewed baselines in `e2e/screenshots/baseline/grid/`.
 */
import { execFileSync } from 'node:child_process';
import { existsSync, mkdirSync, readdirSync, readFileSync, rmSync, writeFileSync } from 'node:fs';
import { inflateRawSync } from 'node:zlib';
import { expect, type Page, test } from '@playwright/test';
import pixelmatch from 'pixelmatch';
import { decodePng } from '../../packages/testing/src/png.ts';
import {
  ask,
  clearSaved,
  core,
  expectSheetFocused,
  focusSheet,
  gridState,
  openFile,
  repo,
  run,
  savedBytes,
  select,
  until,
  value,
  values,
} from './grid-helpers.ts';

const out = (p: string) => repo(`e2e/screenshots/grid-gate/${p}`);
const baseline = (name: string) => repo(`e2e/screenshots/baseline/grid/${name}`);

// ── files ───────────────────────────────────────────────────────────────────────────────────

/** Every entry of a zip package, inflated. */
function unzip(zip: Buffer): Map<string, Buffer> {
  const files = new Map<string, Buffer>();
  const eocd = zip.lastIndexOf(Buffer.from([0x50, 0x4b, 0x05, 0x06]));
  if (eocd < 0) return files;
  const count = zip.readUInt16LE(eocd + 10);
  let at = zip.readUInt32LE(eocd + 16);
  for (let i = 0; i < count; i++) {
    const method = zip.readUInt16LE(at + 10);
    const size = zip.readUInt32LE(at + 20);
    const nameLen = zip.readUInt16LE(at + 28);
    const extra = zip.readUInt16LE(at + 30);
    const comment = zip.readUInt16LE(at + 32);
    const local = zip.readUInt32LE(at + 42);
    const name = zip.subarray(at + 46, at + 46 + nameLen).toString('utf8');
    const start = local + 30 + zip.readUInt16LE(local + 26) + zip.readUInt16LE(local + 28);
    const data = zip.subarray(start, start + size);
    files.set(name, method === 0 ? Buffer.from(data) : inflateRawSync(data));
    at += 46 + nameLen + extra + comment;
  }
  return files;
}

const unescapeXml = (s: string) =>
  s
    .replace(/&lt;/g, '<')
    .replace(/&gt;/g, '>')
    .replace(/&quot;/g, '"')
    .replace(/&apos;/g, "'")
    .replace(/&#x([0-9a-f]+);/gi, (_, h) => String.fromCodePoint(Number.parseInt(h, 16)))
    .replace(/&#(\d+);/g, (_, d) => String.fromCodePoint(Number(d)))
    .replace(/&amp;/g, '&');

const attr = (attrs: string, name: string) =>
  new RegExp(`(?:^|\\s)${name}="([^"]*)"`).exec(attrs)?.[1] ?? null;

/** A cell of a saved sheet: its formula (the shared formula's text for a dependent) and value. */
interface SavedCell {
  formula: string | null;
  value: string | number | null;
}

/** The sheets of an xlsx package by name: every cell with its formula and cached value. */
function workbookCells(zip: Buffer): Map<string, Map<string, SavedCell>> {
  const files = unzip(zip);
  const text = (p: string) => files.get(p)?.toString('utf8') ?? '';
  const sst = [...text('xl/sharedStrings.xml').matchAll(/<si>([\s\S]*?)<\/si>/g)].map((m) =>
    unescapeXml([...(m[1] ?? '').matchAll(/<t(?:\s[^>]*)?>([\s\S]*?)<\/t>/g)].map((t) => t[1]).join('')),
  );
  const rels = new Map<string, string>();
  for (const m of text('xl/_rels/workbook.xml.rels').matchAll(/<Relationship\b([^>]*)\/?>/g)) {
    const id = attr(m[1] ?? '', 'Id');
    const target = attr(m[1] ?? '', 'Target');
    if (id && target) rels.set(id, target.startsWith('/') ? target.slice(1) : `xl/${target}`);
  }
  const out = new Map<string, Map<string, SavedCell>>();
  for (const m of text('xl/workbook.xml').matchAll(/<sheet\b([^>]*)\/>/g)) {
    const name = unescapeXml(attr(m[1] ?? '', 'name') ?? '');
    const path = rels.get(attr(m[1] ?? '', 'r:id') ?? '') ?? '';
    const xml = text(path);
    const cells = new Map<string, SavedCell>();
    const shared = new Map<string, string>();
    for (const c of xml.matchAll(/<c\b([^>]*?)(?:\/>|>([\s\S]*?)<\/c>)/g)) {
      const a = c[1] ?? '';
      const inner = c[2] ?? '';
      const ref = attr(a, 'r');
      if (!ref) continue;
      const t = attr(a, 't');
      const f = /<f\b([^>]*?)(?:\/>|>([\s\S]*?)<\/f>)/.exec(inner);
      let formula: string | null = null;
      if (f) {
        const si = attr(f[1] ?? '', 'si');
        const body = f[2] !== undefined ? unescapeXml(f[2]) : '';
        if (body) {
          formula = body;
          if (si !== null) shared.set(si, body);
        } else formula = (si !== null ? shared.get(si) : undefined) ?? '';
      }
      const v = /<v>([\s\S]*?)<\/v>/.exec(inner)?.[1];
      const inline = /<is>([\s\S]*?)<\/is>/.exec(inner)?.[1];
      let val: string | number | null = null;
      if (t === 's') val = sst[Number(v)] ?? '';
      else if (t === 'inlineStr')
        val = unescapeXml(
          [...(inline ?? '').matchAll(/<t(?:\s[^>]*)?>([\s\S]*?)<\/t>/g)].map((x) => x[1]).join(''),
        );
      else if (t === 'str') val = unescapeXml(v ?? '');
      else if (t === 'b') val = v === '1' ? 'TRUE' : 'FALSE';
      else if (t === 'e') val = unescapeXml(v ?? '');
      else if (v !== undefined) val = Number(v);
      cells.set(ref, { formula, value: val });
    }
    out.set(name, cells);
  }
  return out;
}

/** Whether two cached values agree: numbers within 1e-9 of their magnitude, the rest as text. */
function agrees(a: string | number | null, b: string | number | null): boolean {
  if (typeof a === 'number' && typeof b === 'number') {
    return Math.abs(a - b) <= 1e-9 * Math.max(1, Math.abs(a), Math.abs(b));
  }
  return String(a ?? '') === String(b ?? '');
}

/** Functions whose value changes on every calculation: not compared. */
const VOLATILE = /\b(NOW|TODAY|RAND|RANDBETWEEN|RANDARRAY)\s*\(/i;

/**
 * Formula cells whose LibreOffice value is not a valid oracle, with the reason (as in
 * `crates/kiln-io-xlsx/tests/libreoffice_values.rs`, which checks the unedited files and fails
 * when one starts to agree). After this test's edits they may agree or not.
 */
const ORACLE_GAPS: Record<string, Record<string, string>> = {
  'formulas.xlsx': {
    'Formulas!C102': 'PERCENTRANK.INC: Excel truncates to 3 significant digits; LibreOffice rounds',
    'Formulas!C176': '_xlfn.NETWORKDAYS.INTL: LibreOffice does not import the function (#NAME?)',
    'Formulas!C178': '_xlfn.WORKDAY.INTL: LibreOffice does not import the function (#NAME?)',
    'Formulas!C233': 'TYPE of a multi-cell range: Excel returns 64; LibreOffice #VALUE!',
    'Formulas!C259': 'CONVERT lbm→kg: Excel uses the exact 0.45359237; LibreOffice an older factor',
  },
};

/** The LibreOffice the file loop recalculates with (`SOFFICE`, else /usr/bin/soffice). */
const SOFFICE = process.env.SOFFICE ?? '/usr/bin/soffice';

/**
 * Whether the file loop may run without LibreOffice: only when `KILN_E2E_NO_LIBREOFFICE=1` says so
 * (a machine without it); otherwise a missing LibreOffice fails the loop.
 */
const LIBREOFFICE_OPTIONAL = process.env.KILN_E2E_NO_LIBREOFFICE === '1';

/** Skips the test, saying why, when LibreOffice is missing and allowed to be; fails otherwise. */
function needLibreOffice(): void {
  if (existsSync(SOFFICE)) return;
  if (LIBREOFFICE_OPTIONAL) {
    console.log(`file loop: skipped, LibreOffice is not installed at ${SOFFICE} (KILN_E2E_NO_LIBREOFFICE=1)`);
    test.skip(true, `LibreOffice is not installed at ${SOFFICE} (KILN_E2E_NO_LIBREOFFICE=1)`);
    return;
  }
  throw new Error(
    `the file loop needs LibreOffice at ${SOFFICE} (set SOFFICE, or KILN_E2E_NO_LIBREOFFICE=1 to skip it)`,
  );
}

/** LibreOffice opens a saved workbook, recalculates every formula and saves it as xlsx. */
function libreOfficeRecalculated(file: string, dir: string): Buffer {
  rmSync(dir, { recursive: true, force: true });
  mkdirSync(dir, { recursive: true });
  execFileSync(repo('tools/oracle-soffice.sh'), [file, 'xlsx', dir], {
    stdio: ['ignore', 'pipe', 'pipe'],
    timeout: 120_000,
  });
  const name = readdirSync(dir).find((f) => f.endsWith('.xlsx'));
  if (!name) throw new Error(`LibreOffice wrote no xlsx for ${file}`);
  return readFileSync(`${dir}/${name}`);
}

/** LibreOffice's refresh of the PivotTables of a workbook (UNO): each one's output as text. */
function libreOfficePivots(file: string): string[][][] {
  const r = JSON.parse(
    execFileSync('python3', [repo('crates/kiln-io-xlsx/tests/oracle/libreoffice-uno.py'), file], {
      encoding: 'utf8',
      timeout: 120_000,
      stdio: ['ignore', 'pipe', 'pipe'],
    }),
  ) as { sheets: { pivots: { cells: string[][] }[] }[] };
  return r.sheets.flatMap((s) => s.pivots.map((p) => p.cells));
}

// ── the sheet ───────────────────────────────────────────────────────────────────────────────

/** The number format of the active cell (the helpers' state type leaves it out). */
const numberFormat = (s: { format: object }) => (s.format as { number_format?: string }).number_format ?? '';

const colName = (c: number) => String.fromCharCode(65 + c);
const a1 = (r: number, c: number) => `${colName(c)}${r + 1}`;

/** Types into the active cell as a user does (the sheet has the keys), then Enter. */
async function typeAt(page: Page, r: number, c: number, text: string): Promise<void> {
  await select(page, r, c);
  await focusSheet(page);
  await page.keyboard.type(text);
  await page.keyboard.press('Enter');
  await until(page, (s) => s.mode === 'Ready', `${a1(r, c)} committed`);
}

/** The drawings on the active sheet (each one selected in turn), then none selected. */
async function drawings(page: Page): Promise<string[]> {
  const kinds: string[] = [];
  for (let i = 0; i < 40; i++) {
    // The core refuses a number past the last drawing.
    const there = await page.evaluate(async (index) => {
      const g = (
        window as unknown as {
          __kilnGrid: { doc: number; client: { execute(d: number, c: unknown): Promise<unknown> } };
        }
      ).__kilnGrid;
      return g.client.execute(g.doc, { Grid: { SelectDrawing: { index } } }).then(
        () => true,
        () => false,
      );
    }, i);
    if (!there) break;
    const d = (await gridState(page)).drawing;
    if (!d) break;
    kinds.push(d.kind);
  }
  await core(page, { SelectDrawing: { index: null } });
  return kinds;
}

const ITEMS: readonly [string, string][] = [
  ['Clay', '420'],
  ['Glaze', '310'],
  ['Gas', '1180'],
  ['Kiln hire', '640'],
];

/**
 * The fixtures of the loop, with an edit to one of each file's own inputs where the file's
 * formulas read it (so Kiln and LibreOffice both recalculate what depends on it).
 */
const LOOP: readonly { file: string; edit?: { sheet: string; cell: [number, number]; text: string } }[] = [
  // Data!A1:A20 holds 1…20, which the lookups read as sorted: the last one grows.
  { file: 'features/formulas.xlsx', edit: { sheet: 'Data', cell: [19, 0], text: '25' } },
  { file: 'features/shared-array-formulas.xlsx', edit: { sheet: 'Formulas', cell: [0, 0], text: '11' } },
  { file: 'features/number-formats.xlsx' },
  { file: 'features/styles.xlsx' },
  { file: 'features/tables.xlsx' },
  { file: 'features/multi-sheet.xlsx' },
  { file: 'features/charts.xlsx' },
  { file: 'features/conditional-formatting.xlsx' },
  { file: 'features/defined-names.xlsx' },
  { file: 'features/pivot-tables.xlsx' },
  { file: 'features/date1904.xlsx' },
  { file: 'features/merges.xlsx' },
  { file: 'manual/pivot-libreoffice.xlsx' },
];

test.describe('Kiln Grid gate: the Office file loop', () => {
  test.describe.configure({ timeout: 240_000 });

  for (const { file, edit } of LOOP) {
    const name = file.split('/').pop() as string;
    test(`office file loop: ${name} is edited, gets a chart and a PivotTable, saves, reopens, and LibreOffice agrees`, async ({
      page,
    }) => {
      needLibreOffice();
      await openFile(page, { name, buffer: readFileSync(repo(`fixtures/xlsx/${file}`)) });
      await clearSaved(page);
      const before = await gridState(page);
      const home = before.active_sheet;
      const homeName = before.sheets[home]?.name ?? '';
      const charts0 = (await drawings(page)).length;

      // An edit to the file's own inputs.
      if (edit) {
        const at = before.sheets.findIndex((s) => s.name === edit.sheet);
        expect(at, `${edit.sheet} is a sheet of ${name}`).toBeGreaterThanOrEqual(0);
        await core(page, { ActivateSheet: { index: at } });
        await typeAt(page, edit.cell[0], edit.cell[1], edit.text);
        await core(page, { ActivateSheet: { index: home } });
      }

      // A block of entries below the sheet's data, typed as a user types them.
      await select(page, 0, 0);
      await focusSheet(page);
      await page.keyboard.press('Control+End');
      const last = await until(page, (s) => /^[A-Z]+\d+$/.test(s.name_box), 'Ctrl+End moved');
      const r0 = Number(/\d+$/.exec(last.name_box)?.[0] ?? '1') + 1;
      const first = r0 + 1;
      const end = r0 + ITEMS.length;
      await typeAt(page, r0, 0, 'Kiln item');
      await typeAt(page, r0, 1, 'Kiln amount');
      await typeAt(page, r0, 2, 'Kiln share');
      for (const [i, [item, amount]] of ITEMS.entries()) {
        await typeAt(page, r0 + 1 + i, 0, item);
        await typeAt(page, r0 + 1 + i, 1, amount);
        await typeAt(page, r0 + 1 + i, 2, `=B${r0 + 2 + i}/SUM(B$${first + 1}:B$${end + 1})`);
      }
      const total = end + 2;
      await typeAt(page, total, 0, 'Total');
      await typeAt(page, total, 1, `=SUM(B${first + 1}:B${end + 1})`);
      await typeAt(page, total, 2, `=ROUND(AVERAGE(B${first + 1}:B${end + 1}),2)`);
      await typeAt(page, total, 3, `=IF(B${total + 1}>2500,"over","within")`);
      await typeAt(page, total, 4, `=COUNTIF(B${first + 1}:B${end + 1},">400")`);
      // An entry changed after the formulas that read it: Gas 1180 → 1250.
      await typeAt(page, r0 + 3, 1, '1250');
      expect(await value(page, total, 1)).toBe('2620');
      expect(await values(page, total, 2, total, 4)).toBe('655\tover\t3');

      // Formats: the header bold (Ctrl+B), amounts as currency, shares as percentages.
      await select(page, r0, 0, r0, 2);
      await focusSheet(page);
      await page.keyboard.press('Control+b');
      await until(page, (s) => s.format.bold, 'the header is bold');
      await select(page, first, 1, total, 1);
      expect(await run(page, 'grid.number.currency')).toBe('ran');
      await select(page, first, 2, end, 2);
      expect(await run(page, 'grid.number.percent')).toBe('ran');
      await select(page, first, 2);
      await until(page, (s) => numberFormat(s).includes('%'), 'shares in percent');

      // A chart of the items and amounts.
      await select(page, r0, 0, end, 1);
      expect(await run(page, 'grid.insert.chart_column')).toBe('ran');
      await until(page, (s) => s.drawing?.kind === 'Chart', 'the chart is inserted');
      await core(page, { SelectDrawing: { index: null } });

      // A PivotTable of the block on a new sheet: Kiln item in Rows, Sum of Kiln amount.
      await select(page, r0 + 1, 0);
      await focusSheet(page);
      await page.getByRole('tab', { name: 'Insert', exact: true }).click();
      await page.getByRole('button', { name: 'PivotTable', exact: true }).click();
      const dialog = page.getByRole('dialog', { name: 'Create PivotTable' });
      await expect(dialog.getByTestId('grid-pivot-source')).toHaveValue(
        new RegExp(`\\$A\\$${r0 + 1}:\\$C\\$${end + 1}$`),
      );
      await dialog.getByRole('button', { name: 'OK' }).click();
      await expect(dialog).toBeHidden();
      const fields = page.getByTestId('grid-pivot-fields');
      await expect(fields).toBeVisible();
      for (const f of ['Kiln item', 'Kiln amount'])
        await fields.locator('.kiln-gan__field').filter({ hasText: f }).getByRole('checkbox').click();
      const pivotSheet = (await gridState(page)).active_sheet;
      await expect
        .poll(async () => values(page, 0, 0, 12, 1), { message: 'the PivotTable shows the items' })
        .toMatch(/Grand Total\t2620/);
      const pivotText = await values(page, 0, 0, 12, 1);
      expect(pivotText).toMatch(/Gas\t1250/);

      // Save (Ctrl+S keeps the name and the format).
      await core(page, { ActivateSheet: { index: home } });
      await focusSheet(page);
      await page.keyboard.press('Control+s');
      const saved = await savedBytes(page, name);
      const dir = out(`file-loop/${name.replace(/\.xlsx$/, '')}`);
      mkdirSync(dir, { recursive: true });
      writeFileSync(`${dir}/${name}`, saved);

      // Kiln opens it again with everything kept.
      await openFile(page, { name, buffer: saved });
      const back = await gridState(page);
      const at = back.sheets.findIndex((s) => s.name === homeName);
      expect(at).toBeGreaterThanOrEqual(0);
      await core(page, { ActivateSheet: { index: at } });
      expect(await values(page, r0, 0, r0, 2)).toBe('Kiln item\tKiln amount\tKiln share');
      await select(page, r0, 1);
      await until(page, (s) => s.format.bold, 'the header is still bold');
      expect(await value(page, r0 + 3, 0)).toBe('Gas');
      await select(page, r0 + 3, 1);
      await until(page, (s) => numberFormat(s).includes('$'), 'amounts still in currency');
      expect(await value(page, r0 + 3, 1)).toMatch(/1,250\.00/);
      await select(page, first, 2);
      const share = await until(
        page,
        (s) => s.formula === `=B${first + 1}/SUM(B$${first + 1}:B$${end + 1})`,
        'the share formula is kept',
      );
      expect(numberFormat(share)).toContain('%');
      await select(page, total, 1);
      await until(page, (s) => s.formula === `=SUM(B${first + 1}:B${end + 1})`, 'the total formula is kept');
      expect(await values(page, total, 1, total, 4)).toMatch(/^\$2,620\.00 ?\t655\tover\t3$/);
      if (edit) {
        await core(page, { ActivateSheet: { index: back.sheets.findIndex((s) => s.name === edit.sheet) } });
        expect(await value(page, edit.cell[0], edit.cell[1])).toBe(edit.text);
        await core(page, { ActivateSheet: { index: at } });
      }
      const charts = await drawings(page);
      expect(charts.length).toBe(charts0 + 1);
      expect(charts.at(-1)).toBe('Chart');
      // The PivotTable, on its own sheet.
      expect(back.sheets.length).toBe(before.sheets.length + 1);
      await core(page, { ActivateSheet: { index: pivotSheet } });
      await expect.poll(() => values(page, 0, 0, 12, 1)).toBe(pivotText);
      await select(page, 3, 0);
      await expect
        .poll(
          async () => ((await gridState(page)) as unknown as { analysis: { pivot: unknown } }).analysis.pivot,
        )
        .not.toBeNull();

      // LibreOffice recalculates the saved file: every formula cell to the value Kiln saved.
      const lo = libreOfficeRecalculated(`${dir}/${name}`, `${dir}/libreoffice`);
      const report: { file: string; compared: number; differences: string[]; gaps: string[] } = {
        file: name,
        compared: 0,
        differences: [],
        gaps: [],
      };
      {
        const ours = workbookCells(saved);
        const theirs = workbookCells(lo);
        const gaps = ORACLE_GAPS[name] ?? {};
        for (const [sheet, cells] of ours) {
          const other = theirs.get(sheet);
          for (const [ref, cell] of cells) {
            if (cell.formula === null || VOLATILE.test(cell.formula)) continue;
            const at = `${sheet}!${ref}`;
            const theirsCell = other?.get(ref);
            if (!theirsCell) {
              report.differences.push(`${at} =${cell.formula}: LibreOffice has no such cell`);
              continue;
            }
            report.compared += 1;
            const same = agrees(cell.value, theirsCell.value);
            if (gaps[at]) {
              if (!same) report.gaps.push(`${at}: ${gaps[at]}`);
            } else if (!same) {
              report.differences.push(
                `${at} =${cell.formula}: Kiln ${JSON.stringify(cell.value)}, LibreOffice ${JSON.stringify(theirsCell.value)}`,
              );
            }
          }
        }
        // The cells typed in this test are among them.
        const mine = ours.get(homeName)?.get(a1(total, 1));
        expect(mine?.value).toBe(2620);
        expect(theirs.get(homeName)?.get(a1(total, 1))?.value).toBe(2620);
        // LibreOffice refreshes the new PivotTable from its source to the same totals.
        const pivots = libreOfficePivots(`${dir}/${name}`);
        const mineLo = pivots.find((p) => p.some((r) => r[0] === 'Kiln hire'));
        expect(mineLo, 'LibreOffice reads the new PivotTable').toBeDefined();
        // (LibreOffice shows the source's currency format on the values; Excel and Kiln General.)
        const amount = (t: string | undefined) => Number((t ?? '').replace(/[$,\s]/g, ''));
        expect(amount(mineLo?.find((r) => r[0] === 'Gas')?.[1])).toBe(1250);
        expect(amount(mineLo?.find((r) => /^(Total Result|Grand Total)$/.test(r[0] ?? ''))?.[1])).toBe(2620);
      }
      writeFileSync(`${dir}/report.json`, `${JSON.stringify(report, null, 2)}\n`);
      console.log(
        `file loop ${name}: ${report.compared} formula cells against LibreOffice, ${report.differences.length} differences, ${report.gaps.length} documented gaps`,
      );
      expect(report.differences, 'formula cells LibreOffice recalculates differently').toEqual([]);
    });
  }
});

// ── the defects of the gate's review ───────────────────────────────────────────────────────

const BUDGET: readonly (readonly string[])[] = [
  ['Item', 'Amount', 'Q2', 'Q3', 'Q4'],
  ['Clay', '420', '380', '455', '510'],
  ['Glaze', '310', '125', '290', '140'],
  ['Gas', '1180', '640', '1240', '1320'],
  ['Shelves', '-200', '560', '0', '0'],
  ['Open day', '380', '0', '0', '380'],
  ['Kiln hire', '640', '700', '-200', '900'],
  ['Studio', '3900', '1', '2', '3'],
];

/** Opens a new workbook and fills it through the core. */
async function openBook(page: Page, rows: readonly (readonly string[])[] = BUDGET): Promise<void> {
  await page.goto('/#/grid?blink=0');
  await page.waitForFunction(
    () =>
      Number(document.querySelector<HTMLElement>('[data-kiln-grid-surface]')?.dataset.kilnFrames ?? 0) > 0,
    null,
    { timeout: 60_000 },
  );
  await page.evaluate(async (rows) => {
    const g = (
      window as unknown as {
        __kilnGrid: { doc: number; client: { execute(d: number, c: unknown): Promise<unknown> } };
      }
    ).__kilnGrid;
    const values = rows.map((r) => r.map((t) => (t === '' ? null : t)));
    for (let r = 0; r < values.length; r++)
      for (let c = 0; c < (values[r] ?? []).length; c++) {
        const text = values[r]?.[c];
        if (!text) continue;
        await g.client.execute(g.doc, {
          Grid: { SelectCell: { row: r, col: c, extend: false, add: false } },
        });
        await g.client.execute(g.doc, { Grid: { BeginEdit: { text } } });
        await g.client.execute(g.doc, { Grid: { CommitEdit: { advance: null } } });
      }
  }, rows);
  await select(page, 0, 0);
  await focusSheet(page);
}

/** Runs a chart command of the suite (Insert ▸ Chart ▸ All Charts picks a kind this way). */
async function suite(page: Page, command: unknown): Promise<void> {
  await page.evaluate(async (c) => {
    const g = (
      window as unknown as {
        __kilnGrid: { doc: number; client: { execute(d: number, c: unknown): Promise<unknown> } };
      }
    ).__kilnGrid;
    await g.client.execute(g.doc, { Suite: c });
  }, command);
}

/** The page point of a cell's centre (from the core's rectangle of the active cell). */
async function cellCentre(page: Page, row: number, col: number): Promise<{ x: number; y: number }> {
  await select(page, row, col);
  const s = (await gridState(page)) as unknown as { features: { cell_rect: number[] } };
  const [x = 0, y = 0, w = 0, h = 0] = s.features.cell_rect;
  const box = await page.locator('[data-kiln-grid-surface]').boundingBox();
  if (!box) throw new Error('no grid surface');
  return { x: box.x + x + w / 2, y: box.y + y + h / 2 };
}

type Rect = { x: number; y: number; width: number; height: number };
const overlaps = (a: Rect, b: Rect) =>
  a.x < b.x + b.width && b.x < a.x + a.width && a.y < b.y + b.height && b.y < a.y + a.height;

test.describe('Kiln Grid gate: the review', () => {
  test.describe.configure({ timeout: 150_000 });

  test('context menu: the whole menu fits a 900 px window wherever the click is, the mini toolbar beside it', async ({
    page,
  }) => {
    await openBook(page);
    for (const row of [2, 9, 14, 26]) {
      const p = await cellCentre(page, row, 2);
      await page.mouse.click(p.x, p.y, { button: 'right' });
      const menu = page.getByTestId('grid-context-menu');
      await expect(menu).toBeVisible();
      await expect(menu.getByRole('menuitem', { name: 'Link' })).toBeVisible();
      // Settled where it fits: on screen, every item showing (no scrolling inside the menu).
      await expect
        .poll(async () => {
          const b = await menu.boundingBox();
          return !!b && b.y >= 0 && b.y + b.height <= 900;
        })
        .toBe(true);
      const [scroll, client] = await menu.evaluate((e) => [e.scrollHeight, e.clientHeight]);
      expect(scroll, `row ${row + 1}: the menu shows all its items`).toBeLessThanOrEqual((client ?? 0) + 1);
      const mini = await page.getByTestId('grid-mini-toolbar').boundingBox();
      const m = await menu.boundingBox();
      expect(mini && m && overlaps(mini, m), `row ${row + 1}: the mini toolbar is clear of the menu`).toBe(
        false,
      );
      if (row === 9) await snap(page, 'context-menu-light-desktop.png');
      await page.keyboard.press('Escape');
      await expect(menu).toBeHidden();
      await expectSheetFocused(page);
    }
  });

  test('statistic charts: Format Axis sets the bounds of a histogram; box and whisker and waterfall draw', async ({
    page,
  }) => {
    await openBook(page);
    await select(page, 0, 0, 7, 1);
    await suite(page, { ChartInsert: { kind: 'Histogram' } });
    await until(page, (s) => s.drawing?.kind === 'Chart', 'the histogram is inserted');
    expect(await run(page, 'ui.grid.format_chart')).toBe('ran');
    const pane = page.getByRole('complementary').or(page.locator('.kiln-chart-pane')).first();
    const min = pane.getByLabel('Minimum', { exact: true });
    await min.fill('1');
    await min.press('Enter');
    const max = pane.getByLabel('Maximum', { exact: true });
    await max.fill('10');
    await max.press('Enter');
    const chart = async () =>
      (
        (await gridState(page)).drawing as unknown as {
          chart: {
            axes: { target: string; min: number; max: number; min_auto: boolean; max_auto: boolean }[];
          };
        } | null
      )?.chart;
    await expect
      .poll(async () => {
        const v = (await chart())?.axes.find((a) => a.target === 'PrimaryValue');
        return v ? [v.min, v.max, v.min_auto, v.max_auto] : null;
      })
      .toEqual([1, 10, false, false]);
    // A number format for the labels.
    await pane.getByRole('combobox', { name: 'Number' }).click();
    await page.getByRole('option').filter({ hasText: /0\.00/ }).first().click();
    await expect
      .poll(async () => (await chart())?.axes.find((a) => a.target === 'PrimaryValue'))
      .toMatchObject({ min: 1, max: 10 });
    await page.keyboard.press('Escape');
    await core(page, { SelectDrawing: { index: null } });
    await page.keyboard.press('Escape');
    // Box and whisker (Studio's 3900 is an outlier) and waterfall, for the review's screenshot.
    await core(page, { SelectDrawing: { index: 0 } });
    await core(page, { PlaceDrawing: { index: 0, x: 330, y: 0, width: 350, height: 210 } });
    await select(page, 0, 0, 7, 1);
    await suite(page, { ChartInsert: { kind: 'BoxWhisker' } });
    await until(page, (s) => s.drawing?.kind === 'Chart', 'the box and whisker chart');
    await core(page, { PlaceDrawing: { index: 1, x: 690, y: 0, width: 350, height: 210 } });
    await select(page, 0, 0, 7, 1);
    await suite(page, { ChartInsert: { kind: 'Waterfall' } });
    await until(page, (s) => s.drawing?.kind === 'Chart', 'the waterfall chart');
    await core(page, { PlaceDrawing: { index: 2, x: 330, y: 220, width: 710, height: 300 } });
    await core(page, { SelectDrawing: { index: null } });
    await select(page, 9, 0);
    await page.evaluate(() => document.fonts.ready);
    await snap(page, 'statistic-charts-light-desktop.png');
    expectBaseline('statistic-charts-light-desktop.png');
  });

  test('page layout view: faint gridlines on the paper, rulers on its edges, a page to click to add data', async ({
    page,
  }) => {
    await openFile(page, {
      name: 'conditional-formatting.xlsx',
      buffer: readFileSync(repo('fixtures/xlsx/features/conditional-formatting.xlsx')),
    });
    expect(await run(page, 'suite.view.mode', { mode: 'PageLayout' })).toBe('ran');
    const view = page.getByTestId('grid-page-layout');
    await expect(view).toBeVisible();
    const lines = view.getByTestId('grid-page-gridlines').first();
    await expect(lines).toBeVisible();
    expect(await lines.locator('line').count()).toBeGreaterThan(20);
    // The ruler starts where the paper does.
    const paper = await view.locator('.kiln-pagelayout__paper').first().boundingBox();
    const ruler = await view
      .locator('.kiln-pagelayout__ruler:not(.kiln-pagelayout__ruler--v)')
      .first()
      .boundingBox();
    expect(paper && ruler && Math.abs(paper.x - ruler.x)).toBeLessThanOrEqual(1.5);
    expect(paper && ruler && Math.abs(paper.width - ruler.width)).toBeLessThanOrEqual(1.5);
    await page.evaluate(() => document.fonts.ready);
    await page.waitForTimeout(500);
    await snap(page, 'page-layout-light-desktop.png');
    expectBaseline('page-layout-light-desktop.png');
    // Click to add data: the first cell under the printed ones.
    const add = view.getByTestId('grid-page-add-data');
    await add.scrollIntoViewIfNeeded();
    await add.click();
    await until(page, (s) => /^A\d+$/.test(s.name_box) && Number(s.name_box.slice(1)) > 20, 'below the data');
    await expectSheetFocused(page);
    await page.keyboard.type('Next page');
    await page.keyboard.press('Enter');
    await expect(view.getByText(/Page Layout · \d+ pages?/)).toBeVisible();
  });

  test('border pen: a stroke right after Draw Border Grid draws, before the chrome hears of the pen', async ({
    page,
  }) => {
    await openBook(page);
    const a = await cellCentre(page, 10, 1);
    const b = await cellCentre(page, 11, 2);
    await select(page, 0, 0);
    // The command and the stroke with no wait between them.
    await page.evaluate(() =>
      (
        window as unknown as { __kiln: { registry: { execute(id: string, a: unknown): Promise<unknown> } } }
      ).__kiln.registry.execute('grid.font.border_pen', { mode: 'Grid' }),
    );
    await page.mouse.move(a.x, a.y);
    await page.mouse.down();
    await page.mouse.move(b.x, b.y, { steps: 3 });
    await page.mouse.up();
    await expect
      .poll(async () => {
        await select(page, 10, 1);
        const f = await ask<{ border_bottom?: { style: string } | null }>(page, 'FormatCells');
        return f.border_bottom?.style ?? null;
      })
      .toBe('Thin');
    // The press drew; it did not select the cells under it.
    await core(page, { SelectCell: { row: 0, col: 0, extend: false, add: false } });
    expect(await run(page, 'grid.font.border_pen_off')).toBe('ran');
  });
});

// ── the visual review ───────────────────────────────────────────────────────────────────────

/** A screenshot without the pointer's tooltip or a focused button's. */
async function snap(page: Page, name: string): Promise<void> {
  await page.mouse.move(2, 600);
  await page.evaluate(() => {
    const a = document.activeElement as HTMLElement | null;
    if (a?.closest('.kiln-ribbon, .kiln-statusbar')) a.blur();
  });
  await page.waitForTimeout(250);
  mkdirSync(out(''), { recursive: true });
  await page.screenshot({ path: out(name) });
}

/** Side of the square regions a screenshot is compared in, px. */
const REGION = 32;

/**
 * How a screenshot differs from another: pixelmatch (threshold 0.1, anti-aliased pixels not
 * counted) over the whole image, and the worst of its 32 px regions.
 */
function screenshotDiff(a: Buffer, b: Buffer): { total: number; worst: number; at: string } {
  const ia = decodePng(a);
  const ib = decodePng(b);
  if (ia.width !== ib.width || ia.height !== ib.height)
    return { total: 1, worst: 1, at: `size ${ia.width}×${ia.height} against ${ib.width}×${ib.height}` };
  const { width: w, height: h } = ia;
  const diff = new Uint8Array(w * h * 4);
  pixelmatch(ia.data, ib.data, diff, w, h, { threshold: 0.1, diffMask: true, includeAA: false });
  let differing = 0;
  let worst = 0;
  let at = '';
  for (let y0 = 0; y0 < h; y0 += REGION) {
    for (let x0 = 0; x0 < w; x0 += REGION) {
      let n = 0;
      const y1 = Math.min(h, y0 + REGION);
      const x1 = Math.min(w, x0 + REGION);
      for (let y = y0; y < y1; y++) {
        for (let x = x0; x < x1; x++) {
          const i = (y * w + x) * 4;
          // Differing pixels are drawn red; anti-aliasing yellow (not counted).
          if (diff[i + 3] !== 0 && diff[i] === 255 && diff[i + 1] === 0) n += 1;
        }
      }
      differing += n;
      const frac = n / ((y1 - y0) * (x1 - x0));
      if (frac > worst) {
        worst = frac;
        at = `${x0},${y0}`;
      }
    }
  }
  return { total: differing / (w * h), worst, at };
}

/**
 * At most 0.1 % of the pixels may differ, and at most 4 % of any 32 px region: what font
 * hinting and antialiasing change between runs, spread thin. A different ribbon tab, a moved
 * control or a changed colour fills whole regions and fails.
 */
const MAX_TOTAL = 0.001;
const MAX_REGION = 0.04;

function matchesBaseline(shot: Buffer, base: Buffer): { ok: boolean; why: string } {
  const d = screenshotDiff(shot, base);
  const why = `${(d.total * 100).toFixed(3)} % of the pixels differ, ${(d.worst * 100).toFixed(1)} % of the region at ${d.at}`;
  return { ok: d.total <= MAX_TOTAL && d.worst <= MAX_REGION, why };
}

/**
 * The screenshot against its reviewed baseline ({@link matchesBaseline}).
 * `KILN_UPDATE_BASELINES=1` writes the baseline from the screenshot instead.
 */
function expectBaseline(name: string): void {
  const shot = readFileSync(out(name));
  const path = baseline(name);
  if (process.env.KILN_UPDATE_BASELINES === '1' || !existsSync(path)) {
    mkdirSync(baseline(''), { recursive: true });
    writeFileSync(path, shot);
    if (process.env.KILN_UPDATE_BASELINES !== '1')
      throw new Error(`${name}: no baseline yet (written; review it)`);
    return;
  }
  const { ok, why } = matchesBaseline(shot, readFileSync(path));
  console.log(`baseline ${name}: ${why}`);
  expect(ok, `${name} against its baseline: ${why}`).toBe(true);
}

test.describe('Kiln Grid gate: screenshots', () => {
  test.describe.configure({ timeout: 180_000 });

  test('screenshots: the Grid chrome in light and dark, desktop and phone, against the baselines', async ({
    page,
    browser,
    baseURL,
  }) => {
    const file = {
      name: 'conditional-formatting.xlsx',
      buffer: readFileSync(repo('fixtures/xlsx/features/conditional-formatting.xlsx')),
    };
    await openFile(page, file);
    await page.evaluate(() => document.fonts.ready);
    await page.waitForTimeout(600);
    await snap(page, 'chrome-light-desktop.png');
    expectBaseline('chrome-light-desktop.png');
    for (const t of ['Insert', 'Page Layout', 'Formulas', 'Data', 'Review', 'View']) {
      await page.getByRole('tab', { name: t, exact: true }).click();
      await page.waitForTimeout(300);
      await snap(page, `${t.toLowerCase().replace(' ', '-')}-tab-light-desktop.png`);
    }
    expectBaseline('formulas-tab-light-desktop.png');
    // The check tells one ribbon tab from another: the Data tab is not the Formulas baseline.
    const other = matchesBaseline(
      readFileSync(out('data-tab-light-desktop.png')),
      readFileSync(baseline('formulas-tab-light-desktop.png')),
    );
    console.log(`the Data tab against the Formulas baseline: ${other.why}`);
    expect(other.ok, `the Data tab passes as the Formulas tab: ${other.why}`).toBe(false);
    await page.getByRole('tab', { name: 'Home', exact: true }).click();
    await page.emulateMedia({ colorScheme: 'dark' });
    await page.waitForTimeout(600);
    await snap(page, 'chrome-dark-desktop.png');
    expectBaseline('chrome-dark-desktop.png');
    await expectNoSeriousA11y(page);
    // A PivotTable and its pane in dark.
    await openFile(page, {
      name: 'pivot-tables.xlsx',
      buffer: readFileSync(repo('fixtures/xlsx/features/pivot-tables.xlsx')),
    });
    const report = (await gridState(page)).sheets.findIndex((s) => s.name === 'Report');
    await core(page, { ActivateSheet: { index: report } });
    await select(page, 4, 1);
    await expect
      .poll(
        async () => ((await gridState(page)) as unknown as { analysis: { pivot: unknown } }).analysis.pivot,
      )
      .not.toBeNull();
    await page.waitForTimeout(600);
    await snap(page, 'pivot-dark-desktop.png');
    expectBaseline('pivot-dark-desktop.png');
    // Sparklines in dark.
    await openFile(page, {
      name: 'sparklines.xlsx',
      buffer: readFileSync(repo('fixtures/xlsx/features/sparklines.xlsx')),
    });
    await page.waitForTimeout(600);
    await snap(page, 'sparklines-dark-desktop.png');
    await page.emulateMedia({ colorScheme: 'light' });
    for (const scheme of ['light', 'dark'] as const) {
      const ctx = await browser.newContext({
        baseURL,
        viewport: { width: 412, height: 915 },
        colorScheme: scheme,
      });
      const phone = await ctx.newPage();
      try {
        await openFile(phone, file);
        await phone.evaluate(() => document.fonts.ready);
        await phone.waitForTimeout(800);
        await snap(phone, `chrome-${scheme}-phone.png`);
        expectBaseline(`chrome-${scheme}-phone.png`);
        expect(await phone.evaluate(() => document.documentElement.scrollWidth)).toBeLessThanOrEqual(412);
      } finally {
        await ctx.close();
      }
    }
  });
});

/** axe-core (WCAG 2.2 A/AA) finds nothing serious or critical. */
async function expectNoSeriousA11y(page: Page): Promise<void> {
  const { expectNoSeriousA11yViolations } = await import('../../packages/testing/src/playwright/a11y.ts');
  await expectNoSeriousA11yViolations(page);
}
