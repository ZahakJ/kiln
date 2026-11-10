/**
 * Kiln Deck's editing area (the Deck mockup): the thumbnail rail on the left, the
 * slide on the stage with the speaker notes under it, the context menus and the dialogs; View ▸
 * Slide Sorter shows every slide instead, Outline View the outline in the rail's place, and the
 * Animation, Designer and Reuse Slides panes open on the right. Every edit is a core command; this is presentation only.
 */
import { t } from '@kiln/i18n';
import { Icon, useFormFactor } from '@kiln/ui-kit';
import { X } from 'lucide-react';
import { useCallback, useEffect, useState } from 'react';
import { FormatChartArea } from './charts/DeckCharts.tsx';
import { DeckMenus } from './DeckMenus.tsx';
import { DeckStage } from './DeckStage.tsx';
import { DeckDialogs } from './dialogs/DeckDialogs.tsx';
import { AnimationPane } from './features/AnimationPane.tsx';
import { DesignIdeasPane } from './features/DesignIdeasPane.tsx';
import { OutlinePane } from './features/OutlinePane.tsx';
import { PreviewOverlay } from './features/PreviewOverlay.tsx';
import { ReuseSlidesPane } from './features/ReuseSlidesPane.tsx';
import { useDeck } from './hooks.ts';
import { MasterRail } from './master/MasterRail.tsx';
import { DeckTouchLayer } from './mobile/DeckTouch.tsx';
import { PhoneSlideBar, PhoneSlideList, useBackToList, usePhoneScreen } from './mobile/PhoneDeck.tsx';
import type { DeckEditorModel } from './model.ts';
import { NotesPane } from './NotesPane.tsx';
import { CommentMarkers, CommentsPane } from './review/CommentsPane.tsx';
import { SpellingPane } from './review/SpellingPane.tsx';
import { NotesPageView } from './views/NotesPageView.tsx';
import { ReadingView } from './views/ReadingView.tsx';
import { GuideHandles, Rulers } from './views/Rulers.tsx';
import './views/views.css';
import './review/review.css';
import { SlideSorter } from './SlideSorter.tsx';
import type { DeckStageController } from './stage.ts';
import { ThumbnailRail } from './ThumbnailRail.tsx';
import './deck.css';
import './features/features.css';

export interface DeckEditorProps {
  model: DeckEditorModel;
  /** The presentation's title (the stage's accessible name). */
  title: string;
  /** The chrome's resolved theme (the stage's pasteboard and handles follow it). */
  theme: 'light' | 'dark';
  /** Blink the caret (screenshots turn it off). */
  blink?: boolean;
  onController?: (c: DeckStageController | null) => void;
}

export function DeckEditor({ model, title, theme, blink, onController }: DeckEditorProps) {
  const notice = useDeck(model, (s) => s.notice);
  const notes = useDeck(model, (s) => s.deck?.notes_visible ?? true);
  const view = useDeck(model, (s) => s.status?.view ?? 'SlideNormal');
  const editing = useDeck(model, (s) => s.deck?.editing ?? false);
  const aspect = useDeck(model, (s) =>
    s.deck ? `${Math.round(s.deck.slide_width)} / ${Math.round(s.deck.slide_height)}` : '16 / 9',
  );
  const animPane = useDeck(model, (s) => s.animPane);
  const designPane = useDeck(model, (s) => s.designPane);
  const reusePane = useDeck(model, (s) => s.reusePane);
  const outline = useDeck(model, (s) => s.outline);
  const commentsPane = useDeck(model, (s) => s.commentsPane);
  const spellingPane = useDeck(model, (s) => s.spellingPane);
  const master = useDeck(model, (s) => !!s.deck?.master_view);
  // A command from the chrome (a chart command, a ribbon box) hands the keys back to the slide.
  useEffect(() => {
    const back = () => model.stage?.focus();
    window.addEventListener('kiln:return-to-document', back);
    return () => window.removeEventListener('kiln:return-to-document', back);
  }, [model]);
  const sorter = view === 'SlideSorter' && !master;
  const reading = view === 'ReadingView' && !master;
  const notesPage = view === 'NotesPage' && !master;
  // on a phone, the slide list first; a tap opens the slide full width (Back: the list).
  const phone = useFormFactor() === 'phone' && !sorter && !notesPage && !reading && !master && !outline;
  const [screen, setScreen] = usePhoneScreen(phone);
  const [root, setRoot] = useState<HTMLDivElement | null>(null);
  const toList = useCallback(() => setScreen('list'), [setScreen]);
  useBackToList(root, phone && screen === 'slide', toList);
  return (
    <div
      ref={setRoot}
      data-phone={phone ? screen : undefined}
      data-kiln-back-layer={phone && screen === 'slide' ? '' : undefined}
      className="kiln-deck"
      data-testid="deck-editor"
      data-view={master ? 'SlideMaster' : view}
      data-outline={(outline && !sorter && !reading) || undefined}
      data-editing={editing || undefined}
      style={{ '--kiln-deck-aspect': aspect } as React.CSSProperties}
    >
      {sorter ? (
        <SlideSorter model={model} theme={theme} />
      ) : notesPage ? (
        <NotesPageView model={model} theme={theme} />
      ) : (
        <>
          {master ? (
            <MasterRail model={model} theme={theme} />
          ) : reading ? null : outline ? (
            <OutlinePane model={model} />
          ) : phone ? (
            screen === 'list' ? (
              <PhoneSlideList model={model} theme={theme} onOpen={() => setScreen('slide')} />
            ) : null
          ) : (
            <ThumbnailRail model={model} theme={theme} />
          )}
          <div className="kiln-deck__main">
            <div className="kiln-deck__stage">
              <DeckStage
                client={model.client}
                model={model}
                theme={theme}
                blink={blink}
                label={title}
                onController={onController}
              />
              <PreviewOverlay model={model} />
              <DeckTouchLayer model={model} />
              {reading ? <ReadingView model={model} /> : null}
              {reading ? null : <GuideHandles model={model} />}
              {reading ? null : <Rulers model={model} />}
              {reading || master ? null : <CommentMarkers model={model} />}
              {notice ? (
                <div className="kiln-deck__notice" role="alert">
                  <span>{notice}</span>
                  <button type="button" aria-label={t('Dismiss')} onClick={() => model.set({ notice: null })}>
                    <Icon icon={X} size={12} />
                  </button>
                </div>
              ) : null}
            </div>
            {phone ? <PhoneSlideBar model={model} /> : null}
            {notes && !reading && !master ? <NotesPane model={model} /> : null}
          </div>
          {animPane && !reading ? <AnimationPane model={model} /> : null}
          {designPane && !reading ? <DesignIdeasPane model={model} /> : null}
          {reusePane && !reading ? <ReuseSlidesPane model={model} /> : null}
          {reading ? null : <FormatChartArea model={model} />}
          {commentsPane && !reading ? <CommentsPane model={model} /> : null}
          {spellingPane && !reading ? <SpellingPane model={model} /> : null}
        </>
      )}
      <DeckMenus model={model} />
      <DeckDialogs model={model} />
    </div>
  );
}
