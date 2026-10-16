# ADR 0002: Rename to Kiln and move the core to Rust

- **Status:** accepted
- **Date:** 2026-10-15
- **Work package:** WP-50 (source: PLAN.md v2.0 §0, §1.1, §3.1–3.8, §3.11, §3.13)
- **Supersedes:** the stack decisions of [ADR 0001](0001-stack.md)

## Context

Version 1.0 of the plan built the suite, then called **Officina**, as one TypeScript and React code base
(TipTap/ProseMirror for documents, Yjs for collaboration, Electron on the desktop, Capacitor on
Android). By WP-16 it had about 60,000 lines of engine code (OOXML, formulas with 484 functions,
document and sheet models, a DOCX reader), 5,300 formula conformance rows and 93 test fixtures.

Two problems remained structural. Pages and PDF output depended on the browser's text layout, so a
document could paginate differently in each WebView and differently again in the PDF; and recalculation,
parsing and layout ran in a single JavaScript heap, which put Excel-class workbooks and Word-class
documents out of reach of the performance budgets.

## Decision 1: the name

The suite is **Kiln**: a kiln is where raw material becomes something durable. The apps are **Kiln
Write** (was Scriba), **Kiln Grid** (was Tabula), **Kiln Deck** (was Scena) and **Kiln Notes** (was
Notula); the start screen is **Kiln Home** (was Atrium). Identifiers follow: npm scope `@kiln/*`, Cargo
crates `kiln-*`, application identifier `io.github.zahakj.kiln`, executable `kiln`, CSS custom
properties `--kiln-*`, repository `ZahakJ/kiln`, Pages at `https://zahakj.github.io/kiln/`, signing
keys in `~/.kiln-keys`, the Python oracle environment `~/.venvs/kiln`. The old names (Officina, Scriba,
Tabula, Scena, Notula, Atrium) appear only in `CHANGELOG.md` history and in this record; a grep in
PLAN.md §1.1 enforces that.

## Decision 2: a native Rust core with a thin TypeScript presentation layer

Condensed from PLAN.md §3.1–3.8:

1. **Everything that computes lives in Rust**: models, layout, formulas, file formats, collaboration,
   storage, search, spell check, PDF and charts, in one Cargo workspace of `kiln-*` crates (Rust 1.99.1,
   edition 2024). The TypeScript layer renders what Rust tells it to and forwards input; it may not parse
   a document format or compute layout, formulas, sorting, search or spelling (enforced by an import
   allow-list in `biome.json` and by review).
2. **One core, three hosts.** `kiln-session` is the only API. `kiln-desktop` (Tauri 2, for Linux,
   Windows, macOS and Android) and `kiln-bridge-wasm` (WebAssembly in a Web Worker, for the web and PWA)
   are thin adapters over it; `kiln-cli` drives the same API from the shell.
3. **One typed protocol.** Every message between Rust and TypeScript is a type in `kiln-protocol`; the
   TypeScript definitions are generated with `ts-rs` and CI fails when they are stale (ADR 0005).
4. **Documents are laid out in Rust and painted from a display list** (the Scene) onto `<canvas>`, so
   screen, print and PDF are identical on every platform (ADR 0003).
5. **UI chrome stays TypeScript + React 19** (ribbon, Backstage, dialogs, panes, Kiln Home), with Radix
   primitives for accessibility and the v1.0 `ui-kit` and `commands` packages kept and re-skinned to the
   brand kit. Solid/Svelte (a rewrite for ~40 KB), Rust web UI (every UI change becomes a Rust compile) and
   Rust native UI (immature accessibility, no web target) were rejected.
6. **Threading** (ADR 0006): one session thread owns all documents, a `rayon` pool does data-parallel
   work (recalculation, parsing, line breaking, glyph rasterisation), a small `tokio` runtime does IO; on
   the web the whole core runs in one Worker with chunked long operations.
7. **Collaboration and history** use Loro 1.16 (ADR 0004).
8. **Never destroy what you don't understand**: readers keep unconsumed OPC parts as opaque bytes and
   writers copy them back; untouched parts are byte-identical after save.
9. **Measured, not claimed**: fidelity scores, performance budgets (PLAN.md §3.9) and parity rows are
   produced by commands and enforced in CI.

## What happens to the v1.0 work (PLAN.md §3.13)

- **Kept:** the Node/pnpm/Turborepo toolchain, the Android SDK, repository hygiene, `packages/tokens`,
  `packages/ui-kit` and `packages/commands` (re-skinned and re-wired over the generated command catalog),
  `packages/testing` (oracles, PNG codec, image comparison), `apps/web` and `e2e`.
- **Ported, with the TypeScript kept as the oracle:** `ooxml`, `formula`, `doc-model`, `io-docx`
  (including the archived writer), `sheet-model` and the archived `io-xlsx` reader now live read-only in
  `reference/ts-core/` (package scope `@kiln-ref/*`, outside the workspace and CI) until WP-97 deletes
  them. The 5,300 conformance rows moved to `conformance/` and the fixtures to `fixtures/`; they are the
  oracle for the Rust port and are kept verbatim.
- **Dropped:** Electron (`apps/desktop` is now the Tauri project), Capacitor (`apps/android`), the
  Hocuspocus server, and the placeholder packages for charts, collaboration, storage, printing, the ODF and
  PPTX readers, the deck model and the editors. TipTap, ProseMirror, Yjs, ECharts, fflate,
  fast-xml-parser, papaparse, numfmt, decimal.js, mathlive, nspell, pdf-lib, pdfjs-dist, dexie, comlink,
  immer and the fontsource packages left the catalog; dev-only oracles (exceljs, mammoth, docx,
  pptxgenjs, docx-preview, pixelmatch, axe) stay.

## Consequences

- Pagination is our own deterministic code using bundled font metrics, identical in every WebView and in
  the PDF; the v1.0 objection to Tauri's per-OS WebViews no longer applies because the WebView no longer
  lays out documents.
- The cost is a port of about 60,000 lines; the TypeScript sources, their tests, the conformance CSVs and
  the fixtures make it a translation against known answers rather than a rewrite from scratch.
- Every UI change is still a fast Vite reload; core changes are Rust compiles, so the build is tuned for
  the 4-CPU, 7 GB development machine (`jobs = 3`, `sccache`, dependencies optimised once).
- Building the desktop shell needs the WebKitGTK development libraries on Linux; the core does not, so
  `kiln-desktop` is excluded from the workspace's default members and CI builds the bundles.
