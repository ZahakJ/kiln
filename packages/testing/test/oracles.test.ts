import { mkdtempSync, readFileSync } from 'node:fs';
import { tmpdir } from 'node:os';
import { join } from 'node:path';
import { describe, expect, it } from 'vitest';
import {
  convertBatchWithSoffice,
  convertWithSoffice,
  decodePng,
  fixtureLabel,
  fixturePath,
  listFixtures,
  loadFixture,
  type PythonDocxDump,
  type PythonPptxDump,
  type PythonXlsxDump,
  pdftoppmAvailable,
  pythonAvailable,
  readDocxWithMammoth,
  readWithPython,
  readXlsxWithExcelJS,
  renderPdfPages,
  sofficeAvailable,
  targetExtension,
} from '../src/index.ts';

const HAS_SOFFICE = sofficeAvailable();
const HAS_PDFTOPPM = pdftoppmAvailable();
const HAS_PYTHON = pythonAvailable();

describe('ExcelJS reader', () => {
  it('reads values, number formats and formulas', async () => {
    const wb = await readXlsxWithExcelJS(loadFixture('xlsx/features/number-formats.xlsx'));
    const sheet = wb.sheets[0];
    expect(sheet?.name).toBe('Formats');
    expect(sheet?.cells.D3).toEqual({ value: 1234.5678, numFmt: '0' });
    expect(sheet?.cells.B8?.value).toBe('"$"#,##0.00');
    expect(sheet?.cells.D9).toEqual({ value: 1234.5, numFmt: '[$€-x-euro2] #,##0.00' });
    const formulas = await readXlsxWithExcelJS(loadFixture('xlsx/features/formulas.xlsx'));
    const fs = formulas.sheets.find((s) => s.name === 'Formulas');
    expect(fs?.cells.C2?.formula).toBe('SUM(Data!A1:A20)');
    const count = Object.keys(fs?.cells ?? {}).filter(
      (k) => k.startsWith('C') && fs?.cells[k]?.formula,
    ).length;
    expect(count).toBe(300);
  });

  it('reads merges, sheet states and defined names', async () => {
    const merges = await readXlsxWithExcelJS(loadFixture('xlsx/features/merges.xlsx'));
    expect(merges.sheets[0]?.merges).toEqual(['A1:D1', 'A3:A8', 'B15:B16', 'C3:F6', 'H10:J12']);
    const hidden = await readXlsxWithExcelJS(loadFixture('xlsx/features/hidden-rows-cols.xlsx'));
    expect(hidden.sheets.map((s) => s.state)).toEqual(['visible', 'hidden', 'veryHidden']);
    const names = await readXlsxWithExcelJS(loadFixture('xlsx/features/defined-names.xlsx'));
    // ExcelJS only models names that refer to ranges; the constant TaxRate=0.2 is not reported.
    expect(names.definedNames).toEqual(['LocalTotal', 'Squares', 'Values']);
  });
});

describe('mammoth reader', () => {
  it('reads text and semantic HTML', async () => {
    const r = await readDocxWithMammoth(loadFixture('docx/features/styles.docx'));
    expect(r.text).toContain('Text styles matrix');
    expect(r.html).toContain('<h1>Heading 1</h1>');
    const t = await readDocxWithMammoth(loadFixture('docx/features/tables.docx'));
    expect(t.html).toContain('<table>');
    expect(t.html).toContain('Merged across three columns');
  });

  it('reads every generated .docx', async () => {
    for (const f of listFixtures('docx')) {
      const r = await readDocxWithMammoth(loadFixture(fixtureLabel(f)));
      expect(r.text.length, f).toBeGreaterThan(0);
    }
  });
});

describe.skipIf(!HAS_PYTHON)('Python reader (openpyxl / python-docx / python-pptx)', () => {
  it('dumps a .docx', async () => {
    const d = (await readWithPython(fixturePath('docx/features/sections.docx'))) as PythonDocxDump;
    expect(d.format).toBe('docx');
    expect(d.sections.map((s) => s.orientation)).toEqual(['PORTRAIT', 'LANDSCAPE', 'LANDSCAPE', 'PORTRAIT']);
    expect(d.paragraphs.some((p) => p.text.startsWith('Section 3'))).toBe(true);
  });

  it('dumps a .xlsx', async () => {
    const d = (await readWithPython(fixturePath('xlsx/features/date1904.xlsx'))) as PythonXlsxDump;
    expect(d.epoch).toBe('1904-01-01T00:00:00');
    expect(d.sheets[0]?.cells.A1?.value).toBe('2026-01-15T00:00:00');
    expect(d.sheets[0]?.cells.B1?.value).toBe('=A1+1');
  });

  it('dumps a .pptx', async () => {
    const d = (await readWithPython(fixturePath('pptx/features/many-slides.pptx'))) as PythonPptxDump;
    expect(d.slides).toHaveLength(30);
    expect(d.slides.filter((s) => s.hidden)).toHaveLength(2);
    expect(d.slide_width).toBe(12192000);
  });

  it('rejects other files', async () => {
    await expect(readWithPython('/tmp/x.txt')).rejects.toThrow(/unsupported/);
  });
});

describe.skipIf(!HAS_SOFFICE)('LibreOffice oracle', () => {
  it('converts a .docx to PDF and renders pages', { timeout: 120_000 }, async () => {
    const out = mkdtempSync(join(tmpdir(), 'kiln-test-'));
    const r = await convertWithSoffice(fixturePath('docx/features/styles.docx'), 'pdf', { outDir: out });
    expect(r.path).toBe(join(out, 'styles.pdf'));
    expect(new TextDecoder().decode(r.bytes.subarray(0, 5))).toBe('%PDF-');
    if (HAS_PDFTOPPM) {
      const pages = await renderPdfPages(r.path, { dpi: 30 });
      expect(pages.length).toBeGreaterThanOrEqual(1);
      const img = decodePng(pages[0] as Buffer);
      // A4 at 30 dpi is about 248 × 351 pixels.
      expect(img.width).toBeGreaterThan(200);
      expect(img.height).toBeGreaterThan(img.width);
    }
  });

  it('recalculates formulas when converting .xlsx to CSV', { timeout: 120_000 }, async () => {
    const r = await convertWithSoffice(fixturePath('xlsx/features/defined-names.xlsx'), 'csv');
    const rows = new TextDecoder().decode(r.bytes).trim().split('\n');
    // D1 = SUM(Values) = 55, D2 = SUM(Squares) * TaxRate = 385 * 0.2 = 77, D3 = LocalTotal * 2 = 110.
    expect(rows[0]?.split(',')[3]).toBe('55');
    expect(rows[1]?.split(',')[3]).toBe('77');
    expect(rows[2]?.split(',')[3]).toBe('110');
  });

  it('converts from bytes and between Office formats', { timeout: 120_000 }, async () => {
    const r = await convertWithSoffice(
      { bytes: loadFixture('docx/features/rtl.docx'), name: 'rtl.docx' },
      'odt',
    );
    expect(r.path.endsWith('rtl.odt')).toBe(true);
    expect(targetExtension('csv:Text - txt - csv (StarCalc):44,34,76')).toBe('csv');
  });
});

describe.skipIf(!HAS_SOFFICE)('LibreOffice opens every generated fixture', () => {
  const inputs = (['docx', 'xlsx', 'pptx'] as const).flatMap((f) => [
    ...listFixtures(f),
    ...listFixtures(f, 'manual'),
  ]);

  it(`converts all ${inputs.length} features/* and manual/* files to PDF without error`, {
    timeout: 900_000,
  }, async () => {
    const out = mkdtempSync(join(tmpdir(), 'kiln-all-'));
    const results = await convertBatchWithSoffice(inputs, 'pdf', out);
    const failures = results.filter((r) => r.error);
    expect(failures).toEqual([]);
    expect(results.filter((r) => r.output)).toHaveLength(inputs.length);
    for (const r of results) {
      const head = readFileSync(r.output as string)
        .subarray(0, 5)
        .toString('latin1');
      expect(head, r.input).toBe('%PDF-');
    }
  });
});
