/**
 * View ▸ Notes Page: the current slide's notes page as it prints — the slide picture above, the
 * speaker notes in their box below, typed into in place — fitted to the window. Page Up and
 * Page Down (or the wheel) go to the previous and next slide.
 */
import { t } from '@kiln/i18n';
import type { DeckNotesPage } from '@kiln/protocol';
import { type CSSProperties, useEffect, useLayoutEffect, useRef, useState, type WheelEvent } from 'react';
import { useDeck } from '../hooks.ts';
import type { DeckEditorModel } from '../model.ts';
import { NotesText } from '../NotesPane.tsx';
import { paintScene } from '../thumbs.ts';

/** The page's margin in the window, CSS px. */
const MARGIN = 24;

export function NotesPageView({ model, theme }: { model: DeckEditorModel; theme: 'light' | 'dark' }) {
  const current = useDeck(model, (s) => s.deck?.current ?? 0);
  const version = useDeck(model, (s) => s.deck?.slides[s.deck.current]?.version ?? 0);
  const [page, setPage] = useState<DeckNotesPage | null>(null);
  const [scale, setScale] = useState(0.8);
  const box = useRef<HTMLDivElement>(null);
  const picture = useRef<HTMLCanvasElement>(null);
  useEffect(() => {
    let live = true;
    void model.query('NotesPage').then((r) => {
      if (live && r && 'NotesPage' in r) setPage(r.NotesPage);
    });
    return () => {
      live = false;
    };
  }, [model]);
  // The page fits the window's height (and width).
  useLayoutEffect(() => {
    const el = box.current;
    if (!el || !page) return;
    const fit = () => {
      const s = Math.min(
        (el.clientHeight - 2 * MARGIN) / page.height,
        (el.clientWidth - 2 * MARGIN) / page.width,
      );
      setScale(Math.max(0.2, s));
    };
    fit();
    const ro = new ResizeObserver(fit);
    ro.observe(el);
    return () => ro.disconnect();
  }, [page]);
  // The slide's picture, drawn by the core at the size it has on the page.
  useEffect(() => {
    if (!page) return;
    let live = true;
    const dpr = typeof window === 'undefined' ? 1 : window.devicePixelRatio || 1;
    void model.thumbnails(current, 1, Math.round(page.slide.width * scale), dpr).then((scene) => {
      const c = picture.current;
      const tile = scene ? paintScene(scene, theme).get(current) : undefined;
      if (!live || !c || !tile) return;
      c.width = tile.width;
      c.height = tile.height;
      c.getContext('2d')?.drawImage(tile, 0, 0);
    });
    return () => {
      live = false;
    };
  }, [model, page, scale, current, version, theme]);
  const onWheel = (e: WheelEvent) => {
    if (Math.abs(e.deltaY) < 20) return;
    void model.core(e.deltaY > 0 ? 'deck.slides.next_slide' : 'deck.slides.previous_slide');
  };
  const at = (r: DeckNotesPage['slide']): CSSProperties => ({
    left: r.x * scale,
    top: r.y * scale,
    width: r.width * scale,
    height: r.height * scale,
  });
  return (
    // biome-ignore lint/a11y/noStaticElementInteractions: the wheel and Page Up/Down go through the pages
    <div
      ref={box}
      className="kiln-decknotespage"
      data-testid="deck-notes-page"
      tabIndex={-1}
      onWheel={onWheel}
      onKeyDown={(e) => {
        if (e.key === 'PageDown' || e.key === 'PageUp') {
          e.preventDefault();
          void model.core(e.key === 'PageDown' ? 'deck.slides.next_slide' : 'deck.slides.previous_slide');
        }
      }}
    >
      {page ? (
        <section
          className="kiln-decknotespage__page"
          style={{ width: page.width * scale, height: page.height * scale }}
          aria-label={t('Notes page of slide {value}', { value: current + 1 })}
        >
          <canvas ref={picture} className="kiln-decknotespage__slide" style={at(page.slide)} />
          <div className="kiln-decknotespage__body" style={{ ...at(page.body), fontSize: 16 * scale }}>
            <NotesText model={model} className="kiln-decknotespage__text" />
          </div>
        </section>
      ) : null}
    </div>
  );
}
