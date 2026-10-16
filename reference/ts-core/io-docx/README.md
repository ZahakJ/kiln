# @kiln-ref/io-docx

Reads (and, from WP-13, writes) `.docx`, `.docm`, `.dotx` and `.dotm` files to and from the Kiln Write
document model (`@kiln-ref/doc-model`).

## Reading

```ts
import { readDocx } from '@kiln-ref/io-docx';

const result = readDocx(bytes); // Uint8Array or ArrayBuffer
result.doc;        // ProseMirror document: sections > (header|footer)* blocks, then footnotes/endnotes
result.styles;     // StyleSheet (styles.xml)
result.numbering;  // Numbering (numbering.xml)
result.sections;   // section properties in order
result.media;      // Map<partName, { contentType, data }> — images referenced by image nodes
result.comments;   // DocComment[] with threads (commentsExtended) and resolved state
result.warnings;   // DocxWarning[] for the Compatibility Checker
```

The reader is pure TypeScript with no DOM or Node APIs, so it runs unchanged in a Web Worker;
`readDocxStream(stream)` accepts a `ReadableStream` (for example `fetch(url).body`). Files that are
not Word documents, or whose main part is unreadable, throw `DocxFormatError`.

### What is read

| WordprocessingML | Model |
|---|---|
| paragraphs, runs, `w:t`, `w:tab`, `w:br`, `w:cr`, hyphens | `paragraph`, text, `tab`, `softBreak`, `pageBreak`, `columnBreak` |
| `w:rPr` | marks via `marksFromRunProps`; unmodelled children kept as raw XML in `runProps.props.xml` |
| `w:pPr` | paragraph attributes; unmodelled children kept in the `xml` attribute |
| `w:sym` | the Unicode equivalent (Symbol, Wingdings, Webdings, …) with the original font/code in `runProps.props.sym` |
| tables (`gridSpan`, `vMerge`, `hMerge`, `gridBefore/After`, `tblLook`, widths in every unit) | `table`/`tableRow`/`tableCell` with `colspan`/`rowspan`; placeholder cells are marked with `GRID_PLACEHOLDER` |
| `w:sectPr`, header/footer references, `titlePg`, `evenAndOddHeaders` | `section` nodes with `header`/`footer` children by type; page borders, note properties and other extras in the section props |
| complex and simple fields (nested) | `field` atoms with the cached result; TOC/INDEX/BIBLIOGRAPHY/TOA → `toc` blocks; HYPERLINK → `link` marks |
| hyperlinks, bookmarks | `link` marks, `bookmarkStart`/`bookmarkEnd` |
| footnotes/endnotes | `footnoteRef` + `footnote` nodes |
| comments, `commentsExtended`, `people` | `comment` marks, `comments`, `people` |
| `w:ins`, `w:del`, `w:moveTo`, `w:moveFrom`, `w:rPrChange`, `w:pPrChange` | `insertion`/`deletion`/`formatChange` marks, paragraph `revision` attribute |
| `wp:inline`/`wp:anchor` pictures, `wps:wsp` shapes and text boxes | `image` (media extracted) and `shape` (DrawingML fill/line, original XML kept) |
| charts | an `opaqueInline` placeholder plus a `DocxChartRef` (`result.charts`) for the charts package |
| VML pictures | `image` (read-only fallback); other VML, SmartArt, groups, canvases, OLE objects → `opaqueInline` |
| `m:oMath`, `m:oMathPara` | `equation` holding the OMML |
| content controls (`w:sdt`) | content unwrapped; properties in the `sdt` attribute (blocks) or `sdt` mark (inline) |
| styles, numbering, settings, theme, document properties | `StyleSheet`, `Numbering`, `DocxSettings`, `Theme`, `DocumentProperties` |

Everything the reader does not model stays in `result.passthrough` (parts and relationships) or in the
raw-XML attributes, so the writer can put it back. Raw XML fragments use the stable Office prefixes
without declaring them (`parseFragment` re-parses them).

### Warnings

Each warning has a `code`, a `severity` (`warning`: dropped or approximated; `info`: preserved but not
editable), a message, the `part` and an XPath-like `path`. The synthetic fixture corpus reads without
warnings.

## Tests

`pnpm --filter @kiln-ref/io-docx test`: fixture structure tests, a text comparison against mammoth for
every generated feature fixture, element-level tests on hand-written packages, the external corpus
(no unhandled exceptions) and a 300-page throughput check.

Licence: AGPL-3.0-or-later.
