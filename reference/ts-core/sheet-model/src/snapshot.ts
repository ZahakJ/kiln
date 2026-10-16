/**
 * Snapshots: the whole workbook (or one sheet) as plain, structured-clone-safe JSON, for version
 * history, journals, undo of sheet deletion and worker transfer, plus a gzip encoding (fflate).
 *
 * Cells are stored row by row in columnar form (`c`, `v`, `f`, `s` arrays) which keeps a million-cell
 * snapshot compact. The workbook's `passthrough` (owned by the file layer) is not included.
 */
import type { Theme } from '@kiln-ref/ooxml';
import { gunzipSync, gzipSync, strFromU8, strToU8 } from 'fflate';
import { ModelError } from './address.ts';
import type { DefinedName, WorkbookProps } from './meta.ts';
import { Sheet, type SheetLayoutJSON, type SheetMetaJSON } from './sheet.ts';
import { StyleTable, type StyleTableJSON } from './styles.ts';
import { decodeValue, type EncodedValue, encodeValue } from './values.ts';
import { Workbook, type WorkbookOptions } from './workbook.ts';

export const SNAPSHOT_FORMAT = 'kiln.workbook';
export const SNAPSHOT_VERSION = 1;

export interface RowSnapshot {
  r: number;
  c: number[];
  v: EncodedValue[];
  /** Formulas (only present when the row has any). */
  f?: (string | null)[];
  /** Style indices (only present when the row has a non-default one). */
  s?: number[];
}

export interface SheetSnapshot {
  id: number;
  name: string;
  meta: SheetMetaJSON;
  layout: SheetLayoutJSON;
  cells: RowSnapshot[];
}

export interface WorkbookSnapshot {
  format: typeof SNAPSHOT_FORMAT;
  version: number;
  props: WorkbookProps;
  styles: StyleTableJSON;
  names: DefinedName[];
  theme?: Theme;
  ids: { sheet: number; table: number; rule: number };
  sheets: SheetSnapshot[];
}

export function snapshotSheet(sheet: Sheet): SheetSnapshot {
  const cells: RowSnapshot[] = [];
  for (const r of sheet.cells.rows()) {
    const ch = sheet.cells.chunk(r);
    if (!ch) continue;
    const row: RowSnapshot = { r, c: Array.from(ch.cols.subarray(0, ch.size)), v: [] };
    let anyF = false;
    let anyS = false;
    for (let i = 0; i < ch.size; i++) {
      row.v.push(encodeValue(ch.values[i]));
      if (ch.formulas[i] !== null) anyF = true;
      if (ch.styleIdx[i] !== 0) anyS = true;
    }
    if (anyF) row.f = ch.formulas.slice(0, ch.size);
    if (anyS) row.s = Array.from(ch.styleIdx.subarray(0, ch.size));
    cells.push(row);
  }
  return { id: sheet.id, name: sheet.name, meta: sheet.metaToJSON(), layout: sheet.layoutToJSON(), cells };
}

export function restoreSheet(snap: SheetSnapshot, mdw: number): Sheet {
  const sheet = new Sheet(snap.id, snap.name, mdw);
  sheet.loadMeta(snap.meta);
  sheet.loadLayout(snap.layout);
  for (const row of snap.cells) {
    for (let i = 0; i < row.c.length; i++) {
      sheet.cells.set(row.r, row.c[i] as number, decodeValue(row.v[i]), row.f?.[i] ?? null, row.s?.[i] ?? 0);
    }
  }
  return sheet;
}

export function snapshotWorkbook(wb: Workbook): WorkbookSnapshot {
  const snap: WorkbookSnapshot = {
    format: SNAPSHOT_FORMAT,
    version: SNAPSHOT_VERSION,
    props: structuredClone(wb.props),
    styles: structuredClone(wb.styles.toJSON()),
    names: structuredClone(wb.names),
    ids: wb.idCounters(),
    sheets: wb.sheets.map(snapshotSheet),
  };
  if (wb.theme) snap.theme = structuredClone(wb.theme);
  return snap;
}

export function restoreWorkbook(
  snap: WorkbookSnapshot,
  opts: Omit<WorkbookOptions, 'sheets'> = {},
): Workbook {
  if (snap.format !== SNAPSHOT_FORMAT) throw new ModelError('Not an Kiln workbook snapshot');
  if (snap.version > SNAPSHOT_VERSION)
    throw new ModelError(`Snapshot version ${snap.version} is newer than this version of Kiln supports`);
  const wb = new Workbook({ ...opts, sheets: [] });
  wb.styles = StyleTable.fromParts(snap.styles);
  wb.theme = snap.theme ? structuredClone(snap.theme) : opts.theme;
  wb.props = structuredClone(snap.props);
  wb.names = structuredClone(snap.names);
  const mdw = wb.maxDigitWidth;
  for (const s of snap.sheets) wb.insertSheetObject(restoreSheet(s, mdw), wb.sheets.length);
  wb.setIdCounters(snap.ids);
  return wb;
}

/** gzip-compressed JSON snapshot. */
export function workbookToBytes(wb: Workbook, level: 0 | 1 | 2 | 3 | 4 | 5 | 6 | 7 | 8 | 9 = 6): Uint8Array {
  return gzipSync(strToU8(JSON.stringify(snapshotWorkbook(wb))), { level });
}

export function workbookFromBytes(bytes: Uint8Array, opts: Omit<WorkbookOptions, 'sheets'> = {}): Workbook {
  let text: string;
  try {
    text = strFromU8(gunzipSync(bytes));
  } catch {
    throw new ModelError('The snapshot is not valid gzip data');
  }
  return restoreWorkbook(JSON.parse(text) as WorkbookSnapshot, opts);
}
