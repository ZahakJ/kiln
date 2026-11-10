/**
 * View ▸ Split: a second pane under the main one showing another part of the same document
 * (Word's split window). It paints from the main surface's tile cache; the core sends the tiles
 * of the region it shows besides the viewport's (`write.view.split_region`). A click or drag in
 * it places the caret or selects, typing goes on in the same document, and while it is the
 * active pane it keeps the caret in view (the main pane stays where it is).
 */
import { t } from '@kiln/i18n';
import type { Rect } from '@kiln/protocol';
import type { SurfaceController } from '@kiln/surface';
import { useEffect, useRef } from 'react';
import type { WriteEditorModel } from '../model.ts';

interface Props {
  model: WriteEditorModel;
  controller: SurfaceController;
}

export function SplitPane({ model, controller }: Props) {
  const scroller = useRef<HTMLDivElement | null>(null);
  const spacer = useRef<HTMLDivElement | null>(null);
  const content = useRef<HTMLCanvasElement | null>(null);
  const overlay = useRef<HTMLCanvasElement | null>(null);

  useEffect(() => {
    const sc = scroller.current;
    const canvas = content.current;
    const over = overlay.current;
    if (!sc || !canvas || !over) return;
    let disposed = false;
    let raf = 0;
    let sent = '';
    let active = false;
    // Starts where the main pane is, a pane further down (Word splits at the middle).
    const start = controller.viewState();
    sc.scrollTop = (start.scrollY + start.height / start.zoom) * start.zoom;

    const geometry = () => {
      const v = controller.viewState();
      const dpr = window.devicePixelRatio || 1;
      return { v, dpr, w: sc.clientWidth, h: sc.clientHeight, scrollY: sc.scrollTop / v.zoom };
    };
    const paint = () => {
      raf = 0;
      if (disposed) return;
      const { v, dpr, w, h, scrollY } = geometry();
      const extent = controller.cache.extent;
      if (spacer.current) spacer.current.style.height = `${Math.max(0, (extent.y + extent.h) * v.zoom)}px`;
      for (const c of [canvas, over]) {
        const cw = Math.round(w * dpr);
        const ch = Math.round(h * dpr);
        if (c.width !== cw || c.height !== ch) {
          c.width = cw;
          c.height = ch;
          c.style.width = `${w}px`;
          c.style.height = `${h}px`;
        }
      }
      const view = {
        width: canvas.width,
        height: canvas.height,
        scrollX: v.scrollX,
        scrollY,
        viewScale: v.zoom * dpr,
      };
      const ctx = canvas.getContext('2d');
      const octx = over.getContext('2d');
      if (ctx) controller.painter.composite(ctx as never, controller.cache, view);
      if (octx) {
        // The caret and the other marks, painted here (the painter's own overlay pass keeps
        // its dirty rectangle for the main pane).
        const vs = view.viewScale;
        const ox = Math.round(view.scrollX * vs);
        const oy = Math.round(scrollY * vs);
        octx.setTransform(1, 0, 0, 1, 0, 0);
        octx.clearRect(0, 0, over.width, over.height);
        const marks = controller.cache.overlay.filter(
          (op) => !(typeof op === 'object' && 'SelectionRects' in op),
        );
        octx.setTransform(vs, 0, 0, vs, -ox, -oy);
        controller.painter.paintOps(octx as never, marks, controller.cache, {
          scale: vs,
          glyphOrigin: [-ox, -oy],
          caretVisible: active,
          touch: false,
        });
        octx.setTransform(1, 0, 0, 1, 0, 0);
      }
      // The core sends this region's tiles with every frame.
      const region: Rect = {
        x: v.scrollX,
        y: scrollY,
        width: Math.max(1, w / v.zoom),
        height: Math.max(1, h / v.zoom),
      };
      const key = `${Math.round(region.x)}|${Math.round(region.y)}|${Math.round(region.width)}|${Math.round(region.height)}`;
      if (key !== sent) {
        sent = key;
        void model.runCore('write.view.split_region', { region }).then(() => controller.requestFrame());
      }
    };
    const schedule = () => {
      if (!raf && !disposed) raf = requestAnimationFrame(paint);
    };
    const offComposite = controller.onComposite(schedule);
    const resize = new ResizeObserver(schedule);
    resize.observe(sc);
    sc.addEventListener('scroll', schedule, { passive: true });

    // Pointer: place the caret, drag to select (in document coordinates).
    const pointAt = (e: PointerEvent) => {
      const r = sc.getBoundingClientRect();
      const { v, scrollY } = geometry();
      return { x: v.scrollX + (e.clientX - r.left) / v.zoom, y: scrollY + (e.clientY - r.top) / v.zoom };
    };
    let dragging = false;
    const onDown = (e: PointerEvent) => {
      if (e.button !== 0) return;
      e.preventDefault();
      setActive(true);
      dragging = true;
      sc.setPointerCapture?.(e.pointerId);
      void model.runCore('write.caret.select_at', {
        point: pointAt(e),
        extend: e.shiftKey,
        unit: e.detail >= 3 ? 'Paragraph' : e.detail === 2 ? 'Word' : 'Char',
      });
      controller.focus();
    };
    const onMove = (e: PointerEvent) => {
      if (!dragging) return;
      void model.runCore('write.caret.select_at', { point: pointAt(e), extend: true, unit: 'Char' });
    };
    const onUp = (e: PointerEvent) => {
      dragging = false;
      sc.releasePointerCapture?.(e.pointerId);
    };
    // The main pane takes the caret back when it is clicked.
    const host = sc.closest('.kiln-write__surface');
    const main = host?.querySelector('.kiln-write__pane-main [data-testid="kiln-surface-scroller"]');
    const pane = sc.closest('.kiln-write-split');
    const onMainDown = () => setActive(false);
    const setActive = (on: boolean) => {
      if (active === on) return;
      active = on;
      pane?.toggleAttribute('data-active', on);
      controller.setRevealHandler(
        on
          ? (caret) => {
              const { v, h, scrollY } = geometry();
              const margin = 24 / v.zoom;
              const bottom = scrollY + h / v.zoom;
              if (caret.y - margin < scrollY) sc.scrollTop = Math.max(0, (caret.y - margin) * v.zoom);
              else if (caret.y + caret.height + margin > bottom) {
                sc.scrollTop = (caret.y + caret.height + margin) * v.zoom - h;
              }
              schedule();
            }
          : null,
      );
      schedule();
    };
    sc.addEventListener('pointerdown', onDown);
    sc.addEventListener('pointermove', onMove);
    sc.addEventListener('pointerup', onUp);
    main?.addEventListener('pointerdown', onMainDown, true);
    schedule();
    return () => {
      disposed = true;
      if (raf) cancelAnimationFrame(raf);
      offComposite();
      resize.disconnect();
      sc.removeEventListener('scroll', schedule);
      sc.removeEventListener('pointerdown', onDown);
      sc.removeEventListener('pointermove', onMove);
      sc.removeEventListener('pointerup', onUp);
      main?.removeEventListener('pointerdown', onMainDown, true);
      controller.setRevealHandler(null);
      void model.runCore('write.view.split_region', { region: null });
    };
  }, [model, controller]);

  return (
    <div className="kiln-write-split" data-testid="write-split-pane">
      <hr className="kiln-write-split__bar" aria-label={t('Split')} />
      <div className="kiln-write-split__view">
        <canvas ref={content} className="kiln-write-split__canvas" data-testid="write-split-content" />
        <canvas ref={overlay} className="kiln-write-split__canvas" />
        <div
          ref={scroller}
          className="kiln-write-split__scroller"
          data-testid="write-split-scroller"
          role="document"
          aria-label={t('Second pane of the document')}
        >
          <div ref={spacer} style={{ width: 1 }} />
        </div>
      </div>
    </div>
  );
}
