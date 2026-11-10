/**
 * Review in Kiln Deck: New Comment (Ctrl+Alt+M), Delete, Previous and Next, Show Comments
 * (the Comments pane) and Spelling (F7, the Spelling pane). The threads and the word Spelling
 * stopped at are the core's (`DeckState.comments`, `DeckState.spelling`); every change is a core
 * command.
 */

import type { CommandDefinition, Disposable } from '@kiln/commands';
import { msg } from '@kiln/i18n';
import { loadOptions } from '@kiln/ui-kit';
import type { DeckEditorModel } from '../model.ts';
import { ensureDeckDictionary } from './dictionary.ts';

/** The ids the Deck chrome implements for review. */
export const DECK_REVIEW_CHROME: readonly string[] = [
  'suite.review.new_comment',
  'suite.review.delete_comment',
  'suite.review.previous_comment',
  'suite.review.next_comment',
  'suite.review.show_comments',
  'suite.review.spelling',
];

/** The name new comments are signed with (File ▸ Options ▸ User name). */
export function reviewer(): { author: string; initials: string } {
  const g = loadOptions().general;
  const author = g.userName.trim() || 'Author';
  const initials =
    g.initials.trim() ||
    author
      .split(/\s+/)
      .map((w) => w[0] ?? '')
      .join('')
      .slice(0, 2)
      .toUpperCase();
  return { author, initials };
}

/** Opens one task pane at the right of the slide (closing the others there). */
export function openPane(model: DeckEditorModel, pane: 'comments' | 'spelling' | null): void {
  model.set({
    commentsPane: pane === 'comments',
    spellingPane: pane === 'spelling',
    ...(pane ? { designPane: false, reusePane: false, animPane: false, chartPane: false } : {}),
  });
}

/** Registers the review commands; dispose to remove them. */
export function registerReviewUi(model: DeckEditorModel): Disposable {
  const comments = () => model.state.deck?.comments ?? [];
  const defs: CommandDefinition<unknown>[] = [
    {
      id: 'suite.review.new_comment',
      title: msg('New Comment'),
      run: () => {
        openPane(model, 'comments');
        model.set({ composer: true });
      },
    },
    {
      id: 'suite.review.delete_comment',
      title: msg('Delete Comment'),
      // With comments in the presentation, so its menu's Delete All… stay in reach.
      isEnabled: () => (model.state.deck?.comment_total ?? 0) > 0,
      run: () => model.core('deck.comment.delete', { scope: 'One' }),
    },
    {
      id: 'suite.review.previous_comment',
      title: msg('Previous Comment'),
      isEnabled: () => (model.state.deck?.comment_total ?? 0) > 0,
      run: async () => {
        openPane(model, 'comments');
        await model.core('deck.comment.next', { backward: true });
      },
    },
    {
      id: 'suite.review.next_comment',
      title: msg('Next Comment'),
      isEnabled: () => (model.state.deck?.comment_total ?? 0) > 0,
      run: async () => {
        openPane(model, 'comments');
        await model.core('deck.comment.next', { backward: false });
      },
    },
    {
      id: 'suite.review.show_comments',
      title: msg('Show Comments'),
      isChecked: () => model.state.commentsPane,
      run: () => {
        openPane(model, model.state.commentsPane ? null : 'comments');
        if (!model.state.commentsPane) model.stage?.focus();
        else if (comments().length === 0) model.set({ composer: false });
      },
    },
    {
      id: 'suite.review.spelling',
      title: msg('Spelling'),
      run: async () => {
        openPane(model, 'spelling');
        await ensureDeckDictionary(model);
        await model.core('deck.spell.check');
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
  return {
    dispose() {
      for (const d of disposables) d.dispose();
    },
  };
}
