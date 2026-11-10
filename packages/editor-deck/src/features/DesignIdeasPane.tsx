/**
 * The Design Ideas pane (Design ▸ Designer ▸ Design Ideas): the core's suggestions for the
 * current slide — layouts that place its title, text and pictures, and a dark variant — as
 * pictures it draws; a click applies one (one undo step). The suggestions follow the slide as it
 * changes (asked again shortly after the last change).
 */
import { t } from '@kiln/i18n';
import type { DeckDesignIdea } from '@kiln/protocol';
import { Icon } from '@kiln/ui-kit';
import { Sparkles, X } from 'lucide-react';
import { useEffect, useRef, useState } from 'react';
import { useDeck } from '../hooks.ts';
import type { DeckEditorModel } from '../model.ts';

/** CSS px of a suggestion in the pane. */
export const IDEA_WIDTH = 248;
/** How long the slide must stay unchanged before the suggestions are asked again. */
const SETTLE_MS = 350;

/** A suggestion with its picture as an object URL. */
interface Shown {
  id: string;
  label: string;
  url: string;
}

/** Object URLs of the suggestions' PNGs (revoke them when done). */
export function ideaUrls(ideas: readonly DeckDesignIdea[]): Shown[] {
  return ideas.map((i) => ({
    id: i.id,
    label: i.label,
    url: URL.createObjectURL(new Blob([new Uint8Array(i.png)], { type: 'image/png' })),
  }));
}

export function DesignIdeasPane({ model }: { model: DeckEditorModel }) {
  const key = useDeck(model, (s) => {
    const d = s.deck;
    const slide = d?.slides[d.current];
    return slide ? `${slide.id}:${slide.version}` : '';
  });
  const [ideas, setIdeas] = useState<Shown[] | null>(null);
  const [busy, setBusy] = useState(false);
  const urls = useRef<Shown[]>([]);
  useEffect(() => {
    if (!key) {
      setIdeas([]);
      return;
    }
    let live = true;
    setBusy(true);
    const dpr = typeof window === 'undefined' ? 1 : Math.min(window.devicePixelRatio || 1, 2);
    const timer = setTimeout(() => {
      void model.query({ DesignIdeas: { width: IDEA_WIDTH, dpr } }).then((r) => {
        if (!live) return;
        const shown = r && 'DesignIdeas' in r ? ideaUrls(r.DesignIdeas) : [];
        for (const o of urls.current) URL.revokeObjectURL(o.url);
        urls.current = shown;
        setIdeas(shown);
        setBusy(false);
      });
    }, SETTLE_MS);
    return () => {
      live = false;
      clearTimeout(timer);
    };
  }, [model, key]);
  // The last pictures go with the pane.
  useEffect(
    () => () => {
      for (const o of urls.current) URL.revokeObjectURL(o.url);
      urls.current = [];
    },
    [],
  );
  const close = () => {
    model.set({ designPane: false });
    model.stage?.focus();
  };
  return (
    <aside className="kiln-sidepane" aria-label={t('Designer')} data-testid="deck-design-pane">
      <div className="kiln-sidepane__head">
        <h2>{t('Designer')}</h2>
        <button
          type="button"
          className="kiln-sidepane__close"
          aria-label={t('Close Designer')}
          onClick={close}
        >
          <Icon icon={X} size={14} />
        </button>
      </div>
      <p className="kiln-sidepane__note">
        <Icon icon={Sparkles} size={14} /> {t('Design ideas for this slide')}
      </p>
      <div className="kiln-designpane__list" aria-busy={busy || undefined}>
        {ideas === null ? (
          <p className="kiln-sidepane__empty">{t('Looking at the slide…')}</p>
        ) : ideas.length === 0 ? (
          <p className="kiln-sidepane__empty">{t('Add a title, text or a picture to get design ideas.')}</p>
        ) : (
          ideas.map((i) => (
            <button
              key={`${i.id}:${i.url}`}
              type="button"
              className="kiln-designpane__idea"
              data-idea={i.id}
              aria-label={i.label}
              title={i.label}
              onClick={() => {
                void model.core('deck.designer.apply', { id: i.id });
                // The keyboard goes back to the slide (Ctrl+Z undoes the idea).
                model.stage?.focus();
              }}
            >
              <img src={i.url} alt="" width={IDEA_WIDTH} draggable={false} />
            </button>
          ))
        )}
      </div>
    </aside>
  );
}
