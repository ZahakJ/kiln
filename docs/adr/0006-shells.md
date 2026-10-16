# ADR 0006: Shells and threading

- **Status:** accepted
- **Date:** 2026-10-15
- **Work package:** WP-50 (condensed from PLAN.md §3.8, with the shell choice from §0)

## Decision: Tauri 2 for desktop and Android, WebAssembly for the web

`apps/desktop/src-tauri` (crate `kiln-desktop`, executable `kiln`, identifier `io.github.zahakj.kiln`) is
the shell for Linux, Windows, macOS and, through Tauri 2 mobile, Android. The web/PWA build runs the same
core compiled to WebAssembly. Electron and Capacitor are dropped: with documents laid out in Rust
(ADR 0003) the WebView only runs the chrome and the canvas painter, so per-OS WebView differences no longer
affect documents, and Tauri gives a much smaller download and memory footprint (budgets B1, B2 and B13 in
PLAN.md §3.9).

## Threading, native

- One **session thread** owns `kiln-session` and every document: a single writer, no locks on models,
  commands processed in order from a `flume` channel.
- A **rayon pool** (CPU count − 1, at least 2) does the data-parallel work the session thread hands out:
  formula recalculation by independent strongly connected components in topological levels, line breaking
  for pages not yet visible, part-level OOXML parsing, image decoding, glyph rasterisation batches and
  fidelity scoring.
- A **tokio runtime** (multi-threaded, 2 workers) does IO: autosave journal writes, file watching,
  collaboration sockets, update checks.
- The Tauri main thread runs only the WebView and the IPC handlers, which forward to the session thread
  and await replies. Long operations report progress events.

## Threading, web

The whole core runs in one dedicated Web Worker, so parsing a 20 MB workbook never blocks the chrome.
`rayon` work is sequential unless the `threads` build is active. Long operations are chunked
(`Session::step()` with an 8 ms budget) so frame requests interleave and the surface keeps painting.

## Painter and memory

The painter runs on the WebView's main thread (decode `SceneDelta`, update tile canvases, composite in
`requestAnimationFrame`; target 4 ms per frame at 1440 × 900 @2×). `kiln-session` evicts layout far from
the viewport, drops unused glyph atlas pages after 30 s, caps the undo log at 1,000 transactions or 64 MB
and streams file writes.

## Status at WP-50

`kiln-desktop` opens one window on the web build and implements `--startup-probe` (prints
`startup-ms=<n>` and `rss-kb=<n>`, then exits), which CI runs under `xvfb-run`. The bridge commands arrive
with WP-52, menus and file associations with WP-88, the Android project with WP-89.
