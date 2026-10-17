/**
 * @kiln/surface — the document surface (PLAN.md §3.6): paints the core's Scene on canvas.
 *
 * - {@link Surface}: the React component (`<Surface client doc />`).
 * - {@link SurfaceController}: frames, compositing, scrolling, zoom, input proxy, selection.
 * - {@link SceneCache} and {@link Painter}: the tile cache and the Canvas 2D painter, the twins of
 *   `kiln_raster::Compositor` and `OpPainter`.
 */

export { A11yMirror, mirrorId, VISUALLY_HIDDEN } from './A11yMirror';
export {
  SurfaceController,
  type SurfaceElements,
  type SurfaceOptions,
  type SurfaceStats,
} from './controller';
export {
  autoscrollSpeed,
  charCount,
  commandForInput,
  commandForKey,
  type KeyLike,
  unitForClicks,
} from './input';
export {
  type CanvasFactory,
  type Ctx2D,
  defaultCanvasFactory,
  device,
  Painter,
  type PainterCanvas,
  tileDeviceSize,
} from './painter';
export {
  channels,
  cssColor,
  DARK,
  LIGHT,
  type PaletteApp,
  paletteFor,
  resolvePaint,
  roleColor,
} from './palette';
export { Surface, type SurfaceProps } from './Surface';
export { type AtlasPage, type CachedTile, DeltaError, glyphPages, SceneCache, tileKey } from './scene-cache';
export * from './shapes';
