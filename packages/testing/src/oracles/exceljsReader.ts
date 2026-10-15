/** ExcelJS as an independent .xlsx reader (dev-only oracle). */
import ExcelJS from 'exceljs';

export type OracleCellValue = number | string | boolean | null | { error: string } | { date: string };

export interface OracleCell {
  value: OracleCellValue;
  /** Formula text without the leading `=` (shared formulas are expanded by ExcelJS). */
  formula?: string;
  numFmt?: string;
}

export interface OracleSheet {
  name: string;
  state: string;
  rowCount: number;
  columnCount: number;
  /** A1 address → cell, for non-empty cells. */
  cells: Record<string, OracleCell>;
  merges: string[];
}

export interface OracleWorkbook {
  sheets: OracleSheet[];
  definedNames: string[];
}

function plain(v: unknown): OracleCellValue {
  if (v === null || v === undefined) return null;
  if (typeof v === 'number' || typeof v === 'string' || typeof v === 'boolean') return v;
  if (v instanceof Date) return { date: v.toISOString() };
  const o = v as Record<string, unknown>;
  if ('error' in o) return { error: String(o.error) };
  if ('richText' in o) return (o.richText as { text: string }[]).map((r) => r.text).join('');
  if ('text' in o && 'hyperlink' in o) return String(o.text);
  if ('result' in o) return plain(o.result);
  return null;
}

/** Reads a workbook with ExcelJS into a plain, comparable structure. */
export async function readXlsxWithExcelJS(bytes: Uint8Array): Promise<OracleWorkbook> {
  const wb = new ExcelJS.Workbook();
  await wb.xlsx.load(Buffer.from(bytes) as unknown as ArrayBuffer);
  const sheets: OracleSheet[] = [];
  wb.eachSheet((ws) => {
    const cells: Record<string, OracleCell> = {};
    ws.eachRow({ includeEmpty: false }, (row) => {
      row.eachCell({ includeEmpty: false }, (cell) => {
        const raw = cell.value as unknown;
        const entry: OracleCell = { value: plain(raw) };
        const f = (cell as unknown as { formula?: string }).formula;
        if (f) entry.formula = f;
        if (cell.numFmt) entry.numFmt = cell.numFmt;
        cells[cell.address] = entry;
      });
    });
    const merges = [...((ws.model as unknown as { merges?: string[] }).merges ?? [])];
    sheets.push({
      name: ws.name,
      state: ws.state,
      rowCount: ws.rowCount,
      columnCount: ws.columnCount,
      cells,
      merges: merges.sort(),
    });
  });
  const names = (wb.definedNames as unknown as { model: { name: string }[] }).model.map((n) => n.name);
  return { sheets, definedNames: names.sort() };
}
