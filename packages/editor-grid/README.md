# @kiln/editor-grid

Kiln Grid's editor views (WP-71, PLAN.md §1.3–§1.4, §3.6): the sheet surface, the formula bar with the name
box, fx and expand, the in-cell editor, formula autocomplete with inline docs and the argument hint, the sheet
tab strip with its menu, the find bar, the context menus, the status bar and the dialogs (Format Cells with
its six tabs, Go To, Paste Special, Insert and Delete Cells, Row Height, Column Width, Tab Color, Insert
Function). The web app mounts it in the app frame (`apps/web/src/routes/grid`).

Presentation only (PLAN.md §3.7). The cells are Rust tiles from `kiln-sheet-layout`, painted on a canvas by
`@kiln/surface`'s painter: four panes (frozen rows and columns, headers) share one tile cache, prefetched
tiles are rendered in the spare time of animation frames, and a native scroller gives the platform's
scrolling. A hidden textarea over the active cell is the input proxy (typing, IME, the clipboard); the text
shown in the cell is the core's, with the references of a formula in their range colours. Keys that the
sheet handles itself (arrows, Enter, Tab, Esc, F2, F4, Alt+Enter, Ctrl+Enter, Delete) go straight to the
core in order with the typing; every other shortcut is the window's keymap. Everything the views show —
the name box, the formula, the mode, suggestions and hints, the tabs, the statistics, the active cell's
format — is the core's answer to `Query::Grid(State)`, asked once per animation frame after a change.

Public API:

- `useGridEditorModel({ client, doc, registry })` — the editor model of a workbook, with the editor's chrome
  commands (`registerGridUi`: the dialogs, Find, the formula bar) registered while mounted.
- `GridEditor` — formula bar, sheet, tabs, menus and dialogs: `<GridEditor model title theme />`.
- `GridStatusBar` — mode, Average/Count/Sum (Numerical Count, Minimum and Maximum in the customise menu),
  views and zoom: `<GridStatusBar model readOnly />`.
- `GridSurface`, `GridSurfaceController` and the pure helpers (`gridKeyAction`, `panes`, `hintParts`,
  `colourRuns`, `statItems`) for tests and reuse.

The surface element mirrors its frame count and last frame time as `data-kiln-frames` and
`data-kiln-frame-ms`, and records `kiln:grid-frame` and `kiln:grid-apply` performance measures (B6 is
measured on them, in production builds too). In development builds the web app also exposes the model as
`window.__kilnGrid` and the controller as `window.__kilnGridSurface` for the end-to-end tests
(`e2e/specs/grid.spec.ts`, `e2e/specs/grid-shortcuts.spec.ts`).
