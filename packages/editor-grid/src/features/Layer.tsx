/**
 * The feature layer over the Grid sheet: the AutoFilter dropdown a header button opens,
 * notes kept visible (Show Note, Show All Notes) and the note or comment card of the cell under
 * the pointer, the validation input message beside the active cell, Format Painter's brush
 * (the next selection takes the formats), the in-cell dropdown button, and dragging page breaks
 * in Page Break Preview. Positions are the core's (surface CSS pixels).
 */
import { t } from '@kiln/i18n';
import type { GridCommentInfo, GridHit } from '@kiln/protocol';
import { useEffect, useRef, useState } from 'react';
import { useStore } from 'zustand';
import { useGrid } from '../hooks.ts';
import type { GridEditorModel } from '../model.ts';
import { FeatureDialogs } from './FeatureDialogs.tsx';
import { FilterMenu, TotalsMenu } from './FilterMenu.tsx';
import { featuresOf } from './model.ts';
import './features.css';

const NO_CARDS: never[] = [];

export function GridFeatureLayer({ model }: { model: GridEditorModel }) {
  const features = featuresOf(model);
  const cards = useGrid(model, (s) => s.grid?.features.cards ?? NO_CARDS);
  const prompt = useGrid(model, (s) => s.grid?.features.prompt ?? null);
  const rect = useGrid(model, (s) => s.grid?.features.cell_rect ?? null);
  const mode = useGrid(model, (s) => s.grid?.mode ?? 'Ready');
  const painter = useStore(features.store, (s) => s.painter);
  const hover = useHoverCard(model);
  const layer = useRef<HTMLDivElement>(null);

  useEffect(() => {
    const onHit = (hit: GridHit, client: { x: number; y: number }, e: PointerEvent): boolean => {
      if (hit === 'ListButton') {
        void features.openDropdown();
        return true;
      }
      if (typeof hit !== 'object') return false;
      if ('FilterButton' in hit) {
        const { row, col } = hit.FilterButton;
        features.set({ filterMenu: { row, col, x: client.x - 8, y: client.y + 12 } });
        return true;
      }
      if ('PageBreak' in hit) {
        dragBreak(model, features, hit.PageBreak.columns, hit.PageBreak.index, e);
        return true;
      }
      return false;
    };
    const onSelected = () => void features.afterSelect();
    model.hitHooks.push(onHit);
    model.selectedHooks.push(onSelected);
    const onKey = (e: KeyboardEvent) => {
      if (e.key === 'Escape' && features.state.painter) features.set({ painter: null });
      // The brush armed: the arrows (with Shift) select where to paint, Enter paints there, as in
      // Excel, so the Format Painter works without the pointer.
      if (
        e.key === 'Enter' &&
        features.state.painter &&
        !e.ctrlKey &&
        !e.metaKey &&
        !e.altKey &&
        document.activeElement?.hasAttribute('data-kiln-grid-proxy')
      ) {
        e.preventDefault();
        e.stopPropagation();
        void features.afterSelect();
        return;
      }
      // Typing with a shape selected writes its text (Excel); F2 edits what is there.
      const d = model.state.grid?.drawing;
      const onSheet = document.activeElement?.hasAttribute('data-kiln-grid-proxy');
      if (onSheet && d?.kind === 'Shape' && !features.state.dialog && !e.ctrlKey && !e.metaKey && !e.altKey) {
        if (e.key.length === 1 || e.key === 'F2') {
          e.preventDefault();
          e.stopPropagation();
          features.openDialog('shapeText', e.key === 'F2' ? undefined : (d.object?.text ?? '') + e.key);
        }
      }
    };
    window.addEventListener('keydown', onKey, true);
    // A ribbon, menu or status bar command hands the keys back to the sheet (the Write gate's
    // returnToDocument), as Office does.
    const back = () => model.surface?.focus();
    window.addEventListener('kiln:return-to-document', back);
    return () => {
      window.removeEventListener('kiln:return-to-document', back);
      model.hitHooks.splice(model.hitHooks.indexOf(onHit), 1);
      model.selectedHooks.splice(model.selectedHooks.indexOf(onSelected), 1);
      window.removeEventListener('keydown', onKey, true);
    };
  }, [model, features]);

  return (
    <div ref={layer} className={painter ? 'kiln-gfx__layer kiln-gfx__layer--painter' : 'kiln-gfx__layer'}>
      {cards.map((c) => (
        <div
          key={`${c.row}-${c.col}`}
          className={c.thread ? 'kiln-gfx__card kiln-gfx__card--thread' : 'kiln-gfx__card'}
          style={{ left: c.x + 10, top: c.y }}
          data-testid="grid-note-card"
        >
          <strong>{c.author}:</strong> {c.text}
          {c.replies ? (
            <div className="kiln-gfx__card-replies">{t('grid.replies', { count: c.replies })}</div>
          ) : null}
        </div>
      ))}
      {hover && !cards.some((c) => c.row === hover.c.row && c.col === hover.c.col) ? (
        <div
          className={hover.c.note ? 'kiln-gfx__card' : 'kiln-gfx__card kiln-gfx__card--thread'}
          style={{ left: hover.x + 14, top: hover.y + 6 }}
          data-testid="grid-hover-card"
        >
          <strong>{hover.c.author}:</strong> {hover.c.text}
          {hover.replies ? (
            <div className="kiln-gfx__card-replies">{t('grid.replies', { count: hover.replies })}</div>
          ) : null}
        </div>
      ) : null}
      {prompt && rect && mode === 'Ready' ? (
        <div
          className="kiln-gfx__prompt-tip"
          role="tooltip"
          style={{ left: (rect[0] ?? 0) + 12, top: (rect[1] ?? 0) + (rect[3] ?? 20) + 6 }}
          data-testid="grid-input-message"
        >
          {prompt[0] ? <strong>{prompt[0]}</strong> : null}
          {prompt[1] ? <span>{prompt[1]}</span> : null}
        </div>
      ) : null}
      <FilterMenu features={features} />
      <TotalsMenu features={features} />
      <FeatureDialogs features={features} />
    </div>
  );
}

/** The note or comment of the cell under the resting pointer. */
function useHoverCard(
  model: GridEditorModel,
): { c: GridCommentInfo; x: number; y: number; replies: number } | null {
  const [card, setCard] = useState<{ c: GridCommentInfo; x: number; y: number; replies: number } | null>(
    null,
  );
  const grid = useGrid(model, (s) => s.grid);
  const anyComments = grid?.features.any_comments ?? false;
  const list = useRef<GridCommentInfo[]>([]);
  useEffect(() => {
    if (!anyComments) {
      list.current = [];
      return;
    }
    void featuresOf(model)
      .ask('Comments')
      .then((r) => {
        list.current = r && 'Comments' in r ? r.Comments : [];
      });
  }, [model, anyComments, grid?.active_sheet]);
  useEffect(() => {
    if (!anyComments) {
      setCard(null);
      return;
    }
    const root = document.querySelector<HTMLElement>('[data-kiln-grid-surface]');
    if (!root) return;
    let timer = 0;
    const onMove = (e: PointerEvent) => {
      window.clearTimeout(timer);
      const box = root.getBoundingClientRect();
      const x = e.clientX - box.left;
      const y = e.clientY - box.top;
      timer = window.setTimeout(async () => {
        const hit = await model.hitTest(x, y);
        const sheet = model.state.grid?.active_sheet ?? 0;
        if (typeof hit === 'object' && 'Cell' in hit) {
          const { row, col } = hit.Cell;
          const c = list.current.find(
            (m) => m.sheet === sheet && m.row === row && m.col === col && !m.parent,
          );
          if (c) {
            const replies = list.current.filter((m) => m.parent === c.id).length;
            setCard({ c, x, y, replies });
            return;
          }
        }
        setCard(null);
      }, 350);
    };
    const onLeave = () => {
      window.clearTimeout(timer);
      setCard(null);
    };
    root.addEventListener('pointermove', onMove);
    root.addEventListener('pointerleave', onLeave);
    return () => {
      window.clearTimeout(timer);
      root.removeEventListener('pointermove', onMove);
      root.removeEventListener('pointerleave', onLeave);
    };
  }, [model, anyComments]);
  return card;
}

/** Drags a page break of Page Break Preview to the row or column under the release point. */
function dragBreak(
  model: GridEditorModel,
  features: ReturnType<typeof featuresOf>,
  columns: boolean,
  index: number,
  start: PointerEvent,
): void {
  const root = document.querySelector<HTMLElement>('[data-kiln-grid-surface]');
  if (!root) return;
  const guide = document.createElement('div');
  guide.className = columns ? 'kiln-gfx__break-guide kiln-gfx__break-guide--v' : 'kiln-gfx__break-guide';
  root.appendChild(guide);
  const box = root.getBoundingClientRect();
  const place = (e: PointerEvent) => {
    if (columns) guide.style.left = `${e.clientX - box.left}px`;
    else guide.style.top = `${e.clientY - box.top}px`;
  };
  place(start);
  const move = (e: PointerEvent) => place(e);
  const up = async (e: PointerEvent) => {
    window.removeEventListener('pointermove', move);
    window.removeEventListener('pointerup', up);
    guide.remove();
    const hit = await model.hitTest(e.clientX - box.left, e.clientY - box.top);
    if (typeof hit === 'object' && 'Cell' in hit) {
      const to = columns ? hit.Cell.col : hit.Cell.row;
      if (to !== index) await features.exec({ MovePageBreak: { columns, from: index, to } });
    }
  };
  window.addEventListener('pointermove', move);
  window.addEventListener('pointerup', up);
}
