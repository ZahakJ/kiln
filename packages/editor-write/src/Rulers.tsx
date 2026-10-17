/**
 * Word's rulers, drawn on two small canvases beside the surface from what the core reports
 * (`Query::Write(Rulers)`: page and text area, columns, indents, tab stops, margins) and the
 * surface's scroll and zoom. Dragging a marker runs the core's commands: the indent markers set the
 * Paragraph dialog's indents, a click adds a left tab stop and dragging one off removes it, and the
 * margin edges set the page margins (PLAN.md §1.3, parity row `write.view.show.ruler`).
 */
import type { RulerInfo, TabAlign, TabStopSpec } from '@kiln/protocol';
import { type PointerEvent as ReactPointerEvent, useEffect, useRef, useState } from 'react';
import { cssVar, PX_PER_PT, usesInches, useThemeKey, useWrite } from './hooks.ts';
import type { SurfaceView, WriteEditorModel } from './model.ts';

export const RULER_SIZE = 22;

type Marker = 'first' | 'hanging' | 'left' | 'right' | 'marginLeft' | 'marginRight' | { tab: number };

interface Drag {
  marker: Marker;
  startX: number;
  /** Current document x (or y for the vertical ruler). */
  value: number;
  off: boolean;
}

/** Screen x of a document x. */
const sx = (v: SurfaceView, x: number) => (x - v.scrollX) * v.zoom;
const sy = (v: SurfaceView, y: number) => (y - v.scrollY) * v.zoom;

function colors(el: Element | null) {
  return {
    track: cssVar(el, '--kiln-sunken', '#EDE6DC'),
    page: cssVar(el, '--kiln-page', '#FFFFFF'),
    tick: cssVar(el, '--kiln-muted', '#7A6E66'),
    text: cssVar(el, '--kiln-fg', '#2B2420'),
    accent: cssVar(el, '--kiln-accent', '#3159C6'),
    line: cssVar(el, '--kiln-line-strong', '#D9CFC4'),
  };
}

function prepare(canvas: HTMLCanvasElement, w: number, h: number): CanvasRenderingContext2D | null {
  const dpr = typeof devicePixelRatio === 'number' && devicePixelRatio > 0 ? devicePixelRatio : 1;
  if (canvas.width !== Math.round(w * dpr) || canvas.height !== Math.round(h * dpr)) {
    canvas.width = Math.round(w * dpr);
    canvas.height = Math.round(h * dpr);
    canvas.style.width = `${w}px`;
    canvas.style.height = `${h}px`;
  }
  const ctx = canvas.getContext('2d');
  if (!ctx) return null;
  ctx.setTransform(dpr, 0, 0, dpr, 0, 0);
  ctx.clearRect(0, 0, w, h);
  return ctx;
}

/** Tick spacing in document px and labels every `major` ticks. */
function scale(inches: boolean): { unit: number; minor: number } {
  return inches ? { unit: 96, minor: 8 } : { unit: 96 / 2.54, minor: 4 };
}

function marks(r: RulerInfo) {
  const left = r.text_rect.x + r.indent_left;
  return {
    left,
    first: left + r.first_line,
    right: r.text_rect.x + r.text_rect.width - r.indent_right,
  };
}

function hitHorizontal(r: RulerInfo, v: SurfaceView, x: number, y: number): Marker | null {
  const m = marks(r);
  const near = (dx: number) => Math.abs(sx(v, dx) - x) <= 6;
  if (y < RULER_SIZE / 2 && near(m.first)) return 'first';
  if (y >= RULER_SIZE / 2 && near(m.left)) return y > RULER_SIZE - 7 ? 'left' : 'hanging';
  if (y >= RULER_SIZE / 2 && near(m.right)) return 'right';
  for (let i = 0; i < r.tabs.length; i++) {
    const t = r.tabs[i];
    if (t && near(r.text_rect.x + t.pos)) return { tab: i };
  }
  if (near(r.text_rect.x)) return 'marginLeft';
  if (near(r.text_rect.x + r.text_rect.width)) return 'marginRight';
  return null;
}

function drawTab(ctx: CanvasRenderingContext2D, x: number, align: TabAlign, color: string) {
  ctx.strokeStyle = color;
  ctx.lineWidth = 1.5;
  const y = RULER_SIZE - 5;
  ctx.beginPath();
  ctx.moveTo(x, y - 6);
  ctx.lineTo(x, y);
  if (align === 'Left') ctx.lineTo(x + 5, y);
  else if (align === 'Right') ctx.lineTo(x - 5, y);
  else {
    ctx.moveTo(x - 4, y);
    ctx.lineTo(x + 4, y);
  }
  ctx.stroke();
  if (align === 'Decimal') {
    ctx.fillStyle = color;
    ctx.fillRect(x + 2, y - 4, 1.5, 1.5);
  }
}

function drawHorizontal(
  canvas: HTMLCanvasElement,
  width: number,
  r: RulerInfo | null,
  v: SurfaceView | null,
  drag: Drag | null,
) {
  const ctx = prepare(canvas, width, RULER_SIZE);
  if (!ctx || !r || !v) return;
  const c = colors(canvas);
  const page = { x: sx(v, r.page_rect.x), w: r.page_rect.width * v.zoom };
  const text = { x: sx(v, r.text_rect.x), w: r.text_rect.width * v.zoom };
  const top = 3;
  const h = RULER_SIZE - 6;
  // The track: margins sunken, the text area as paper, rounded like the mockup.
  ctx.fillStyle = c.track;
  roundRect(ctx, page.x, top, page.w, h, 4);
  ctx.fill();
  ctx.fillStyle = c.page;
  ctx.fillRect(text.x, top, text.w, h);
  for (const col of r.columns) {
    const cx = sx(v, col.x);
    ctx.fillRect(cx, top, col.width * v.zoom, h);
  }
  // Ticks from the left margin, both ways.
  const inches = usesInches();
  const { unit, minor } = scale(inches);
  ctx.fillStyle = c.tick;
  ctx.font = `600 9px ${cssVar(canvas, '--kiln-font-ui', 'sans-serif')}`;
  ctx.textAlign = 'center';
  ctx.textBaseline = 'middle';
  const step = unit / minor;
  const from = Math.floor((r.page_rect.x - r.text_rect.x) / step);
  const to = Math.ceil((r.page_rect.x + r.page_rect.width - r.text_rect.x) / step);
  for (let i = from; i <= to; i++) {
    const x = sx(v, r.text_rect.x + i * step);
    if (x < page.x + 2 || x > page.x + page.w - 2) continue;
    if (i % minor === 0) {
      if (i !== 0) ctx.fillText(String(Math.abs(i / minor)), x, top + h / 2);
    } else {
      const len = i % (minor / 2) === 0 ? 5 : 2.5;
      ctx.fillRect(x - 0.5, top + h / 2 - len / 2, 1, len);
    }
  }
  // Tab stops.
  r.tabs.forEach((t, i) => {
    const dragging = drag && typeof drag.marker === 'object' && drag.marker.tab === i;
    if (dragging && drag.off) return;
    const x = dragging ? sx(v, drag.value) : sx(v, r.text_rect.x + t.pos);
    drawTab(ctx, x, t.align, c.text);
  });
  // Indent markers (the dragged one follows the pointer).
  const m = marks(r);
  const at = (k: Marker, x: number) => (drag && drag.marker === k ? sx(v, drag.value) : sx(v, x));
  ctx.fillStyle = c.accent;
  const tri = (x: number, y: number, down: boolean) => {
    ctx.beginPath();
    ctx.moveTo(x - 4.5, y);
    ctx.lineTo(x + 4.5, y);
    ctx.lineTo(x, down ? y + 5 : y - 5);
    ctx.closePath();
    ctx.fill();
  };
  tri(at('first', m.first), 1, true);
  const leftX = drag && drag.marker === 'left' ? sx(v, drag.value) : at('hanging', m.left);
  tri(leftX, RULER_SIZE - 6, false);
  ctx.fillRect(leftX - 4.5, RULER_SIZE - 5, 9, 4);
  tri(at('right', m.right), RULER_SIZE - 6, false);
  // A guide line while dragging.
  if (drag && !drag.off) {
    ctx.fillStyle = c.accent;
    ctx.fillRect(sx(v, drag.value) - 0.5, 0, 1, RULER_SIZE);
  }
}

function drawVertical(
  canvas: HTMLCanvasElement,
  height: number,
  r: RulerInfo | null,
  v: SurfaceView | null,
  drag: Drag | null,
) {
  const ctx = prepare(canvas, RULER_SIZE, height);
  if (!ctx || !r || !v) return;
  const c = colors(canvas);
  const left = 3;
  const w = RULER_SIZE - 6;
  const pageTop = sy(v, r.page_rect.y);
  const pageH = r.page_rect.height * v.zoom;
  const top = drag?.marker === 'marginLeft' ? sy(v, drag.value) : sy(v, r.page_rect.y + r.margin_top);
  const bottom = drag?.marker === 'marginRight' ? sy(v, drag.value) : sy(v, r.page_rect.y + r.margin_bottom);
  ctx.fillStyle = c.track;
  roundRect(ctx, left, pageTop, w, pageH, 4);
  ctx.fill();
  ctx.fillStyle = c.page;
  ctx.fillRect(left, top, w, Math.max(0, bottom - top));
  const inches = usesInches();
  const { unit, minor } = scale(inches);
  const step = unit / minor;
  ctx.fillStyle = c.tick;
  ctx.font = `600 9px ${cssVar(canvas, '--kiln-font-ui', 'sans-serif')}`;
  ctx.textAlign = 'center';
  ctx.textBaseline = 'middle';
  const origin = r.page_rect.y + r.margin_top;
  const from = Math.floor(-r.margin_top / step);
  const to = Math.ceil((r.page_rect.height - r.margin_top) / step);
  for (let i = from; i <= to; i++) {
    const y = sy(v, origin + i * step);
    if (y < pageTop + 2 || y > pageTop + pageH - 2) continue;
    if (i % minor === 0) {
      if (i !== 0) ctx.fillText(String(Math.abs(i / minor)), left + w / 2, y);
    } else {
      const len = i % (minor / 2) === 0 ? 5 : 2.5;
      ctx.fillRect(left + w / 2 - len / 2, y - 0.5, len, 1);
    }
  }
}

function roundRect(ctx: CanvasRenderingContext2D, x: number, y: number, w: number, h: number, r: number) {
  ctx.beginPath();
  if (typeof ctx.roundRect === 'function') ctx.roundRect(x, y, w, h, r);
  else ctx.rect(x, y, w, h);
}

/** The ruler above the surface. */
export function HorizontalRuler({ model, width }: { model: WriteEditorModel; width: number }) {
  const canvas = useRef<HTMLCanvasElement>(null);
  const rulers = useWrite(model, (s) => s.rulers);
  const view = useWrite(model, (s) => s.view);
  const [drag, setDrag] = useState<Drag | null>(null);
  // The colours are read from the theme when drawing: a theme change repaints.
  const theme = useThemeKey();

  useEffect(() => {
    void theme;
    if (canvas.current) drawHorizontal(canvas.current, width, rulers, view, drag);
  }, [width, rulers, view, drag, theme]);

  const docX = (e: ReactPointerEvent) => {
    const rect = canvas.current?.getBoundingClientRect();
    const v = model.state.view;
    if (!rect || !v) return 0;
    return v.scrollX + (e.clientX - rect.left) / v.zoom;
  };

  const onDown = (e: ReactPointerEvent<HTMLCanvasElement>) => {
    const r = model.state.rulers;
    const v = model.state.view;
    const rect = canvas.current?.getBoundingClientRect();
    if (!r || !v || !rect || e.button !== 0) return;
    const marker = hitHorizontal(r, v, e.clientX - rect.left, e.clientY - rect.top);
    if (!marker) {
      // A click in the text area adds a left tab stop there.
      const x = docX(e) - r.text_rect.x;
      if (x > 0 && x < r.text_rect.width) {
        const tabs: TabStopSpec[] = [
          ...r.tabs.map((t) => ({ pos: t.pos / PX_PER_PT, align: t.align, leader: 'None' as const })),
          { pos: Math.round((x / PX_PER_PT) * 4) / 4, align: 'Left', leader: 'None' },
        ];
        void model.run('write.format.tabs', { tabs });
      }
      return;
    }
    e.currentTarget.setPointerCapture?.(e.pointerId);
    setDrag({ marker, startX: e.clientX, value: docX(e), off: false });
  };

  const onMove = (e: ReactPointerEvent<HTMLCanvasElement>) => {
    if (!drag) {
      const r = model.state.rulers;
      const v = model.state.view;
      const rect = canvas.current?.getBoundingClientRect();
      if (r && v && rect && canvas.current) {
        const hit = hitHorizontal(r, v, e.clientX - rect.left, e.clientY - rect.top);
        canvas.current.style.cursor =
          hit === 'marginLeft' || hit === 'marginRight' ? 'col-resize' : hit ? 'grab' : 'copy';
      }
      return;
    }
    const rect = canvas.current?.getBoundingClientRect();
    const off =
      !!rect && typeof drag.marker === 'object' && Math.abs(e.clientY - (rect.top + RULER_SIZE / 2)) > 28;
    setDrag({ ...drag, value: docX(e), off });
  };

  const onUp = () => {
    const r = model.state.rulers;
    if (!drag || !r) {
      setDrag(null);
      return;
    }
    const d = drag;
    setDrag(null);
    if (Math.abs(d.value - docX({ clientX: d.startX } as ReactPointerEvent)) < 0.5) return;
    const m = marks(r);
    const pt = (px: number) => Math.round((px / PX_PER_PT) * 4) / 4;
    const textLeft = r.text_rect.x;
    if (typeof d.marker === 'object') {
      const i = d.marker.tab;
      const tabs: TabStopSpec[] = r.tabs
        .map((t, k) => ({
          pos: k === i ? pt(d.value - textLeft) : t.pos / PX_PER_PT,
          align: t.align,
          leader: 'None' as const,
        }))
        .filter((_, k) => !(k === i && d.off));
      void model.run('write.format.tabs', { tabs });
      return;
    }
    switch (d.marker) {
      case 'first':
        void model.run('write.format.paragraph', { format: { first_line: pt(d.value - m.left) } });
        break;
      case 'hanging':
        // The left indent moves, the first line stays where it was.
        void model.run('write.format.paragraph', {
          format: { indent_left: pt(d.value - textLeft), first_line: pt(m.first - d.value) },
        });
        break;
      case 'left':
        void model.run('write.format.paragraph', { format: { indent_left: pt(d.value - textLeft) } });
        break;
      case 'right':
        void model.run('write.format.paragraph', {
          format: { indent_right: pt(textLeft + r.text_rect.width - d.value) },
        });
        break;
      case 'marginLeft':
        void model.run('write.layout.page_setup', { setup: { left: pt(d.value - r.page_rect.x) } });
        break;
      case 'marginRight':
        void model.run('write.layout.page_setup', {
          setup: { right: pt(r.page_rect.x + r.page_rect.width - d.value) },
        });
        break;
    }
  };

  return (
    // biome-ignore lint/a11y/noAriaHiddenOnFocusable: a pointer shortcut only (as in Word); its keyboard equivalents are the Paragraph and Tabs dialogs, so assistive technology skips the drawing
    <canvas
      ref={canvas}
      className="kiln-write-ruler kiln-write-ruler--h"
      data-testid="write-ruler"
      aria-hidden="true"
      onPointerDown={onDown}
      onPointerMove={onMove}
      onPointerUp={onUp}
      onPointerCancel={() => setDrag(null)}
      onDoubleClick={() => model.openDialog('tabs')}
    />
  );
}

/** The ruler left of the surface (the caret page's margins). */
export function VerticalRuler({ model, height }: { model: WriteEditorModel; height: number }) {
  const canvas = useRef<HTMLCanvasElement>(null);
  const rulers = useWrite(model, (s) => s.rulers);
  const view = useWrite(model, (s) => s.view);
  const [drag, setDrag] = useState<Drag | null>(null);
  // The colours are read from the theme when drawing: a theme change repaints.
  const theme = useThemeKey();

  useEffect(() => {
    void theme;
    if (canvas.current) drawVertical(canvas.current, height, rulers, view, drag);
  }, [height, rulers, view, drag, theme]);

  const docY = (clientY: number) => {
    const rect = canvas.current?.getBoundingClientRect();
    const v = model.state.view;
    if (!rect || !v) return 0;
    return v.scrollY + (clientY - rect.top) / v.zoom;
  };

  const hit = (clientY: number): Marker | null => {
    const r = model.state.rulers;
    const v = model.state.view;
    const rect = canvas.current?.getBoundingClientRect();
    if (!r || !v || !rect) return null;
    const y = clientY - rect.top;
    if (Math.abs(sy(v, r.page_rect.y + r.margin_top) - y) <= 5) return 'marginLeft';
    if (Math.abs(sy(v, r.page_rect.y + r.margin_bottom) - y) <= 5) return 'marginRight';
    return null;
  };

  return (
    // biome-ignore lint/a11y/noAriaHiddenOnFocusable: a pointer shortcut only (as in Word); its keyboard equivalent is the Page Setup dialog
    <canvas
      ref={canvas}
      className="kiln-write-ruler kiln-write-ruler--v"
      data-testid="write-ruler-vertical"
      aria-hidden="true"
      onPointerDown={(e) => {
        const marker = hit(e.clientY);
        if (!marker || e.button !== 0) return;
        e.currentTarget.setPointerCapture?.(e.pointerId);
        setDrag({ marker, startX: e.clientY, value: docY(e.clientY), off: false });
      }}
      onPointerMove={(e) => {
        if (drag) setDrag({ ...drag, value: docY(e.clientY) });
        else if (canvas.current) canvas.current.style.cursor = hit(e.clientY) ? 'row-resize' : 'default';
      }}
      onPointerUp={() => {
        const r = model.state.rulers;
        const d = drag;
        setDrag(null);
        if (!d || !r || Math.abs(d.value - docY(d.startX)) < 0.5) return;
        const pt = (px: number) => Math.round((px / PX_PER_PT) * 4) / 4;
        if (d.marker === 'marginLeft')
          void model.run('write.layout.page_setup', { setup: { top: pt(d.value - r.page_rect.y) } });
        else
          void model.run('write.layout.page_setup', {
            setup: { bottom: pt(r.page_rect.y + r.page_rect.height - d.value) },
          });
      }}
      onPointerCancel={() => setDrag(null)}
    />
  );
}
