/**
 * Kiln Deck on a phone: a presentation opens on its slides as a vertical list,
 * with a floating + for a new slide; a tap opens that slide full width to edit, with the slide
 * bar under it (the previous and next slide, where you are, the speaker notes and Present) and
 * the notes under that when they are shown. Back (the app bar's, the browser's, Android's) goes
 * from the slide to the list before it leaves the presentation.
 */
import { t } from '@kiln/i18n';
import { swallowRelease } from '@kiln/surface';
import { Icon } from '@kiln/ui-kit';
import { ChevronLeft, ChevronRight, EyeOff, NotebookPen, Play, Plus } from 'lucide-react';
import { useEffect, useMemo, useRef, useState } from 'react';
import { useDeck } from '../hooks.ts';
import type { DeckEditorModel } from '../model.ts';
import { ThumbCanvas } from '../ThumbnailRail.tsx';
import { ThumbCache } from '../thumbs.ts';
import './mobile.css';

/** A finger held this long on a slide opens its menu (ms). */
const HOLD_MS = 500;

export type PhoneScreen = 'list' | 'slide';

/**
 * The phone's screen: the slide list, or a slide being edited. While a slide is open its element
 * is a layer Back closes (`data-kiln-back-layer`; the `kiln:back` event).
 */
export function usePhoneScreen(phone: boolean): [PhoneScreen, (s: PhoneScreen) => void] {
  const [screen, setScreen] = useState<PhoneScreen>('list');
  useEffect(() => {
    if (!phone) setScreen('list');
  }, [phone]);
  return [screen, setScreen];
}

/** Listens for Back on `el` while the slide is open. */
export function useBackToList(el: HTMLElement | null, open: boolean, toList: () => void): void {
  useEffect(() => {
    if (!el || !open) return;
    const back = () => toList();
    el.addEventListener('kiln:back', back);
    return () => el.removeEventListener('kiln:back', back);
  }, [el, open, toList]);
}

export function PhoneSlideList({
  model,
  theme,
  onOpen,
}: {
  model: DeckEditorModel;
  theme: 'light' | 'dark';
  onOpen: () => void;
}) {
  const deck = useDeck(model, (s) => s.deck);
  const list = useRef<HTMLUListElement>(null);
  const [width, setWidth] = useState(360);
  const cache = useMemo(() => new ThumbCache(model, 360, theme), [model]);
  const [revision, setRevision] = useState(0);
  const hold = useRef<{ timer: ReturnType<typeof setTimeout>; fired: boolean } | null>(null);

  useEffect(() => cache.subscribe(() => setRevision(cache.revision)), [cache]);
  useEffect(() => {
    const el = list.current;
    if (!el || typeof ResizeObserver === 'undefined') return;
    const ro = new ResizeObserver(() => setWidth(Math.max(160, Math.round(el.clientWidth - 64))));
    ro.observe(el);
    return () => ro.disconnect();
  }, []);
  useEffect(() => {
    cache.setTheme(theme);
    cache.setWidth(width);
    if (model.state.deck) void cache.update(model.state.deck.slides);
  }, [cache, theme, width, model]);
  useEffect(() => {
    if (deck) void cache.update(deck.slides);
  }, [cache, deck?.slides, deck]);
  // The current slide stays in view (a new one, the one just edited).
  useEffect(() => {
    list.current
      ?.querySelector<HTMLElement>(`[data-slide-index="${deck?.current ?? 0}"]`)
      ?.scrollIntoView?.({ block: 'nearest' });
  }, [deck?.current, deck?.slides.length]);

  if (!deck) return <div className="kiln-deckphone" />;
  const height = (width * deck.slide_height) / Math.max(1, deck.slide_width);
  const open = (index: number) => {
    void model.core('deck.slides.select', { index, extend: false, toggle: false }).then(() => onOpen());
  };
  return (
    <div className="kiln-deckphone" data-testid="deck-phone-list">
      <div className="kiln-deckphone__head">
        <h2 className="kiln-deckphone__count">
          {t('{count, plural, one {# slide} other {# slides}}', { count: deck.slides.length })}
        </h2>
        <button
          type="button"
          className="kiln-deckphone__present"
          data-testid="deck-phone-present-all"
          onClick={() => void model.run('deck.show.from_beginning')}
        >
          <Icon icon={Play} size={16} />
          <span>{t('Present')}</span>
        </button>
      </div>
      <ul ref={list} className="kiln-deckphone__list" aria-label={t('Slides')}>
        {deck.slides.map((slide, index) => {
          const current = index === deck.current;
          return (
            <li key={slide.id} className="kiln-deckphone__row">
              <button
                type="button"
                className="kiln-deckphone__slide"
                data-slide-index={index}
                data-current={current || undefined}
                data-hidden={slide.hidden || undefined}
                aria-current={current ? 'true' : undefined}
                onPointerDown={(e) => {
                  if (e.pointerType !== 'touch') return;
                  const at = { x: e.clientX, y: e.clientY };
                  const h = {
                    fired: false,
                    timer: setTimeout(() => {
                      h.fired = true;
                      swallowRelease();
                      // A long press: the slide's menu (Duplicate, Delete, Hide…), as a right-click.
                      void model.core('deck.slides.select', { index, extend: false, toggle: false });
                      model.set({ menu: { ...at, kind: 'rail' } });
                    }, HOLD_MS),
                  };
                  hold.current = h;
                }}
                onPointerMove={() => {
                  if (hold.current && !hold.current.fired) clearTimeout(hold.current.timer);
                }}
                onPointerUp={() => {
                  if (hold.current && !hold.current.fired) clearTimeout(hold.current.timer);
                }}
                onClick={() => {
                  if (hold.current?.fired) {
                    hold.current = null;
                    return;
                  }
                  open(index);
                }}
                onContextMenu={(e) => {
                  e.preventDefault();
                  if (hold.current?.fired) return;
                  void model.core('deck.slides.select', { index, extend: false, toggle: false });
                  model.set({ menu: { x: e.clientX, y: e.clientY, kind: 'rail' } });
                }}
              >
                <span className="kiln-deckphone__number">
                  <span className="kiln-visually-hidden">{t('Slide')} </span>
                  {index + 1}
                </span>
                <span className="kiln-deckphone__thumb" style={{ height }}>
                  <ThumbCanvas
                    cache={cache}
                    slide={slide}
                    width={width}
                    height={height}
                    revision={revision}
                  />
                  {slide.hidden ? (
                    <span className="kiln-deckphone__hidden">
                      <Icon icon={EyeOff} size={14} />
                    </span>
                  ) : null}
                </span>
                <span className="kiln-deckphone__title">
                  {slide.title || t('Untitled slide')}
                  {slide.hidden ? <span className="kiln-visually-hidden">{t(', hidden')}</span> : null}
                </span>
              </button>
            </li>
          );
        })}
      </ul>
      <button
        type="button"
        className="kiln-deckphone__fab"
        aria-label={t('New Slide')}
        data-testid="deck-phone-new-slide"
        onClick={() => void model.run('deck.slides.new_slide')}
      >
        <Icon icon={Plus} size={26} />
      </button>
    </div>
  );
}

/** Under the slide being edited: the previous and next slide, where you are, Notes and Present. */
export function PhoneSlideBar({ model }: { model: DeckEditorModel }) {
  const current = useDeck(model, (s) => s.deck?.current ?? 0);
  const count = useDeck(model, (s) => s.deck?.slides.length ?? 0);
  const notes = useDeck(model, (s) => s.deck?.notes_visible ?? true);
  return (
    <div className="kiln-deckphone__bar" role="toolbar" aria-label={t('Slide')} data-testid="deck-phone-bar">
      <button
        type="button"
        className="kiln-deckphone__barbtn"
        aria-label={t('Previous Slide')}
        disabled={current <= 0}
        onClick={() => void model.core('deck.slides.previous_slide')}
      >
        <Icon icon={ChevronLeft} size={20} className="kiln-deckphone__flip" />
      </button>
      <span className="kiln-deckphone__where" aria-live="polite">
        {t('Slide {position} of {count}', { position: current + 1, count })}
      </span>
      <button
        type="button"
        className="kiln-deckphone__barbtn"
        aria-label={t('Next Slide')}
        disabled={current >= count - 1}
        onClick={() => void model.core('deck.slides.next_slide')}
      >
        <Icon icon={ChevronRight} size={20} className="kiln-deckphone__flip" />
      </button>
      <span className="kiln-deckphone__gap" />
      <button
        type="button"
        className="kiln-deckphone__chip"
        aria-pressed={notes}
        data-testid="deck-phone-notes"
        onClick={() => void model.run('deck.view.notes')}
      >
        <Icon icon={NotebookPen} size={16} />
        <span>{t('Notes')}</span>
      </button>
      <button
        type="button"
        className="kiln-deckphone__play"
        aria-label={t('Present from this slide')}
        data-testid="deck-phone-present"
        onClick={() => void model.run('deck.show.from_current')}
      >
        <Icon icon={Play} size={18} />
      </button>
    </div>
  );
}
