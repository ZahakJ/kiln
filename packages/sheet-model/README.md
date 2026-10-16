# @officina/sheet-model

The Tabula (Grid) workbook model (PLAN.md §3.3): workbook and sheets, the sparse cell store, cell
styles, merges, defined names, tables, conditional formatting, validation and the other sheet
metadata, the undo/command log, the binding to the formula engine and the number-format bridge.

```ts
import { CommandLog, Workbook, WorkbookCalc, formatCell } from '@officina/sheet-model';

const wb = new Workbook();                 // one sheet, "Sheet1"
const calc = new WorkbookCalc(wb);         // formula engine bound to the store
const log = new CommandLog(wb);            // undo/redo
const s = wb.sheets[0].id;

log.exec({ type: 'setCells', sheet: s, cells: [{ row: 4, col: 0, value: 10 }, { row: 0, col: 1, formula: '=A5*2' }] });
calc.getValue(s, 0, 1);                    // 20 (automatic mode recalculates after every command)
log.exec({ type: 'insertRows', sheet: s, at: 2, count: 1 });
wb.sheets[0].cells.getFormula(0, 1);       // 'A6*2'
log.undo();                                // exact: formulas, names, merges, tables… restored
formatCell(1234.5, '#,##0.00');            // { text: '1,234.50', align: 'right', … }
```

## Model

- **`Workbook`** — ordered `sheets`, `names` (`DefinedName`: workbook or sheet scope, hidden,
  reserved `_xlnm.*` names such as the print area), `styles` (`StyleTable`), `theme`
  (`@officina/ooxml` `Theme`, Office theme by default), `props` (`date1904`, calculation mode and
  iteration settings, active sheet) and an opaque `passthrough` slot for the file layer. Emits
  `WorkbookEvent`s (`cells`, `structure`, `move`, sheet add/remove/rename/move, `names`, `tables`,
  `layout`, `meta`, `props`, and one `commit` per top-level command).
- **`Sheet`** — `cells: SparseStore`, `rows`/`cols` (`Axis<RowProps|ColProps>`: heights in points,
  widths in SpreadsheetML character units, hidden/filtered, outline level, collapsed, row/column
  formats, pixel offsets and hit-testing for the renderer), `merges`, `cf`, `validations`, `tables`,
  `hyperlinks`, `notes`, `threads`, `view` (freeze, split, zoom, selection, gridlines, headings,
  formulas, right-to-left), `print` (orientation, paper, scaling/fit, margins, headers/footers,
  breaks), `autoFilter`, `charts`/`drawings` anchors (objects referenced by id), visibility and tab
  colour. Defaults follow Excel: 15 pt (20 px) rows and 8.43-character (64 px) columns with
  Calibri 11; other fonts go through `FontMetrics` (the grid can inject canvas measurements).
- **`SparseStore`** — `Map<row, RowChunk>`; a `RowChunk` keeps parallel arrays sorted by column
  (`cols: Int32Array`, `values`, `formulas`, `styleIdx: Int32Array`) with O(1) random access through
  a lazily built per-row hash index. Row-major `forEach`, `forEachInRange`, `getUsedRange`, bulk
  `setRange`, `clearRange`, row/column shifting. Formulas are stored without `=`; a formula cell's
  value is its cached result.
- **`StyleTable`** — `cellXfs`-like indices over interned fonts, fills (pattern and gradient),
  borders (all 13 line styles per side plus diagonals), number format codes, alignment and
  protection; equal formats always share one index (`patch` merges a `StylePatch`). Differential
  formats (`dxfs`) for conditional formatting, named cell styles and the built-in number formats
  (`BUILTIN_NUMFMTS`).
- Metadata types in `meta.ts`: tables (ListObjects with header/totals rows, column totals functions
  and calculated columns, table style, filter), every conditional-format rule type of §2.3 (cell
  value, formula, text, blanks/errors, dates, duplicates/unique, top/bottom, above/below average,
  colour scales, data bars, icon sets — with priority, stop-if-true and dxf styles), data
  validation, AutoFilter and sort state, anchors.

## Commands and undo

Every mutation is a plain JSON `Command` (`setCells`, `setRange`, `clearRange`, `setStyle`,
`setStyleIndex`, `insertRows/Cols`, `deleteRows/Cols`, `moveRange`, `copyRange` with paste-special
options and tiling, `sort`, `addSheet`, `removeSheet`, `renameSheet`, `moveSheet`, `setRowProps`,
`setColProps`, `merge` (merge, merge & center, merge across), `unmerge`, `defineName`, `deleteName`,
`addTable`, `removeTable` (convert to range), `updateTable`, `setFreeze`, `setSheetMeta`, `setProps`,
`batch`). `applyCommand(wb, cmd)` validates (throwing `ModelError` without changing anything),
applies and returns the exact inverse. Structural edits rewrite every formula in the workbook —
cells, names, conditional formats, validation and table formulas — with the formula package's
reference shifting; their inverses restore whatever was rewritten, so undo is exact even where a
reference became `#REF!`.

`CommandLog` keeps undo/redo stacks (redo re-applies the original command; ids are resolved first so
replays are deterministic), groups commands into nestable transactions (`transaction(label, fn)`,
`begin`/`commit`/`rollback`) and serialises the history (`serialize`/`load`, `serializeCommand`,
`parseCommand`).

## Calculation

`WorkbookCalc` implements the `@officina/formula` host interface over the store
(`SheetModelHost`: literal values, used bounds, hidden/filtered rows for SUBTOTAL/AGGREGATE, tables
for structured references), mirrors sheets, formulas and names into an `Engine`, keeps it in sync
from workbook events and recalculates incrementally after each command (automatic mode) or on demand
(`recalc`, `recalcAll`, `recalcSheet` in manual mode). Results are written back to the store as cached
values. `getValue`, `getResult`, `getSpillRange`, `isCircular`, `getDisplay` (formatted),
`evaluate` and `explain(sheet, row, col)` for the Formula Lab. Changing the date system rebuilds the
engine. The engine runs in-process; WP-17 wraps it in a worker.

## Number formats

`formatCell(value, code, { locale, date1904, generalWidth })` returns the display text, the colour
from `[Red]`/`[Color n]`, the default alignment, and *runs* for the renderer: `fill` runs (`*` — the
accounting layout's space between the currency symbol and the number) and `pad` runs (`_)` — the
width of a character). General follows Excel's cell rules (11 characters, rounding, scientific
switch); fixed-point formats round the 15-digit decimal value half away from zero like Excel
(1.005 → 1.01); everything else goes through `numfmt`. Known differences: `A/P` prints `AM/PM`, and
fractions use continued-fraction convergents (`# ??/??` of π is `3 1/7`).

## Snapshots

`snapshotWorkbook` / `restoreWorkbook` (plain, structured-clone-safe JSON with columnar cell rows),
`snapshotSheet` / `restoreSheet`, and `workbookToBytes` / `workbookFromBytes` (gzip via fflate).

## Scripts

`pnpm lint`, `pnpm typecheck`, `pnpm test` (500+ tests: store semantics, shifting matrices, number
formats, commands, undo/redo property tests, calculation integration, snapshots), `pnpm bench`
(1M-cell fill ≈ 0.15 s and iteration ≈ 17 ms against targets of 2 s and 300 ms). Property-test stress
runs: `PROP_SEEDS=400 PROP_STEPS=80 pnpm test -- test/undo.property.test.ts`.

Licence: AGPL-3.0-or-later.
