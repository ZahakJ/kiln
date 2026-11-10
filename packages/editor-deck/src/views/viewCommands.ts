/**
 * View ▸ Ruler, kept on this device as PowerPoint keeps it (the rulers and the guides'
 * handles are drawn over the stage, `Rulers.tsx`); and Reading View's show, which follows the
 * view mode.
 */

import type { CommandDefinition, Disposable } from '@kiln/commands';
import { msg } from '@kiln/i18n';
import type { DeckEditorModel } from '../model.ts';
import { followReadingView } from './ReadingView.tsx';

/** The ids the Deck chrome implements for the View tab. */
export const DECK_VIEW_CHROME: readonly string[] = ['suite.view.ruler'];

const RULER_KEY = 'kiln.deck.ruler';

/** Whether the ruler was on when the editor was last used on this device. */
export function rulerPreference(): boolean {
  try {
    return globalThis.localStorage?.getItem(RULER_KEY) === '1';
  } catch {
    return false;
  }
}

/** Registers the View commands; dispose to remove them. */
export function registerViewUi(model: DeckEditorModel): Disposable {
  const defs: CommandDefinition<unknown>[] = [
    {
      id: 'suite.view.ruler',
      title: msg('Ruler'),
      isChecked: () => model.state.ruler,
      run: () => {
        const on = !model.state.ruler;
        model.set({ ruler: on });
        try {
          globalThis.localStorage?.setItem(RULER_KEY, on ? '1' : '0');
        } catch {
          // Preferences are best-effort (private windows).
        }
      },
    },
  ];
  const reg = model.registry;
  const disposables: Disposable[] = [];
  for (const d of defs) {
    const state = reg.getState(d.id);
    if (state.exists && !state.pending) continue;
    disposables.push(reg.register(d));
  }
  // Reading View's show starts and ends with the view, from wherever the view is chosen.
  const off = reg.subscribe((ev) => {
    if (ev.type === 'executed' && ev.id === 'suite.view.mode' && ev.result.status === 'ran')
      void followReadingView(model);
  });
  return {
    dispose() {
      off();
      for (const d of disposables) d.dispose();
    },
  };
}
