/**
 * Page Layout view's cells on a printed page: a click selects the cell under it (Shift
 * extends, Ctrl adds, a drag selects a range), a double-click edits it, and typing starts an
 * entry, all through the sheet's own commands and input (the keys stay on the sheet's input
 * proxy). The selection and the entry are drawn over the page where File ▸ Print puts those
 * cells; the core says where that is (`GridQuery::PageMap`).
 */
import { t } from '@kiln/i18n';
import type { GridPageBand, GridPageMap, GridState } from '@kiln/protocol';
import { useEffect, useRef, useState } from 'react';
import { useGrid } from '../hooks.ts';
import type { GridEditorModel } from '../model.ts';

interface CellRect {
  x: number;
  y: number;
  w: number;
  h: number;
}

interface Range {
  r0: number;
  c0: number;
  r1: number;
  c1: number;
}

/** The cell at a page point (page px), searching the page's own cells last-drawn first. */
export function cellAt(map: GridPageMap, x: number, y: number): { row: number; col: number } | null {
  for (const b of [...map.bands].reverse()) {
    const row = b.rows.find((r) => y >= b.y + r.at && y < b.y + r.at + r.size);
    const col = b.cols.find((c) => x >= b.x + c.at && x < b.x + c.at + c.size);
    if (row && col) return { row: row.index, col: col.index };
  }
  return null;
}

/** The rectangle of a range's part on one band (page px), or null when none of it is there. */
function bandRect(b: GridPageBand, r: Range): CellRect | null {
  const rows = b.rows.filter((x) => x.index >= r.r0 && x.index <= r.r1);
  const cols = b.cols.filter((x) => x.index >= r.c0 && x.index <= r.c1);
  const r0 = rows[0];
  const r1 = rows.at(-1);
  const c0 = cols[0];
  const c1 = cols.at(-1);
  if (!r0 || !r1 || !c0 || !c1) return null;
  return {
    x: b.x + c0.at,
    y: b.y + r0.at,
    w: c1.at + c1.size - c0.at,
    h: r1.at + r1.size - r0.at,
  };
}

/** The range's rectangles on a page (one per band it reaches). */
export function rangeRects(map: GridPageMap, r: Range): CellRect[] {
  return map.bands.flatMap((b) => {
    const rect = bandRect(b, r);
    return rect ? [rect] : [];
  });
}

async function selection(
  model: GridEditorModel,
): Promise<{ ranges: Range[]; active: { row: number; col: number } }> {
  try {
    const r = (await model.client.query(model.doc, 'Selection')) as {
      Selection?: {
        kind: {
          Cells?: {
            ranges: { start: { row: number; col: number }; end: { row: number; col: number } }[];
            active: { row: number; col: number };
          };
        };
      };
    };
    const c = r.Selection?.kind.Cells;
    if (!c) return { ranges: [], active: { row: 0, col: 0 } };
    return {
      ranges: c.ranges.map((x) => ({
        r0: Math.min(x.start.row, x.end.row),
        c0: Math.min(x.start.col, x.end.col),
        r1: Math.max(x.start.row, x.end.row),
        c1: Math.max(x.start.col, x.end.col),
      })),
      active: { row: c.active.row, col: c.active.col },
    };
  } catch {
    return { ranges: [], active: { row: 0, col: 0 } };
  }
}

/** The entry being typed with its caret (the sheet's input proxy has the keys). */
function Entry({ grid }: { grid: GridState }) {
  const ed = grid.editor;
  if (!ed) return null;
  const at = Math.min(ed.start, ed.text.length);
  return (
    <span className="kiln-pagelayout__entry-text">
      {ed.text.slice(0, at)}
      <span className="kiln-pagelayout__caret" aria-hidden="true" />
      {ed.text.slice(at)}
    </span>
  );
}

/** How far the paper's edges are inside the page's box (its preview's margin), CSS px. */
export interface PaperInset {
  top: number;
  right: number;
  bottom: number;
  left: number;
}

export function PageLayoutCells({
  model,
  page,
  revision,
  extent,
  onPaper,
}: {
  model: GridEditorModel;
  page: number;
  /** The preview scene's size (the printed page and the margin drawn round it), page px. */
  extent: { w: number; h: number };
  /** The workbook's revision: the cells move when rows, columns or the page setup change. */
  revision: number;
  /** Told where the paper is in the page's box (the view frames the paper alone). */
  onPaper?: (inset: PaperInset) => void;
}) {
  const grid = useGrid(model, (s) => s.grid);
  const [map, setMap] = useState<GridPageMap | null>(null);
  const [sel, setSel] = useState<{ ranges: Range[]; active: { row: number; col: number } } | null>(null);
  const layer = useRef<HTMLDivElement>(null);
  const dragging = useRef(false);
  // Where the printed page is drawn in this layer (the preview's canvas) and at what scale.
  const [frame, setFrame] = useState<{ x: number; y: number; kx: number; ky: number } | null>(null);
  useEffect(() => {
    const el = layer.current;
    const canvas = el?.parentElement?.querySelector('canvas');
    if (!el || !canvas || !map) return;
    const measure = () => {
      const a = el.getBoundingClientRect();
      const c = canvas.getBoundingClientRect();
      if (c.width <= 0 || c.height <= 0) return;
      // The canvas shows the scene: the page centred in its margin.
      const kx = c.width / Math.max(extent.w, map.width);
      const ky = c.height / Math.max(extent.h, map.height);
      const x = c.left - a.left + ((extent.w - map.width) / 2) * kx;
      const y = c.top - a.top + ((extent.h - map.height) / 2) * ky;
      setFrame({ x, y, kx, ky });
      onPaper?.({
        top: y,
        left: x,
        right: Math.max(0, a.width - x - map.width * kx),
        bottom: Math.max(0, a.height - y - map.height * ky),
      });
    };
    measure();
    if (typeof ResizeObserver === 'undefined') return;
    const ro = new ResizeObserver(measure);
    ro.observe(el);
    ro.observe(canvas);
    return () => ro.disconnect();
  }, [map, extent.w, extent.h, onPaper]);
  useEffect(() => {
    let live = true;
    void model.query({ PageMap: { page } }).then((r) => {
      if (live && r && 'PageMap' in r) setMap(r.PageMap ?? null);
    });
    return () => {
      live = false;
    };
  }, [model, page, revision]);
  useEffect(() => {
    let live = true;
    void selection(model).then((s) => {
      if (live) setSel(s);
    });
    return () => {
      live = false;
    };
  }, [model, grid]);
  if (!map) return null;
  /** A pointer event's point in page px. */
  const point = (e: { clientX: number; clientY: number }) => {
    const el = layer.current;
    if (!el || !frame) return null;
    const r = el.getBoundingClientRect();
    return { x: (e.clientX - r.left - frame.x) / frame.kx, y: (e.clientY - r.top - frame.y) / frame.ky };
  };
  const f = frame ?? { x: 0, y: 0, kx: 1, ky: 1 };
  const px = (r: CellRect) => ({
    left: f.x + r.x * f.kx,
    top: f.y + r.y * f.ky,
    width: r.w * f.kx,
    height: r.h * f.ky,
  });
  const active = sel
    ? rangeRects(map, { r0: sel.active.row, c0: sel.active.col, r1: sel.active.row, c1: sel.active.col })
    : [];
  const editing =
    grid?.mode !== undefined && grid.mode !== 'Ready' && !!grid.editor && !grid.editor.formula_bar;
  // Excel's faint gridlines on the page (they do not print unless Print ▸ Gridlines is on).
  const lines =
    frame && grid?.gridlines !== false
      ? map.bands.flatMap((b, bi) => {
          const first = b.rows[0];
          const last = b.rows.at(-1);
          const left = b.cols[0];
          const right = b.cols.at(-1);
          if (!first || !last || !left || !right) return [];
          const x0 = f.x + (b.x + left.at) * f.kx;
          const x1 = f.x + (b.x + right.at + right.size) * f.kx;
          const y0 = f.y + (b.y + first.at) * f.ky;
          const y1 = f.y + (b.y + last.at + last.size) * f.ky;
          const snap = (v: number) => Math.round(v) + 0.5;
          return [
            ...[first.at, ...b.rows.map((r) => r.at + r.size)].map((at, i) => {
              const y = snap(f.y + (b.y + at) * f.ky);
              return <line key={`h${bi}-${i}`} x1={x0} x2={x1} y1={y} y2={y} />;
            }),
            ...[left.at, ...b.cols.map((c) => c.at + c.size)].map((at, i) => {
              const x = snap(f.x + (b.x + at) * f.kx);
              return <line key={`v${bi}-${i}`} x1={x} x2={x} y1={y0} y2={y1} />;
            }),
          ];
        })
      : [];
  return (
    // biome-ignore lint/a11y/noStaticElementInteractions: the pointer selects cells on the printed page; the keys stay on the sheet's input proxy.
    <div
      ref={layer}
      className="kiln-pagelayout__cells"
      data-testid="grid-page-cells"
      data-page={page}
      onPointerDown={(e) => {
        if (e.button !== 0) return;
        const p = point(e);
        const cell = p && cellAt(map, p.x, p.y);
        if (!cell) return;
        e.preventDefault();
        dragging.current = true;
        e.currentTarget.setPointerCapture?.(e.pointerId);
        const args = { row: cell.row, col: cell.col, extend: e.shiftKey, add: e.ctrlKey || e.metaKey };
        // A click elsewhere commits the entry first, as on the sheet.
        void (editing ? model.core('grid.editing.commit', { advance: null }) : Promise.resolve(true)).then(
          () => model.core('grid.nav.select_cell', args),
        );
        model.surface?.focus();
      }}
      onPointerMove={(e) => {
        if (!dragging.current) return;
        const p = point(e);
        const cell = p && cellAt(map, p.x, p.y);
        if (cell)
          void model.core('grid.nav.select_cell', { row: cell.row, col: cell.col, extend: true, add: false });
      }}
      onPointerUp={(e) => {
        dragging.current = false;
        e.currentTarget.releasePointerCapture?.(e.pointerId);
        model.surface?.focus();
      }}
      onDoubleClick={(e) => {
        const p = point(e);
        if (p && cellAt(map, p.x, p.y)) {
          void model.core('grid.editing.edit_cell');
          model.surface?.focus();
        }
      }}
    >
      {lines.length > 0 ? (
        <svg className="kiln-pagelayout__grid" aria-hidden="true" data-testid="grid-page-gridlines">
          {lines}
        </svg>
      ) : null}
      {sel?.ranges.flatMap((r, i) =>
        rangeRects(map, r)
          .filter(() => r.r0 !== r.r1 || r.c0 !== r.c1)
          .map((rect, j) => (
            <div key={`r${i}-${j}`} className="kiln-pagelayout__range" style={px(rect)} aria-hidden="true" />
          )),
      )}
      {active.map((rect, i) =>
        editing && grid ? (
          <div
            key={`e${i}`}
            className="kiln-pagelayout__entry"
            style={{ ...px(rect), minWidth: px(rect).width }}
            data-testid="grid-page-entry"
          >
            <Entry grid={grid} />
          </div>
        ) : (
          <div
            key={`a${i}`}
            className="kiln-pagelayout__active"
            style={px(rect)}
            data-testid="grid-page-active"
            aria-hidden="true"
          />
        ),
      )}
    </div>
  );
}

/**
 * The empty page after the last printed one, as in Excel: "Click to add data" selects the cell
 * below the printed cells in their first column, where typing starts the next page.
 */
export function AddDataPage({
  model,
  last,
  width,
  height,
}: {
  model: GridEditorModel;
  /** The last printed page. */
  last: number;
  /** The page's size on screen, CSS px. */
  width: number;
  height: number;
}) {
  const [next, setNext] = useState<{ row: number; col: number } | null>(null);
  const grid = useGrid(model, (s) => s.grid);
  useEffect(() => {
    let live = true;
    // Under the printed cells, in the first printed column (the first page's; pages run down,
    // then over, so the last page ends on the last printed row).
    const own = (page: number) =>
      model
        .query({ PageMap: { page } })
        .then((r) => (r && 'PageMap' in r ? r.PageMap?.bands.at(-1) : undefined));
    void Promise.all([own(0), own(last)]).then(([firstPage, lastPage]) => {
      const row = lastPage?.rows.at(-1);
      const col = firstPage?.cols[0];
      if (live) setNext(row && col ? { row: row.index + 1, col: col.index } : null);
    });
    return () => {
      live = false;
    };
  }, [model, last, grid?.features.pages]);
  if (!next) return null;
  return (
    <div className="kiln-pagelayout__page kiln-pagelayout__page--empty" style={{ width, height }}>
      <button
        type="button"
        className="kiln-pagelayout__add"
        data-testid="grid-page-add-data"
        onClick={() => {
          void model.core('grid.nav.select_cell', {
            row: next.row,
            col: next.col,
            extend: false,
            add: false,
          });
          model.surface?.focus();
        }}
      >
        {t('Click to add data')}
      </button>
    </div>
  );
}
