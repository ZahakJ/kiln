# kiln-io-docx

Reads `.docx`, `.dotx`, `.docm` and `.dotm` files into the Kiln Write model (`kiln-doc`), keeping
every part it does not understand, so the writer (WP-65) can put the file back together. Ported
from the v1.0 TypeScript reader (WP-11) and extended to the Rust model: block content controls,
move marks, one-sided wrapping, Word's own table cell model and text box content are modelled
rather than flattened. Pure Rust, no I/O: the same code runs natively and in the web build.

```rust
let read = kiln_io_docx::read_docx(&bytes)?;      // DocxError for non-Word input
read.document.validate()?;                         // always valid
for w in &read.warnings { eprintln!("{w}"); }      // the Compatibility Checker's input
let summary = kiln_io_docx::Summary::of(&read);    // outline and counts
```

## What a read returns (`DocxRead`)

| Field | What it holds |
|---|---|
| `document` | the `kiln_doc::Document`: sections with headers/footers, notes (separators kept, typed), threaded comments, styles, numbering, settings, properties, theme, media |
| `passthrough` | every part and relationship the reader did not model (`kiln_ooxml::passthrough::PassthroughSet`) |
| `extras` | `DocxExtras`: raw XML of unmodelled properties per paragraph, run range, table, row, cell and section; every style and abstract list as read; latent styles; the settings root; page borders and section note settings; original note and comment ids; `w14:paraId`; people |
| `package_properties` | core, app and custom properties exactly as read (including values the model does not hold) |
| `relationships` | relationship id → target per source part, for ids referenced from kept XML |
| `charts` | chart drawings with their part, for the charts work |
| `kind`, `main_part` | document, template, macro-enabled; the main part name |
| `warnings` | `DocxWarning { code, severity, message, part, path }` with stable codes (`drawing.smartArt`, `field.multiParagraph`, …) |

What is modelled, kept or approximated is listed in `docs/FORMATS.md` (Word documents).

## Design notes

- **Two-phase ids.** Note and comment ids are mapped to model nodes before any story is read, so a
  footnote can carry a comment and any story can refer to any note. References to notes or
  comments that do not exist are dropped with a diagnostic, so the document always validates.
- **Pieces, then paragraphs.** Runs become pieces (text or object, run properties, marks, the
  run's unmodelled XML); a paragraph is assembled once, which keeps runs normalised and records
  run extras by character range.
- **Fields** are a stack across paragraphs: a field's result is collected and summarised into a
  `Field` object (instruction + cached result), released as plain text if it crosses a paragraph,
  turned into a link for `HYPERLINK`, or into a `FieldBlock` for TOC, INDEX, BIBLIOGRAPHY and TOA.
- **Bounded nesting.** Containers deeper than 64 levels (tables are then kept as opaque XML) and
  text boxes deeper than 4 are not followed, so hostile files cannot exhaust the stack; the XML
  layer refuses elements nested beyond 2,000 levels.
- **Symbols** keep font and code; `symbol_to_unicode` maps Symbol, Webdings and Wingdings 1–3 with
  the `dingbat-to-unicode` 1.0.2 table (BSD-2-Clause, © 2021 Michael Williamson; generated into
  `src/common/dingbats.rs`).

## Tests

`cargo nextest run -p kiln-io-docx`:

- `tests/read_fixtures.rs` — structure of every feature fixture (numbering labels, merges, sections,
  headers, fields, comments, revisions, images, text boxes, properties, passthrough); every
  `fixtures/docx/**/*.docx` and `fixtures/large` reads into a valid document; the feature corpus
  reads without diagnostics; the external corpus never crashes.
- `tests/read_elements.rs` — the v1.0 element tests on hand-written packages (Strict, templates,
  content controls, equations, moves, nested and form fields, VML, charts, OLE, threads, settings).
- `tests/read_mammoth.rs` — body text against mammoth (`fixtures/docx/read/*.mammoth.txt`, made by
  `node packages/testing/scripts/mammoth-docx-text.mjs`; `--check` reports stale files): all
  feature fixtures score 1.0 (threshold 0.99).
- `tests/read_hostile.rs` — nesting at the parser's limits on a 1 MiB stack, unbalanced fields and
  markers, corrupted bytes and arbitrary values (proptest) never panic.
- `tests/read_performance.rs` — 300 synthetic pages: about 50 ms release, 200 ms debug.

`cargo run -p kiln-cli -- inspect --docx <file>` prints the outline, counts and diagnostics.

Licence: AGPL-3.0-or-later.
