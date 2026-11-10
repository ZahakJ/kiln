/**
 * View ▸ Slide Sorter: every slide as a numbered thumbnail in rows. Click selects (Shift extends,
 * Ctrl toggles), drag moves the selected slides, double-click or Enter opens the slide in Normal
 * view; the keys of the rail work here too.
 */
import { t } from '@kiln/i18n';
import { useEffect, useMemo, useRef, useState } from 'react';
import { useDeck } from './hooks.ts';
import { railKeyAction } from './keys.ts';
import type { DeckEditorModel } from './model.ts';
import { ThumbCanvas } from './ThumbnailRail.tsx';
import { ThumbCache } from './thumbs.ts';

const WIDTH = 220;

export function SlideSorter({ model, theme }: { model: DeckEditorModel; theme: 'light' | 'dark' }) {
  const deck = useDeck(model, (s) => s.deck);
  const cache = useMemo(() => new ThumbCache(model, WIDTH, theme), [model]);
  const [revision, setRevision] = useState(0);
  const [drop, setDrop] = useState<number | null>(null);
  const press = useRef<{ x: number; y: number; index: number; dragging: boolean } | null>(null);
  const grid = useRef<HTMLDivElement>(null);
  useEffect(() => cache.subscribe(() => setRevision(cache.revision)), [cache]);
  useEffect(() => {
    cache.setTheme(theme);
    if (deck) void cache.update(deck.slides);
  }, [cache, theme, deck]);
  useEffect(() => grid.current?.focus({ preventScroll: true }), []);
  if (!deck) return null;
  const height = (WIDTH * deck.slide_height) / Math.max(1, deck.slide_width);
  const selected = new Set(deck.selected);
  const dropAt = (x: number, y: number): number => {
    let best = deck.slides.length;
    for (const el of grid.current?.querySelectorAll<HTMLElement>('[data-slide-index]') ?? []) {
      const r = el.getBoundingClientRect();
      if (y < r.bottom && x < r.left + r.width / 2 && y >= r.top - 12) {
        best = Number(el.dataset.slideIndex);
        break;
      }
    }
    return best;
  };
  const open = (index: number) => {
    void model.core('deck.slides.select', { index, extend: false, toggle: false });
    void model.run('suite.view.mode', { mode: 'SlideNormal' });
  };
  return (
    <div
      ref={grid}
      className="kiln-decksorter"
      role="listbox"
      aria-label={t('Slide Sorter')}
      aria-multiselectable="true"
      tabIndex={0}
      data-testid="deck-sorter"
      onKeyDown={(e) => {
        if (e.key === 'Enter') {
          e.preventDefault();
          open(deck.current);
          return;
        }
        const a = railKeyAction(e, deck.current, deck.slides.length);
        if (!a) return;
        e.preventDefault();
        e.stopPropagation();
        void model.core(a.id, a.args);
      }}
    >
      {deck.slides.map((slide, index) => {
        return (
          <div
            key={slide.id}
            className="kiln-decksorter__item"
            role="option"
            tabIndex={-1}
            aria-selected={selected.has(index)}
            aria-label={
              slide.title
                ? t('Slide {n, number}, {title}', { n: index + 1, title: slide.title })
                : t('Slide {n, number}', { n: index + 1 })
            }
            data-slide-index={index}
            data-current={index === deck.current || undefined}
            data-selected={selected.has(index) || undefined}
            data-hidden={slide.hidden || undefined}
            data-drop={drop === index ? 'before' : undefined}
            onPointerDown={(e) => {
              if (e.button !== 0) return;
              press.current = { x: e.clientX, y: e.clientY, index, dragging: false };
              (e.currentTarget as HTMLElement).setPointerCapture?.(e.pointerId);
              const mod = e.ctrlKey || e.metaKey;
              if (e.shiftKey || mod || !selected.has(index))
                void model.core('deck.slides.select', {
                  index,
                  extend: e.shiftKey,
                  toggle: mod && !e.shiftKey,
                });
            }}
            onPointerMove={(e) => {
              const p = press.current;
              if (!p) return;
              if (!p.dragging && Math.hypot(e.clientX - p.x, e.clientY - p.y) < 5) return;
              p.dragging = true;
              setDrop(dropAt(e.clientX, e.clientY));
            }}
            onPointerUp={(e) => {
              const p = press.current;
              press.current = null;
              setDrop(null);
              if (p?.dragging) void model.core('deck.slides.move', { to: dropAt(e.clientX, e.clientY) });
            }}
            onDoubleClick={() => open(index)}
          >
            <div className="kiln-decksorter__thumb" style={{ width: WIDTH, height }}>
              <ThumbCanvas cache={cache} slide={slide} width={WIDTH} height={height} revision={revision} />
            </div>
            <span className="kiln-decksorter__number">{index + 1}</span>
          </div>
        );
      })}
    </div>
  );
}
