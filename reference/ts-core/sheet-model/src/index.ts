/**
 * @kiln-ref/sheet-model — the Kiln Grid workbook model (PLAN.md §3.3).
 *
 * - `Workbook` / `Sheet` / `SparseStore` / `RowChunk`: sheets, the sparse cell store, row/column
 *   properties and metadata (merges, names, tables, conditional formatting, validation, hyperlinks,
 *   notes, views, print settings, AutoFilter, drawing anchors).
 * - `StyleTable`: deduplicated cell formats (`cellXfs`-style indices) and differential formats.
 * - Commands + `CommandLog`: every mutation with an exact inverse; undo/redo; transactions; JSON.
 * - `WorkbookCalc`: the formula engine bound to the store (incremental recalc, spills, names, tables).
 * - `formatCell`: number formats via numfmt, with Excel's General rules.
 * - Snapshots: structured-clone-safe JSON and gzip.
 */
export const PACKAGE_NAME = '@kiln-ref/sheet-model' as const;

export * from './address.ts';
export * from './axis.ts';
export * from './calc.ts';
export * from './command-log.ts';
export * from './commands.ts';
export * from './format.ts';
export * from './meta.ts';
export * from './sheet.ts';
export * from './shift.ts';
export * from './snapshot.ts';
export * from './store.ts';
export * from './structured.ts';
export * from './styles.ts';
export * from './values.ts';
export * from './workbook.ts';
