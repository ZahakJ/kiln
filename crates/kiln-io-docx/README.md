# kiln-io-docx

Reads `.docx`, `.dotx`, `.docm` and `.dotm` files into the Kiln Write model (`kiln-doc`), keeping
every part it does not understand, and writes the model back (WP-65): what was read comes back
exactly, what no edit touched comes back byte for byte. Ported from the v1.0 TypeScript reader
(WP-11) and writer, and extended to the Rust model: block content controls, move marks, one-sided
wrapping, Word's own table cell model and text box content are modelled rather than flattened.
Pure Rust, no I/O: the same code runs natively and in the web build.

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
- **Bounded nesting.** Containers deeper than 64 levels and text boxes deeper than 4 are not
  followed, so hostile files cannot exhaust the stack; a table is followed only when its cells fit
  under that limit too, otherwise it is kept whole as opaque XML. Parts are parsed with
  `keep_too_deep_as_raw`: elements past the XML layer's 2,000-level tree limit stay verbatim
  markup, so such files open and save without loss instead of being refused.
- **Symbols** keep font and code; `symbol_to_unicode` maps Symbol, Webdings and Wingdings 1–3 with
  the `dingbat-to-unicode` 1.0.2 table (BSD-2-Clause, © 2021 Michael Williamson; generated into
  `src/common/dingbats.rs`).

## Writing

```rust
let pkg = kiln_ooxml::opc::OpcPackage::open(&bytes)?;
let read = kiln_io_docx::read_package(&pkg)?;
// … edit read.document through transactions, recording what they touch:
let mut touched = kiln_io_docx::TouchedParts::new();
touched.record(&doc, &tx);
let source = kiln_io_docx::WriteSource::from_read(&read).with_package(&pkg);
let out = kiln_io_docx::write_docx(&doc, Some(&source), &kiln_io_docx::WriteOptions {
    now: Some(kiln_io_docx::w3cdtf(unix_seconds)), // dcterms:modified, cp:revision + 1
    preserve: Some(&touched),                      // copy untouched parts byte for byte
    ..Default::default()
})?;
// out.bytes, out.regenerated (parts written from the model), out.issues
let report = kiln_io_docx::compat_report(&doc, Default::default()); // Compatibility Checker
```

`write_docx(&doc, None, …)` writes a new document (styles, settings, theme, font table, Word's
note separators and comment companions as needed). `rewrite_docx(bytes, …)` is read → write with
every model part regenerated (the fidelity scorer's and `kiln-cli convert`'s round trip).

| Module | What it writes |
|---|---|
| `write/mod.rs` | the package: part names (original ones kept), placeholders, passthrough first (its relationship ids stay valid), stories, shared parts, relationship repair for kept XML, preserved parts, properties, relationships parts restored when unchanged |
| `write/story.rs` | paragraphs cut into segments where formatting, marks, kept run properties or objects change; marks as wrappers (`w:sdt`, `w:hyperlink`, `w:ins`/`w:del`) shared by neighbours; comment ranges; fields (complex, simple, nested `{ … }`, field blocks); notes, bookmarks, symbols, breaks; tables with row and cell content controls; section breaks in the last paragraph |
| `write/props.rs` | `w:rPr`, `w:pPr`, `w:tblPr`, `w:trPr`, `w:tcPr`, `w:sectPr` in schema order with kept raw children slotted in; classifiers of what the reader models (for merging styles and list levels) |
| `write/drawing.rs` | drawings verbatim when unchanged, patched when edited (size, placement, wrap, alt text, decorative, rotation, flips, image, crop, geometry), built when new; text box content from the model |
| `write/parts.rs` | styles (merged with their raw XML), numbering, settings (patched), people, comment threads and ids, font table, theme (kept when unchanged) |
| `write/sdt.rs` | content control properties kept and patched |
| `write/touched.rs` | `TouchedParts`: the parts a transaction touches |
| `write/compat.rs` | `compat_report`: what a save loses or changes, then what was lost on open |
| `write/xml.rs` | string XML emission, namespace declarations of the parts, relationship references |

Design notes:

- **Original names and ids.** Parts keep the names they were read under (`/word/header3.xml` stays
  `header3.xml`); a relationship to the same target reuses its original id, fresh ids avoid every
  original one, and ids that kept XML refers to (charts, OLE objects, images of kept shapes,
  picture bullets) are re-added with their original targets.
- **Kept XML is merged, not trusted blindly.** A drawing's or content control's kept XML is only
  written verbatim when the model's view of it (derived exactly as the reader derives it) is
  unchanged; otherwise only what changed is patched.
- **Stable ids.** The writer writes stories in the order the reader numbers them, so a round trip
  gives the same node ids: tests compare whole documents and extras, not summaries.

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

- `tests/roundtrip_fixtures.rs` — every fixture (features, manual, the downloaded corpus) reads back
  into the same model and extras; untouched parts are byte-identical; writing is deterministic and a
  second generation identical; mammoth reads the same text from the rewritten feature files.
- `tests/write_elements.rs`, `tests/write_stories.rs`, `tests/write_package.rs` — the v1.0 writer
  tests ported: runs, paragraphs, revisions, fields, content controls, tables; drawings (kept,
  patched, new, every wrap), equations, comments and threads, notes, bookmarks, hyperlinks,
  sections, styles, numbering, settings; parts and relationships, properties, determinism,
  passthrough, macros, templates, Strict, new documents, issues.
- `tests/write_touched.rs` — edits regenerate only the parts they touch.
- `tests/write_hostile.rs` — arbitrary content (proptest) and deep nesting on a 1 MiB stack, up
  to 5,000 nested tables (every level's text saved).
- `tests/roundtrip_edits.rs` — edited-then-saved golden files (`fixtures/docx/roundtrip/<case>/`):
  the parts an edit regenerates must equal the committed expected output (`KILN_BLESS=1` rewrites
  them), every other part is the original.

`cargo run -p kiln-cli -- inspect --docx <file>` prints the outline, counts and diagnostics;
`cargo run -p kiln-cli -- convert in.docx out.docx` saves a file the way Kiln does.

Licence: AGPL-3.0-or-later.
