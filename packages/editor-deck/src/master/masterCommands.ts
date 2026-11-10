/**
 * Slide Master view in the chrome: View ▸ Slide Master opens it on the current slide's
 * layout with the Slide Master tab in front; Rename asks for the master's or layout's name.
 * Every change is a core command (`deck.master.*`).
 */

import type { CommandDefinition, Disposable } from '@kiln/commands';
import { msg } from '@kiln/i18n';
import { showRibbonTab } from '@kiln/ui-kit';
import type { DeckEditorModel } from '../model.ts';

/** The ids the Deck chrome implements for Slide Master view. */
export const DECK_MASTER_CHROME: readonly string[] = ['deck.view.slide_master', 'ui.deck.rename_master'];

/** Registers the Slide Master commands; dispose to remove them. */
export function registerMasterUi(model: DeckEditorModel): Disposable {
  const defs: CommandDefinition<unknown>[] = [
    {
      id: 'deck.view.slide_master',
      title: msg('Slide Master'),
      isChecked: () => !!model.state.deck?.master_view,
      run: async () => {
        if (await model.core('deck.master.view', { on: true })) {
          await model.fetch();
          showRibbonTab('slideMaster');
          model.stage?.focus();
        }
      },
    },
    {
      id: 'ui.deck.rename_master',
      title: msg('Rename…'),
      isEnabled: () => !!model.state.deck?.master_view,
      run: () => model.openDialog('renameMaster'),
    },
  ];
  const reg = model.registry;
  const disposables: Disposable[] = [];
  for (const d of defs) {
    const state = reg.getState(d.id);
    if (state.exists && !state.pending) continue;
    disposables.push(reg.register(d));
  }
  return {
    dispose() {
      for (const d of disposables) d.dispose();
    },
  };
}
