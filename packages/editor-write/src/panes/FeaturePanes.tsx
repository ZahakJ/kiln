/**
 * The right-hand task pane of Kiln Write's features: Comments, Reviewing, Selection or Spelling
 * (one at a time, beside the Styles pane), in the editor's pane chrome.
 */
import { Icon } from '@kiln/ui-kit';
import { X } from 'lucide-react';
import { useStore } from 'zustand';
import { type FeaturePane, featuresOf } from '../features/model.ts';
import type { WriteEditorModel } from '../model.ts';
import { CommentsPane } from './CommentsPane.tsx';
import { ReviewingPane } from './ReviewingPane.tsx';
import { SelectionPane } from './SelectionPane.tsx';
import { SpellingPane } from './SpellingPane.tsx';

const TITLES: Record<FeaturePane, string> = {
  comments: 'Comments',
  review: 'Revisions',
  selection: 'Selection',
  spelling: 'Editor',
};

export function FeaturePanes({ model }: { model: WriteEditorModel }) {
  const features = featuresOf(model);
  const pane = useStore(features.store, (s) => s.pane);
  if (!pane) return null;
  const title = TITLES[pane];
  const close = () => {
    features.set({ pane: null });
    model.registry.setContext({ writeFeaturePane: null });
    model.controller?.focus();
  };
  return (
    <aside
      className="kiln-write__pane kiln-write__pane--right kiln-write__pane--feature"
      aria-label={title}
      data-testid={`write-pane-${pane}`}
    >
      <header className="kiln-write__pane-head">
        <h2>{title}</h2>
        <button
          type="button"
          className="kiln-write__pane-close"
          aria-label={`Close ${title}`}
          onClick={close}
        >
          <Icon icon={X} size={14} />
        </button>
      </header>
      <div className="kiln-write__pane-body">
        {pane === 'comments' ? <CommentsPane model={model} /> : null}
        {pane === 'review' ? <ReviewingPane model={model} /> : null}
        {pane === 'selection' ? <SelectionPane model={model} /> : null}
        {pane === 'spelling' ? <SpellingPane model={model} /> : null}
      </div>
    </aside>
  );
}
