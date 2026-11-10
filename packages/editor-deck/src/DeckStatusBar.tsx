/**
 * Deck's status bar (the Deck mockup): "Slide 3 of 12", the language, the
 * Notes toggle, the view buttons (Normal, Slide Sorter, Reading View), the zoom slider and Fit
 * Slide to Current Window.
 */
import { languageName, msg, t } from '@kiln/i18n';
import type { ViewMode } from '@kiln/protocol';
import { Icon, StatusBar, type StatusBarItem, type StatusBarView } from '@kiln/ui-kit';
import { Maximize, Presentation } from 'lucide-react';
import { useMemo, useSyncExternalStore } from 'react';
import { useDeck } from './hooks.ts';
import type { DeckEditorModel } from './model.ts';
import { readingStep } from './views/ReadingView.tsx';

const VIEWS: readonly StatusBarView[] = [
  { id: 'SlideNormal', label: msg('Normal'), icon: 'NormalView' },
  { id: 'SlideSorter', label: msg('Slide Sorter'), icon: 'SlideSorter' },
  { id: 'ReadingView', label: msg('Reading View'), icon: 'ReadingView' },
];

/** The left items: the slide position, the language, read-only when it applies. */
export function slideItems(
  current: number | undefined,
  total: number | undefined,
  readOnly: boolean,
  hidden: boolean,
  theme?: string,
  master = false,
  language = 'en-US',
): StatusBarItem[] {
  const out: StatusBarItem[] = [];
  if (master) out.push({ id: 'slide', label: t('View'), content: t('Slide Master'), announce: true });
  else if (total !== undefined && current !== undefined)
    out.push({
      id: 'slide',
      label: t('Slide number'),
      content: hidden
        ? t('Slide {current, number} of {total, number} (hidden)', {
            current: Math.min(current + 1, total),
            total,
          })
        : t('Slide {current, number} of {total, number}', { current: Math.min(current + 1, total), total }),
    });
  if (theme)
    out.push({
      id: 'theme',
      label: t('Theme'),
      content: <span className="kiln-deckstatus__wide">{theme}</span>,
    });
  out.push({ id: 'language', label: t('Language'), content: languageName(language) });
  if (readOnly) out.push({ id: 'readonly', label: t('Read-only'), content: t('Read-only'), announce: true });
  return out;
}

/** A registry command's state, followed as it changes. */
function useCommandState(model: DeckEditorModel, id: string) {
  const reg = model.registry;
  const key = useSyncExternalStore(
    (fn) => reg.subscribe(fn),
    () => JSON.stringify(reg.getState(id)),
  );
  return useMemo(() => JSON.parse(key) as ReturnType<typeof reg.getState>, [key]);
}

export function DeckStatusBar({ model, readOnly }: { model: DeckEditorModel; readOnly: boolean }) {
  const current = useDeck(model, (s) => s.deck?.current);
  const total = useDeck(model, (s) => s.deck?.slides.length);
  const hidden = useDeck(model, (s) => (s.deck ? (s.deck.slides[s.deck.current]?.hidden ?? false) : false));
  const notes = useDeck(model, (s) => s.deck?.notes_visible ?? true);
  const status = useDeck(model, (s) => s.status);
  const language = status?.language ?? undefined;
  const fit = useDeck(model, (s) => s.fit);
  const theme = useDeck(model, (s) => s.deck?.theme);
  const master = useDeck(model, (s) => !!s.deck?.master_view);
  const comments = useCommandState(model, 'suite.review.show_comments');
  const left = useMemo(
    () => slideItems(current, total, readOnly, hidden, theme, master, language),
    [current, total, readOnly, hidden, theme, master, language],
  );
  const reading = useDeck(model, (s) => s.status?.view === 'ReadingView' && !s.deck?.master_view);
  const right = useMemo<StatusBarItem[]>(
    () => [
      // Reading View's Previous and Next, as PowerPoint's status bar has them.
      ...(reading
        ? (['Previous', 'Next'] as const).map((step) => ({
            id: step.toLowerCase(),
            label: step === 'Next' ? t('Next') : t('Previous'),
            content: (
              <button
                type="button"
                className="kiln-deckstatus__toggle"
                data-testid={`deck-reading-${step.toLowerCase()}`}
                onClick={() => void readingStep(model, step === 'Next')}
              >
                {step === 'Next' ? t('Next') : t('Previous')}
              </button>
            ),
          }))
        : []),
      {
        id: 'notes',
        label: t('Notes'),
        content: (
          <button
            type="button"
            className="kiln-deckstatus__toggle"
            aria-pressed={notes}
            data-testid="deck-notes-toggle"
            onClick={() => void model.run('deck.view.notes')}
          >
            {t('Notes')}
          </button>
        ),
      },
      ...(comments.visible
        ? [
            {
              id: 'comments',
              label: t('Comments'),
              content: (
                <button
                  type="button"
                  className="kiln-deckstatus__toggle kiln-deckstatus__wide"
                  aria-pressed={comments.checked === true}
                  disabled={!comments.enabled || comments.pending}
                  title={comments.pending ? t('Comments are not in this build yet') : undefined}
                  data-testid="deck-comments-toggle"
                  onClick={() => void model.run('suite.review.show_comments')}
                >
                  {t('Comments')}
                </button>
              ),
            },
          ]
        : []),
      {
        id: 'fit',
        label: t('Fit slide to current window'),
        content: (
          <button
            type="button"
            className="kiln-deckstatus__fit"
            aria-label={t('Fit slide to current window')}
            aria-pressed={fit}
            title={t('Fit slide to current window')}
            data-testid="deck-fit"
            onClick={() => void model.run('deck.view.fit_to_window')}
          >
            <Icon icon={Maximize} size={14} />
          </button>
        ),
      },
    ],
    [model, notes, comments, fit, reading],
  );
  // PowerPoint's fourth view button: the slide show from the current slide.
  const afterViews = useMemo<StatusBarItem[]>(
    () => [
      {
        id: 'show',
        label: t('Slide Show'),
        content: (
          <button
            type="button"
            className="kiln-deckstatus__fit"
            aria-label={t('Slide Show')}
            title={t('Slide Show')}
            data-testid="deck-status-show"
            onClick={() => void model.run('deck.show.from_current')}
          >
            <Icon icon={Presentation} size={14} />
          </button>
        ),
      },
    ],
    [model],
  );
  const view = status?.view ?? 'SlideNormal';
  return (
    <StatusBar
      left={left}
      right={right}
      afterViews={afterViews}
      storageKey="kiln.statusbar.deck"
      views={{
        value: VIEWS.some((v) => v.id === view) ? view : 'SlideNormal',
        options: VIEWS,
        onChange: (m) => void model.run('suite.view.mode', { mode: m as ViewMode }),
      }}
      zoom={{
        value: Math.round((status?.zoom ?? 1) * 100),
        onChange: (pct) => {
          model.set({ fit: false });
          void model.run('suite.view.zoom', { zoom: pct / 100 });
        },
        presets: [400, 200, 150, 100, 66, 50, 33],
        fits: [
          {
            id: 'fit',
            label: fit ? t('Fit Slide to Current Window (on)') : t('Fit Slide to Current Window'),
            onSelect: () => void model.run('deck.view.fit_to_window'),
          },
        ],
      }}
    />
  );
}
