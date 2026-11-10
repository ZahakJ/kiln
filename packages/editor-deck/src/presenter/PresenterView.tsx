/**
 * Presenter View: the slide the audience sees (the same show, painted
 * small), the next slide, the speaker notes with their size, the timer and the clock, the slide
 * counter, the pen, laser and screen tools, See All Slides and End Show. The keys of the show work
 * here; every action goes through `send`, which relays it to the audience's window.
 */

import type { Keymap } from '@kiln/commands';
import { formatDate, t } from '@kiln/i18n';
import type { DeckShowInfo, DeckShowStatus, DocId, KilnClient } from '@kiln/protocol';
import { Icon } from '@kiln/ui-kit';
import {
  ChevronLeft,
  ChevronRight,
  Eraser,
  Grid3x3,
  Highlighter,
  Minus,
  MousePointer2,
  Pause,
  PenLine,
  Play,
  Plus,
  RotateCcw,
  Square,
  Zap,
} from 'lucide-react';
import { useCallback, useEffect, useRef, useState } from 'react';
import { deckResult } from '../model.ts';
import type { ShowClock } from '../show/clock.ts';
import type { ShowEngine } from '../show/engine.ts';
import { ShowView, showBinding } from '../show/ShowView.tsx';
import { SlideGrid, ThumbCanvas, useSlideThumbs } from '../show/SlideGrid.tsx';
import './presenter.css';

export interface PresenterViewProps {
  client: KilnClient;
  doc: DocId;
  keymap: Keymap;
  send: (id: string, args?: unknown) => Promise<unknown>;
  onEnd: (status: DeckShowStatus) => void;
  clock?: ShowClock;
  /** The engine of the current slide (the route kicks it after a relayed command). */
  onEngine?: (engine: ShowEngine | null) => void;
  /** Whether the audience's window answered (shown in the top bar). */
  audience?: 'waiting' | 'connected' | 'none';
}

/** `h:mm:ss` or `mm:ss`. */
export function formatElapsed(ms: number): string {
  const s = Math.max(0, Math.floor(ms / 1000));
  const h = Math.floor(s / 3600);
  const m = Math.floor((s % 3600) / 60);
  const sec = s % 60;
  const two = (n: number) => String(n).padStart(2, '0');
  return h > 0 ? `${h}:${two(m)}:${two(sec)}` : `${two(m)}:${two(sec)}`;
}

function useTimer() {
  const [start, setStart] = useState(() => Date.now());
  const [paused, setPaused] = useState<number | null>(null);
  const [, tick] = useState(0);
  useEffect(() => {
    const t = setInterval(() => tick((x) => x + 1), 500);
    return () => clearInterval(t);
  }, []);
  const elapsed = (paused ?? Date.now()) - start;
  return {
    elapsed,
    paused: paused !== null,
    toggle: () => {
      if (paused === null) setPaused(Date.now());
      else {
        setStart((s) => s + (Date.now() - paused));
        setPaused(null);
      }
    },
    reset: () => {
      setStart(Date.now());
      setPaused((p) => (p === null ? null : Date.now()));
    },
  };
}

export function PresenterView({
  client,
  doc,
  keymap,
  send,
  onEnd,
  clock,
  onEngine,
  audience,
}: PresenterViewProps) {
  const [status, setStatus] = useState<DeckShowStatus | null>(null);
  const [info, setInfo] = useState<DeckShowInfo | null>(null);
  const [notesSize, setNotesSize] = useState(18);
  const [grid, setGrid] = useState(false);
  const [pens, setPens] = useState(false);
  const engine = useRef<ShowEngine | null>(null);
  const root = useRef<HTMLElement>(null);
  const timer = useTimer();
  const [now, setNow] = useState(() => new Date());
  useEffect(() => {
    const t = setInterval(() => setNow(new Date()), 10_000);
    return () => clearInterval(t);
  }, []);

  const run = useCallback(
    async (id: string, args?: unknown) => {
      try {
        await send(id, args);
      } catch {
        // A step the show refuses (it has just ended): nothing to show.
      } finally {
        engine.current?.kick();
      }
    },
    [send],
  );

  useEffect(() => {
    if (status === null) return;
    let live = true;
    void client
      .query(doc, { Deck: 'ShowInfo' })
      .then((r) => {
        const d = deckResult(r);
        if (live && d && 'ShowInfo' in d) setInfo(d.ShowInfo);
      })
      .catch(() => undefined);
    return () => {
      live = false;
    };
  }, [client, doc, status]);

  useEffect(() => {
    const onKey = (e: KeyboardEvent) => {
      if (e.defaultPrevented || grid) return;
      const t = e.target as HTMLElement | null;
      if (t?.closest('[data-kiln-shortcuts="off"]')) return;
      const b = showBinding(keymap, e);
      if (!b) return;
      e.preventDefault();
      void run(b.command, b.args);
    };
    window.addEventListener('keydown', onKey, true);
    return () => window.removeEventListener('keydown', onKey, true);
  }, [keymap, run, grid]);

  useEffect(() => {
    root.current?.focus({ preventScroll: true });
  }, []);

  const next = status?.next ?? null;
  const thumbs = useSlideThumbs(client, doc, next === null ? [] : [next], 360);
  const tool = status?.tool ?? 'Arrow';
  const focusView = () => root.current?.focus({ preventScroll: true });
  const toolButton = (id: string, label: string, icon: typeof PenLine, active: boolean) => (
    <button
      type="button"
      className="kiln-presenter__menuitem"
      aria-pressed={active}
      onClick={() => {
        setPens(false);
        void run(id).then(focusView);
      }}
    >
      <Icon icon={icon} size={16} />
      <span>{label}</span>
    </button>
  );
  return (
    <section
      ref={root}
      className="kiln-presenter"
      data-testid="deck-presenter"
      data-slide={status?.slide ?? ''}
      tabIndex={-1}
      aria-label={t('Presenter View')}
    >
      <header className="kiln-presenter__top">
        <div className="kiln-presenter__timer" data-testid="deck-presenter-timer">
          <span className="kiln-presenter__elapsed" role="timer" aria-label={t('Elapsed time')}>
            {formatElapsed(timer.elapsed)}
          </span>
          <button
            type="button"
            className="kiln-presenter__icon"
            aria-label={timer.paused ? t('Resume the timer') : t('Pause the timer')}
            onClick={() => {
              timer.toggle();
              focusView();
            }}
          >
            <Icon icon={timer.paused ? Play : Pause} size={16} />
          </button>
          <button
            type="button"
            className="kiln-presenter__icon"
            aria-label={t('Restart the timer')}
            onClick={() => {
              timer.reset();
              focusView();
            }}
          >
            <Icon icon={RotateCcw} size={16} />
          </button>
        </div>
        <span className="kiln-presenter__clock">
          {formatDate(now, { hour: '2-digit', minute: '2-digit' })}
        </span>
        <span className="kiln-presenter__audience" data-state={audience ?? 'none'}>
          {audience === 'connected'
            ? t('Audience window open')
            : audience === 'waiting'
              ? t('Opening the audience window…')
              : ''}
        </span>
        <button
          type="button"
          className="kiln-presenter__end"
          onClick={() => void run('deck.show.end')}
          data-testid="deck-presenter-end"
        >
          {t('End Slide Show')}
        </button>
      </header>
      <main className="kiln-presenter__main">
        <section className="kiln-presenter__current" aria-label={t('Current slide')}>
          <div className="kiln-presenter__slide">
            <ShowView
              client={client}
              doc={doc}
              keymap={keymap}
              send={send}
              onEnd={onEnd}
              clock={clock}
              embedded
              onStatus={setStatus}
              onEngine={(e) => {
                engine.current = e;
                onEngine?.(e);
              }}
            />
          </div>
          <div className="kiln-presenter__tools">
            <div className="kiln-presenter__toolgroup">
              <button
                type="button"
                className="kiln-presenter__icon"
                aria-label={t('Pen and laser pointer tools')}
                aria-expanded={pens}
                data-active={tool !== 'Arrow' || undefined}
                onClick={() => setPens((p) => !p)}
              >
                <Icon icon={PenLine} size={18} />
              </button>
              {pens ? (
                <fieldset className="kiln-presenter__menu" aria-label={t('Pointer options')}>
                  {toolButton('deck.show.laser', 'Laser Pointer', Zap, tool === 'Laser')}
                  {toolButton('deck.show.pen', 'Pen', PenLine, tool === 'Pen')}
                  {toolButton('deck.show.highlighter', 'Highlighter', Highlighter, tool === 'Highlighter')}
                  {toolButton('deck.show.eraser', 'Eraser', Eraser, tool === 'Eraser')}
                  {toolButton('deck.show.erase', 'Erase All Ink on Slide', Eraser, false)}
                  {toolButton('deck.show.arrow', 'Arrow', MousePointer2, tool === 'Arrow')}
                </fieldset>
              ) : null}
            </div>
            <button
              type="button"
              className="kiln-presenter__icon"
              aria-label={t('See all slides')}
              onClick={() => setGrid(true)}
            >
              <Icon icon={Grid3x3} size={18} />
            </button>
            <button
              type="button"
              className="kiln-presenter__icon"
              aria-label={t('Black or unblack the slide show')}
              data-active={status?.screen === 'Black' || undefined}
              onClick={() => void run('deck.show.black').then(focusView)}
            >
              <Icon icon={Square} size={18} />
            </button>
            <span className="kiln-presenter__spacer" />
            <button
              type="button"
              className="kiln-presenter__nav"
              aria-label={t('Previous')}
              onClick={() => void run('deck.show.previous').then(focusView)}
            >
              <Icon icon={ChevronLeft} size={22} />
            </button>
            <span className="kiln-presenter__count" data-testid="deck-presenter-count">
              {status
                ? t('Slide {position} of {count}', { position: status.position, count: status.count })
                : ''}
            </span>
            <button
              type="button"
              className="kiln-presenter__nav"
              aria-label={t('Next')}
              onClick={() => void run('deck.show.next').then(focusView)}
            >
              <Icon icon={ChevronRight} size={22} />
            </button>
          </div>
        </section>
        <aside className="kiln-presenter__side">
          <div className="kiln-presenter__next">
            <h2>{t('Next')}</h2>
            {next === null ? (
              <div className="kiln-presenter__nonext">{t('End of slide show')}</div>
            ) : (
              <ThumbCanvas
                source={thumbs.get(next)}
                label={t('Next: slide {value}', { value: next + 1 })}
                className="kiln-presenter__nextthumb"
              />
            )}
          </div>
          <div className="kiln-presenter__notes">
            <div className="kiln-presenter__noteshead">
              <h2>{t('Notes')}</h2>
              <button
                type="button"
                className="kiln-presenter__icon"
                aria-label={t('Make the text smaller')}
                onClick={() => setNotesSize((s) => Math.max(10, s - 2))}
              >
                <Icon icon={Minus} size={16} />
              </button>
              <button
                type="button"
                className="kiln-presenter__icon"
                aria-label={t('Make the text larger')}
                onClick={() => setNotesSize((s) => Math.min(48, s + 2))}
              >
                <Icon icon={Plus} size={16} />
              </button>
            </div>
            <div
              className="kiln-presenter__notestext"
              data-testid="deck-presenter-notes"
              style={{ fontSize: `${notesSize}px` }}
            >
              {info?.notes ? info.notes : <span className="kiln-presenter__nonotes">{t('No notes')}</span>}
            </div>
          </div>
        </aside>
      </main>
      {grid ? (
        <SlideGrid
          client={client}
          doc={doc}
          current={status?.slide ?? 0}
          onPick={(index) => {
            setGrid(false);
            void run('deck.show.go_to', { index }).then(focusView);
          }}
          onClose={() => {
            setGrid(false);
            focusView();
          }}
        />
      ) : null}
    </section>
  );
}
