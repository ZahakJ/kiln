# File formats and fidelity

Kiln reads and writes Office Open XML (`.docx`, `.xlsx`, `.pptx`) natively, in
the Rust crates `kiln-ooxml` (the shared package, XML and DrawingML layer) and
`kiln-io-docx`, `kiln-io-xlsx`, `kiln-io-pptx`, `kiln-io-odf`, `kiln-io-csv` and
`kiln-io-pdf`. This page says what each reader and writer covers, what is kept
without being understood, and what is lost. The numbers in the last section are
the targets the test suite holds every release to.

> **Status.** The crates exist as scaffolds; the coverage below is the
> specification each one is built and tested against (the v1.0 lists, carried
> over unchanged).

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

Reading and writing are in place: Kiln Write opens and saves
`.docx`, `.dotx`, `.docm` and `.dotm` (File ▸ Save, Save As, `kiln-cli convert`). A
`.docm` or `.dotm` saved as itself keeps its VBA project byte for byte (macros never
run); saved as `.docx` it drops it, and the Compatibility Checker says so. Groups of
shapes (`wpg:wgp`) are read with their shapes, pictures, text and nested groups and
written for groups made in Kiln; `wp:docPr/@hidden` and `w:noProof` are modelled
.

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

**Written back:** everything above is written from the model in the
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

**Approximate in layout:** text wrap around floating objects follows
Word's rules with square and tight wrapping, so a line can break one word
differently from Word. The target is within one line per page on the fixture
corpus.

## Excel workbooks (`.xlsx`, `.xltx`, `.xlsm`) — `kiln-io-xlsx`

**Shipped.** Reading and writing, with the round trip measured on every fixture
(`docs/qa/fidelity.md`: structural, values and formulas 1.000, untouched parts byte-identical,
LibreOffice opens every output), the values and formats checked against exceljs and the
calculated results against LibreOffice. Kiln Grid shows the workbook and
draws its charts.

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
written by `kiln-charts`).

**Kept, not edited:** pivot tables and pivot caches are preserved and shown as
their last values (Kiln's own pivot tables are written in Excel's format and
refresh in Excel), the VBA project of an `.xlsm`, external links, custom XML,
slicers, data tables (What-If Analysis), form controls, printer settings and
any part or extension Kiln does not know.

**Changed on save:** shared formulas are written cell by cell; `calcChain.xml` is left out
(Excel rebuilds it); a Strict (ISO/IEC 29500) file is saved as transitional; phonetic
(furigana) runs of strings are not kept; macros are not kept when an `.xlsm` is saved as
`.xlsx`. Every part no edit touched is copied byte for byte.

**Charts** (`kiln-charts`): every chart part is read into Kiln's chart
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

`.pptx`, `.pptm`, `.potx`, `.potm`, `.ppsx` and `.ppsm` files open into the Deck model (reader;
`kiln-cli inspect --pptx` shows what was read) and are saved back to any of those formats
(writer; `kiln-cli convert in.pptx out.pptx`). Details: `crates/kiln-io-pptx/README.md`.

**Read into the model:** slide and notes size, `firstSlideNum`, the default text style; masters
with their theme, colour map, background, title/body/other text styles and header/footer flags;
every layout (the eleven standard ones and custom ones) with placeholders that inherit position,
shape and text formatting master → layout → slide; slides with their layout, name, background,
colour map override, hidden flag, transition, animations, notes page and comments; sections.
Shapes: preset geometries with adjust values and custom geometry, fills, lines and effects,
Quick Style references, text boxes, placeholders by type and index, alt text and titles, click
hyperlinks; text with paragraph levels, alignment, indents, bullets (character, numbered and
picture), spacing, run formatting, theme colours (with their exact RGB), line breaks, fields,
hyperlinks to web pages, slides and actions, and equations (kept as OMML); pictures with
cropping and opacity; video and audio (embedded or linked) with poster frame and trim; tables
with spans, merges, cell fills, borders, margins, anchoring, text direction, table style and its
options; charts (read with kiln-charts and stored with their embedded workbooks); groups with
their coordinate space; connectors glued to their shapes; notes pages; legacy and threaded
(modern) comments with replies and resolved state; document properties. Images, media and charts
are referred to by part name in the model.

**Mapped:** SmartArt is imported as a group of the ordinary shapes PowerPoint drew for it (an
empty frame when the file has no drawing) and OLE objects as their preview picture; both keep
their original XML and parts for saving. Transitions map to Kiln's set, and a transition without
an exact twin becomes its nearest relative (Fade when there is none) with the original kept. The
animation timeline keeps the main sequence and trigger sequences with start, delay, duration,
repeat and the options of spin, grow/shrink and colour pulse; animations Kiln does not play are
kept in the file and dropped only if you change the shapes on that slide.

**Kept, not edited:** embedded OLE objects, the VBA project of a `.pptm`, custom XML, ink, 3-D
scene and bevel settings, custom shows, the photo album, extension lists, presentation and view
properties, table styles, thumbnails and printer settings. Diagnostics (`PptxWarning`, with
stable codes) say what was approximated, what is kept without being editable and what was
dropped; a malformed slide, layout or theme is reported and the rest of the file opens.

**Saving:** every part Kiln models is written from the model in PowerPoint's element order —
the presentation, masters with their themes, layouts, slides, notes pages and the notes master,
legacy or threaded comments with their authors, document properties — keeping the part names,
relationship ids, slide and layout ids and shape ids the file had, so the XML kept from reading
(SmartArt and OLE frames, content for newer PowerPoint versions in `mc:AlternateContent`, ink,
3-D settings, extension lists) goes back into place and still points at its parts. Everything
else is copied byte for byte with its relationships. A transition is written back exactly as it
was while you leave it alone (sounds and PowerPoint 2010+ options included), and so is a slide's
animation timing while its effects and the shapes they animate are unchanged, so effects Kiln
does not play yet (motion paths, media verbs, …) survive; edited timing is regenerated the way
PowerPoint writes it, with the kept effects merged in. Newer transitions (Reveal, Flash, Morph)
and exact durations are written inside `mc:AlternateContent` with a Fade fallback for older
readers. Theme colours in text come back with Office's exact transform (`tint`, `shade`, …),
not just their slot. Saves from an editing session copy every slide, layout, master and notes
page you did not touch byte for byte. Saving a `.pptm` as `.pptx` removes the macros, and the
Compatibility Checker says so, along with missing clips or pictures, transitions that need a
newer PowerPoint, threaded comments and SmartArt (saved as it was read).

## Kiln Deck themes (`.kdeck-theme`) — `kiln-deck`

The twelve themes of Design ▸ Themes ship as `.kdeck-theme` files (`assets/themes/`), compiled
into every build. A theme file is UTF-8 XML in the `urn:kiln:deck-theme:1` namespace:

- **`a:theme`**: a complete DrawingML theme — colour scheme, font scheme and format scheme (an
  empty `a:fmtScheme` takes Office's) — written unchanged as `ppt/theme/theme1.xml` when a
  presentation is saved as `.pptx`, so the colours, fonts and Quick Styles survive in
  PowerPoint.
- **`kd:master`**: the background (a DrawingML fill or `kd:bgRef` to a background style), the
  layout grid (`kd:grid`: margins, title height, gaps, gutter), the title, body, other and
  footer text styles as DrawingML list styles (`a:lvl1pPr` … with `a:defRPr`) laid over Kiln's
  defaults, placeholder overrides (`kd:ph type="…" n="…"` with position, `a:bodyPr`,
  `a:lstStyle`, `kd:prompt`) and decoration (`kd:shape` with a preset or `a:custGeom`, fill,
  outline and effects).
- **`kd:layout type="…"`**: per standard layout, a background, light-on-dark text (`dark`),
  hiding the master's decoration (`hideMaster`), placeholder overrides and decoration.
- **`kd:variant`**: the Variants gallery — another colour scheme, font scheme, dark mapping or
  background.

Positions are fractions of the slide width and height, so one theme serves 16:9, 4:3 and custom
sizes. Building a theme produces a master with title, body, date, footer and slide-number
placeholders and the eleven standard layouts with PowerPoint's placeholder types and indices,
so slides keep their content when a theme is applied and files match up in PowerPoint. Files
from an unknown future version, unknown placeholder types or presets, and positions outside
−1…2 are refused with a message; nothing in a theme file can crash Kiln (property-tested).

## OpenDocument (`.odt`, `.ods`, `.odp`) — `kiln-io-odf`

ODF 1.3 text documents, spreadsheets and presentations (and the `.ott`, `.ots`, `.otp`
templates) open into the same models as the Office formats and save from them
(`kiln-cli convert` converts in both directions, Kiln Write opens and saves `.odt`, Kiln Grid
`.ods` in the desktop and Android apps; `.odp` in Kiln Deck). For v0.1
ODF is native only: the web core leaves it out (kiln-session's `odf` feature) to stay within
budget B12, and the web chrome offers only the formats the core reports it can save (parity row
`suite.files.odf-web`). Kiln's models are shaped like OOXML, so every ODF construct goes through a mapping
table, documented in `crates/kiln-io-odf/README.md` and on its modules. Packages are opened with
the same zip-bomb limits as Office files, encrypted packages are refused with a message, XML is
parsed without DTDs or external entities, and namespace prefixes may be anything (they are
matched by URI). A package without `styles.xml` or a manifest opens with default styles.

**Text documents:** paragraphs and headings with their styles, direct formatting, outline
levels, tabs, borders and shading; named paragraph and character styles, the default style;
lists (bullets, numbering, nesting, restarts) through list styles; tables with column widths,
spans, row heights, borders, shading, padding and nested tables; sections with columns; page
styles (size, orientation, margins, header and footer distances, columns) with default, first
and even headers and footers; footnotes and endnotes; fields (page number and count, dates and
times with their formats, document properties, cross-references, sequences); tables of contents
and other indexes; bookmarks and hyperlinks; comments with ranges, replies and resolved state;
tracked insertions, deletions and format changes; pictures (inline and anchored, with wrapping
and position) and text boxes; shapes with their text (saved with their fill and outline, read
with Word's default look); charts; line numbering; document properties. Lists restart and
continue as ODF says (`text:continue-list`, `text:continue-numbering`, `text:start-value`).
Tables with relative column widths fill their table (or the text width) and keep their name.
A page border and its padding are folded into the margins (the model has no page border).
Word's table styles are written out as the cell formatting they produce, and saved files state
Word's layout rules for LibreOffice (every compatibility flag LibreOffice sets for a Word file).

**Spreadsheets:** cell values of every type (numbers, percentages, currency, dates and times
in either date system, booleans, text with rich-text runs, errors); formulas translated between
OpenFormula and Excel syntax, function names included (`COM.MICROSOFT.CEILING`, `LEGACY.NORMSDIST`,
…), with their cached results; matrix formulas as array formulas (and dynamic arrays as matrix
formulas); cell styles (font, fill, borders, alignment, wrap, rotation, indent, protection) and
number formats through data styles (sections, conditions, colours, dates, times, elapsed time,
fractions, scientific, currency, text); merged cells; row heights and column widths, hidden
rows and columns, outline groups; several sheets with visibility and tab colours; named ranges
and expressions; print ranges and titles; page setup; conditional formats (cell value, formula,
text, top/bottom, average, duplicates, errors, dates, colour scales, data bars, icon sets);
data validation with its input help and error alert; notes; hyperlinks; the AutoFilter range;
sheet protection; frozen panes, zoom, gridlines and the active cell; headers and footers with
their fields; charts and pictures anchored to cells. Number formats keep padding (`_)`), fills
(`* `), literals between digits (`###-####`), blank digits (`??`), engineering exponents
(`##0.0E+0`) and hidden sections (`;;;`). Pattern fills are written as their blended colour;
Excel tables keep their range as database ranges. Formula cells are never folded into repeats;
repeat counts are expanded within a budget (a hostile file reads with a warning, in bounded time
and memory).

**Presentations:** slide size; each layout becomes a master page (as LibreOffice writes
PowerPoint files) with its placeholders, decoration, background and the theme colours; the
title and body text styles; slides with background, hidden flag, transition (the Transitions
tab's effects, duration, automatic advance) and speaker notes; placeholders matched to their
layout; text boxes and shapes with preset geometry and adjust values, rotation and flips, fills
(solid, gradient, picture, pattern), outlines with dashes and arrowheads, shadows, glow and soft
edges, text with levels, bullets (character, numbered, picture), alignment, spacing and run
formatting; pictures with cropping; tables with spans, fills, borders and padding (PowerPoint's
default table style written out as cell formatting); groups; lines and connectors; charts;
comments; video and audio.

**Charts** (all three kinds): bar (column and bar, clustered, stacked, percent), line, area,
pie, doughnut, scatter, bubble and radar charts, combo charts and secondary axes, with series
names, values and categories, series and point colours, markers, data labels, title, legend,
axis titles, scaling and gridlines. Spreadsheets reference the sheets' cells (the caches are
refreshed from them); text documents and presentations carry the data in the chart's own table.

**Not yet:** stock, surface, of-pie and the Office 2016 chart types; shapes and groups on sheets;
slide animations; equations in text documents (math objects come in as their replacement
picture, as do OLE objects); page borders. Every such loss is listed by the reader or writer
(`ods_issues`, `odt_issues`, `odp_issues`), and the Compatibility Checker shows it before a save
as `.ods` or `.odt`.

## Other formats

- **CSV and TSV** (`kiln-io-csv`): RFC 4180, with the delimiter, quoting
  and encoding detected (UTF-8 with or without BOM, UTF-16, Windows-1252) and every field
  recognised as Excel's cell entry does (numbers, percentages, currency, dates, times, logicals,
  errors). Saving writes the active sheet as Excel does: cells as displayed, Excel's quoting,
  CRLF, UTF-8.
- **JSON** (`kiln-io-csv`): an array of objects becomes a table (keys in the order they
  first appear, typed values); a table saves as an array of objects.
- **Markdown, plain text and HTML:** open in Kiln Write and export from it. Plain text opens
  with its encoding detected (UTF-8 with or without BOM, UTF-16 LE/BE by BOM, else
  Windows-1252) and its line endings, and saves back in both; Markdown and Web Page export
  keep headings, lists, emphasis, links, tables and notes (structure only: the Compatibility
  Checker lists what they cannot hold).
- **PDF**: export only — see the next section.

## PDF (export and print) — `kiln-io-pdf`

Kiln writes PDF 1.7 with `krilla` 0.8.2 from the same layout and Scene ops that
paint the document on screen (`crates/kiln-io-pdf/README.md`):

- **Text is text**: glyphs from the layout with their exact positions, fonts embedded and
  subset (CID, `Identity-H`), Unicode from the layout's clusters, so text selects, searches and
  extracts (99.96 % of the feature corpus's words come back from `pdftotext`).
- **Pictures**: JPEG embedded unchanged, PNG re-compressed losslessly, GIF/BMP from pixels.
- **Tagged PDF**, PDF/UA-1 whenever the content allows it (otherwise plain tagged PDF and the
  reasons — pictures without alt text, glyphs no font has): title, language, `H1`–`H6` from
  outline levels, `P`, `Table`/`TR`/`TH`/`TD`, `Figure` with alt text, `Link` with annotations,
  `TOC`/`TOCI`, `Note`; headers, footers, shading and rules are artifacts.
- **Navigation and metadata**: bookmarks from headings, internal links to bookmarks, page labels
  from the sections' page-number formats, XMP with title, authors, subject, keywords, language,
  creator `Kiln Write` and producer `Kiln`.
- **Options** (File ▸ Print, Export ▸ PDF, `kiln-cli convert in.docx out.pdf --help`): page
  ranges (`1-3, 5, 8-`), odd/even, copies, scale to Letter/Legal/A3/A4/A5, zoom, 1–16 pages per
  sheet, grayscale, markup, hidden text, orientation and margin overrides, tags and bookmarks.
- Print hands the same PDF to the platform: the system PDF viewer on desktop (its print dialog),
  a hidden frame on the web, `PrintManager` on Android.

## Fonts

Office's default fonts are proprietary, so Kiln bundles open fonts with the
same metrics, which keeps line breaks and page breaks where Word put them
(`kiln-text` loads them on every platform; the table is
`crates/kiln-text/src/substitution.rs`):

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
`cargo xtask fidelity` runs it and writes `docs/qa/fidelity.md`.
Untouched parts must be byte-identical after save, and LibreOffice must open
every output.

| Format | Text | Structure | Visual | Other |
|---|---|---|---|---|
| `.docx` | 1.00 | ≥ 0.95 | ≥ 0.90 | |
| `.xlsx` | | | | values 1.00, formulas 1.00, styles ≥ 0.97 |
| `.pptx` | 1.00 | ≥ 0.95 | ≥ 0.85 | animations of unchanged slides byte-identical |
| `.odt`, `.ods`, `.odp` | 1.00 | ≥ 0.90 | ≥ 0.80 | LibreOffice opens 100 % of outputs; values and formulas 1.00 for `.ods`; the import metric (`kiln-io-odf/tests/import_metric.rs`: the ODF reader against the Office reader of each fixture's source) ≥ 0.90 |
