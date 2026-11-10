/**
 * The print preview: paints the core's preview Scene of one sheet (Query `PrintPreview`) with the
 * same Canvas 2D painter as the document surface, so the preview is pixel-for-pixel what the core
 * prints. The core rasterises the glyphs at the scale this component asks for (its CSS size ×
 * device pixel ratio); nothing is laid out or measured here.
 */
import type { SceneDelta } from '@kiln/protocol';
import { Painter, type PainterCanvas, paletteFor, SceneCache } from '@kiln/surface';
import { useEffect, useRef } from 'react';
import { useTheme } from '../theme/index.ts';

export interface PrintPreviewProps {
  /** The core's scene of the sheet (`PrintPreview.scene`). */
  scene: SceneDelta | null;
  /** Accessible description ("Page 2 of 12, Letter, portrait"). */
  label: string;
  /** Shown over the preview while a newer one is being prepared. */
  busy?: boolean;
  className?: string;
}

/** CSS px the scene covers at its own scale and the device pixel ratio. */
export function sceneCssSize(scene: SceneDelta, dpr: number): { width: number; height: number } {
  const css = scene.scale / (dpr > 0 ? dpr : 1);
  return { width: scene.extent.w * css, height: scene.extent.h * css };
}

export function PrintPreview({ scene, label, busy = false, className }: PrintPreviewProps) {
  const canvas = useRef<HTMLCanvasElement>(null);
  const theme = useTheme();
  const palette = theme.resolved === 'light' ? 'light' : 'dark';

  useEffect(() => {
    const el = canvas.current;
    if (!el || !scene) return;
    const dpr = typeof window === 'undefined' ? 1 : window.devicePixelRatio || 1;
    const cache = new SceneCache<PainterCanvas>();
    try {
      cache.apply(scene);
    } catch {
      return; // a malformed scene leaves the previous picture
    }
    const width = Math.max(1, Math.round(scene.extent.w * scene.scale));
    const height = Math.max(1, Math.round(scene.extent.h * scene.scale));
    el.width = width;
    el.height = height;
    const { width: cssW, height: cssH } = sceneCssSize(scene, dpr);
    el.style.width = `${cssW}px`;
    el.style.height = `${cssH}px`;
    const ctx = el.getContext('2d');
    if (!ctx) return;
    const painter = new Painter(paletteFor(palette));
    ctx.clearRect(0, 0, width, height);
    painter.composite(ctx, cache, { width, height, scrollX: 0, scrollY: 0, viewScale: scene.scale });
  }, [scene, palette]);

  return (
    <div className={className ? `kiln-print-preview ${className}` : 'kiln-print-preview'} aria-busy={busy}>
      {scene ? (
        <canvas
          ref={canvas}
          className="kiln-print-preview__sheet"
          role="img"
          aria-label={label}
          data-testid="print-preview"
        />
      ) : null}
    </div>
  );
}
