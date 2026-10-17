# kiln-sheet-layout

The surface of Kiln Grid (PLAN.md §3.6 "Grid specifics"): sheet geometry and cell painting into the Scene, so
the on-screen grid, `kiln render` PNGs, golden tests and print previews draw the same pixels. `kiln-session`'s
Grid app drives it; nothing here knows about the UI.

```rust
use kiln_sheet_layout::{geometry_of, render_sheet, GridScene, RenderOptions};

let geo = geometry_of(sheet, true);                  // Fenwick-indexed rows and columns, merges, panes
let rect = geo.cell_rect(row, col);                  // sheet pixels at 100 %
let hit = geo.hit_test(&view, x, y, header_w, Some(&selection)); // cell, header, resize border, fill handle
let mut scene = GridScene::new(fonts);               // one tile cache for every pane of the view
scene.prepare(&mut workbook, sheet_index, &geo, request); // tiles of the regions a frame shows
let png = render_sheet(&mut workbook, sheet_index, fonts, &RenderOptions::default()); // PNGs, goldens
```

## What it does

| Area | Supported |
|---|---|
| Geometry | 1,048,576 × 16,384 cells; row and column offsets in O(log n) (Fenwick trees over blocks of sizes); hidden rows and columns; custom and default sizes in Excel's units (max digit width, points); zoom 10 %–400 %; device-pixel snapping |
| Panes | frozen rows and columns as four viewports onto one sheet space sharing one tile cache; split views; row and column headers with the selection highlight; the corner |
| Cells | fills (solid, the 17 patterns, linear and path gradients); gridlines; borders with Excel's precedence and all 13 line styles, diagonal borders; text from `kiln-formula::numfmt` (General sized to the column, `#####`, Accounting's repeat fill, colours of the format) shaped by `kiln-text`; horizontal (General, left, centre, right, fill, justify, centre across selection, distributed) and vertical alignment, indent, wrap, shrink to fit, rotation and vertical text; overflow into empty neighbours; merged cells; rich-text runs |
| Visuals | conditional formats (fills, fonts, data bars, colour scales, icon sets), table styles under the cell format, themed automatic colours for dark mode; hyperlink styling; note and comment markers; the green error-check triangle |
| Overlay | the selection (jade frame and wash), the fill handle, the copy marquee, the fill preview, spill ranges, the colours of a formula's references and the in-cell editor's text |
| Accessibility | an A11yTree of the visible grid: `aria-rowcount`/`aria-colcount`, the visible rows and cells with their values, the active cell with its value and formula, the headers |

Golden PNGs of `fixtures/xlsx/features` live in `fixtures/golden/grid/` (light and dark); `tests/golden.rs`
compares them (`KILN_UPDATE_GOLDEN=1 cargo nextest run -p kiln-sheet-layout --test golden` rewrites them after a reviewed change).
