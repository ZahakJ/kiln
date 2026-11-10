/**
 * Kiln Deck's slide under a finger. The stage sends every press to the core as a
 * mouse's would (a tap selects, a drag moves); this layer adds:
 *
 * - 28 px handles on a selected shape (each a 44 px target) and the rotation handle, which press
 *   the core's own handles and drag them by the finger's travel, so a resize or a turn is the
 *   core's, with its snapping;
 * - teardrop handles at the ends of a text selection (a double tap selects a word), which move
 *   that end as a Shift+click there does;
 * - the floating context toolbar over a selected shape;
 * - a long press: the context menu of what is under the finger;
 * - pinch to zoom (two fingers also pan) and a double tap on the slide's background: the slide
 *   fits the stage, again for a closer look.
 */
import { attachGestures, doubleTapZoom, ownLongPress, swallowRelease, touchSelection } from '@kiln/surface';
import { useEffect, useRef } from 'react';
import type { DeckEditorModel } from '../model.ts';
import { DeckStageController } from '../stage.ts';
import './mobile.css';

/** The core's rotation handle: this far above the top edge's middle, in screen px. */
const CORE_ROTATE_OFFSET = 22;
/** The rotation handle a finger takes: further out, clear of the top handle. */
const TOUCH_ROTATE_OFFSET = 44;
/** Under this many screen px a side has only its corner handles (PowerPoint, the core). */
const SMALL = 24;

type Grip = 'nw' | 'n' | 'ne' | 'e' | 'se' | 's' | 'sw' | 'w' | 'rotate' | 'text-start' | 'text-end';
const SIZE_GRIPS: readonly [Grip, number, number][] = [
  ['nw', 0, 0],
  ['n', 0.5, 0],
  ['ne', 1, 0],
  ['e', 1, 0.5],
  ['se', 1, 1],
  ['s', 0.5, 1],
  ['sw', 0, 1],
  ['w', 0, 0.5],
];
const ALL: readonly Grip[] = [...SIZE_GRIPS.map((g) => g[0]), 'rotate', 'text-start', 'text-end'];

interface Pt {
  x: number;
  y: number;
}

/**
 * Where the handles of a box are, in slide px: the eight sizing handles and the rotation handle
 * (the core's and the finger's), turned with the box about its centre.
 */
export function shapeHandles(
  box: { x: number; y: number; width: number; height: number; rotation: number },
  zoom: number,
): { grips: [Grip, Pt][]; rotateCore: Pt; rotateTouch: Pt; center: Pt } {
  const cx = box.x + box.width / 2;
  const cy = box.y + box.height / 2;
  const a = (box.rotation * Math.PI) / 180;
  const [sin, cos] = [Math.sin(a), Math.cos(a)];
  const turn = (x: number, y: number): Pt => {
    const dx = x - cx;
    const dy = y - cy;
    return { x: cx + dx * cos - dy * sin, y: cy + dx * sin + dy * cos };
  };
  const small = box.width * zoom < SMALL || box.height * zoom < SMALL;
  const grips = SIZE_GRIPS.filter(([g]) => !small || g.length === 2).map(
    ([g, fx, fy]) => [g, turn(box.x + fx * box.width, box.y + fy * box.height)] as [Grip, Pt],
  );
  const z = Math.max(zoom, 0.05);
  return {
    grips,
    rotateCore: turn(cx, box.y - CORE_ROTATE_OFFSET / z),
    rotateTouch: turn(cx, box.y - TOUCH_ROTATE_OFFSET / z),
    center: { x: cx, y: cy },
  };
}

const NO_MODS = { shift: false, ctrl: false, alt: false };

export function DeckTouchLayer({ model }: { model: DeckEditorModel }) {
  const els = useRef<Partial<Record<Grip, HTMLSpanElement | null>>>({});

  useEffect(() => {
    const c = model.stage;
    if (!(c instanceof DeckStageController)) return;
    const root = c.root;
    let touch = false;
    let holding = false;
    /** Where each handle presses the core (slide px). */
    const at: Partial<Record<Grip, Pt>> = {};
    let text: { start: Pt; end: Pt } | null = null;

    const toScreen = (p: Pt) => {
      const v = c.view();
      return { x: v.left + p.x * v.zoom, y: v.top + p.y * v.zoom };
    };
    const show = (g: Grip, p: Pt | null) => {
      const el = els.current[g];
      if (!el) return;
      if (!p) {
        el.hidden = true;
        return;
      }
      const s = toScreen(p);
      el.hidden = false;
      el.style.transform = `translate(${Math.round(s.x)}px, ${Math.round(s.y)}px)`;
    };

    const place = () => {
      const s = model.state.deck;
      const v = c.view();
      for (const g of ALL) delete at[g];
      text = null;
      const box = s?.selection_box;
      const lines = new Set(
        (s?.shapes ?? []).filter((x) => x.kind === 'connector' || x.kind === 'line').map((x) => x.id),
      );
      const shape = touch && s && box && s.selection.length > 0 && !s.selection.every((id) => lines.has(id));
      if (shape && box) {
        const px = {
          x: box.x / 0.75,
          y: box.y / 0.75,
          width: box.width / 0.75,
          height: box.height / 0.75,
          rotation: box.rotation,
        };
        const h = shapeHandles(px, v.zoom);
        for (const [g, p] of h.grips) at[g] = p;
        at.rotate = h.rotateCore;
        for (const [g] of SIZE_GRIPS) show(g, at[g] ?? null);
        show('rotate', h.rotateTouch);
      } else {
        for (const [g] of SIZE_GRIPS) show(g, null);
        show('rotate', null);
      }
      // A text selection's ends: the overlay's highlight (slide px).
      const rects =
        touch && s?.editing
          ? c.cache.overlay.flatMap((op) =>
              typeof op === 'object' && 'SelectionRects' in op ? op.SelectionRects.rects : [],
            )
          : [];
      if (rects.length > 0) {
        const first = rects.reduce((a, b) =>
          b.y < a.y - 0.5 || (Math.abs(b.y - a.y) < 0.5 && b.x < a.x) ? b : a,
        );
        const last = rects.reduce((a, b) =>
          b.y > a.y + 0.5 || (Math.abs(b.y - a.y) < 0.5 && b.x + b.w > a.x + a.w) ? b : a,
        );
        text = {
          start: { x: first.x, y: first.y + first.h / 2 },
          end: { x: last.x + last.w, y: last.y + last.h / 2 },
        };
        show('text-start', { x: first.x, y: first.y + first.h });
        show('text-end', { x: last.x + last.w, y: last.y + last.h });
      } else {
        show('text-start', null);
        show('text-end', null);
      }
      // The floating toolbar over a selected shape.
      if (shape && box && !holding && !s?.editing) {
        const corners = SIZE_GRIPS.map(([g]) => at[g]).filter((p): p is Pt => !!p);
        const r = root.getBoundingClientRect();
        const xs = corners.map((p) => r.left + toScreen(p).x);
        const ys = corners.map((p) => r.top + toScreen(p).y);
        const rect = {
          left: Math.min(...xs),
          top: Math.min(...ys) - TOUCH_ROTATE_OFFSET,
          right: Math.max(...xs),
          bottom: Math.max(...ys),
        };
        touchSelection.set({
          kind: 'object',
          rect,
          target: root,
          bounds: { left: r.left, top: r.top, right: r.right, bottom: r.bottom },
          point: {
            x: (rect.left + rect.right) / 2,
            y: (rect.top + rect.bottom) / 2 + TOUCH_ROTATE_OFFSET / 2,
          },
        });
      } else touchSelection.clear(root);
    };

    const stopComposite = c.onComposite(place);
    const stopOwn = ownLongPress(root);
    const stopState = model.store.subscribe((st, prev) => {
      if (st.deck !== prev.deck) place();
    });
    const onDown = (e: PointerEvent) => {
      const was = touch;
      touch = e.pointerType === 'touch';
      if (was !== touch) place();
    };
    root.addEventListener('pointerdown', onDown, true);

    let pinchFrom = 1;
    let pinchAt: Pt | null = null;
    // The stage and the handles over it: a pinch may start on a handle.
    const stopGestures = attachGestures(
      root.parentElement ?? root,
      {
        onLongPress: (p) => {
          swallowRelease();
          c.openMenuAt(p.x, p.y);
        },
        onDoubleTap: (p) => {
          // On a shape the double tap is the core's (it edits the text); on the background it zooms.
          const q = c.toSlide({ clientX: p.x, clientY: p.y });
          void model.cursorAt(q.x, q.y).then((cursor) => {
            if (cursor !== 'Default') return;
            const v = c.view();
            const fit = c.fitZoom();
            const next = doubleTapZoom(v.zoom, fit, 4);
            if (Math.abs(next - fit) < 0.005) model.stage?.fitToWindow();
            else c.zoomAbout(next, p.x, p.y);
          });
        },
        onPinchStart: (center) => {
          pinchFrom = c.view().zoom;
          pinchAt = center;
          touchSelection.clear(root);
        },
        onPinch: (scale, center) => {
          if (pinchAt) c.panBy(center.x - pinchAt.x, center.y - pinchAt.y);
          pinchAt = center;
          c.zoomAbout(pinchFrom * scale, center.x, center.y, false);
        },
        onPinchEnd: (scale, center) => {
          pinchAt = null;
          c.zoomAbout(pinchFrom * scale, center.x, center.y);
        },
      },
      { ownPinch: true, ignore: '.kiln-deck-touch__handle, .kiln-deck__notice' },
    );

    // Dragging a handle: the core's handle is pressed and moved by the finger's travel.
    const cleanups: Array<() => void> = [];
    for (const g of ALL) {
      const el = els.current[g];
      if (!el) continue;
      let drag: { id: number; from: Pt; press: Pt } | null = null;
      let busy = false;
      let want: Pt | null = null;
      const moveTo = async () => {
        if (busy) return;
        busy = true;
        while (want) {
          const p = want;
          want = null;
          if (g === 'text-start' || g === 'text-end') {
            // A Shift+click where the finger is moves this end (the other one is the anchor).
            await model.core('deck.stage.pointer_down', { ...p, ...NO_MODS, shift: true, clicks: 1 });
            await model.core('deck.stage.pointer_up', { ...p, ...NO_MODS, shift: true });
          } else await model.core('deck.stage.pointer_move', { ...p, ...NO_MODS });
          c.requestFrame();
        }
        busy = false;
      };
      const down = (e: PointerEvent) => {
        e.preventDefault();
        e.stopPropagation();
        const press = at[g] ?? (g === 'text-start' ? text?.end : g === 'text-end' ? text?.start : undefined);
        if (!press) return;
        try {
          el.setPointerCapture(e.pointerId);
        } catch {
          // Synthetic events.
        }
        holding = true;
        touchSelection.clear(root);
        const from = c.toSlide(e);
        drag = { id: e.pointerId, from, press };
        if (g === 'text-start' && text) {
          // The end becomes the anchor: a click there first.
          void model.core('deck.stage.pointer_down', { ...text.end, ...NO_MODS, clicks: 1 });
          void model.core('deck.stage.pointer_up', { ...text.end, ...NO_MODS });
        } else if (g !== 'text-end')
          void model.core('deck.stage.pointer_down', { ...press, ...NO_MODS, clicks: 1 });
      };
      const move = (e: PointerEvent) => {
        if (!drag || e.pointerId !== drag.id) return;
        e.stopPropagation();
        const p = c.toSlide(e);
        // The rotation follows the finger itself; a sizing handle moves by the finger's travel.
        want =
          g === 'rotate' || g.startsWith('text')
            ? p
            : { x: drag.press.x + p.x - drag.from.x, y: drag.press.y + p.y - drag.from.y };
        void moveTo();
      };
      const up = (e: PointerEvent) => {
        if (!drag || e.pointerId !== drag.id) return;
        const d = drag;
        drag = null;
        holding = false;
        const p = c.toSlide(e);
        const end =
          g === 'rotate' || g.startsWith('text')
            ? p
            : { x: d.press.x + p.x - d.from.x, y: d.press.y + p.y - d.from.y };
        const finish = async () => {
          while (busy) await new Promise((r) => setTimeout(r, 16));
          if (!g.startsWith('text')) await model.core('deck.stage.pointer_up', { ...end, ...NO_MODS });
          model.refresh();
          c.requestFrame();
        };
        void finish();
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
      stopState();
      stopGestures();
      root.removeEventListener('pointerdown', onDown, true);
      for (const f of cleanups) f();
      touchSelection.clear(root);
    };
  }, [model, model.stage]);

  return (
    <div className="kiln-deck-touch" aria-hidden="true">
      {ALL.map((g) => (
        <span
          key={g}
          ref={(el) => {
            els.current[g] = el;
          }}
          className={`kiln-deck-touch__handle kiln-deck-touch__handle--${g.startsWith('text') ? 'text' : g === 'rotate' ? 'rotate' : 'size'}`}
          data-grip={g}
          data-testid={`deck-handle-${g}`}
          hidden
        />
      ))}
    </div>
  );
}
