# @kiln/surface

The document surface (PLAN.md §3.6, `docs/adr/0003-rendering.md`): paints the Rust core's Scene on canvas.
Presentation only — it never lays out text or decides what a command does.

```tsx
<Surface client={client} doc={doc} label="Report.docx" theme="system" />
```

- **`SceneCache`** mirrors what the core's `DeltaTracker` believes the painter holds: tiles by `TileId`,
  atlas pages (rows written as they arrive), images and the overlay; `apply(delta)` validates first and
  leaves the cache unchanged on bad input (`DeltaError`).
- **`Painter`** executes `DrawOp`s on Canvas 2D with kiln-raster's pixel rules: tiles render once into
  their own canvases (`ceil(w × scale)` device pixels), composited at `round(x × scale) − round(scroll ×
  scale)`; glyph runs are composed on the CPU from the A8 atlas into one layer per batch; tile rendering is
  a resumable job (`tileJob`, `prefetch`) spread over spare frame time; shadows are exact Gaussian bands
  around the caster, cached per geometry; the selection wash blends into the content (`darken`/`lighten`),
  other marks go to the overlay canvas (blinking caret with the Kiln spark, composition underline,
  squiggles, teardrop handles on touch).
- **`SurfaceController`** (driven by `<Surface>`): frames (one in flight, coalesced; the first one after
  mounting resets the core's tracker with `suite.view.repaint`), DPR-aware sizing with `ResizeObserver`,
  native scrolling through a transparent scroller with a document-sized spacer (pages centred, phones
  zoomed to fit), Ctrl+wheel/pinch/keyboard zoom, the input proxy (a 1 × 1 px `contenteditable` at the
  caret: `keydown` → Word's keys, `beforeinput` → text and deletions, composition events → IME commands,
  paste/copy/cut), pointer selection with click counting and autoscroll, touch tap/long press/handles,
  caret blinking, and the a11y tree refresh.
- **`<A11yMirror>`**: the visible pages' paragraphs as an off-screen DOM subtree (`role=document`), the
  proxy's `aria-activedescendant`, and a polite live region.
- **`palette.ts`/`shapes.ts`**: the brand palettes and the shared shapes, twins of `kiln_scene` (checked by
  `palette.test.ts` against `fixtures/golden/scene/palette.json`).

Tests: `pnpm --filter @kiln/surface test` (decoding Rust vectors and a real frame, the cache, every painter
path with a recording canvas, the keymap, and the component in jsdom: proxy, mirror, input, IME,
clipboard, pointer selection, zoom). End to end: `pnpm e2e --grep 'surface|painter-parity'` against the
playground route (`#/playground`).
