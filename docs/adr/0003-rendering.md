# ADR 0003: Rendering — Rust layout, Rust glyphs, canvas painting

- **Status:** accepted; implemented for the scratch document by WP-54
- **Date:** 2026-10-16 (decision 2026-10-15, WP-50)
- **Work packages:** WP-50 (decision), WP-54 (Scene, painters, input proxy, accessibility mirror)

## Decision

All three document surfaces (Write pages, the Grid, Deck slides) are laid out in Rust and painted from a
retained display list, the **Scene** (`kiln-scene`), onto HTML `<canvas>` (Canvas 2D) by a small
TypeScript painter (`packages/surface`). The browser never lays out document text. UI chrome remains HTML.
The same Scene is painted by `kiln-raster` (tiny-skia) for `kiln render`, thumbnails and golden tests, and
later by the PDF writer.

## Why

1. **Identical everywhere.** Pagination and line breaking are deterministic Rust code over bundled font
   metrics, so a 300-page `.docx` paginates the same in WebKitGTK, WebView2, WKWebView, Android WebView,
   Chromium and in the PDF export, which renders the same Scene.
2. **Performance.** Rust owns layout, invalidation, culling and the glyph cache; the painter only blits
   tiles. A long document lays out and paints only what is near the viewport.
3. **One renderer for everything:** screen, print preview, PDF, thumbnails, Kiln Home previews, the CLI's
   `kiln render` PNGs and visual regression tests that run without a browser.
4. **Look:** we control text rasterisation, selection shapes, the caret (with the brand's spark),
   squiggles, guides and shadows at every zoom and pixel ratio.

**Rejected: DOM editing surfaces driven by the Rust model** (contenteditable/ProseMirror for Write, DOM/SVG
for Deck, canvas only for the Grid): pagination would depend on each browser's line boxes and differ from
the PDF, floats and footnotes become CSS workarounds, three rendering technologies need maintaining, and a
Rust layout engine is needed for PDF and thumbnails anyway. The losses (native IME, accessibility, spell
check) are addressed below.

## The Scene (`kiln-scene`, re-exported as `kiln_protocol::scene`)

- **Vocabulary.** `DrawOp` is a closed set: `FillRect`, `StrokeRect`, `Path` (kurbo-built, non-zero or
  even-odd, joins/caps/dashes), `Glyphs` (quads from an atlas page in one colour), `OutlineGlyphs`, `Image`,
  `PushClip`/`PopClip` (rectangle or path), `PushTransform`/`PopTransform`, `Gradient` (linear/radial),
  `Shadow` (Gaussian box shadow, or the theme's page shadow), `Squiggle`, `Underline`, `Caret`,
  `SelectionRects`, `Guide` and touch `Handle`s. Paths travel as a verb list plus a flat `f32` list, pixel
  buffers as postcard byte strings (`serialize_bytes`), so frames decode quickly in TypeScript.
- **Colours are paints:** a literal `0xRRGGBBAA` or a `ColorRole` (`Page`, `Ink` = Word's automatic text,
  `Selection`, `Caret`, …) resolved by the painter's `Palette`. Dark mode is a palette swap — tuned, not
  inverted, with the page's ember glow — and never a relayout. The palettes hold the brand kit's values and
  are checked against `fixtures/golden/scene/palette.json` on both sides.
- **Tiles.** A scene has *tile slots* for the whole document (a rectangle and a cheap content version) and
  *built tiles* (ops in tile-local CSS pixels, FxHash of the ops, a hit-test index) for the region a frame
  needs. Write tiles are whole pages including their shadow; pages taller than ~1,500 px will be split into
  1,024 px bands when the Write engine needs it, grid tiles are 256 px, Deck has one tile per slide.
- **Glyphs are device-exact.** Glyph runs are rasterised by `kiln-text` (swash) for the scene's
  `scale = zoom × DPR` at quarter-pixel positions and placed in *device pixels* relative to the tile's
  device origin `round(tile.x × scale)`. Painters copy them 1:1 without resampling, which is what keeps text
  crisp; glyphs at 96 device px or larger, and transformed text, are outline paths.
- **`SceneDelta`** (postcard): `reset`, `scale`, `extent`, background, new or changed tiles, removed tile
  ids, atlas page rows, images and the overlay when it changed. `DeltaTracker` mirrors what the painter
  holds (tile versions and hashes, overlay hash, scale): a frame sends only built tiles the painter lacks in
  the *send* region (the viewport plus one viewport on each side — the prefetch margin) and removes tiles
  outside the *keep* region (plus one and a half viewports), deleted or stale tiles. A scale change or the
  `suite.view.repaint` command (a remounted surface) resets the painter.

## Frames (`kiln-session::frame`)

`Session::frame(doc)` asks the document model's `FrameSource` to prepare its scene for the document's
viewport and returns the tracker's delta with the glyph atlas rows the painter lacks. The scratch document
lays out its paragraphs with `kiln-text` (Calibri 11 pt, 1.08 line spacing, 8 pt after, Letter pages with
1-inch margins), paginates line by line, versions every page by its lines, and builds page tiles lazily;
an edit relays out only the changed paragraphs and resends only the pages whose content changed. Documents
that are never painted (the control channel, tests) never lay out.

## Painters

- **`kiln-raster`** (`Compositor`): applies the same deltas the canvas painter receives, renders each tile
  once into its own pixmap, and composites tiles and the overlay with the canvas painter's rounding.
  `kiln-cli render --scratch file.txt --png out.png` and the golden tests (`fixtures/golden/scene/*.png`,
  0.3 % differing pixels allowed, diff images on failure) use it.
- **`packages/surface`** (`SceneCache`, `Painter`, `SurfaceController`, `<Surface>`): a content canvas and an
  overlay canvas sized to the surface × DPR. Tiles render once into their own canvases; glyph runs are
  composed on the CPU from the A8 atlas into one layer per tile (source-over is associative, so this equals
  blitting each glyph, at a fraction of the cost of thousands of `drawImage` calls), and tile rendering is a
  resumable job spread over the spare time of animation frames. Compositing is one `drawImage` per visible
  tile at integer device offsets. The selection wash is drawn into the content with `darken`/`lighten`
  (it reads as painted under the text); the caret, composition underline, squiggles and handles are on the
  overlay canvas, which blinks without recompositing.
- **Parity.** `e2e/specs/painter-parity.spec.ts` compares the canvas with the kiln-raster goldens: 0.004 %
  differing pixels for `lorem.txt` in light and dark, 0 % for a scrolled word selection at 2× DPR (budget
  1 %). Shared shapes (spark, teardrop handles, squiggles, rounded rectangles, the erf-based shadow) are
  twins in `kiln_scene::shapes` and `packages/surface/src/shapes.ts`.

## Scrolling and zoom

Scrolling is native: a transparent scroller above the canvases holds a spacer the size of the document at
the current zoom, so wheel, trackpad, touch panning, keyboard focus and scrollbars behave as the platform
does. On every scroll the cached tiles are composited at the new offset in the next animation frame and a
frame for the new viewport is requested (one in flight at most); the core has already prepared the tiles
around it. A page narrower than the surface is centred by a negative horizontal scroll; a page wider than a
phone starts zoomed to fit its width. Ctrl+wheel, trackpad and touch pinch zoom around the pointer; the
cached tiles are stretched immediately and re-rasterised after the gesture pauses (140 ms). Measured on the
build machine: 500 paragraphs scrolled at 2,160 px/s with the surface's work per frame at p95 2–4 ms and no
dropped frames when the machine is otherwise idle.

## Text input and IME

A 1 × 1 px transparent `contenteditable` input proxy (`role=textbox`, `spellcheck=false`,
`autocapitalize=off`) sits at the core-reported caret inside the scroller, so IME candidate windows and soft
keyboards appear in the right place; it is focused whenever the surface is active. `keydown` maps Word's
keys (arrows with Ctrl/Shift, Home/End, Page Up/Down, Ctrl+A/Z/Y, Tab) to commands; `beforeinput` maps
text, paragraph breaks and every deletion kind; `compositionupdate` sends `SetComposition{text, caret}` and
`compositionend` sends `CommitComposition{text}`; the core owns the composition (its text is in the
paragraph, laid out and underlined, but enters the undo history only when committed, as one typing step) and
other commands commit it implicitly. Paste inserts plain text; copy and cut use the selected text the
surface fetched (`Query::SelectedText`) when the selection changed. The proxy is emptied after every input.

## Selection, hit testing, touch

`WriteCommand::SelectAt{point, extend, unit}` hit tests in the core (the tile's hit-test index of lines,
then `kiln-text`) and selects by character, word (with its trailing spaces, as in Word) or paragraph;
dragging extends by the unit of the press; Shift+click extends. The surface counts clicks itself (pointer
events have no `detail`), coalesces drag updates to one in flight and autoscrolls when the pointer leaves
the surface. On touch, a tap places the caret, a long press selects a word, and 44 × 44 px hit targets over
the teardrop handles drag either end of the selection.

## Accessibility

`Query::A11y` returns the paragraphs of the visible pages ± one page (always including the caret's) with
their bounds, the active paragraph and the caret position. `<Surface>` mirrors it into an off-screen DOM
subtree (`role=document`, visually clipped, not `display: none`), points the proxy's
`aria-activedescendant` at the caret's paragraph and announces `announce` strings in a polite live region.
`axe-core` reports no serious or critical violations on the playground.

## Known limits

- One painter per document view: two surfaces showing the same document would share one `DeltaTracker`.
  Split views need a view id in `frame()` (protocol change, with the Write engine).
- The web core embeds Carlito only (the 4.5 MB budget, B12): scripts it lacks (CJK, Arabic) show missing
  glyphs on the web until fonts load at run time (WP-94); natively the bundled fonts apply, without CJK.
- The glyph atlas does not evict yet (pages grow with every size); a zoom change starts a new atlas.
- A GPU painter (`vello`/`wgpu`) remains a later candidate; the Scene format is backend-neutral.
