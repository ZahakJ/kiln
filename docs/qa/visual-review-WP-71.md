# Visual review: WP-71 (Kiln Grid surface and editor)

Reviewed on 2026-10-17 against the brand kit's mockups `docs/design/mockups/grid-light.png` and `grid-dark.png`
(`/home/muth/kiln-brand/final/mockups/`). Screenshots come from `e2e/specs/grid.spec.ts`:

- "opens a workbook; screenshots light and dark, desktop and phone; axe" opens
  `fixtures/xlsx/features/styles.xlsx` through the web file picker, selects B2:C5 and writes
  `e2e/screenshots/grid/editor-{light,dark}-{desktop,phone}.png` (1440 × 900 and 390 × 844), then types
  `=VLO` in B13 for `autocomplete-dark-desktop.png` and opens Format Cells for `format-cells-light.png`;
- "frozen panes stay while the sheet scrolls" writes `frozen-scrolled.png`.

The cells themselves were also reviewed in the golden renders of every `fixtures/xlsx/features` workbook
(`fixtures/golden/grid/*-{light,dark}.png`, written by `kiln-sheet-layout`'s golden test and identical to what the
web surface paints): styles (fonts, the 17 fill patterns, gradients, every border style), number formats,
merges, rich text, hyperlinks, comments, hidden rows and columns, freeze panes, tables and conditional
formatting (fills, data bars, colour scales, icon sets).

axe-core (WCAG 2.2 A/AA and best practices) reports **0 serious or critical violations** on the Grid view with a
workbook open, light and dark (`grid.spec.ts`, the same test), and on the Grid app frame in both themes
(`chrome.spec.ts` "grid frame … passes axe").

| Mockup element | Verdict | Kiln Grid | Deviations | Fixed during the review |
|---|---|---|---|---|
| Title bar (Grid wordmark in jade, save/undo/redo, title + extension, save-state chip, search with Alt+Q) | Matches | The app frame of WP-58 with Grid's jade | Avatar and window buttons are the desktop shell's | — |
| Ribbon Home tab: Clipboard, Font, Alignment, Number, Styles, Cells, Editing, Kiln (Explain) | Matches | Every control wired to a core command (Font, size, grow/shrink, B/I/U, borders gallery, fill and font colour, the six alignments, orientation, wrap, merge, indent, number format combo and buttons, Insert/Delete/Format, AutoSum, Fill, Clear, Sort & Filter, Find); launchers open Format Cells on their tab | Mockup's label "Layout" is "Page Layout" (Excel's name, §1.4); Conditional Formatting, Format as Table and Cell Styles open WP-72's features (hidden in release builds until then) | — |
| Formula bar: name box with dropdown, ✕ / ✓, fx, the formula with references in range colours, expand chevron | Matches | Same, with range colours from the core; the name box goes to references and names and defines a new name for the selection | The mockup's right-aligned cell comment ("Total spend for 2026") is a note preview, not in WP-71 | Name box defines names (Excel's habit) |
| Column and row headers: small caps letters, selected header tinted jade with an underline | Matches | Headers are painted by `kiln-sheet-layout`; selected rows and columns take the jade wash, whole-row/column selections the stronger tint | — | — |
| Cells: warm paper (light) and kiln-black (dark) sheet, hairline gridlines, Figtree default text | Matches | Paper/charcoal sheet, hairline gridlines in both themes; the workbook's own fonts (Calibri → Carlito metrics) | Default rows are Excel's 20 px (15 pt), slightly tighter than the mockup's 28 px: Excel's geometry wins so files look as in Excel | — |
| Active cell: 2.5 px jade frame with the square fill handle; range wash | Matches | Same frame and handle; the range wash under the text; the copy marquee and fill preview dashed | — | — |
| Header band (dark jade fill, white caps) and status pills of the mockup's sample | Matches when the file has them | Cell fills and fonts are the workbook's; dark mode keeps explicit fills and picks text colour for its fill | — | Text on light fills in dark mode is no longer adapted to the page (`painter.ts`, alpha 0xFE marks text chosen for its own fill) |
| Sheet tabs: + button, active tab raised with jade label and colour dot | Matches | Tabs with tab colours as dots, rename by double-click, drag to reorder, context menu | — | — |
| Status bar: mode, save state, Average/Count/Sum, view buttons, zoom slider and percentage | Matches | Mode (Ready/Enter/Edit/Point), statistics formatted like the active cell, Normal/Page Layout/Page Break views, zoom with presets and Zoom to Selection | Save state lives in the title chip | — |
| Chart card floating over the cells ("Spend by quarter") | WP-73 carry-over | Charts in workbooks are read and kept; drawing them on the sheet and the chart ribbon are the carry-over of this package | — | — |
| Formula autocomplete (not in the mockup) | Brand-consistent | The kit's popover: suggestions with an fx/N badge, the highlighted function's syntax, summary and "Tab to insert"; the argument hint with the current argument in bold | — | It now opens above the cell when there is no room below (it was clipped by the sheet at the bottom of the window) |
| Format Cells (not in the mockup) | Brand-consistent | The kit's dialog: Fraunces title, the six tabs, category list, sample, type list, live preview | — | — |
| Phone (390 px) | Brand-consistent | The phone ribbon of WP-58 (Paste, Cut, Copy, Format Painter, more), name box and formula bar on one row, the sheet scrolls under fixed headers, tabs and zoom | — | — |

**Frozen panes.** `frozen-scrolled.png` shows row 1 and column A frozen at B2 after scrolling to row 202: the
frozen row, the frozen column and the headers stay, the hairline guides mark the panes, and the four panes share
one tile cache (the frozen tiles are kept while the cells scroll; `delta_regions` in `kiln-scene`).

**Remaining differences, not regressions.** Default row height follows Excel (above). The mockup's avatar,
"Editing" mode chip and Help tab belong to collaboration and later packages, as for Write.
