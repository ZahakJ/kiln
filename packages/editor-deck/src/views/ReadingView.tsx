/**
 * View ▸ Reading View (the status bar's third button): the slide show inside the window, from the
 * current slide, with the title bar and the status bar still there. Clicks, the arrows, Space
 * and Page Down go through it as in the show; Esc or the end of the show goes back to the view
 * before (Normal).
 */
import { createDefaultKeymap } from '@kiln/commands';
import { useMemo } from 'react';
import { useDeck } from '../hooks.ts';
import type { DeckEditorModel } from '../model.ts';
import type { ShowEngine } from '../show/engine.ts';
import { ShowView } from '../show/ShowView.tsx';

/** The engine of each editor's Reading View, while it shows. */
const engines = new WeakMap<DeckEditorModel, ShowEngine>();

/** The status bar's Previous and Next: a step of the show, painted at once. */
export async function readingStep(model: DeckEditorModel, next: boolean): Promise<void> {
  await model.core(next ? 'deck.show.next' : 'deck.show.previous');
  engines.get(model)?.kick();
}

/**
 * Starts or ends Reading View's show to follow the view mode the core reports (the status bar's
 * button, View ▸ Reading View, or another view chosen while it runs).
 */
export async function followReadingView(model: DeckEditorModel): Promise<void> {
  const r = await model.client.query(model.doc, 'Status').catch(() => null);
  const view = r && typeof r === 'object' && 'Status' in r ? r.Status.view : null;
  if (view === 'ReadingView' && !model.state.reading) {
    const from = model.state.deck?.current ?? 0;
    if (await model.core('deck.show.start', { from, mode: 'Show' })) model.set({ reading: true });
  } else if (view !== 'ReadingView' && model.state.reading) {
    model.set({ reading: false });
    await model.core('deck.show.end');
  }
}

export function ReadingView({ model }: { model: DeckEditorModel }) {
  const keymap = useMemo(() => createDefaultKeymap(), []);
  const running = useDeck(model, (s) => s.reading);
  if (!running) return null;
  return (
    <div className="kiln-deckreading" data-testid="deck-reading-view">
      <ShowView
        client={model.client}
        doc={model.doc}
        keymap={keymap}
        send={(id, args) => model.core(id, args)}
        windowed
        onEngine={(e) => {
          if (e) engines.set(model, e);
          else engines.delete(model);
        }}
        onEnd={() => {
          model.set({ reading: false });
          void model.run('suite.view.mode', { mode: 'SlideNormal' }).then(() => {
            model.refresh();
            model.stage?.focus();
          });
        }}
      />
    </div>
  );
}
