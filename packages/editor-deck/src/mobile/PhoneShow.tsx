/**
 * The slide show under a finger: Present goes full screen and turns to landscape
 * on a phone (where the browser lets a page lock it; otherwise a hint asks to turn it); a swipe
 * goes to the next or previous slide, a tap is the show's own click (the left third goes back,
 * the rest forward, a link opens), and Notes shows the speaker notes over the slide's foot.
 */
import { isRtl, t } from '@kiln/i18n';
import type { DocId, KilnClient } from '@kiln/protocol';
import { attachGestures } from '@kiln/surface';
import { Icon } from '@kiln/ui-kit';
import { NotebookPen, Smartphone, X } from 'lucide-react';
import { type RefObject, useEffect, useState } from 'react';
import type { ShowEngine } from '../show/engine.ts';
import './mobile.css';

/** A phone's screen (a touch screen whose short side is a phone's). */
export function isPhoneScreen(): boolean {
  if (typeof window === 'undefined' || typeof matchMedia !== 'function') return false;
  return matchMedia('(pointer: coarse)').matches && Math.min(window.innerWidth, window.innerHeight) < 640;
}

/** Turns a phone to landscape for the show (full screen only; a browser may refuse). */
export async function lockLandscape(): Promise<void> {
  if (!isPhoneScreen()) return;
  const o = (screen as Screen & { orientation?: { lock?: (o: string) => Promise<void> } }).orientation;
  await o?.lock?.('landscape').catch(() => undefined);
}

/** Gives the screen its own orientation back when the show ends. */
export function unlockOrientation(): void {
  const o = (screen as Screen & { orientation?: { unlock?: () => void } }).orientation;
  try {
    o?.unlock?.();
  } catch {
    // Not locked (a browser that cannot lock it).
  }
}

export function ShowTouch({
  root,
  engine,
  run,
  client,
  doc,
  slide,
  tool,
}: {
  root: RefObject<HTMLDivElement | null>;
  engine: () => ShowEngine | null;
  run: (id: string, args?: unknown) => Promise<void>;
  client: KilnClient;
  doc: DocId;
  slide: number | null;
  tool: string;
}) {
  const [notesOpen, setNotesOpen] = useState(false);
  const [notes, setNotes] = useState<string | null>(null);
  const [portrait, setPortrait] = useState(false);
  const [hint, setHint] = useState(true);

  // Swipes and taps (the arrow; the pens draw with the finger).
  useEffect(() => {
    const el = root.current;
    if (!el || tool !== 'Arrow') return;
    const point = (x: number, y: number) => engine()?.slidePoint(x, y) ?? null;
    return attachGestures(
      el,
      {
        onSwipe: (dir) => {
          if (dir !== 'left' && dir !== 'right') return;
          // The slides go the way the interface reads.
          const forward = isRtl() ? dir === 'right' : dir === 'left';
          void run(forward ? 'deck.show.next' : 'deck.show.previous');
        },
        onTap: (p) => {
          const q = point(p.x, p.y);
          if (!q) return;
          void run('deck.show.pointer', { phase: 'Down', ...q, touch: true }).then(() =>
            run('deck.show.pointer', { phase: 'Up', ...q, touch: true }),
          );
        },
      },
      { ignore: '.kiln-show__bar, .kiln-showtouch, .kiln-show__grid' },
    );
  }, [root, engine, run, tool]);

  // The speaker notes of the slide shown.
  useEffect(() => {
    if (!notesOpen) return;
    let live = true;
    void client
      .query(doc, { Deck: 'ShowInfo' })
      .then((r) => {
        const d = (r as { Deck?: { ShowInfo?: { notes?: string } } }).Deck;
        if (live) setNotes(d?.ShowInfo?.notes ?? '');
      })
      .catch(() => {
        if (live) setNotes('');
      });
    return () => {
      live = false;
    };
  }, [client, doc, slide, notesOpen]);

  // Held upright: a hint to turn the phone, for a while.
  useEffect(() => {
    if (!isPhoneScreen()) return;
    const mq = matchMedia('(orientation: portrait)');
    const on = () => setPortrait(mq.matches);
    on();
    mq.addEventListener('change', on);
    const timer = setTimeout(() => setHint(false), 4000);
    return () => {
      mq.removeEventListener('change', on);
      clearTimeout(timer);
    };
  }, []);

  return (
    <div className="kiln-showtouch">
      {portrait && hint ? (
        <div className="kiln-showtouch__hint" role="status">
          <Icon icon={Smartphone} size={18} className="kiln-showtouch__turn" />
          <span>{t('Turn your phone sideways for a larger slide')}</span>
        </div>
      ) : null}
      {notesOpen ? (
        <section
          className="kiln-showtouch__notes"
          aria-label={t('Speaker notes')}
          data-testid="deck-show-notes"
        >
          <div className="kiln-showtouch__noteshead">
            <h2>{t('Notes')}</h2>
            <button
              type="button"
              className="kiln-showtouch__close"
              aria-label={t('Hide notes')}
              onClick={() => setNotesOpen(false)}
            >
              <Icon icon={X} size={18} />
            </button>
          </div>
          <div className="kiln-showtouch__notestext">
            {notes === null ? '' : notes || <span className="kiln-showtouch__none">{t('No notes')}</span>}
          </div>
        </section>
      ) : null}
      <button
        type="button"
        className="kiln-showtouch__toggle"
        aria-pressed={notesOpen}
        data-testid="deck-show-notes-toggle"
        onClick={() => setNotesOpen((o) => !o)}
      >
        <Icon icon={NotebookPen} size={18} />
        <span>{t('Notes')}</span>
      </button>
    </div>
  );
}
