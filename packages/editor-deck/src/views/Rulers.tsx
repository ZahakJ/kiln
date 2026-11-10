/**
 * View ▸ Ruler and the drawing guides' handles, over the stage. The rulers count from the middle of
 * the slide outwards, as PowerPoint's do, in the measurement unit of File ▸ Options; the selected
 * object's extent is light on them, and while text is edited the horizontal ruler has its
 * paragraph's indent markers, dragged to set the first-line and left indents
 * (`deck.paragraph.indent`). The guides (View ▸ Guides, drawn by the core) are dragged here: off
 * the slide to remove one, with Ctrl to copy it; the label says how far it is from the middle.
 */
import { t } from '@kiln/i18n';
import { useMeasurementUnit } from '@kiln/ui-kit';
import { type PointerEvent, useEffect, useLayoutEffect, useRef, useState } from 'react';
import { useDeck } from '../hooks.ts';
import type { DeckEditorModel } from '../model.ts';

/** The rulers' thickness, CSS px. */
export const RULER = 20;
/** Slide px per inch (96) and per centimetre. */
const PX_PER_IN = 96;
const PX_PER_CM = 96 / 2.54;

interface View {
  zoom: number;
  left: number;
  top: number;
  width: number;
  height: number;
}

/** The stage's view of the slide, followed every frame while `on`. */
function useStageView(model: DeckEditorModel, on: boolean): View | null {
  const [view, setView] = useState<View | null>(null);
  useEffect(() => {
    if (!on) return;
    let frame = 0;
    let last = '';
    const tick = () => {
      const v = model.stage?.view?.() ?? null;
      const key = v ? `${v.zoom}:${v.left}:${v.top}:${v.width}:${v.height}` : '';
      if (key !== last) {
        last = key;
        setView(v);
      }
      frame = requestAnimationFrame(tick);
    };
    tick();
    return () => cancelAnimationFrame(frame);
  }, [model, on]);
  return view;
}

/** Major ticks (labelled) and minor ticks of a ruler: slide px per unit and subdivisions. */
export function tickPlan(unit: string): { step: number; minor: number } {
  return unit === 'in' ? { step: PX_PER_IN, minor: 8 } : { step: PX_PER_CM, minor: 2 };
}

/** A distance from the middle as the label says it ("2.25"), in the ruler's unit. */
export function fromMiddle(px: number, unit: string): string {
  const v = Math.abs(px) / (unit === 'in' ? PX_PER_IN : PX_PER_CM);
  return v.toFixed(2);
}

function RulerCanvas({
  vertical,
  view,
  length,
  span,
  slideSize,
  unit,
}: {
  vertical: boolean;
  view: View;
  /** The ruler's length, CSS px. */
  length: number;
  /** The light span (the selection), slide px. */
  span: [number, number] | null;
  slideSize: number;
  unit: string;
}) {
  const ref = useRef<HTMLCanvasElement>(null);
  useLayoutEffect(() => {
    const c = ref.current;
    const g = c?.getContext('2d');
    if (!c || !g) return;
    const dpr = window.devicePixelRatio || 1;
    c.width = Math.round((vertical ? RULER : length) * dpr);
    c.height = Math.round((vertical ? length : RULER) * dpr);
    g.setTransform(dpr, 0, 0, dpr, 0, 0);
    const css = getComputedStyle(c);
    const fg = css.getPropertyValue('--kiln-fg-muted').trim() || '#6b5f57';
    const bg = css.getPropertyValue('--kiln-sunken').trim() || '#eee6dc';
    const page = css.getPropertyValue('--kiln-raised').trim() || '#fff';
    const origin = vertical ? view.top : view.left;
    const at = (slidePx: number) => origin + slidePx * view.zoom - (vertical ? RULER : RULER);
    g.fillStyle = bg;
    g.fillRect(0, 0, vertical ? RULER : length, vertical ? length : RULER);
    // The slide's extent, and the selection's.
    const band = (a: number, b: number, colour: string) => {
      g.fillStyle = colour;
      const p0 = at(a);
      const p1 = at(b);
      if (vertical) g.fillRect(3, p0, RULER - 6, p1 - p0);
      else g.fillRect(p0, 3, p1 - p0, RULER - 6);
    };
    band(0, slideSize, page);
    if (span) band(span[0], span[1], css.getPropertyValue('--kiln-panel').trim() || '#f7f1ea');
    const { step, minor } = tickPlan(unit);
    const mid = slideSize / 2;
    g.strokeStyle = fg;
    g.fillStyle = fg;
    g.lineWidth = 1;
    g.font = '9px system-ui, sans-serif';
    g.textAlign = 'center';
    g.textBaseline = 'middle';
    const reach = Math.ceil(slideSize / 2 / step) + 1;
    for (let k = -reach * minor; k <= reach * minor; k++) {
      const p = Math.round(at(mid + (k * step) / minor)) + 0.5;
      if (p < 0 || p > length) continue;
      const major = k % minor === 0;
      const half = minor % 2 === 0 && k % (minor / 2) === 0;
      const len = major ? 0 : half ? 6 : 3;
      if (major) {
        const label = String(Math.abs(k / minor));
        if (vertical) {
          g.save();
          g.translate(RULER / 2, p);
          g.rotate(-Math.PI / 2);
          g.fillText(label, 0, 0);
          g.restore();
        } else g.fillText(label, p, RULER / 2);
        continue;
      }
      g.beginPath();
      if (vertical) {
        g.moveTo(RULER / 2 - len / 2, p);
        g.lineTo(RULER / 2 + len / 2, p);
      } else {
        g.moveTo(p, RULER / 2 - len / 2);
        g.lineTo(p, RULER / 2 + len / 2);
      }
      g.stroke();
    }
  }, [vertical, view, length, span, slideSize, unit]);
  return (
    <canvas
      ref={ref}
      className={vertical ? 'kiln-deckruler__v' : 'kiln-deckruler__h'}
      style={vertical ? { width: RULER, height: length } : { width: length, height: RULER }}
    />
  );
}

/** The horizontal ruler's indent markers for the paragraph being edited. */
function IndentMarkers({ model, view }: { model: DeckEditorModel; view: View }) {
  const area = useDeck(model, (s) => s.deck?.text_area ?? null);
  const left = useDeck(model, (s) => s.deck?.format?.indent_left ?? 0);
  const first = useDeck(model, (s) => s.deck?.format?.indent_first ?? 0);
  const [drag, setDrag] = useState<{ which: 'first' | 'hanging' | 'left'; dx: number } | null>(null);
  const start = useRef(0);
  if (!area) return null;
  // Points → slide px.
  const pt = 4 / 3;
  const x = (slidePx: number) => view.left + slidePx * view.zoom - RULER;
  const dxPt = drag ? drag.dx / view.zoom / pt : 0;
  const firstPt = Math.max(
    0,
    left + first + (drag?.which === 'first' ? dxPt : drag?.which === 'left' ? dxPt : 0),
  );
  const leftPt = Math.max(0, left + (drag && drag.which !== 'first' ? dxPt : 0));
  const markerAt = (p: number) => x(area.x + p * pt);
  const begin = (which: 'first' | 'hanging' | 'left') => (e: PointerEvent) => {
    e.preventDefault();
    e.stopPropagation();
    (e.currentTarget as Element).setPointerCapture(e.pointerId);
    start.current = e.clientX;
    setDrag({ which, dx: 0 });
  };
  const move = (e: PointerEvent) => {
    if (drag) setDrag({ ...drag, dx: e.clientX - start.current });
  };
  const end = () => {
    if (!drag) return;
    setDrag(null);
    // The first line keeps its place when the hanging marker moves the left indent.
    const newLeft = leftPt;
    const newFirst = drag.which === 'hanging' ? left + first - newLeft : firstPt - newLeft;
    void model.core('deck.paragraph.indent', {
      left: Math.round(newLeft * 2) / 2,
      first_line: Math.round(newFirst * 2) / 2,
    });
  };
  return (
    <div className="kiln-deckruler__indents" onPointerMove={move} onPointerUp={end}>
      <span
        className="kiln-deckruler__marker kiln-deckruler__marker--first"
        style={{ left: markerAt(firstPt) }}
        title={t('First Line Indent')}
        data-testid="deck-ruler-first"
        onPointerDown={begin('first')}
      />
      <span
        className="kiln-deckruler__marker kiln-deckruler__marker--hanging"
        style={{ left: markerAt(leftPt) }}
        title={t('Hanging Indent')}
        data-testid="deck-ruler-hanging"
        onPointerDown={begin('hanging')}
      />
      <span
        className="kiln-deckruler__marker kiln-deckruler__marker--left"
        style={{ left: markerAt(leftPt) }}
        title={t('Left Indent')}
        data-testid="deck-ruler-left"
        onPointerDown={begin('left')}
      />
    </div>
  );
}

/** View ▸ Ruler: the horizontal and vertical rulers along the stage's edges. */
export function Rulers({ model }: { model: DeckEditorModel }) {
  const on = useDeck(model, (s) => s.ruler);
  const unit = useMeasurementUnit() === 'in' ? 'in' : 'cm';
  const view = useStageView(model, on);
  const box = useDeck(model, (s) => s.deck?.selection_box ?? null);
  const slideW = useDeck(model, (s) => s.deck?.slide_width ?? 1280);
  const slideH = useDeck(model, (s) => s.deck?.slide_height ?? 720);
  const root = useRef<HTMLDivElement>(null);
  const [size, setSize] = useState({ w: 0, h: 0 });
  useLayoutEffect(() => {
    const el = root.current?.parentElement;
    if (!el || !on) return;
    const fit = () => setSize({ w: el.clientWidth, h: el.clientHeight });
    fit();
    const ro = new ResizeObserver(fit);
    ro.observe(el);
    return () => ro.disconnect();
  }, [on]);
  if (!on) return <div ref={root} hidden />;
  // The selection box is in points; the rulers work in slide px.
  const k = 4 / 3;
  return (
    <div ref={root} className="kiln-deckruler" data-testid="deck-ruler" aria-hidden="true">
      {view ? (
        <>
          <div className="kiln-deckruler__top">
            <RulerCanvas
              vertical={false}
              view={view}
              length={Math.max(0, size.w - RULER)}
              span={box ? [box.x * k, (box.x + box.width) * k] : null}
              slideSize={slideW}
              unit={unit}
            />
            <IndentMarkers model={model} view={view} />
          </div>
          <div className="kiln-deckruler__left">
            <RulerCanvas
              vertical
              view={view}
              length={Math.max(0, size.h - RULER)}
              span={box ? [box.y * k, (box.y + box.height) * k] : null}
              slideSize={slideH}
              unit={unit}
            />
          </div>
          <div className="kiln-deckruler__corner" />
        </>
      ) : null}
    </div>
  );
}

/** The drawing guides' handles: a strip on each guide that drags it. */
export function GuideHandles({ model }: { model: DeckEditorModel }) {
  const shown = useDeck(model, (s) => (s.deck?.guides ?? false) && !s.deck?.master_view);
  const guides = useDeck(model, (s) => s.deck?.guide_list ?? NONE);
  const slideW = useDeck(model, (s) => s.deck?.slide_width ?? 1280);
  const slideH = useDeck(model, (s) => s.deck?.slide_height ?? 720);
  const unit = useMeasurementUnit() === 'in' ? 'in' : 'cm';
  const view = useStageView(model, shown);
  const [drag, setDrag] = useState<{ index: number; pos: number; copy: boolean } | null>(null);
  if (!shown || !view) return null;
  const toSlide = (e: PointerEvent, horizontal: boolean, el: Element) => {
    const r = el.parentElement?.getBoundingClientRect();
    if (!r) return 0;
    return horizontal
      ? (e.clientY - r.top - view.top) / view.zoom
      : (e.clientX - r.left - view.left) / view.zoom;
  };
  return (
    <div className="kiln-deckguides">
      {guides.map((g, index) => {
        const pos = drag?.index === index ? drag.pos : g.pos;
        const size = g.horizontal ? slideH : slideW;
        const style = g.horizontal
          ? { top: view.top + pos * view.zoom - 4, left: view.left, width: view.width }
          : { left: view.left + pos * view.zoom - 4, top: view.top, height: view.height };
        const off = pos < -8 / view.zoom || pos > size + 8 / view.zoom;
        return (
          // biome-ignore lint/a11y/useSemanticElements: a focusable separator is a movable splitter, which an hr cannot be
          <div
            key={index}
            className="kiln-deckguide"
            data-horizontal={g.horizontal || undefined}
            data-off={(drag?.index === index && off) || undefined}
            data-testid="deck-guide"
            style={style}
            // From the keyboard too: the arrows move a guide (Shift for a larger step), Delete
            // removes it, as dragging it along or off the slide does.
            tabIndex={0}
            role="separator"
            aria-orientation={g.horizontal ? 'horizontal' : 'vertical'}
            aria-label={
              g.horizontal
                ? t('Horizontal guide, {distance} from the middle', {
                    distance: fromMiddle(g.pos - size / 2, unit),
                  })
                : t('Vertical guide, {distance} from the middle', {
                    distance: fromMiddle(g.pos - size / 2, unit),
                  })
            }
            aria-valuenow={Math.round(g.pos)}
            aria-valuemin={0}
            aria-valuemax={Math.round(size)}
            onKeyDown={(e) => {
              const back = g.horizontal ? 'ArrowUp' : 'ArrowLeft';
              const fwd = g.horizontal ? 'ArrowDown' : 'ArrowRight';
              if (e.key === 'Delete' || e.key === 'Backspace') {
                e.preventDefault();
                e.stopPropagation();
                void model.core('deck.view.remove_guide', { index });
              } else if (e.key === back || e.key === fwd) {
                e.preventDefault();
                e.stopPropagation();
                const step = (e.shiftKey ? 10 : 1) * (tickPlan(unit).step / tickPlan(unit).minor);
                const to = Math.max(0, Math.min(size, g.pos + (e.key === fwd ? step : -step)));
                void model.core('deck.view.move_guide', { index, pos: to });
              }
            }}
            onPointerDown={(e) => {
              if (e.button !== 0) return;
              e.preventDefault();
              e.stopPropagation();
              e.currentTarget.setPointerCapture(e.pointerId);
              setDrag({ index, pos: g.pos, copy: e.ctrlKey || e.metaKey });
            }}
            onPointerMove={(e) => {
              if (drag?.index !== index) return;
              setDrag({ ...drag, pos: toSlide(e, g.horizontal, e.currentTarget) });
            }}
            onPointerUp={() => {
              if (drag?.index !== index) return;
              setDrag(null);
              if (off) void model.core('deck.view.remove_guide', { index });
              else if (drag.copy)
                void model.core('deck.view.add_guide', { horizontal: g.horizontal, pos: drag.pos });
              else void model.core('deck.view.move_guide', { index, pos: drag.pos });
            }}
          >
            {drag?.index === index ? (
              <span className="kiln-deckguide__label">
                {off ? t('Remove') : fromMiddle(drag.pos - size / 2, unit)}
              </span>
            ) : null}
          </div>
        );
      })}
    </div>
  );
}

const NONE: readonly { horizontal: boolean; pos: number }[] = [];
