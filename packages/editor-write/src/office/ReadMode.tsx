/**
 * Read Mode's page turning and zoomed pictures (Word's Read Mode): the pages stand side by side
 * as columns sized to the window; the arrows at the edges, Page Down and Page Up, the arrow keys
 * and Space turn a screen at a time, Home and End go to the first and last screen and Esc goes
 * back to Print Layout. A double-click on a picture shows it zoomed over the page, as stored in
 * the file (Esc or a click closes it).
 */
import { t } from '@kiln/i18n';
import type { PagesInfo, Rect } from '@kiln/protocol';
import type { SurfaceController } from '@kiln/surface';
import { Icon } from '@kiln/ui-kit';
import { ChevronLeft, ChevronRight, X } from 'lucide-react';
import { useCallback, useEffect, useRef, useState } from 'react';
import { useWrite } from '../hooks.ts';
import type { WriteEditorModel } from '../model.ts';

interface Props {
  model: WriteEditorModel;
  controller: SurfaceController;
}

interface Zoomed {
  url: string;
  alt: string;
}

/** The first page of each screen: pages whose left edges fit in one window width. */
export function screens(rects: readonly Rect[], width: number): number[] {
  const out: number[] = [];
  let i = 0;
  while (i < rects.length) {
    out.push(i);
    const left = rects[i]?.x ?? 0;
    let j = i + 1;
    while (j < rects.length && (rects[j]?.x ?? 0) + (rects[j]?.width ?? 0) <= left + width + 1) j++;
    i = j;
  }
  return out;
}

export function ReadMode({ model, controller }: Props) {
  const view = useWrite(model, (s) => s.status?.view ?? 'PrintLayout');
  const vs = useWrite(model, (s) => s.view);
  const revision = useWrite(model, (s) => s.status?.page?.total);
  const [pages, setPages] = useState<PagesInfo | null>(null);
  const [zoomed, setZoomed] = useState<Zoomed | null>(null);
  const reading = view === 'ReadMode';
  useEffect(() => {
    if (!reading) return;
    let live = true;
    void model.query('Pages').then((r) => {
      if (live && r && 'Pages' in r) setPages(r.Pages);
    });
    return () => {
      live = false;
    };
  }, [model, reading, revision, vs?.width, vs?.zoom]);

  const firsts = pages && vs ? screens(pages.rects, vs.width / (vs.zoom || 1)) : [];
  const at = (() => {
    if (!pages || !vs) return 0;
    let k = 0;
    firsts.forEach((p, i) => {
      // The screen whose first page starts in the left half of the window (the last screen
      // cannot scroll all the way to its page).
      if ((pages.rects[p]?.x ?? 0) <= vs.scrollX + vs.width / (vs.zoom || 1) / 2) k = i;
    });
    return k;
  })();
  const turn = useCallback(
    (to: number) => {
      if (!pages || firsts.length === 0) return;
      const k = Math.max(0, Math.min(firsts.length - 1, to));
      const page = pages.rects[firsts[k] ?? 0];
      if (page) controller.scrollToX(page.x - 16, true);
    },
    [pages, firsts, controller],
  );

  // Reading starts with the keys in the document (Word).
  useEffect(() => {
    if (reading) controller.focus();
  }, [reading, controller]);

  // Keys turn screens while reading (the document itself does not take them).
  const live = useRef({ at, turn, count: firsts.length });
  live.current = { at, turn, count: firsts.length };
  useEffect(() => {
    if (!reading) return;
    const onKey = (e: KeyboardEvent) => {
      if (e.ctrlKey || e.metaKey || e.altKey) return;
      const t = e.target as HTMLElement | null;
      // Not while a field, a menu or a dialog has the keys.
      if (
        t?.closest(
          'input, textarea, select, [role="dialog"], [role="menu"], [role="listbox"], [role="radio"], [role="tab"], [role="menuitem"]',
        ) &&
        !t.closest('[data-testid="kiln-surface"]')
      )
        return;
      const { at: now, turn: go, count } = live.current;
      let to: number | null = null;
      if (['PageDown', 'ArrowRight', 'ArrowDown', ' '].includes(e.key)) to = now + 1;
      else if (['PageUp', 'ArrowLeft', 'ArrowUp'].includes(e.key)) to = now - 1;
      else if (e.key === 'Home') to = 0;
      else if (e.key === 'End') to = count - 1;
      else if (e.key === 'Escape' && !zoomedRef.current) {
        e.preventDefault();
        e.stopPropagation();
        void model.run('suite.view.mode', { mode: 'PrintLayout' });
        return;
      }
      if (to === null) return;
      e.preventDefault();
      e.stopPropagation();
      go(to);
    };
    window.addEventListener('keydown', onKey, true);
    return () => window.removeEventListener('keydown', onKey, true);
  }, [reading, model]);

  // A double-click on a picture zooms it.
  const zoomedRef = useRef<Zoomed | null>(null);
  zoomedRef.current = zoomed;
  useEffect(() => {
    if (!reading) return;
    // The pane, not only the surface: the frame of a selected picture lies over it.
    const surface = document.querySelector<HTMLElement>('.kiln-write__pane-main');
    if (!surface) return;
    const onDbl = async (e: MouseEvent) => {
      const p = controller.documentPoint(e.clientX, e.clientY);
      const r = await model.query('Objects');
      const objects = r && 'Objects' in r ? r.Objects : [];
      const hit = objects.find(
        (o) =>
          o.kind === 'Picture' &&
          o.rect &&
          p.x >= o.rect.x &&
          p.x <= o.rect.x + o.rect.width &&
          p.y >= o.rect.y &&
          p.y <= o.rect.y + o.rect.height,
      );
      if (!hit) return;
      e.preventDefault();
      e.stopPropagation();
      const pic = await model.query({ ObjectPicture: { id: hit.id } });
      const data = pic && 'ObjectPicture' in pic ? pic.ObjectPicture : null;
      if (!data) return;
      const url = URL.createObjectURL(new Blob([new Uint8Array(data.bytes)], { type: data.content_type }));
      setZoomed({ url, alt: data.alt_text ?? hit.name });
    };
    surface.addEventListener('dblclick', onDbl, true);
    return () => surface.removeEventListener('dblclick', onDbl, true);
  }, [reading, model, controller]);
  useEffect(() => () => (zoomed ? URL.revokeObjectURL(zoomed.url) : undefined), [zoomed]);
  useEffect(() => {
    if (!zoomed) return;
    const onKey = (e: KeyboardEvent) => {
      if (e.key === 'Escape') {
        e.preventDefault();
        e.stopPropagation();
        setZoomed(null);
      }
    };
    window.addEventListener('keydown', onKey, true);
    return () => window.removeEventListener('keydown', onKey, true);
  }, [zoomed]);

  if (!reading) return null;
  return (
    <>
      <button
        type="button"
        className="kiln-write-read__turn kiln-write-read__turn--prev"
        aria-label={t('Previous screen')}
        title={t('Previous screen (Page Up)')}
        disabled={at <= 0}
        onClick={() => turn(at - 1)}
        data-testid="write-read-prev"
      >
        <Icon icon={ChevronLeft} size={22} />
      </button>
      <button
        type="button"
        className="kiln-write-read__turn kiln-write-read__turn--next"
        aria-label={t('Next screen')}
        title={t('Next screen (Page Down)')}
        disabled={at >= firsts.length - 1}
        onClick={() => turn(at + 1)}
        data-testid="write-read-next"
      >
        <Icon icon={ChevronRight} size={22} />
      </button>
      <output className="kiln-write-read__where" data-testid="write-read-screen">
        {t('Screen {n, number} of {total, number}', {
          n: Math.min(at + 1, Math.max(1, firsts.length)),
          total: Math.max(1, firsts.length),
        })}
      </output>
      {zoomed ? (
        // biome-ignore lint/a11y/useKeyWithClickEvents: Esc closes it (above); the click is for the mouse
        <div
          className="kiln-write-read__zoom"
          role="dialog"
          aria-modal="true"
          aria-label={t('Zoomed picture')}
          data-testid="write-read-zoom"
          onClick={() => setZoomed(null)}
        >
          <img src={zoomed.url} alt={zoomed.alt} />
          <button
            type="button"
            className="kiln-write-read__close"
            aria-label={t('Close')}
            onClick={() => setZoomed(null)}
          >
            <Icon icon={X} size={18} />
          </button>
        </div>
      ) : null}
    </>
  );
}
