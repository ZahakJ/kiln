# kiln-sheet

The Kiln Grid workbook model (PLAN.md §3.4), ported from the v1.0 TypeScript `sheet-model` (WP-69) and bound to the `kiln-formula` engine. A `Workbook` holds ordered sheets, defined names (workbook and sheet scope, the reserved `_xlnm.` names), the style table, the theme, calculation and workbook properties, workbook protection, threaded-comment authors, custom lists and a passthrough slot for parts the file layer keeps verbatim. Each `Sheet` has a `SparseStore` of cells, row and column properties and its metadata: merges, tables, conditional formatting, data validation, hyperlinks, notes and threaded comments, the view (freeze, split, zoom, selection, gridlines, right-to-left), page setup, AutoFilter, drawing anchors, visibility, tab colour, outline settings and sheet protection. The whole 1,048,576 × 16,384 grid is addressable.

The formula engine owns formulas, the dependency graph and results; the sheets live inside the engine's host, so recalculation reads literal values straight from the stores. Every edit is a `Command` that validates first (a refused command changes nothing), mutates model and engine together, recalculates in automatic mode and returns its exact inverse; `CommandLog` builds undo/redo, nestable transactions and serialisable history on that. Workbook-wide edits (inserting and deleting rows and columns, moves, sheet renames and removal, table renames and conversion) rewrite every formula — cells, names, conditional formats, validation criteria, table column formulas, hyperlink locations — and their inverses restore what was rewritten, so undo is exact even where a reference became `#REF!`. A property test applies random sequences of 30 command kinds and checks, after every step, that the inverse restores the previous state, that redo restores the next, that the engine holds exactly the model's formulas and that results equal a workbook rebuilt from scratch.

## Public API

| Module | What it gives |
|---|---|
| `workbook` | `Workbook` (`new`, `with_options`, `sheets`, `sheet`, `sheet_by_name`, `value`, `formula`, `record`, `display`, `take_changes`, the low-level `load_value`/`load_formula`/`load_style`/`load_name`/`load_sheet_meta`/`load_sheet_layout` for file readers), `CellRecord`, `Changes`, `WorkbookOptions`, `check_sheet_name` |
| `store` | `SparseStore` (row chunks with columnar `cols`/`values`/`meta` arrays, a lazily built per-row hash index above 24 cells, `get`, `iter`, `iter_range` with dense and sparse row paths, `used_range`, `extent`, `shift_rows`/`shift_cols`), `CellKind` (value, formula, legacy formula, raw formula kept for round trips), `CellData` |
| `styles`, `color` | `StyleTable` (`cellXfs`-like interning of fonts, fills, borders, number formats, alignment and protection; `patch`, `intern`, `from_parts`/`to_parts` keeping file indices; differential formats; named styles), `StylePatch`, `Dxf`, `BUILTIN_NUMFMTS`; `Color` (RGB, theme with tint, indexed, automatic), `Theme`, Excel's tint arithmetic |
| `axis` | Row and column properties, pixel geometry with O(log n) offsets and hit-testing, Excel's column-width units, `FontMetrics` |
| `meta` | Tables, every conditional-format rule type (with the x14 data-bar and icon-set options), data validation, AutoFilter and sort state, hyperlinks, notes, threaded comments, views, print settings, drawing anchors, sheet and workbook protection (Excel's legacy password hash), calculation and workbook properties |
| `commands`, `command_log` | `Command` (cells, ranges, clear, formats, insert/delete rows and columns, move, copy with every Paste Special option and operation, sort, AutoFill, filter, remove duplicates, text to columns, sheets, row/column properties, merges, names, tables, freeze panes, metadata, properties, batches, restore), `Workbook::apply` (returns the inverse), `Workbook::resolve` (deterministic replay), `CommandLog` (`exec`, `undo`, `redo`, `begin`/`commit`/`rollback`, `transaction`, `history`, JSON) |
| `calc` | Recalculation on edit in automatic mode, Calculate Now (F9), Calculate Full and Calculate Sheet, manual mode, iterative calculation, results and spill ranges, circular references, `evaluate_at`, `explain` (Formula Lab), cached results written back to the store |
| `cf`, `validation` | `Workbook::conditional_formats` (per-cell formats, colour-scale fills, data bars with negative values and axes, icons; priorities and Stop If True); `Workbook::validate`, `validation_list`, `validation_prompt`, `invalid_cells` |
| `clipboard`, `fill`, `sort`, `filter`, `data` | Copy (values, formulas, formats, displayed text, merges) to TSV and HTML, TSV and HTML from other applications into typed cells, `paste_command`; AutoFill series (linear and growth trends, dates by day, weekday, month and year, lists, numbered text, copies); sort order (Excel's, or ICU collation by locale with the `collation` feature; custom lists, case sensitivity, colour sorts); AutoFilter evaluation (values, date groups, custom criteria with wildcards, top 10, dynamic dates and averages, colours, icons); typed-entry recognition, Remove Duplicates, Text to Columns |
| `serial` | `Workbook::to_bytes`/`from_bytes` (postcard behind a `KILNGRID` header and version), `snapshot`, `snapshot_sheet` |

## Features

- `parallel` (default): independent formulas of a topological level are evaluated on the rayon pool.
- `collation` (default): locale-aware sorting through ICU4X. Without it sorts use Excel's own text order.

`cargo build --target wasm32-unknown-unknown -p kiln-sheet --no-default-features` builds the single-threaded core for the web.

## Performance

`cargo run --release -p kiln-sheet --example edit_bench` measures budgets B8 and B9 through the model (store, command application, engine, write-back of results), not the bare engine: a full recalculation of 1,000,008 formula cells on four threads, and single-cell edits applied as commands in a 100,008-formula workbook. `tests/incremental.rs` checks in the same workbook that an edit evaluates exactly its 36 dependents.

Licence: AGPL-3.0-or-later.
