# kiln-io-xlsx

Reads `.xlsx`, `.xlsm`, `.xltx` and `.xltm` workbooks into the Kiln Grid model (`kiln-sheet`) and
writes the model back, keeping every part it does not understand (WP-70, PLAN.md §3.10). What
was read comes back exactly; what no edit touched comes back byte for byte. Pure Rust, no I/O:
the same code runs natively and in the web build (`--no-default-features` leaves out the
parallel recalculation).

```rust
let mut read = kiln_io_xlsx::read_xlsx(&bytes)?;          // XlsxError for non-workbooks
for w in &read.warnings { eprintln!("{w}"); }              // damaged references, unknown values
read.workbook.apply(&command)?;                            // edit through kiln-sheet commands
let saved = kiln_io_xlsx::write_xlsx(&read.workbook, Some(&read.source), &WriteOptions::default())?;
std::fs::write("out.xlsx", saved.bytes)?;                  // saved.regenerated / dropped / issues
```

## Reading

`read_xlsx` returns `XlsxRead { workbook, source, warnings }`. Formulas are calculated on open
(the file's calculation settings are restored, which recalculates once); with
`ReadOptions::defer_calculation` the workbook comes back with the stored results to show and
`XlsxRead::calculate` binds and calculates the formulas later, which is how budget B7 separates
the first paint from the full calculation.

| Part | Read into the model |
|---|---|
| `workbook.xml` | sheets (worksheets; chartsheets and dialog sheets are kept in place), sheet ids, visibility, defined names (workbook and sheet scope, `_xlnm.` names; names the model refuses are kept verbatim), `date1904`, code name, calculation settings, workbook protection (legacy and SHA-512 hashes), active and first tab, tab ratio |
| worksheets | `sheetData` **streamed** by a small strict tokenizer (`read/scan.rs`, never a tree): every value type (`n`, `s`, `str`, `inlineStr`, `b`, `e`, `d`), rich text, styles, shared formulas expanded per cell, legacy array formulas (Ctrl+Shift+Enter) and dynamic arrays (`cm` + `metadata.xml`), data tables kept whole, `vm`/`ph` and unknown `<f>` attributes kept; rows and columns (heights, widths, hidden, outline, styles); views (freeze, split, zoom, selection, gridlines, headings, RTL, view mode); protection; AutoFilter with every filter kind and sort state; merges; conditional formatting with the Excel 2010 (`x14`) data bar and icon set options and x14-only rules; data validation (2007 and `x14`); hyperlinks; print options, margins, page setup, headers and footers, page breaks; tab colour, outline settings, default sizes |
| `styles.xml` | number formats, fonts, fills (pattern and gradient), borders, cell formats with alignment and protection, named cell styles, differential formats, custom table styles — indices kept exactly |
| `sharedStrings.xml` | streamed; plain and rich text |
| tables | ranges, header and totals rows, columns with totals functions, labels and calculated formulas, styles, filters |
| comments + VML | notes with author, text, visibility and size |
| threaded comments + people | threads, replies, resolution, mentions |
| drawings | anchors (two-cell, one-cell, absolute) of charts, pictures, shapes, groups and connectors, with names, alternative text and the part each refers to |
| theme | the twelve colours and the heading and body fonts |

`_xlfn.`, `_xlfn._xlws.` and `_xlpm.` prefixes are taken off formulas (`formula_text`) and put
back on save. Strict (ISO/IEC 29500) files are read and saved as transitional.

## Writing

`write_xlsx(&workbook, Some(&source), &options)` starts from the original package. The workbook,
worksheets, styles and shared strings are written from the model; in each, an element the model
still holds unchanged is the element read (so attributes and extensions the model does not cover
survive) and everything else is serialised from the model in schema order. Notes, threaded
comments, tables, drawing anchors, the theme and the people list are rewritten only when they
changed. Pivot tables and caches, slicers, the VBA project (only into `.xlsm`/`.xltm`), external
links, custom XML, charts, pictures, printer settings and anything unknown keep their bytes and
relationship ids; parts only removed content referred to (the table of a deleted sheet) are
removed; `calcChain.xml` is dropped (Excel rebuilds it). Without a source, a complete new package
is written. Output is deterministic; `WriteOptions::now` stamps the modification date.

Formulas are written as the model holds them, prefixed as Excel needs; a non-legacy formula that
spills (or depends on array evaluation) is written as a dynamic array with the `XLDAPR` cell
metadata Excel expects, a legacy array formula with its original range. Shared formulas are
written per cell.

## Tests

- `cargo nextest run -p kiln-io-xlsx` — unit tests per module (parse/serialise pairs of every
  element, the tokenizer, formula prefixes, strings, styles), and:
  - `tests/roundtrip.rs`: every fixture (features, manual, the POI corpus when present) read,
    written with its source and as a new package, and read again: same facts, values and
    formulas; untouched parts byte-identical; a second save stable;
  - `tests/edits.rs`: cell values, formulas, dynamic arrays, formats, inserted rows (tables and
    names follow), added, renamed and removed sheets, notes, hyperlinks, conditional formats,
    validation, merges, freeze panes, row and column sizes, names, `.xlsm` ↔ `.xlsx` — each saved,
    re-read to the same model and opened by LibreOffice;
  - `tests/libreoffice_values.rs`: every formula cell of the feature fixtures against
    LibreOffice's recalculated values, and the CSV export against LibreOffice's (14 cells where
    LibreOffice 24.2 lacks the function or differs from Excel by design are listed with the
    reason);
  - `tests/exceljs_oracle.rs`: values, formulas and merges against exceljs (all agree) and the
    format summary (≥ 0.99 per workbook) — `node packages/testing/scripts/exceljs-xlsx.mjs`
    regenerates `fixtures/xlsx/read/*.exceljs.json`;
  - `tests/hostile.rs`: damaged and hostile parts (DTDs, unknown entities, unbalanced markup,
    absurd nesting, out-of-range references and styles, random mutations) give errors or
    warnings, never panics, and whatever is read saves and reads again.
- `cargo xtask fidelity --format xlsx` — the corpus through `kiln-testing` (adapter
  `crates/kiln-testing/src/formats/xlsx.rs`); results in `docs/qa/fidelity.md`.
- `cargo run --release -p kiln-io-xlsx --example open_bench fixtures/large/sheet-1m.xlsx` — B7:
  on the reference machine about 0.7 s to read the 1,000,000-cell workbook ready to paint
  (`parse_ms`) and 0.8 s to bind and calculate its 200,000 formulas (`calc_ms`).
  `kiln-bench run xlsx-open` measures the same for `cargo xtask budgets`.
- `cargo run -p kiln-io-xlsx --example rewrite in.xlsx out.xlsx [--new]` — read and save, with
  the reader's warnings and the writer's report.

Licence: AGPL-3.0-or-later.
