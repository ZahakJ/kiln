# kiln-doc-layout

The layout engine of Kiln Write (PLAN.md §3.4, §3.6): turns a `kiln-doc` document into pages and paints them as
Scene tiles, identically on every platform, on screen and in print.

```rust
use kiln_doc_layout::{Change, DocLayout, DocScene, View, default_fonts, geometry};

let mut layout = DocLayout::new(default_fonts());
layout.layout_until(&doc, 2);          // the first pages, fast (B3: first page painted)
layout.resume(&doc, None);             // the rest
layout.update(&doc, &Change::Nodes(vec![para_id])); // after an edit: re-flow from the first change
let caret = geometry::caret_in(&layout, pos);       // pages, columns, tables
let hit = geometry::hit_test(&layout, x, y, None);
let mut scene = DocScene::new();       // tiles for the pages near the viewport
scene.prepare(&mut layout, &doc, scale, &send, &keep, overlay);
```

## What it lays out

| Area | Supported |
|---|---|
| Sections | page size, margins (gutter), orientation, columns (equal or custom widths, line between; balancing off), vertical alignment (top, centre, bottom), next page / continuous / next column / even / odd breaks (blank pages for parity), page numbering restarts and formats |
| Paragraphs | resolved styles from `kiln-doc` (style chains, table styles, numbering), fonts per script slot (ASCII, high ANSI, East Asian, complex), caps and small caps, super/subscript, position, character spacing, highlight and run shading, underline/strike kinds, list labels with tab/space suffix, tabs and leaders (stops past the margin align at it), alignment incl. justify and distribute, indents, line spacing (multiple, exactly, at least), paragraph spacing (Word's larger-of rule, contextual spacing, no space at the top of an overflowed page), borders (with their room) and shading, RTL |
| Keep rules | keep with next (whole chains, backtracking to the chain start unless it is already at a column top), keep lines together, widow/orphan control (two lines), page break before, forced page and column breaks inside paragraphs |
| Tables | fixed layout from the grid; autofit from preferred widths, the grid and measured narrowest/widest content, to the table's preferred width or the window; alignment and indent (Word 2013+ edge, outdent before); merged cells (gridSpan, vMerge stretching the last row); nested tables; cell margins, vertical alignment, shading; borders with Word's conflict rule (heavier wins, then darker); rows split across pages, `cantSplit` rows move whole (a row taller than an empty page splits anyway, as in Word; an exact-height row taller than the page is clipped), header rows repeat on every page they fit on |
| Drawings | inline pictures and shapes on the baseline (sized placeholders in the text engine); floating objects anchored to page, margin, column, paragraph, line, character, left/right/top/bottom margins with offsets or alignment, kept on the page; wrapping square, tight and through (wrap polygon, else the picture's alpha or the shape's geometry, 8-px bands), top and bottom, behind, in front, both sides/left/right/largest; text boxes (insets, vertical anchor); DrawingML presets via `kiln-ooxml` with fills and outlines; rotation and flips; PNG/JPEG/GIF/BMP decoded once at ≤ 2048 px; EMF/WMF/charts as quiet placeholders |
| Headers and footers | first / even / default per section with Link to Previous, live PAGE, NUMPAGES, SECTIONPAGES, DATE, TIME, FILENAME, AUTHOR, TITLE; a tall header pushes the body down; floating header pictures (logos, watermarks) |
| Notes | footnotes at the page bottom with a separator, reserved as their references are placed (the page gets shorter); endnotes after the text; numbering from settings (decimal / lower roman) or custom marks |
| Fields | results shown as cached until updated on command; [`fields`] computes updates (PAGE, NUMPAGES, PAGEREF, REF, DATE…) and TOC entries with page numbers |
| Comments | balloon geometry right of the page (painting is the UI's) |
| Views | print layout, web layout (reflow to the window), read mode (screen pages side by side, 1.25× type), draft (one column) |
| Geometry | hit testing per story, carets with Word's second caret at bidi boundaries, selection rectangles with paragraph marks, line ends, line-to-line motion across pages and columns, ruler data, an accessibility tree of the visible pages ± 1 |
| Scene | one tile per page (1024-px bands for tall web/draft pages), the theme's page shadow, glyphs from the atlas, text in the automatic colour painted in the theme's ink (black/white against shading, as Word does), pictures uploaded once |

## Incremental layout

The flow records a snapshot of its state before every top-level block. An edit invalidates the changed paragraphs'
cached layouts and re-flows from the first changed block (or the start of its keep-with-next chain); when the state
before a later block equals the previous layout's, everything after is reused unchanged (`Stats::converged`).
`update_until` stops near the viewport and shows the previous layout's later pages until `resume` catches up, so typing
in a 300-page document re-flows a few pages per keystroke.

## Measured

- `fixtures/large/doc-300p.docx`: 300 pages (LibreOffice: 300); first painted page ≈ 70 ms, read + full pagination
  ≈ 150 ms (release, this machine; budgets B3 < 400 ms / < 1.5 s, `cargo xtask budgets --only B3`).
- Feature fixtures: page counts equal LibreOffice's on 19 of 21, within one page on all. The two differences are
  Word behaviour LibreOffice does not reproduce: Word inserts a blank page before an odd-page section that would start
  on an even page (`sections`), and Word places endnotes directly after the text (`footnotes`).

## Tests

- `tests/flow_rules.rs`: every keep/widow rule, forced breaks, columns, table header repetition and row splitting,
  footnotes shortening the page, every float wrap mode, incremental updates equal to a full layout, carets and hits
  across pages, views.
- `tests/fixtures.rs`: snapshots of every DOCX fixture (`fixtures/golden/write/snapshots`), the LibreOffice page-count
  oracle (`fixtures/golden/write/oracle-pages.json`; `KILN_ORACLE=1 … --run-ignored all oracle` regenerates it), golden
  PNGs of 14 pages at 96 dpi plus a dark-theme page (`KILN_UPDATE_GOLDEN=1` rewrites them — review the images), field
  updates.
- `tests/fixture_probe.rs` (ignored): `PROBE_DIR=… cargo test -p kiln-doc-layout --test fixture_probe -- --ignored`
  renders every feature fixture for a visual check.

## Known limits

- Footnotes are kept whole on the page of their reference (no continuation across pages yet).
- Floating tables (`tblpPr`) are laid out in the flow; column balancing at continuous section breaks is not applied
  (the engine's default is off, as planned).
- Line numbering, page borders and page colour are not drawn yet.
- The bundled fonts have no Arabic or CJK glyphs; such text needs system or added fonts (`kiln-text`).
- Horizontal character scaling (`w:w`) and text effects (emboss, outline, shadow) are ignored.
