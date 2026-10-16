# kiln-scene

The retained display list of Kiln (PLAN.md §3.6, `docs/adr/0003-rendering.md`). Every surface — Write pages,
Grid tiles, Deck slides — becomes a `Scene` of tiles holding `DrawOp`s plus an overlay; painters
(`packages/surface` on Canvas 2D, `kiln-raster` on tiny-skia, later the PDF writer) only execute ops.

- **Vocabulary:** `Bounds`; `Paint` (literal `0xRRGGBBAA` or a `ColorRole` such as `Page`, `Ink`,
  `Selection`, `Caret`) resolved by a `Palette` (`Palette::light()`/`dark()`, the brand kit's values,
  mirrored in `fixtures/golden/scene/palette.json`); `PathData` (verbs + flat `f32` coordinates, built from
  `kurbo` paths); `DrawOp`: `FillRect`, `StrokeRect`, `Path` (fill and/or stroke, non-zero/even-odd),
  `Glyphs` (atlas quads in device pixels), `OutlineGlyphs`, `Image`, `PushClip`/`PopClip`,
  `PushTransform`/`PopTransform`, `Gradient`, `Shadow`, `Squiggle`, `Underline`, `Caret`, `SelectionRects`,
  `Guide`, `Handle`. `cull` drops ops outside a region.
- **Scene:** tile slots for the whole document (rectangle + content version) and built tiles for the region
  a frame needs (`Tile` with an FxHash of its ops, `HitIndex` of lines for hit testing), plus the overlay.
  `Scene::hit_test` finds the line under (or nearest) a point.
- **Deltas:** `SceneDelta` (tiles, removed ids, atlas rows, images, overlay, reset, scale, extent) with
  postcard `to_postcard`/`from_postcard`; `DeltaTracker` mirrors what a painter holds and produces the
  smallest delta for a send region (prefetch) and a keep region.
- **Shapes** (`shapes`): rounded rectangles, the Kiln caret spark, squiggles, teardrop touch handles and the
  erf-based Gaussian shadow profile — twins of `packages/surface/src/shapes.ts`.

Every wire type derives `serde` and `ts_rs::TS` and is re-exported from `kiln_protocol::scene`, so
`cargo xtask protocol` generates its TypeScript type, schema and postcard test vectors.

Tests: `cargo nextest run -p kiln-scene` (geometry, paths, culling, hashing, hit testing, delta tracking,
postcard round trips, shapes, the palette fixture).
