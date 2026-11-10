/**
 * Kiln Write's layer over the surface: the balloons of tracked changes in the margin
 * (Show Markup ▸ Balloons), View ▸ Gridlines over the pages, drag and drop of selected text (a
 * dotted drop caret follows the pointer; Ctrl copies), Shift+click on objects to select several,
 * and the frames of the objects added to the selection. Geometry is the core's, in document
 * pixels; this layer maps it to the screen.
 */
import { t } from '@kiln/i18n';
import type { PagesInfo, Rect, RevisionBalloonInfo, TextPos } from '@kiln/protocol';
import type { SurfaceController } from '@kiln/surface';
import { loadOptions } from '@kiln/ui-kit';
import { useEffect, useRef, useState } from 'react';
import { useStore } from 'zustand';
import { featuresOf } from '../features/model.ts';
import { useWrite } from '../hooks.ts';
import type { WriteEditorModel } from '../model.ts';

interface Props {
  model: WriteEditorModel;
  controller: SurfaceController;
  layer: HTMLElement | null;
}

function toLayer(controller: SurfaceController, layer: HTMLElement | null, r: Rect) {
  if (!layer) return null;
  const c = controller.clientRectOf(r);
  const base = layer.getBoundingClientRect();
  return { left: c.left - base.left, top: c.top - base.top, width: c.width, height: c.height };
}

// ── Revision balloons ────────────────────────────────────────────────────────────────────────

function RevisionBalloons({ model, controller, layer }: Props) {
  const fs = useWrite(model, (s) => s.format?.features ?? null);
  useWrite(model, (s) => s.view);
  const revision = useWrite(model, (s) => s.status);
  const [balloons, setBalloons] = useState<RevisionBalloonInfo[]>([]);
  const wanted = !!fs && fs.markup === 'All' && fs.revisions > 0 && fs.balloons !== 'Inline';
  useEffect(() => {
    if (!wanted) {
      setBalloons([]);
      return;
    }
    let live = true;
    void model.query('RevisionBalloons').then((r) => {
      if (live && r && 'RevisionBalloons' in r) setBalloons(r.RevisionBalloons);
    });
    return () => {
      live = false;
    };
  }, [model, wanted, revision, fs?.revisions, fs?.balloons]);
  if (!wanted || balloons.length === 0) return null;
  return (
    <>
      {balloons.map((b) => {
        const box = toLayer(controller, layer, b.rect);
        if (!box) return null;
        const select = async (accept: boolean | null) => {
          const pos: TextPos = { block: b.para, offset: b.offset };
          await model.runCore('write.caret.select_to', { pos, extend: false });
          if (accept !== null)
            await model.run(accept ? 'write.review.accept' : 'write.review.reject', { scope: 'Current' });
        };
        return (
          <article
            key={`${b.para}:${b.offset}:${b.deleted}`}
            className="kiln-revision-balloon"
            style={{
              left: box.left,
              top: box.top,
              width: Math.min(260, box.width),
              borderColor: `#${b.color}`,
            }}
            data-testid="write-revision-balloon"
            aria-label={t('{text}, by {author}', { text: b.text, author: b.author })}
          >
            <button type="button" className="kiln-revision-balloon__text" onClick={() => void select(null)}>
              <strong style={{ color: `#${b.color}` }}>{b.deleted ? t('Deleted') : t('Formatted')}</strong>
              {b.deleted ? <span>{b.text.replace(/^Deleted: /, '')}</span> : null}
              <span className="kiln-revision-balloon__author">{b.author}</span>
            </button>
            <span className="kiln-revision-balloon__acts">
              <button
                type="button"
                aria-label={t('Accept')}
                title={t('Accept')}
                onClick={() => void select(true)}
              >
                ✓
              </button>
              <button
                type="button"
                aria-label={t('Reject')}
                title={t('Reject')}
                onClick={() => void select(false)}
              >
                ✕
              </button>
            </span>
          </article>
        );
      })}
    </>
  );
}

// ── Gridlines ────────────────────────────────────────────────────────────────────────────────

/** Word's drawing grid: 0.13 inch squares over each page's text area (document px). */
const GRID = 0.13 * 96;

function Gridlines({ model, controller, layer }: Props) {
  const on = useWrite(model, (s) => s.gridlines);
  useWrite(model, (s) => s.view);
  const status = useWrite(model, (s) => s.status);
  const [pages, setPages] = useState<PagesInfo | null>(null);
  useEffect(() => {
    if (!on) return;
    void model.query('Pages').then((r) => {
      if (r && 'Pages' in r) setPages(r.Pages);
    });
  }, [model, on, status?.page?.total, status?.zoom, status?.view]);
  if (!on || !pages) return null;
  const zoom = controller.getZoom();
  const rulers = model.state.rulers;
  const step = GRID * zoom;
  return (
    <>
      {pages.rects.map((p, i) => {
        // The text area of the caret's page from the rulers; the margins are the same on every
        // page of a section, which is what the grid needs.
        const text = rulers?.text_rect;
        const inset =
          text && rulers
            ? { x: text.x - rulers.page_rect.x, y: text.y - rulers.page_rect.y }
            : { x: 96, y: 96 };
        const w = text?.width ?? p.width - 192;
        const h = text?.height ?? p.height - 192;
        const box = toLayer(controller, layer, { x: p.x + inset.x, y: p.y + inset.y, width: w, height: h });
        if (!box || box.top > (layer?.clientHeight ?? 0) || box.top + box.height < 0) return null;
        return (
          <div
            key={i}
            className="kiln-write-gridlines"
            data-testid="write-gridlines"
            style={{
              left: box.left,
              top: box.top,
              width: box.width,
              height: box.height,
              backgroundSize: `${step}px ${step}px`,
            }}
          />
        );
      })}
    </>
  );
}

// ── Drag and drop of text, Shift+click on objects ────────────────────────────────────────────

interface DragState {
  x: number;
  y: number;
  dragging: boolean;
}

function inRects(rects: readonly Rect[], x: number, y: number): boolean {
  return rects.some((r) => x >= r.x && x <= r.x + r.width && y >= r.y && y <= r.y + r.height);
}

function DragText({ model, controller, layer }: Props) {
  const drag = useRef<DragState | null>(null);
  const [caret, setCaret] = useState<{
    box: { left: number; top: number; height: number };
    copy: boolean;
  } | null>(null);
  const drop = useRef<{ pos: TextPos; inside: boolean } | null>(null);
  useEffect(() => {
    const surface = layer?.parentElement;
    if (!surface) return;
    let asking = false;
    const ask = async (clientX: number, clientY: number, copy: boolean) => {
      if (asking) return;
      asking = true;
      try {
        const p = controller.documentPoint(clientX, clientY);
        const r = await model.query({ DropPoint: { x: Math.round(p.x), y: Math.round(p.y) } });
        const t = r && 'DropPoint' in r ? r.DropPoint : null;
        drop.current = t ? { pos: t.pos, inside: t.inside } : null;
        const box = t && !t.inside ? toLayer(controller, layer, t.caret) : null;
        setCaret(box ? { box: { left: box.left, top: box.top, height: box.height }, copy } : null);
      } finally {
        asking = false;
      }
    };
    const onDown = (e: PointerEvent) => {
      if (e.button !== 0 || e.pointerType === 'touch') return;
      const target = e.target as HTMLElement;
      if (target.closest('.kiln-comment, .kiln-object-frame, .kiln-revision-balloon')) return;
      const p = controller.documentPoint(e.clientX, e.clientY);
      // Shift or Ctrl+click on a floating object adds it to the selected objects (Word).
      if (e.shiftKey || e.ctrlKey || e.metaKey) {
        const objects = featuresOf(model).state.objects ?? [];
        const hit = objects.find(
          (o) => o.rect && o.wrap !== 'InLine' && inRects([o.rect], p.x, p.y) && !o.selected,
        );
        if (hit) {
          e.preventDefault();
          e.stopPropagation();
          void model.run('write.arrange.select', { id: hit.id, add: true });
          return;
        }
      }
      // Options ▸ Advanced ▸ Allow text to be dragged and dropped: off, a press in the selection
      // places the caret as anywhere else.
      if (!loadOptions().advanced.dragAndDrop) return;
      const sel = model.state.selection;
      if (!sel || sel.rects.length === 0 || e.shiftKey || !inRects(sel.rects, p.x, p.y)) return;
      // A press inside the selection: a drag moves the text; a click without moving places the
      // caret (the surface does not see the press either way).
      e.preventDefault();
      e.stopPropagation();
      drag.current = { x: e.clientX, y: e.clientY, dragging: false };
      surface.setPointerCapture?.(e.pointerId);
    };
    const onMove = (e: PointerEvent) => {
      const d = drag.current;
      if (!d) return;
      e.stopPropagation();
      if (!d.dragging && Math.hypot(e.clientX - d.x, e.clientY - d.y) < 4) return;
      d.dragging = true;
      surface.classList.add('is-dragging-text');
      void ask(e.clientX, e.clientY, e.ctrlKey || e.metaKey);
    };
    const onUp = (e: PointerEvent) => {
      const d = drag.current;
      if (!d) return;
      e.stopPropagation();
      drag.current = null;
      surface.classList.remove('is-dragging-text');
      surface.releasePointerCapture?.(e.pointerId);
      setCaret(null);
      if (!d.dragging) {
        const p = controller.documentPoint(e.clientX, e.clientY);
        void model.runCore('write.caret.select_at', { point: p, extend: false, unit: 'Char' });
        return;
      }
      const target = drop.current;
      drop.current = null;
      if (target && !target.inside)
        void model.run('write.edit.move_selection', { to: target.pos, copy: e.ctrlKey || e.metaKey });
    };
    const onKey = (e: KeyboardEvent) => {
      if (e.key === 'Escape' && drag.current) {
        drag.current = null;
        drop.current = null;
        setCaret(null);
        surface.classList.remove('is-dragging-text');
      }
    };
    surface.addEventListener('pointerdown', onDown, true);
    surface.addEventListener('pointermove', onMove, true);
    surface.addEventListener('pointerup', onUp, true);
    window.addEventListener('keydown', onKey, true);
    return () => {
      surface.removeEventListener('pointerdown', onDown, true);
      surface.removeEventListener('pointermove', onMove, true);
      surface.removeEventListener('pointerup', onUp, true);
      window.removeEventListener('keydown', onKey, true);
    };
  }, [model, controller, layer]);
  if (!caret) return null;
  return (
    <div
      className={`kiln-write-drop-caret${caret.copy ? ' is-copy' : ''}`}
      data-testid="write-drop-caret"
      style={{ left: caret.box.left, top: caret.box.top, height: caret.box.height }}
    />
  );
}

// ── Several selected objects ─────────────────────────────────────────────────────────────────

function SelectedFrames({ model, controller, layer }: Props) {
  const objects = useStore(featuresOf(model).store, (s) => s.objects);
  useWrite(model, (s) => s.view);
  const extra = (objects ?? []).filter((o) => o.in_selection && !o.selected && o.rect);
  if (extra.length === 0) return null;
  return (
    <>
      {extra.map((o) => {
        const box = o.rect ? toLayer(controller, layer, o.rect) : null;
        if (!box) return null;
        return (
          <div
            key={o.id}
            className="kiln-write-multi-frame"
            data-testid="write-multi-frame"
            style={{ left: box.left - 2, top: box.top - 2, width: box.width + 4, height: box.height + 4 }}
          />
        );
      })}
    </>
  );
}

export function OfficeLayer(props: Props) {
  return (
    <>
      <Gridlines {...props} />
      <RevisionBalloons {...props} />
      <SelectedFrames {...props} />
      <DragText {...props} />
    </>
  );
}
