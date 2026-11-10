# kiln-raster

The CPU painter of Kiln: turns Scenes into RGBA bitmaps with `tiny-skia`, for `kiln render`, golden-image
tests, thumbnails and as a native fallback (`docs/adr/0003-rendering.md`).

- `Compositor` is the raster twin of the canvas painter's tile cache: `apply(&SceneDelta)` (validated; a
  malformed delta changes nothing), then `render(&View, &Palette)` renders each tile once into its own
  pixmap at the scene's device scale and composites tiles and overlay with the canvas painter's rounding
  (`device`, `tile_device_size`), so the two agree pixel for pixel (`e2e/specs/painter-parity.spec.ts`).
- `OpPainter` executes every `DrawOp`: anti-aliased fills, strokes (joins, caps, dashes) and paths
  (non-zero/even-odd), glyph runs copied 1:1 from A8 or colour atlas pages, outline glyphs, images
  (bilinear), anti-aliased clip masks, nested transforms, linear/radial gradients, exact Gaussian box shadows
  outside their caster, squiggles, underlines, the caret with the Kiln spark, the selection wash
  (darken/lighten per palette), guides and touch handles. `PaintOptions` carries the caret blink phase and
  touch mode.
- `Rgba` encodes and decodes PNGs; `render_delta` renders one delta from an empty cache.
- `golden::check(&Rgba, path)` compares with a committed PNG using pixelmatch's YIQ metric (threshold 0.1)
  and fails above 0.3 % differing pixels, writing `<name>.actual.png` and `<name>.diff.png` to
  `$KILN_GOLDEN_DIFF_DIR` (default: `kiln-golden` in the temporary directory); `KILN_UPDATE_GOLDEN=1`
  rewrites the golden.

Tests: `cargo nextest run -p kiln-raster` (every op at pixel level, deltas, validation, PNG, golden diff).
The scene goldens themselves (`fixtures/golden/scene/*.png`) are produced and checked by
`kiln-session`'s scratch tests.
