# How Officina is built

This is the map of the code: what each package does, how a document moves from
a file on disk to the screen and back, and the decisions behind that. The full
plan, with the reasoning for every choice and the work still ahead, is
[PLAN.md](../PLAN.md); this document is its condensed, current form.

## One web core, three shells

Officina is one TypeScript and React application. The browser, the desktop and
Android all run the same build:

| Shell | Package | What it adds |
|---|---|---|
| Web / PWA | `apps/web` | Vite build, service worker, File System Access API, IndexedDB |
| Desktop | `apps/desktop` | Electron: native file dialogs and menus, printing, file associations, AppImage/deb/NSIS/dmg packaging |
| Android | `apps/android` | Capacitor: file picker with content URIs, share sheet, intents, Android printing |
| Collaboration | `apps/collab-server` | Hocuspocus (Yjs) server with SQLite persistence, optional |

Electron was chosen over a system WebView (Tauri) because a word processor
lays out pages: shipping one Chromium everywhere makes pagination and PDF output
identical on every desktop OS. The cost is a larger download and more memory.

The shells talk to the core through one interface, `PlatformHost` in
`packages/storage`: open, save, save as, recent files, print, share, app data
directory. Everything above that line is shared.

## Packages

```
apps/
  web/             Vite + React SPA/PWA: routes / (Atrium), /write, /grid, /deck, /notes, /present
  desktop/         Electron main and preload, builder config
  android/         Capacitor project (the generated Gradle project is committed)
  collab-server/   Hocuspocus + SQLite
packages/
  tokens/          design tokens ("Horizon"), themes, fonts
  ui-kit/          Ribbon, Backstage, status bar, command palette, panes, dialogs
  commands/        command registry, keymaps, Key Tips
  doc-model/       Scriba's ProseMirror schema, styles, numbering, sections, font substitution
  sheet-model/     Tabula's workbook model, sparse cell store, styles, undo log
  deck-model/      Scena's presentation model: masters, layouts, slides, shapes, themes
  formula/         formula engine: tokenizer, parser, dependency graph, evaluator, functions
  ooxml/           OPC container, XML, relationships, DrawingML, themes, passthrough
  io-docx/ io-xlsx/ io-pptx/ io-odf/   file readers and writers
  charts/          chart model, ECharts renderer, chart XML
  editor-write/ editor-grid/ editor-deck/   the three editors
  collab/          Yjs providers, presence, version history
  storage/         PlatformHost, recent files, autosave journal, recovery
  print-pdf/       print layouts and PDF export
  i18n/            English, German, Arabic
  templates/       built-in templates
  notes/           Notula
  testing/         fixture corpus, oracles, fidelity scorer
```

Internal names are plain (`editor-write`, `sheet-model`); the product names
(Scriba, Tabula, Scena, Notula, Atrium) appear only in what users see.

## State and commands

- **Document state** lives in the document models, never copied into React
  state. Scriba, Scena and Notula documents are Yjs documents; Tabula's workbook
  is a `SheetModel` with its own command log. React reads them through small
  selector hooks built on `useSyncExternalStore`.
- **UI state** (selection, panes, zoom, ribbon tab) lives in one zustand store
  per application.
- **Every change goes through a command**: `execute(id, args)` in
  `@officina/commands`. The ribbon, keyboard shortcuts, the command palette,
  native menus and the tests all call the same commands, so a feature is wired
  once and reachable everywhere.
- **Workers** run formula recalculation and file parsing and serialisation off
  the main thread, through `comlink`.

## Document models

- **Scriba** uses a ProseMirror schema that mirrors WordprocessingML:
  sections, paragraphs with Word's paragraph properties, runs with Word's
  character properties, tables with merged cells, fields, footnotes, comments
  and tracked changes as marks. Styles resolve through `basedOn` chains and the
  document theme. New documents use Word 2016-2021 defaults (Calibri-metric
  Carlito 11 pt), because that is what most `.docx` files in the wild expect.
- **Tabula** stores cells in a sparse store of row chunks with parallel
  arrays: compact to keep, fast to iterate, constant-time to look up. Styles are
  deduplicated into a table like Excel's `cellXfs`. Edits are commands with
  inverses, which gives undo and redo.
- **Scena** keeps masters, layouts and slides with shapes positioned in EMU
  (the unit PowerPoint stores, so nothing drifts through rounding). Shape text
  uses a subset of the Scriba schema, so the same editor edits it.

## The formula engine

Tabula's engine (`packages/formula`) is written for this project and MIT
licensed. HyperFormula was rejected because it is GPL-3.0-only and does not
follow Excel's semantics closely enough.

- Tokenizer and Pratt parser for Excel's grammar: A1 and R1C1, sheet-qualified
  and 3-D references, whole rows and columns, structured table references,
  array constants, implicit intersection.
- A dependency graph with interval trees for ranges, so `SUM(A:A)` is one edge,
  not a million.
- Incremental recalculation in topological order, cycle detection, optional
  iterative calculation.
- Dynamic arrays with spill and `#SPILL!`, lazy arguments for `IF`, `LET` and
  `LAMBDA`, volatile functions.
- Excel's number behaviour: 15 significant digits, its "almost equal"
  comparison, the 1900 date system including the 1900-02-29 bug.
- More than 350 functions, each with documentation that feeds autocomplete and
  the Formula Lab, and a conformance suite of more than 1,500 cases checked
  against Excel's documented results and LibreOffice.

## File formats

The rule is *never destroy what you don't understand*. Each reader builds the
model and keeps every part of the file it did not fully consume (macros,
embedded objects, custom XML, pivot caches, unknown extensions) as bytes; the
writer re-emits what it modelled and copies the rest back with relationships
fixed up. Details per format, and what is known to be lost, are in
[FORMATS.md](FORMATS.md).

## Rendering

- **Scriba** is TipTap (ProseMirror) with a pagination layer that measures
  each block and lays out pages, keeping Word's rules (keep with next, widow and
  orphan control, rows that cannot split). Headers, footers and footnote areas
  are drawn per page; floating pictures wrap with CSS shapes. PDF export uses the
  same page model.
- **Tabula** draws the visible cells on a `<canvas>` with row and column
  virtualisation; only the cell editor, popups and charts are DOM.
- **Scena** renders each slide as a 960×540 stage scaled to fit: shapes are
  DOM and SVG generated from DrawingML's 187 preset geometries; thumbnails are
  the same component at a smaller scale. Slide show and presenter view are their
  own window and talk over `BroadcastChannel`.

## Collaboration, history and recovery

- Yjs documents sync through `@hocuspocus/provider` to the optional
  collaboration server and persist locally in IndexedDB, so edits made offline
  merge when the connection returns. Undo only reverts your own changes.
- Version history keeps a snapshot on every save and every ten minutes of
  editing (up to 200 per document), with naming, preview, restore and a word
  diff for Scriba.
- AutoSave journals every change to IndexedDB; documents that were not closed
  cleanly appear in the Document Recovery pane at the next start. "AutoSave to
  file" writes the real file two seconds after you stop typing.

## Performance targets

| Scenario | Target |
|---|---|
| Open a 10 MB `.xlsx` with 500,000 cells | first paint < 4 s, full calculation < 8 s |
| Scroll 1,000,000 populated cells | ≥ 55 fps |
| Recalculate after one edit in a 100,000-formula workbook | < 50 ms (p95) |
| Open a 300-page `.docx` | first page < 3 s, pagination done < 6 s |
| Typing latency in that document | < 50 ms (p95) |
| Open a 200-slide `.pptx` | < 3 s |
| Desktop cold start to Atrium | < 2.5 s |

`pnpm bench` measures them.

## Accessibility and languages

The ribbon follows the WAI-ARIA tabs and toolbar patterns with roving focus;
dialogs trap focus; the canvas grid exposes the active cell and headers to
screen readers; text contrast is at least 4.5:1 in every theme; motion respects
`prefers-reduced-motion`; axe-core runs on every main screen in the end-to-end
tests. The interface is translated with i18next (English, German, Arabic), and
Arabic flips the whole layout right to left.

## Security

Documents are untrusted input. XML entity expansion is off; archives are
limited to 2 GB uncompressed and 64,000 parts; formulas run in Officina's own
interpreter with no `eval`; links open through the platform with a scheme
allow-list. Electron runs with context isolation, the sandbox and a content
security policy, and its IPC messages are validated with `zod`. Known gaps are
listed in [SECURITY.md](../SECURITY.md).
