# ADR 0003: Rendering — Rust layout, Rust glyphs, canvas painting

- **Status:** accepted
- **Date:** 2026-10-15
- **Work package:** WP-50 (condensed from PLAN.md §3.6)

## Decision

All three document surfaces (Write pages, the Grid, Deck slides) are laid out in Rust and painted from a
retained display list, the **Scene** (`kiln-scene`), onto HTML `<canvas>` (Canvas 2D) by a small
TypeScript painter (`packages/surface`). The browser never lays out document text. UI chrome remains HTML.

## Why

1. **Identical everywhere.** Pagination and line breaking are deterministic Rust code over bundled font
   metrics, so a 300-page `.docx` paginates the same in WebKitGTK, WebView2, WKWebView, Android WebView,
   Chromium and in the PDF export, which renders the same Scene.
2. **Performance.** Rust owns layout, invalidation, culling and the glyph cache, multi-threaded natively;
   the painter only blits tiles and glyph quads. A million-cell sheet is tiles over a sparse store, never
   DOM nodes; a long document lays out and paints only visible pages plus two.
3. **One renderer for everything:** screen, print preview, PDF, thumbnails, Kiln Home previews, the CLI's
   `kiln render` PNGs and visual regression tests that run without a browser (`kiln-raster`).
4. **Look:** we control text rasterisation, selection shapes, squiggles, rulers and guides at every zoom
   and pixel ratio.

## Rejected: DOM editing surfaces driven by the Rust model

contenteditable/ProseMirror-style editing for Write, DOM/SVG for Deck and canvas only for the Grid would
give native IME, accessibility and spell checking, but pagination would depend on each browser's line
boxes and differ from the PDF, floats and footnotes become CSS workarounds, three rendering technologies
need maintaining, and a Rust layout engine would still be needed for PDF and thumbnails.

## How the losses are addressed

- **Text pipeline (`kiln-text`):** fonts via `fontique`/`fontdb` (bundled OFL fonts everywhere, system
  fonts on desktop and Android), shaping with `parley` 0.12 over `harfrust` 0.14, metrics and outlines
  with `skrifa`, bidi with `unicode-bidi`, UAX #14/#29 breaking, optional hyphenation; `swash` rasterises
  glyphs into atlas pages (A8 text, RGBA emoji) that the painter uploads once and blits. Layout and
  rasterisation use the same metrics.
- **Scene:** layers of tiles (256 px grid tiles, whole pages split into 1024 px bands, one tile per slide)
  with draw operations; `frame()` returns only tiles whose hash changed plus an overlay layer (selection,
  caret, handles, guides, composition underline, squiggles, collaborator cursors). Scrolling translates
  cached tiles while Rust prefetches one viewport ahead.
- **Text input and IME:** a 1 × 1 px `contenteditable` input proxy sits at the Rust-reported caret and
  forwards `beforeinput`/composition events; Rust owns the composition state (the VS Code and Google Docs
  mechanism).
- **Accessibility:** Rust exposes an `A11yTree` for the visible region, mirrored into an off-screen DOM
  subtree with ARIA roles; an `aria-live` region announces changes; an "Accessible text view" renders the
  document as semantic DOM.
- **Selection, hit testing, touch:** geometry is computed in Rust; gestures are interpreted in TypeScript
  and sent as commands.
- **Spell check:** `kiln-spell` (spellbook, Hunspell-compatible) produces squiggles in the Scene.

A GPU painter (`vello`/`wgpu`) is a later candidate; the Scene format is backend-neutral.
