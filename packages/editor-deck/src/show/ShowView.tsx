/**
 * The slide show on a screen: the core's show painted full size, the keys
 * of the show (`slideshow` scope of the keymap: N, P, arrows, Space, number and Enter, B, W, Home,
 * End, Esc, Ctrl+L, Ctrl+P, Ctrl+I, Ctrl+E, E, Ctrl+A), the pointer (clicks advance; on a touch
 * screen a tap on the left third goes back; the pen, highlighter, eraser and laser draw on the
 * slide), PowerPoint's faint toolbar at the bottom left, See All Slides, the slide's video and
 * audio played over it, and the "End of slide show" screen. Every action is a core command; `send` lets the route relay it to the other
 * window of Presenter View.
 */

import type { Keymap } from '@kiln/commands';
import { t } from '@kiln/i18n';
import type { DeckShowFrame, DeckShowStatus, DocId, KilnClient } from '@kiln/protocol';
import { announce, Icon } from '@kiln/ui-kit';
import {
  ChevronLeft,
  ChevronRight,
  Eraser,
  Grid3x3,
  Highlighter,
  MousePointer2,
  PenLine,
  Square,
  X,
  Zap,
} from 'lucide-react';
import { type ReactNode, useCallback, useEffect, useRef, useState } from 'react';
import { ShowTouch } from '../mobile/PhoneShow.tsx';
import type { ShowClock } from './clock.ts';
import { ShowEngine } from './engine.ts';
import { MediaLayer } from './MediaLayer.tsx';
import { SlideGrid } from './SlideGrid.tsx';
import './show.css';

export interface ShowViewProps {
  client: KilnClient;
  doc: DocId;
  keymap: Keymap;
  /** Runs a show command (and, in Presenter View, relays it to the other window). */
  send: (id: string, args?: unknown) => Promise<unknown>;
  /** The show is over. */
  onEnd: (status: DeckShowStatus) => void;
  clock?: ShowClock;
  /** The current slide of Presenter View: no toolbar, no keys (the presenter's view has them). */
  embedded?: boolean;
  /** Reading View: the show inside the window, with its keys and clicks but no toolbar. */
  windowed?: boolean;
  /** Every frame's status. */
  onStatus?: (status: DeckShowStatus) => void;
  /** The engine, once it runs (tests and Presenter View kick it after relayed commands). */
  onEngine?: (engine: ShowEngine | null) => void;
  /** Shown over the slide (the rehearsal's timer). */
  children?: ReactNode;
}

/** Milliseconds without the mouse moving before the cursor and toolbar hide. */
const IDLE_MS = 3000;

/** The keymap binding of a key event during the show. */
export function showBinding(keymap: Keymap, e: KeyboardEvent): { command: string; args?: unknown } | null {
  const b = keymap.resolve(e, { app: 'deck', scopes: ['slideshow'] });
  return b && b.scope === 'slideshow' ? { command: b.command, args: b.args } : null;
}

export function ShowView({
  client,
  doc,
  keymap,
  send,
  onEnd,
  clock,
  embedded = false,
  windowed = false,
  onStatus,
  onEngine,
  children,
}: ShowViewProps) {
  const root = useRef<HTMLDivElement>(null);
  const canvas = useRef<HTMLCanvasElement>(null);
  const engine = useRef<ShowEngine | null>(null);
  const [status, setStatus] = useState<DeckShowStatus | null>(null);
  const [idle, setIdle] = useState(false);
  const [grid, setGrid] = useState(false);
  const [pens, setPens] = useState(false);
  const idleTimer = useRef<ReturnType<typeof setTimeout> | null>(null);
  const ended = useRef(onEnd);
  ended.current = onEnd;
  const statusRef = useRef(onStatus);
  statusRef.current = onStatus;
  const engineRef = useRef(onEngine);
  engineRef.current = onEngine;
  const currentEngine = useCallback(() => engine.current, []);

  // The slide shown and the end screen, read out as they change (PowerPoint's narration).
  const position = status ? `${status.position}/${status.count}` : '';
  const screenShown = status?.screen;
  useEffect(() => {
    if (embedded || !status) return;
    if (status.screen === 'End') announce(t('End of slide show'));
    else if (status.screen === 'Black') announce(t('Black screen'));
    else if (status.screen === 'White') announce(t('White screen'));
    else announce(t('Slide {position} of {count}', { position: status.position, count: status.count }));
    // Only the position and the screen are read.
  }, [position, screenShown, embedded]);

  useEffect(() => {
    const cv = canvas.current;
    if (!cv) return;
    let last = -1;
    const e = new ShowEngine(client, doc, cv, {
      clock,
      onFrame: (f: DeckShowFrame) => {
        if (f.status.version !== last) {
          last = f.status.version;
          setStatus(f.status);
          statusRef.current?.(f.status);
        }
      },
      onEnd: (s) => ended.current(s),
    });
    engine.current = e;
    e.start();
    engineRef.current?.(e);
    return () => {
      engineRef.current?.(null);
      e.dispose();
      engine.current = null;
    };
  }, [client, doc, clock]);

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

  // The keys of the show.
  useEffect(() => {
    if (embedded) return;
    const onKey = (e: KeyboardEvent) => {
      if (e.defaultPrevented) return;
      const t = e.target as HTMLElement | null;
      const off = t?.closest('[data-kiln-shortcuts="off"]');
      if (grid) {
        // The grid has the keys (Tab, the arrows, Enter and Space on a slide); Escape closes it.
        if (e.key === 'Escape' && !off) {
          e.preventDefault();
          setGrid(false);
        }
        return;
      }
      if (off) return;
      // Enter and Space on a button of the show's toolbar press it (See all slides, End show).
      if ((e.key === 'Enter' || e.key === ' ') && t?.closest('button')) return;
      const b = showBinding(keymap, e);
      if (!b) return;
      e.preventDefault();
      e.stopPropagation();
      void run(b.command, b.args);
    };
    window.addEventListener('keydown', onKey, true);
    return () => window.removeEventListener('keydown', onKey, true);
  }, [keymap, run, embedded, grid]);

  useEffect(() => {
    if (!embedded) root.current?.focus({ preventScroll: true });
  }, [embedded]);

  const wake = () => {
    setIdle(false);
    if (idleTimer.current) clearTimeout(idleTimer.current);
    idleTimer.current = setTimeout(() => setIdle(true), IDLE_MS);
  };
  useEffect(
    () => () => {
      if (idleTimer.current) clearTimeout(idleTimer.current);
    },
    [],
  );

  const tool = status?.tool ?? 'Arrow';
  const pointer = (phase: 'Down' | 'Move' | 'Up' | 'Leave') => (e: React.PointerEvent<HTMLCanvasElement>) => {
    if (embedded && tool === 'Arrow') return;
    // a finger's taps and swipes with the arrow are the touch layer's (ShowTouch).
    if (e.pointerType === 'touch' && tool === 'Arrow' && !embedded) return;
    if (phase === 'Down' && e.button !== 0) return;
    const p = engine.current?.slidePoint(e.clientX, e.clientY);
    if (!p) return;
    if (phase === 'Down') {
      try {
        e.currentTarget.setPointerCapture(e.pointerId);
      } catch {
        // Not every pointer can be captured (synthetic events).
      }
    }
    // With the arrow, only presses and releases matter (moves only wake the toolbar).
    if (phase === 'Move' && tool === 'Arrow') {
      wake();
      return;
    }
    void run('deck.show.pointer', { phase, x: p.x, y: p.y, touch: e.pointerType === 'touch' });
  };

  const screen = status?.screen ?? 'Slide';
  const cursor =
    tool === 'Laser'
      ? 'none'
      : tool === 'Pen' || tool === 'Highlighter'
        ? 'crosshair'
        : tool === 'Eraser'
          ? 'cell'
          : idle
            ? 'none'
            : screen === 'End'
              ? 'pointer'
              : 'default';
  const focusShow = () => root.current?.focus({ preventScroll: true });
  const toolButton = (id: string, label: string, icon: typeof PenLine, active: boolean) => (
    <button
      type="button"
      className="kiln-show__tool"
      aria-label={label}
      aria-pressed={active}
      title={label}
      onClick={() => {
        setPens(false);
        void run(id).then(focusShow);
      }}
    >
      <Icon icon={icon} size={16} />
      <span>{label}</span>
    </button>
  );

  return (
    <div
      ref={root}
      className="kiln-show"
      data-testid={embedded ? 'deck-show-embedded' : 'deck-show'}
      data-screen={screen}
      data-tool={tool}
      data-slide={status?.slide ?? ''}
      data-click={status?.click ?? ''}
      data-embedded={embedded || undefined}
      data-windowed={windowed || undefined}
      tabIndex={-1}
      role="application"
      aria-roledescription={t('slide show')}
      aria-label={
        status
          ? t('Slide {position} of {count}', { position: status.position, count: status.count })
          : t('Slide show')
      }
      onPointerMove={embedded ? undefined : wake}
      onContextMenu={(e) => e.preventDefault()}
    >
      <canvas
        ref={canvas}
        className="kiln-show__canvas"
        data-testid={embedded ? 'deck-show-embedded-canvas' : 'deck-show-canvas'}
        style={{ cursor }}
        onPointerDown={pointer('Down')}
        onPointerMove={pointer('Move')}
        onPointerUp={pointer('Up')}
        onPointerLeave={pointer('Leave')}
      />
      {!embedded && screen === 'Slide' ? (
        <MediaLayer client={client} doc={doc} slide={status?.slide ?? null} engine={currentEngine} />
      ) : null}
      {screen === 'End' && !embedded ? (
        <div className="kiln-show__end">{t('End of slide show, click to exit.')}</div>
      ) : null}
      {children}
      {!embedded && !windowed ? (
        <ShowTouch
          root={root}
          engine={currentEngine}
          run={run}
          client={client}
          doc={doc}
          slide={status?.slide ?? null}
          tool={tool}
        />
      ) : null}
      {!embedded && !windowed && status?.kind !== 'Kiosk' ? (
        <nav
          className="kiln-show__bar"
          data-hidden={idle && !pens ? '' : undefined}
          aria-label={t('Slide show')}
        >
          <button
            type="button"
            className="kiln-show__btn"
            aria-label={t('Previous')}
            title={t('Previous')}
            onClick={() => void run('deck.show.previous').then(focusShow)}
          >
            <Icon icon={ChevronLeft} size={18} />
          </button>
          <button
            type="button"
            className="kiln-show__btn"
            aria-label={t('Next')}
            title={t('Next')}
            onClick={() => void run('deck.show.next').then(focusShow)}
          >
            <Icon icon={ChevronRight} size={18} />
          </button>
          <span className="kiln-show__sep" />
          <button
            type="button"
            className="kiln-show__btn"
            aria-label={t('Pen and laser pointer tools')}
            aria-expanded={pens}
            title={t('Pen and laser pointer tools')}
            data-active={tool !== 'Arrow' || undefined}
            onClick={() => setPens((p) => !p)}
          >
            <Icon icon={PenLine} size={18} />
          </button>
          <button
            type="button"
            className="kiln-show__btn"
            aria-label={t('See all slides')}
            title={t('See all slides')}
            onClick={() => setGrid(true)}
          >
            <Icon icon={Grid3x3} size={18} />
          </button>
          <button
            type="button"
            className="kiln-show__btn"
            aria-label={t('Black screen')}
            title={t('Black screen (B)')}
            data-active={screen === 'Black' || undefined}
            onClick={() => void run('deck.show.black').then(focusShow)}
          >
            <Icon icon={Square} size={18} />
          </button>
          <button
            type="button"
            className="kiln-show__btn"
            aria-label={t('End show')}
            title={t('End show (Esc)')}
            onClick={() => void run('deck.show.end')}
          >
            <Icon icon={X} size={18} />
          </button>
          {pens ? (
            <fieldset className="kiln-show__tools" aria-label={t('Pointer options')}>
              {toolButton('deck.show.laser', 'Laser Pointer', Zap, tool === 'Laser')}
              {toolButton('deck.show.pen', 'Pen', PenLine, tool === 'Pen')}
              {toolButton('deck.show.highlighter', 'Highlighter', Highlighter, tool === 'Highlighter')}
              {toolButton('deck.show.eraser', 'Eraser', Eraser, tool === 'Eraser')}
              {toolButton('deck.show.erase', 'Erase All Ink on Slide', Eraser, false)}
              {toolButton('deck.show.arrow', 'Arrow', MousePointer2, tool === 'Arrow')}
            </fieldset>
          ) : null}
        </nav>
      ) : null}
      {grid ? (
        <SlideGrid
          client={client}
          doc={doc}
          current={status?.slide ?? 0}
          onPick={(index) => {
            setGrid(false);
            void run('deck.show.go_to', { index }).then(focusShow);
          }}
          onClose={() => {
            setGrid(false);
            focusShow();
          }}
        />
      ) : null}
    </div>
  );
}
