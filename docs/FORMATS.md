# File formats and fidelity

Officina reads and writes Office Open XML (`.docx`, `.xlsx`, `.pptx`) natively.
This page says what each reader understands, what is kept without being
understood, and what is lost. The numbers in the last section are the targets
the test suite holds every release to.

## The rule: keep what you don't understand

An Office file is a zip package of XML parts linked by relationships. Each
reader builds Officina's model from the parts it knows and keeps every other
part, and every part it only partly read, as the original bytes. On save the
writer regenerates the parts it modelled and copies the kept parts back, with
their relationships and content types fixed up.

So a workbook with macros, an embedded spreadsheet in a document or a custom
XML part comes back out of Officina with those parts intact, even though
Officina cannot run or edit them.

Before saving to an Office format, the **Compatibility Checker**
(File ▸ Info ▸ Check for Issues) lists anything in the document that the target
format cannot hold or that Officina will not carry through.

## Word documents (`.docx`, `.dotx`, `.docm`)

**Read and written:** paragraphs and runs with style inheritance (document
defaults, styles, theme fonts and colours); tables with merged cells, borders,
shading and table styles; sections with page size, margins, columns, different
first page and odd/even headers and footers; numbering and multi-level lists;
footnotes and endnotes; comments and threaded replies; tracked insertions,
deletions and formatting changes; fields (TOC, PAGE, NUMPAGES, DATE, REF and
HYPERLINK are live, other fields keep their last result); inline and floating
pictures with wrapping; text boxes; hyperlinks and bookmarks; symbols;
document properties.

**Kept, not edited:** equations (`m:oMath`, shown and preserved), VML drawings
from older files (read-only), embedded OLE objects, the VBA project of a `.docm`,
custom XML.

**Approximate:** text wrap around floating objects follows Word's rules with
square and tight wrapping, so a line can break one word differently from Word.
The target is within one line per page on the fixture corpus.

## Excel workbooks (`.xlsx`, `.xltx`, `.xlsm`)

**Read and written:** sheets, cell values of every type, shared and rich-text
strings, formulas (including shared formulas, legacy array formulas and dynamic
arrays), defined names, number formats, fonts, fills, borders, cell styles,
merged cells, frozen panes and selection, column widths and row heights,
conditional formatting (including the Excel 2010 data bars and icon sets),
data validation, hyperlinks, page setup, headers and footers, print areas and
breaks, AutoFilter, tables, pictures and shapes in drawings, charts, notes and
threaded comments, the 1900 and 1904 date systems.

**Kept, not edited:** pivot tables and pivot caches are preserved and shown as
their last values (Officina's own pivot tables are written in Excel's format and
refresh in Excel), the VBA project of an `.xlsm`, external links, custom XML.

**Charts:** column, bar, line, area, pie, doughnut, scatter, bubble, radar and
combinations, with axes, legends, data labels and series formatting. 3-D bar
charts are drawn in 2-D. Treemap and sunburst charts are read where possible.

## PowerPoint presentations (`.pptx`, `.potx`, `.pptm`)

**Read and written:** slide size, masters, layouts and slides with placeholder
inheritance; shapes with all 187 preset geometries, fills, lines and effects;
text with paragraph and character formatting and fields; pictures with
cropping; tables; charts; groups and connectors; backgrounds; transitions;
speaker notes; comments; sections.

**Mapped:** SmartArt is imported as a group of ordinary shapes. Transitions map
to Officina's set, and an unknown transition becomes Fade. Animations Officina
supports are editable; the others are kept in the file and dropped only if you
change the shapes on that slide.

**Kept, not edited:** embedded OLE objects, the VBA project of a `.pptm`,
custom XML.

## OpenDocument (`.odt`, `.ods`, `.odp`)

Imported into and exported from the same models as the Office formats.
Officina's models are shaped like OOXML, so an ODF file goes through a mapping;
features with no OOXML counterpart are reported by the Compatibility Checker.

## Other formats

- **CSV and TSV:** RFC 4180, with the delimiter, quoting and encoding detected
  (UTF-8 with or without BOM, UTF-16, Windows-1252).
- **JSON:** an array of objects becomes a table.
- **Markdown, plain text and HTML:** open in Scriba and export from it.
- **PDF:** export only, from the same page layout you see on screen, with the
  document outline as bookmarks.

## Fonts

Office's default fonts are proprietary, so Officina bundles open fonts with the
same metrics, which keeps line breaks and page breaks where Word put them:

| In the file | Rendered with |
|---|---|
| Calibri | Carlito |
| Cambria | Caladea |
| Arial | Liberation Sans |
| Times New Roman | Liberation Serif |
| Courier New | Liberation Mono |
| Aptos | Inter (no metric-compatible open font exists; the Compatibility Checker says so) |

The file keeps the original font names, so the document looks as intended
again when opened in Office.

## Fidelity targets

The fixture corpus is round-tripped (opened and saved) and compared with the
original and with LibreOffice's rendering. `pnpm fidelity` runs it.

| Format | Text | Structure | Visual | Other |
|---|---|---|---|---|
| `.docx` | 1.00 | ≥ 0.95 | ≥ 0.90 | |
| `.xlsx` | | | | values 1.00, formulas 1.00, styles ≥ 0.97 |
| `.pptx` | | ≥ 0.95 | ≥ 0.85 | |
