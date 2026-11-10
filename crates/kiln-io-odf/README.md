# kiln-io-odf

Imports and exports OpenDocument (ODF 1.3) text documents, spreadsheets and presentations
(`.odt`, `.ods`, `.odp`, and the `.ott`, `.ots`, `.otp` templates) through the same models as the
Office formats: `kiln_doc::Document`, `kiln_sheet::Workbook` and `kiln_deck::Presentation`.

Kiln's models are shaped like OOXML, so every ODF construct goes through a mapping table,
documented on its module:

| Module | What it maps |
|---|---|
| `package` | the zip (`mimetype` first and stored, `META-INF/manifest.xml`), zip-bomb limits, encryption refused |
| `xml` | parts parsed with canonical namespace prefixes; a deterministic writer |
| `props`, `text` | character, paragraph and list formatting; paragraphs, headings, lists, tables, sections and page styles, headers and footers, notes, fields, comments, tracked changes, frames, pictures and shapes |
| `sheet`, `formula`, `numfmt` | cells, cell styles, data styles ⇄ Excel number formats, OpenFormula ⇄ Excel formulas, merges, rows and columns, sheets, names, conditional formats, validation, notes, hyperlinks, page setup with headers and footers, views, charts and pictures in cells |
| `deck` | slides, master pages, layouts, shapes and their geometry, text, pictures, tables, notes, comments, transitions, charts |
| `chart` | chart objects (`Object N/` with its local table) ⇄ the kiln-charts model |
| `meta` | `meta.xml` document properties |

kiln-session uses the crate behind its `odf` feature (on by default). The web core
(kiln-bridge-wasm) builds without it for now: the crate adds about 220 KB gzipped, which takes the
release core over budget B12.

What has no counterpart is reported in each read's warnings and by `ods_issues`, `odt_issues` and
`odp_issues` (what a save cannot keep), which the Compatibility Checker lists. Grid keeps a
workbook's charts beside it: pass them as `WriteOptions::charts` (and pictures not in the
passthrough store as `WriteOptions::media`); `OdsRead::charts` returns them. Readers never panic on input: malformed values become defaults or warnings, and
only a package that is not an ODF document of the expected kind is an error.

```rust
let read = kiln_io_odf::read_ods(&std::fs::read("budget.ods")?)?;
let bytes = kiln_io_odf::write_ods(&read.workbook, &kiln_io_odf::WriteOptions::default())?;
```

Tests: `cargo nextest run -p kiln-io-odf` reads, writes and re-reads every ODF fixture in
`fixtures/odf` (made by LibreOffice from the Office feature files) and converts every Office
feature file to ODF. `tests/import_metric.rs` compares the ODF reader's result for each fixture
with the Office reader's result for its source (an import measure independent of this crate's
writer); `tests/oracle.rs` has LibreOffice read Kiln's output (repeated formulas, charts in all
three kinds); `tests/malformed.rs` reads the hostile corpus in `fixtures/odf/malformed` within
seconds and bounded memory. `cargo xtask fidelity --format odf` scores the round trips with the
LibreOffice oracle (visual ≥ 0.80).
