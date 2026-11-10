/**
 * Kiln Grid under a finger: a finger pans the sheet and a tap selects (the
 * controller), and this layer adds the rest:
 *
 * - the two range handles at the selection's corners (dragging one extends the range to the
 *   cell under the finger);
 * - the floating context toolbar over a range, or over the active cell tapped again;
 * - a long press: the right-click menu of the cell (the controller selects it first);
 * - pinch to zoom (the sheet scales under the fingers, then the core's zoom takes it) and a
 *   double tap: the columns in use fit the screen's width, again for a closer look;
 * - no on-screen keyboard for a tap on a cell (the formula bar has it, pinned above it).
 *
 * Mouse and keyboard work as before: none of this runs for them.
 */
import type { GridHit } from '@kiln/protocol';
import {
  attachGestures,
  doubleTapZoom,
  openContextMenuAt,
  ownLongPress,
  swallowRelease,
  touchSelection,
} from '@kiln/surface';
import { useEffect, useRef } from 'react';
import { GridSurfaceController } from '../controller.ts';
import { selectionRanges } from '../features/model.ts';
import type { GridEditorModel } from '../model.ts';
import './mobile.css';

const ZOOM_MIN = 0.1;
const ZOOM_MAX = 4;

type Corner = 'start' | 'end';

/** The zoom at which columns up to `right` (sheet px, at 100 %) fit `width`, with the row headers. */
export function fitColumnsZoom(layout: { header_w: number }, width: number, right: number): number {
  const fit = width / (layout.header_w + Math.max(64, right));
  return Math.min(2, Math.max(0.25, Math.floor(fit * 100) / 100));
}

export function GridTouchLayer({ model }: { model: GridEditorModel }) {
  const layer = useRef<HTMLDivElement>(null);
  const handles = useRef<Record<Corner, HTMLSpanElement | null>>({ start: null, end: null });

  useEffect(() => {
    const c = model.surface;
    if (!(c instanceof GridSurfaceController)) return;
    const root = c.root;
    const coarse = typeof matchMedia === 'function' && matchMedia('(pointer: coarse)').matches;
    /** The last press was a finger's (the handles and the toolbar show). */
    let touch = false;
    /** A handle is held. */
    let holding = false;
    /** The toolbar shows over the selection (a range, or the active cell tapped again). */
    let toolbar = false;
    let activeAtPress: string | null = null;

    // A tap on a cell gives the keys to the sheet without the on-screen keyboard: typing goes to
    // the formula bar, which brings it (and sits on it).
    const proxy = root.querySelector<HTMLTextAreaElement>('[data-kiln-grid-proxy]');
    if (coarse && proxy) proxy.inputMode = 'none';

    const activeKey = () => {
      const r = selectionRanges(model).at(-1);
      return r ? `${r.r0}:${r.c0}:${r.r1}:${r.c1}` : null;
    };
    const isRange = () => selectionRanges(model).some((r) => r.r0 !== r.r1 || r.c0 !== r.c1);

    const place = () => {
      // A selected chart or picture takes the finger (to move and size it), not the pan.
      if (model.state.grid?.drawing) root.dataset.touchDrawing = '';
      else delete root.dataset.touchDrawing;
      const frame = touch && !model.state.grid?.editor ? c.selectionFrame() : null;
      const box = root.getBoundingClientRect();
      for (const corner of ['start', 'end'] as const) {
        const el = handles.current[corner];
        const p = frame?.[corner] ?? null;
        if (!el) continue;
        if (!p || p.x < 0 || p.y < 0 || p.x > box.width || p.y > box.height) {
          el.hidden = true;
          continue;
        }
        el.hidden = false;
        el.style.transform = `translate(${Math.round(p.x)}px, ${Math.round(p.y)}px)`;
      }
      // The floating toolbar over the range.
      if (!frame || holding || !(toolbar || isRange()) || !frame.start || !frame.end) {
        touchSelection.clear(c.scroller);
        return;
      }
      const left = box.left + Math.min(frame.start.x, frame.end.x);
      const right = box.left + Math.max(frame.start.x, frame.end.x);
      const top = box.top + Math.min(frame.start.y, frame.end.y);
      const bottom = box.top + Math.max(frame.start.y, frame.end.y);
      touchSelection.set({
        kind: 'cells',
        rect: { left, top, right, bottom },
        target: c.scroller,
        point: { x: (left + right) / 2, y: (top + bottom) / 2 },
        bounds: { left: box.left, top: box.top, right: box.right, bottom: box.bottom },
      });
    };

    const stopComposite = c.onComposite(place);
    const stopOwn = ownLongPress(c.scroller);
    const onDown = (e: PointerEvent) => {
      const was = touch;
      touch = e.pointerType === 'touch';
      if (touch) activeAtPress = activeKey();
      if (!touch) toolbar = false;
      if (was !== touch) place();
    };
    root.addEventListener('pointerdown', onDown, true);
    const onKey = () => {
      if (!touch && !toolbar) return;
      toolbar = false;
      place();
    };
    window.addEventListener('keydown', onKey, true);

    // Pinch: the drawn sheet scales under the fingers, then the core zooms and the point under
    // them stays put.
    const canvas = root.querySelector<HTMLCanvasElement>('.kiln-grid-surface__canvas');
    let pinchFrom = 1;
    /** The zoom the last double tap fitted the columns in use at. */
    let fitted: number | null = null;
    const keepAnchor = (zoom: number, before: number, ax: number, ay: number, toLeft = false) => {
      const l = model.state.grid?.layout;
      if (!l) return;
      const sc = c.scroller;
      const sheetX = l.frozen_w + Math.abs(sc.scrollLeft) / before + (ax - l.header_w * before) / before;
      const sheetY = l.frozen_h + sc.scrollTop / before + (ay - l.header_h * before) / before;
      let settled = false;
      const apply = () => {
        if (settled || Math.abs(c.getZoom() - zoom) > 1e-3) return;
        settled = true;
        stop();
        const left = toLeft ? 0 : Math.max(0, (sheetX - l.frozen_w) * zoom - (ax - l.header_w * zoom));
        const top = Math.max(0, (sheetY - l.frozen_h) * zoom - (ay - l.header_h * zoom));
        sc.scrollLeft = l.rtl ? -left : left;
        sc.scrollTop = top;
        if (canvas) canvas.style.transform = '';
      };
      const stop = c.onComposite(apply);
      setTimeout(() => {
        if (settled) return;
        settled = true;
        stop();
        if (canvas) canvas.style.transform = '';
      }, 1500);
    };
    /**
     * The right edge of the columns in use (sheet px): where the filled cells of the accessibility
     * tree end when that is in view, else the core's scrollable width less the screen it adds.
     */
    const usedRight = async (l: { content_w: number }, width: number, zoom: number): Promise<number> => {
      const screen = width / Math.max(zoom, 0.01);
      const fromWidth = l.content_w - screen;
      const tree = await model.client.a11y(model.doc).catch(() => null);
      // The sheet x at the view's right edge.
      const viewRight =
        (model.state.grid?.layout.frozen_w ?? 0) +
        Math.abs(c.scroller.scrollLeft) / Math.max(zoom, 0.01) +
        screen;
      let right = 0;
      for (const n of tree?.nodes ?? [])
        if ((n.role === 'GridCell' || n.role === 'Cell') && n.bounds && n.text)
          right = Math.max(right, n.bounds.x + n.bounds.width);
      // The filled cells end in view: they are the columns in use.
      return right > 0 && right < viewRight - 1 ? right : fromWidth;
    };
    const zoomTo = (zoom: number, ax: number, ay: number, toLeft = false) => {
      const before = c.getZoom();
      const z = Math.min(ZOOM_MAX, Math.max(ZOOM_MIN, Math.round(zoom * 100) / 100));
      if (Math.abs(z - before) < 0.005) {
        if (canvas) canvas.style.transform = '';
        return;
      }
      keepAnchor(z, before, ax, ay, toLeft);
      void model.run('suite.view.zoom', { zoom: z });
    };

    const stopGestures = attachGestures(
      root,
      {
        onTap: (p) => {
          // The active cell tapped again: the toolbar over it (once more: away).
          const box = root.getBoundingClientRect();
          void model.hitTest(p.x - box.left, p.y - box.top, true).then((hit: GridHit) => {
            if (typeof hit !== 'object' || !('Cell' in hit)) {
              toolbar = false;
              place();
              return;
            }
            const { row, col } = hit.Cell;
            toolbar = activeAtPress === `${row}:${col}:${row}:${col}` ? !toolbar : false;
            place();
          });
        },
        onLongPress: (p) => {
          toolbar = false;
          swallowRelease();
          openContextMenuAt(c.scroller, p.x, p.y);
        },
        onDoubleTap: (p) => {
          const l = model.state.grid?.layout;
          if (!l) return;
          const box = root.getBoundingClientRect();
          const z = c.getZoom();
          // At the width this layer fitted: a closer look (and from there, the width again).
          if (fitted !== null && Math.abs(z - fitted) < 0.005) {
            zoomTo(doubleTapZoom(z, fitted, ZOOM_MAX), p.x - box.left, p.y - box.top);
            return;
          }
          void usedRight(l, box.width, z).then((right) => {
            const fit = fitColumnsZoom(l, box.width, right);
            fitted = fit;
            zoomTo(fit, p.x - box.left, p.y - box.top, true);
          });
        },
        onPinchStart: () => {
          pinchFrom = c.getZoom();
          touchSelection.clear(c.scroller);
        },
        onPinch: (scale, center) => {
          if (!canvas) return;
          const box = root.getBoundingClientRect();
          const s = Math.min(ZOOM_MAX, Math.max(ZOOM_MIN, pinchFrom * scale)) / pinchFrom;
          canvas.style.transformOrigin = `${center.x - box.left}px ${center.y - box.top}px`;
          canvas.style.transform = `scale(${s})`;
        },
        onPinchEnd: (scale, center) => {
          const box = root.getBoundingClientRect();
          zoomTo(pinchFrom * scale, center.x - box.left, center.y - box.top);
        },
      },
      { ownPinch: true, ignore: '.kiln-grid-touch__handle' },
    );

    // The handles: dragging one moves that corner of the range to the cell under the finger.
    const cleanups: Array<() => void> = [];
    for (const corner of ['start', 'end'] as const) {
      const el = handles.current[corner];
      if (!el) continue;
      let busy = false;
      let want: { x: number; y: number } | null = null;
      let fixed: { row: number; col: number } | null = null;
      const step = async () => {
        if (busy) return;
        busy = true;
        while (want && fixed) {
          const p = want;
          want = null;
          const box = root.getBoundingClientRect();
          const hit = await model.hitTest(p.x - box.left, p.y - box.top, true);
          if (typeof hit !== 'object' || !('Cell' in hit) || !fixed) continue;
          const { row, col } = hit.Cell;
          const f = fixed;
          await model.core(
            'grid.nav.select_range',
            corner === 'end'
              ? { r0: f.row, c0: f.col, r1: row, c1: col }
              : { r0: row, c0: col, r1: f.row, c1: f.col },
          );
        }
        busy = false;
      };
      const down = (e: PointerEvent) => {
        e.preventDefault();
        e.stopPropagation();
        const r = selectionRanges(model).at(-1);
        if (!r) return;
        fixed = corner === 'end' ? { row: r.r0, col: r.c0 } : { row: r.r1, col: r.c1 };
        holding = true;
        touchSelection.clear(c.scroller);
        try {
          el.setPointerCapture(e.pointerId);
        } catch {
          // Synthetic events.
        }
      };
      const move = (e: PointerEvent) => {
        if (!holding || !fixed) return;
        e.stopPropagation();
        // The finger covers the handle: the cell under the handle's tip is a little above it.
        want = { x: e.clientX, y: e.clientY - 12 };
        void step();
      };
      const up = () => {
        if (!fixed) return;
        holding = false;
        fixed = null;
        toolbar = true;
        place();
      };
      el.addEventListener('pointerdown', down);
      el.addEventListener('pointermove', move);
      el.addEventListener('pointerup', up);
      el.addEventListener('pointercancel', up);
      cleanups.push(() => {
        el.removeEventListener('pointerdown', down);
        el.removeEventListener('pointermove', move);
        el.removeEventListener('pointerup', up);
        el.removeEventListener('pointercancel', up);
      });
    }

    return () => {
      stopComposite();
      stopOwn();
      stopGestures();
      root.removeEventListener('pointerdown', onDown, true);
      window.removeEventListener('keydown', onKey, true);
      for (const f of cleanups) f();
      touchSelection.clear(c.scroller);
      if (canvas) canvas.style.transform = '';
      if (proxy) proxy.removeAttribute('inputmode');
    };
  }, [model, model.surface]);

  return (
    <div ref={layer} className="kiln-grid-touch" aria-hidden="true">
      {(['start', 'end'] as const).map((corner) => (
        <span
          key={corner}
          ref={(el) => {
            handles.current[corner] = el;
          }}
          className={`kiln-grid-touch__handle kiln-grid-touch__handle--${corner}`}
          data-testid={`grid-range-handle-${corner}`}
          hidden
        />
      ))}
    </div>
  );
}
