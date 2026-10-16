# How Kiln is built

This is the map of the code: what each crate and package does, how a document
moves from a file to the screen and back, and the decisions behind that. The
full plan, with the reasoning for every choice and the work still ahead, is
[PLAN.md](../PLAN.md) (§3); this is its condensed, current form. The decision
records are in [adr/](adr/).

> **Status (WP-50).** The structure below exists: every crate builds, is linted
> and tested in CI, and the shells start. The engines inside the crates are
> built by the work packages named in each crate's README.

## Principles

1. **Everything that computes lives in Rust**: models, layout, formulas, file
   formats, collaboration, storage, search, spell check, PDF, charts. The
   TypeScript layer renders what Rust tells it to and forwards input.
2. **One core, three hosts.** `kiln-session` is the only API; the Tauri shell,
   the WebAssembly bridge and the command line are thin adapters over it.
3. **One typed protocol.** Every message between Rust and TypeScript is a type
   in `kiln-protocol`, and the TypeScript side is generated from it.
4. **Never destroy what you don't understand.** Readers keep every part they
   did not consume; untouched parts are byte-identical after save.
5. **Measured, not claimed.** Fidelity, performance budgets and parity are
   produced by commands and enforced in CI.

## Runtime

```
  TypeScript presentation (React 19): Home · Write · Grid · Deck · Notes · Present
  ui-kit · commands · surface (<canvas> painter, input proxy, a11y shadow tree)
          │  KilnClient: execute(Command) · events · frames
   ┌──────┴────────────────┐            ┌─────────────────────────┐
   │ TauriClient            │            │ WasmWorkerClient        │
   │ kiln-desktop (Tauri 2) │            │ kiln-bridge-wasm        │
   │ Linux/Win/mac/Android  │            │ in a Web Worker         │
   └──────┬────────────────┘            └────────────┬────────────┘
          └──────────────── kiln-session ─────────────┘
     documents · command dispatch · selection · viewports · frames · events
     kiln-doc · kiln-sheet · kiln-deck · kiln-text · kiln-scene · kiln-io-* · kiln-collab · kiln-storage
```

A document is opened with `Session::open`, parsed on a worker thread (native)
or in the Worker (web), and kept with its passthrough parts. The UI sets a
viewport and asks for frames. Edits are `Command`s: the dispatcher checks
availability, applies an undoable transaction, re-lays out what changed and
emits `Event::Changed`; the surface then fetches a `SceneDelta` containing only
the tiles that changed.

| State | Lives in | Never in |
|---|---|---|
| Document model, layout, calculation, undo, passthrough, versions | Rust (`kiln-session`) | React |
| Selection, caret, viewport, composition text | Rust (authoritative), mirrored to TypeScript for overlays | — |
| UI state: ribbon tab, open panes, dialog fields, theme, density | TypeScript (`zustand`) | Rust |

Rust reaches files, dialogs, the clipboard, printing and preferences through a
`Host` trait: the Tauri shell implements it natively, the web bridge by asking
the main thread (File System Access API, OPFS, IndexedDB).

## Repository layout

```
crates/                Rust core (no UI code)
  kiln-common          ids, units, colours, geometry, errors, feature tiers
  kiln-protocol        every Rust↔TypeScript type; ts-rs export
  kiln-text            fonts, shaping, bidi, line breaking, hyphenation, glyph atlas
  kiln-scene           retained display list, SceneDelta, hit-test helpers
  kiln-raster          tiny-skia painter: Scene → PNG (CLI, tests, thumbnails)
  kiln-ooxml           OPC, XML, DrawingML, 187 preset shapes, passthrough, deterministic writer
  kiln-doc             Write model (WordprocessingML-shaped), transactions, undo
  kiln-doc-layout      paragraph → line → page layout, tables, floats, footnotes → Scene
  kiln-formula         parser, dependency graph, multi-threaded recalculation, number formats, functions
  kiln-sheet           workbook model, sparse columnar store, styles, command log
  kiln-sheet-layout    grid viewport layout and cell painting → Scene tiles
  kiln-deck            presentation model: masters, layouts, slides, shapes, themes
  kiln-deck-layout     slide rendering, thumbnails, guides, animation evaluation → Scene
  kiln-charts          chart model, layout and rendering, chartSpace I/O
  kiln-io-docx/-xlsx/-pptx/-odf/-csv   readers and writers with passthrough
  kiln-io-pdf          Scene → PDF (krilla)
  kiln-spell           Hunspell-compatible spell check (spellbook)
  kiln-search          inverted index and fuzzy matcher
  kiln-collab          Loro bindings, presence, version history, sync protocol
  kiln-collab-server   WebSocket relay (axum, tokio, redb)
  kiln-storage         .kwrite/.kgrid/.kdeck bundles, autosave journal, recovery, recents
  kiln-session         the facade the shells use
  kiln-bridge-wasm     wasm-bindgen exports of kiln-session
  kiln-cli             `kiln-cli` (installed as `kiln`): convert, render, inspect, control channel
  kiln-bench           benchmarks and budget assertions
  kiln-testing         fixtures, LibreOffice oracle, fidelity scorer, conformance runner
xtask/                 cargo xtask verify | protocol | wasm | fidelity | budgets | parity | notices
apps/
  web/                 @kiln/web: Vite + React; hosts the wasm core in a Worker
  desktop/             @kiln/desktop: Tauri 2 project; src-tauri/ is the kiln-desktop crate
packages/              TypeScript, presentation only
  tokens/              design tokens (regenerated from the brand kit by WP-51)
  ui-kit/              ribbon, Backstage, status bar, palette, panes, dialogs, Key Tips
  commands/            UI command registry, keymaps, Key Tips letters, synonyms
  testing/             Playwright helpers, oracles, PNG codec, image comparison
fixtures/              .docx/.xlsx/.pptx test corpus (generated, hand-made, downloaded)
conformance/           formula conformance CSVs (5,300 rows), the oracle for kiln-formula
reference/ts-core/     the v1.0 TypeScript engines, read-only port oracle (deleted by WP-97)
e2e/                   Playwright against the web build
docs/                  landing page, manual, ADRs, QA records
tools/                 setup scripts, environment check, LibreOffice oracle, fixture generator, lock
```

Internal names stay generic (`kiln-doc`, `editor-write`); the product names
(Kiln Write, Kiln Grid, Kiln Deck, Kiln Notes, Kiln Home) appear in what users
see.

## Thinness contract

`packages/*` and `apps/*` may render, hold UI state, validate form input shape,
map DOM events to commands and decode and paint scenes. They may not parse or
write a document format, compute layout, formulas, number formats, sorting,
filtering, search or spelling, or keep document content beyond what the
current frame needs. `biome.json` bans the document-format and formula
libraries in those directories; reviews check the rest (CONTRIBUTING.md).

## Rendering

Document surfaces are laid out in Rust and painted from the Scene onto
`<canvas>`; the browser never lays out document text, so pages are identical on
every platform and in the PDF ([ADR 0003](adr/0003-rendering.md)). Text input
goes through a hidden input proxy at the caret (IME, Android keyboard);
accessibility through a shadow DOM tree mirrored from Rust's `A11yTree`.

## Threads

Native: one session thread owns all documents, a `rayon` pool does
data-parallel work (recalculation, parsing, line breaking, glyph
rasterisation), a two-worker `tokio` runtime does IO. Web: the core runs in one
Worker and chunks long operations into 8 ms steps
([ADR 0006](adr/0006-shells.md)).

## Collaboration and history

Loro 1.16 is the CRDT for live editing, offline merge, local undo and version
history; `kiln-collab-server` is a self-hosted WebSocket relay
([ADR 0004](adr/0004-crdt.md)).

## Performance budgets

Selected budgets from PLAN.md §3.9, enforced in CI as they become measurable:

| # | Case | Budget |
|---|---|---|
| B1 | Cold start to Kiln Home (Tauri, Linux) | < 700 ms |
| B2 | Idle memory with Home open | < 150 MB |
| B3 | 300-page .docx: first page / fully paginated | < 400 ms / < 1.5 s |
| B5 | Typing latency in that document | < 16 ms |
| B7 | 10 MB .xlsx, 500k cells: first paint / full calculation | < 1.2 s / < 2.5 s |
| B8 | Full recalculation of 1M formula cells | < 2.0 s |
| B12 | Wasm core / TypeScript chrome (gzip) | < 4.5 MB / < 450 KB |
| B13 | Installers: AppImage / deb / NSIS / dmg / APK | < 40 / 30 / 30 / 35 / 35 MB |
| B15 | Clean workspace build (this machine) / CI pipeline | < 6 min / < 25 min |

Measured values are recorded in [qa/perf.md](qa/perf.md).

## Building

```sh
bash tools/setup-rust.sh && bash tools/setup-node.sh && bash tools/verify-env.sh
pnpm install
cargo xtask verify        # fmt, clippy -D warnings, nextest
pnpm verify               # Biome, tsc, Vitest
pnpm e2e                  # Playwright against the web build
cargo run -p kiln-cli -- --version
```

The machine has 4 CPUs and 7 GB of RAM: Cargo uses three jobs, `sccache`
caches compilation, dependencies are optimised once in the dev profile, and the
shared `target/` directory is never cleaned wholesale.
