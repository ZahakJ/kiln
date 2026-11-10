/**
 * The chrome side of Design Ideas, Outline View, Reuse Slides and Insert ▸ Video, Audio and
 * Equation: the panes' toggles, the media file picker and the equation dialog. What the panes show comes from the core
 * (`DeckQuery::DesignIdeas`, `OutlineView`, `ReuseList`) and every change is a core command.
 */

import type { CommandDefinition, Disposable } from '@kiln/commands';
import { msg } from '@kiln/i18n';
import type { DeckEditorModel } from '../model.ts';
import { insertMedia } from './media.ts';

/** The ids the Deck chrome implements for these features. */
export const DECK_FEATURE_CHROME: readonly string[] = [
  'deck.drawing.design_ideas',
  'deck.view.outline_view',
  'deck.insert.reuse_slides',
  'deck.insert.video',
  'deck.insert.audio',
  'suite.insert.equation',
];

/** Registers the feature panes' commands; dispose to remove them. */
export function registerFeatureUi(model: DeckEditorModel): Disposable {
  const defs: CommandDefinition<unknown>[] = [
    {
      id: 'deck.drawing.design_ideas',
      title: msg('Design Ideas'),
      isChecked: () => model.state.designPane,
      run: () => model.set({ designPane: !model.state.designPane, reusePane: false }),
    },
    {
      // View ▸ Outline View: the outline takes the rail's place; Normal brings the rail back.
      id: 'deck.view.outline_view',
      title: msg('Outline View'),
      isChecked: () => model.state.outline,
      run: () => model.set({ outline: !model.state.outline }),
    },
    {
      id: 'deck.insert.reuse_slides',
      title: msg('Reuse Slides'),
      isChecked: () => model.state.reusePane,
      run: () => model.set({ reusePane: !model.state.reusePane, designPane: false }),
    },
    { id: 'deck.insert.video', title: msg('Video'), run: () => insertMedia(model, 'Video') },
    { id: 'deck.insert.audio', title: msg('Audio'), run: () => insertMedia(model, 'Audio') },
    { id: 'suite.insert.equation', title: msg('Equation'), run: () => model.openDialog('equation') },
  ];
  const reg = model.registry;
  const disposables: Disposable[] = [];
  for (const d of defs) {
    const state = reg.getState(d.id);
    if (state.exists && !state.pending) continue;
    disposables.push(reg.register(d));
  }
  // Normal view (the status bar's button, View ▸ Normal) leaves Outline View, as PowerPoint does.
  const off = reg.subscribe((ev) => {
    if (ev.type !== 'executed' || ev.result.status !== 'ran') return;
    if (ev.id === 'suite.view.mode' && model.state.outline) model.set({ outline: false });
  });
  return {
    dispose() {
      off();
      for (const d of disposables) d.dispose();
    },
  };
}
