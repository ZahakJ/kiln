# File formats and fidelity

Kiln reads and writes Office Open XML (`.docx`, `.xlsx`, `.pptx`) natively, in
the Rust crates `kiln-ooxml` (the shared package, XML and DrawingML layer) and
`kiln-io-docx`, `kiln-io-xlsx`, `kiln-io-pptx`, `kiln-io-odf`, `kiln-io-csv` and
`kiln-io-pdf`. This page says what each reader and writer covers, what is kept
without being understood, and what is lost. The numbers in the last section are
the targets the test suite holds every release to.

> **Status (WP-50).** The crates exist as scaffolds; the coverage below is the
> specification each one is built and tested against (the v1.0 lists, carried
> over unchanged). Each format's work package keeps its section current and
> marks what has shipped: `kiln-ooxml` WP-55, `kiln-io-docx` WP-62/65,
> `kiln-io-xlsx` and `kiln-io-csv` WP-70, `kiln-io-pptx` WP-77/79, charts WP-73,
> `kiln-io-pdf` WP-67, `kiln-io-odf` WP-87.

## The rule: keep what you don't understand

An Office file is a zip package of XML parts linked by relationships. Each
reader builds Kiln's model from the parts it knows and keeps every other
part, and every part it only partly read, as the original bytes. On save the
writer regenerates the parts it modelled and copies the kept parts back, with
their relationships and content types fixed up.

So a workbook with macros, an embedded spreadsheet in a document or a custom
XML part comes back out of Kiln with those parts intact, even though
Kiln cannot run or edit them.

Before saving to an Office format, the **Compatibility Checker**
(File ▸ Info ▸ Check for Issues) lists anything in the document that the target
format cannot hold or that Kiln will not carry through.

## Word documents (`.docx`, `.dotx`, `.docm`) — `kiln-io-docx`

Reading (WP-62) and writing (WP-65) are in place: Kiln Write opens and saves
`.docx`, `.dotx` and `.docm` (File ▸ Save, Save As, `kiln-cli convert`).

**Read into the document model:** paragraphs and runs with every paragraph and
font setting Word stores (styles, document defaults, theme fonts and colours with
tints and shades, toggles, character spacing, borders, shading, languages,
right-to-left); the style sheet with `basedOn`, linked and table styles and their
conditional formats; numbering (abstract lists, instances, level overrides,
legal numbering, picture bullet ids); tables in Word's own cell model (grid
spans, vertical merges, legacy horizontal merges, `gridBefore`/`gridAfter`,
nested and floating tables, widths in every unit); sections (page size and
orientation, margins and gutter, columns, break type, different first page,
headers and footers by kind and shared between sections, page and line
numbering, vertical alignment, document grid); footnotes and endnotes with
custom marks; comments with Word 2013+ threads, resolved state and authors
(`people.xml`); tracked insertions, deletions, moves, formatting and paragraph
property changes and row insertions/deletions; fields — simple and complex,
nested field codes written as `{ … }`, cached results, TOC/INDEX/BIBLIOGRAPHY/TOA
as multi-paragraph field blocks, HYPERLINK fields as links; hyperlinks and
bookmarks; symbols (`w:sym`, font and code kept, with the Symbol/Wingdings/Webdings
mapping to Unicode for display); content controls (block ones wrap their blocks,
inline ones are marks; checkbox, list, date, building-block galleries); pictures
inline and floating with every wrap mode, wrap side and polygon, alignment or
offset positions, crop, rotation, flips, alt text and the decorative flag;
shapes and text boxes (`wps:wsp`) with their text; equations as OMML; settings
(default tab stop, even/odd headers, mirror margins, track changes, colour
mapping, compatibility mode, note settings); core, app and custom properties.

**Kept as written, not edited:** every part the reader does not model goes to
the passthrough with its relationships (custom XML, font table, web settings,
glossary, `vbaProject.bin`, embedded objects, chart parts, SmartArt, thumbnails,
printer settings); unmodelled properties are kept as raw XML beside the model
(`DocxExtras`: `w:framePr`, `w:cnfStyle`, `w14` text effects, `w:noProof`,
`w:tblPrChange`, page borders, `w:paperSrc`, latent styles, the settings part,
`w14:paraId`); charts, groups, SmartArt, canvases, ink, OLE objects and VML
shapes other than pictures and text boxes keep their XML and show as
placeholders; `w:altChunk` imports are opaque blocks. Each of these is reported
with its part and XPath to the Compatibility Checker.

**Approximate or dropped (reported):** positions given as a percentage of the
page read as an offset of zero; phonetic guides (ruby) keep the base text only;
alignment tabs (`w:ptab`) become ordinary tabs; editing permission ranges and
subdocument links are dropped; legacy form fields become plain fields; a field
whose result spans paragraphs (other than TOC-like fields) keeps its result as
text; tables nested so deep that their cells would pass 64 containers (about 28
tables), text boxes more than 4 deep, and anything past the XML layer's
2,000-element tree limit (Apache POI's 5,000-table `deep-table-cell.docx`) are
kept verbatim as XML and saved unchanged, but not shown; how deep a table can be
and still be followed is measured from its content (content controls, links and
other wrappers in its cells count). A cell that ends with a nested table (or is
empty) gets the end-of-cell paragraph Word gives it; a cell that ended with a
nested table in the file is saved that way again while that paragraph is still
empty and unformatted, since Word and LibreOffice lay the two forms out
differently.
Packages that are encrypted, legacy
binary `.doc`, not zip files or beyond the zip limits are refused with a typed
error; a malformed secondary part is skipped with a diagnostic, never a crash.

**Written back (WP-65):** everything above is written from the model in the
order Word's schema requires, merged with the raw XML the reader kept: unmodelled
paragraph, run, table, row, cell and section properties go back in their schema
position; styles and list definitions are regenerated from the model and keep
the children the model does not cover (`w:rsid`, `w:tmpl`, `w15:tentative`,
`w14` effects in style run properties); `settings.xml` is the original part
with the modelled values patched in (zoom, rsids, compatibility settings and
the protection hash survive); drawings read from the file are written back
verbatim — `mc:AlternateContent` and VML fallback included — unless they were
edited, in which case only what changed is patched (size, placement and wrap,
alt text and the decorative flag, rotation and flips, the picture and its crop,
the preset geometry) and the stale fallback is dropped; text box content is
always written from the model; content control properties are kept and patched
(a ticked check box, an edited list); comments come back with their threads,
resolved state, `w14:paraId`s and authors, and `commentsIds.xml` is kept in step
when comments are added or removed; footnote and endnote separators keep their
ids, so `settings.xml` still points at them; header and footer parts, media
and every passthrough part keep their names and relationship ids. Tracked
moves are written as a deletion and an insertion (the reader reports that).
New documents get Word's parts (styles, settings with Word 2013+ compatibility
settings, theme, font table, separators when there are notes, `commentsIds`
and `people.xml` when there are comments).

**Byte-identical when untouched:** saving copies every part no edit touched
from the file that was opened, byte for byte — the session records the parts
each transaction touches (`TouchedParts`: the body, each header and footer,
footnotes, endnotes, comments, styles, numbering, settings) — so typing in the
body rewrites `document.xml` and the document properties only. Relationship
parts whose relationships did not change, an unchanged theme, media and every
passthrough part are copied as they were in any save. Content types and
relationships are regenerated deterministically: the same model and options
give the same bytes, and a second round trip writes the first one's bytes.

**Compatibility Checker:** `Query::CompatReport { format }` (and
`kiln_io_docx::compat_report`) lists, with counts, what a save loses or changes
(pictures without image data, drawings without markup, invalid control
characters, references to deleted notes or comments, orphaned replies, tracked
moves, macros when a `.docm` is saved as `.docx`, unused headers) followed by
what was lost or kept read-only on open (the reader's diagnostics).

**Verified by:** `cargo nextest run -p kiln-io-docx` — structural assertions for
every feature fixture, the v1.0 element tests ported, body text equal to
mammoth's (an independent reader) on all 19 feature fixtures, every
`fixtures/docx` file and the real-world corpus read into valid documents,
hostile nesting read on a 1 MiB stack, corrupted bytes never panic, 300 pages
read in about 50 ms (release). `kiln-cli inspect --docx <file>` prints the heading
outline, counts and diagnostics. For the writer: every fixture (features,
manual, the downloaded corpus) reads back into the same model *and* the same
reader extras, node ids included; every part the writer did not regenerate is
byte-identical; writing is deterministic and a second generation is identical;
mammoth reads the same text from the rewritten files; the v1.0 writer tests
(elements, stories, package) are ported; edits regenerate only the parts they
touch; arbitrary content (proptest) is written and read back; 300 pages are read
and written in about 120 ms (release). `cargo xtask fidelity --format docx`
scores structure, text, LibreOffice rendering, LibreOffice opening and untouched
bytes over the corpus (`docs/qa/fidelity.md`). `kiln-cli convert in.docx out.docx`
saves a file the way Kiln does and prints the Compatibility Checker's list.

**Approximate in layout (WP-63):** text wrap around floating objects follows
Word's rules with square and tight wrapping, so a line can break one word
differently from Word. The target is within one line per page on the fixture
corpus.

## Excel workbooks (`.xlsx`, `.xltx`, `.xlsm`) — `kiln-io-xlsx`

**Shipped (WP-70).** Reading and writing, with the round trip measured on every fixture
(`docs/qa/fidelity.md`: structural, values and formulas 1.000, untouched parts byte-identical,
LibreOffice opens every output), the values and formats checked against exceljs and the
calculated results against LibreOffice. The grid that shows the workbook is WP-71, charts are
drawn by WP-73.

**Read and written:** sheets, cell values of every type, shared and rich-text
strings, formulas (including shared formulas, legacy array formulas and dynamic
arrays), defined names, number formats, fonts, fills, borders, cell styles,
merged cells, frozen panes and selection, column widths and row heights,
conditional formatting (including the Excel 2010 data bars and icon sets),
data validation, hyperlinks, page setup, headers and footers, print areas and
breaks, AutoFilter, tables, notes and threaded comments, the 1900 and 1904 date
systems, workbook and sheet protection.

**Read, kept and moved:** pictures, shapes and charts in drawings — their anchors are in the
model and follow edits; the objects themselves are kept as written (Kiln's own charts are
written by WP-73).

**Kept, not edited:** pivot tables and pivot caches are preserved and shown as
their last values (Kiln's own pivot tables are written in Excel's format and
refresh in Excel), the VBA project of an `.xlsm`, external links, custom XML,
slicers, data tables (What-If Analysis), form controls, printer settings and
any part or extension Kiln does not know.

**Changed on save:** shared formulas are written cell by cell; `calcChain.xml` is left out
(Excel rebuilds it); a Strict (ISO/IEC 29500) file is saved as transitional; phonetic
(furigana) runs of strings are not kept; macros are not kept when an `.xlsm` is saved as
`.xlsx`. Every part no edit touched is copied byte for byte.

**Charts** (`kiln-charts`, shipped in WP-73): every chart part is read into Kiln's chart
model and written back from it — `c:chartSpace` for column, bar, line, area, pie, doughnut,
scatter, bubble and radar charts and their combinations (with a secondary axis), and Excel
2016's `cx:chartSpace` for treemap and sunburst. Read and written: series with their
references and cached values (or literal data), multi-level categories, axes (category, value
and date; bounds, units, logarithmic scale, reversed order, number formats, titles, tick marks
and labels, gridlines), legends and legend entries, titles (rich text or linked to a cell),
data labels (value, category, series name, percentage, bubble size, position, number format,
per-point labels), series and point formatting (fill, outline, markers, explosion, invert if
negative), trendlines, chart styles and the chart and plot areas. Kept unchanged in place:
everything else (error bars, up/down bars, drop and high-low lines, 3-D view and walls, data
tables, `c:extLst` extensions, `mc:AlternateContent`), and every `c:spPr`, `c:txPr` and
`c:rich` that was not edited. Every chart of the corpus round-trips with its structure intact
(`crates/kiln-charts/tests/roundtrip.rs`: 1.000). 3-D charts are drawn in 2-D; stock, surface
and bar-of-pie charts are kept and drawn as an empty plot area. Charts in Word documents and
presentations keep their embedded workbook (`embeddings/*.xlsx`), which Kiln reads to edit
the data.

## PowerPoint presentations (`.pptx`, `.potx`, `.pptm`) — `kiln-io-pptx`

**Read and written:** slide size, masters, layouts and slides with placeholder
inheritance; shapes with all 187 preset geometries, fills, lines and effects;
text with paragraph and character formatting and fields; pictures with
cropping; tables; charts; groups and connectors; backgrounds; transitions;
speaker notes; comments; sections.

**Mapped:** SmartArt is imported as a group of ordinary shapes. Transitions map
to Kiln's set, and an unknown transition becomes Fade. Animations Kiln
supports are editable; the others are kept in the file and dropped only if you
change the shapes on that slide.

**Kept, not edited:** embedded OLE objects, the VBA project of a `.pptm`,
custom XML.

## OpenDocument (`.odt`, `.ods`, `.odp`) — `kiln-io-odf`

Imported into and exported from the same models as the Office formats.
Kiln's models are shaped like OOXML, so an ODF file goes through a mapping;
features with no OOXML counterpart are reported by the Compatibility Checker.

## Other formats

- **CSV and TSV** (`kiln-io-csv`, shipped with WP-70): RFC 4180, with the delimiter, quoting
  and encoding detected (UTF-8 with or without BOM, UTF-16, Windows-1252) and every field
  recognised as Excel's cell entry does (numbers, percentages, currency, dates, times, logicals,
  errors). Saving writes the active sheet as Excel does: cells as displayed, Excel's quoting,
  CRLF, UTF-8.
- **JSON** (`kiln-io-csv`, WP-70): an array of objects becomes a table (keys in the order they
  first appear, typed values); a table saves as an array of objects.
- **Markdown, plain text and HTML:** open in Kiln Write and export from it.
- **PDF** (`kiln-io-pdf`, on `krilla`): export only, from the same Scene you
  see on screen, with subset fonts, the document outline as bookmarks, page
  labels and tagged structure.

## Fonts

Office's default fonts are proprietary, so Kiln bundles open fonts with the
same metrics, which keeps line breaks and page breaks where Word put them
(`kiln-text` loads them on every platform; the table is
`crates/kiln-text/src/substitution.rs` from WP-53):

| In the file | Rendered with |
|---|---|
| Calibri | Carlito |
| Cambria | Caladea |
| Arial | Liberation Sans |
| Times New Roman | Liberation Serif |
| Courier New | Liberation Mono |
| Aptos | the brand kit's UI sans (no metric-compatible open font exists; the Compatibility Checker says so) |

The file keeps the original font names, so the document looks as intended
again when opened in Office.

## Fidelity targets

The fixture corpus in `fixtures/` is round-tripped (opened and saved) and
compared with the original and with LibreOffice's rendering.
`cargo xtask fidelity` runs it and writes `docs/qa/fidelity.md` (WP-60).
Untouched parts must be byte-identical after save, and LibreOffice must open
every output.

| Format | Text | Structure | Visual | Other |
|---|---|---|---|---|
| `.docx` | 1.00 | ≥ 0.95 | ≥ 0.90 | |
| `.xlsx` | | | | values 1.00, formulas 1.00, styles ≥ 0.97 |
| `.pptx` | | ≥ 0.95 | ≥ 0.85 | |
