/**
 * The Reviewing pane (Review ▸ Reviewing Pane): a summary of the revisions (insertions,
 * deletions, formatting, comments), the reviewers in their colours, and every tracked change in
 * document order with its author, time and text — Accept or Reject each one, or go to it.
 */
import type { RevisionEntryInfo, RevisionKindInfo } from '@kiln/protocol';
import { Icon } from '@kiln/ui-kit';
import { Check, X } from 'lucide-react';
import type { CSSProperties } from 'react';
import { useStore } from 'zustand';
import { featuresOf, whenLabel } from '../features/model.ts';
import type { WriteEditorModel } from '../model.ts';

const VERB: Record<RevisionKindInfo, string> = {
  Inserted: 'Inserted',
  Deleted: 'Deleted',
  MovedFrom: 'Moved from',
  MovedTo: 'Moved to',
  Formatted: 'Formatted',
  ParagraphFormatted: 'Formatted',
  ParagraphInserted: 'Inserted',
  ParagraphDeleted: 'Deleted',
  RowInserted: 'Inserted',
  RowDeleted: 'Deleted',
};

function EntryRow({ model, e }: { model: WriteEditorModel; e: RevisionEntryInfo }) {
  const goto = () => {
    const t = e.target;
    const text =
      t.kind !== 'ParagraphFormatted' && t.kind !== 'ParagraphInserted' && t.kind !== 'ParagraphDeleted';
    if (t.kind === 'RowInserted' || t.kind === 'RowDeleted') return;
    void model.runCore('write.caret.select_to', {
      pos: { block: t.node, offset: text ? t.start : 0 },
      extend: false,
    });
    if (text && t.end > t.start)
      void model.runCore('write.caret.select_to', { pos: { block: t.node, offset: t.end }, extend: true });
  };
  return (
    <li
      className={`kiln-revision${e.current ? ' is-current' : ''}`}
      style={{ '--author': `#${e.color}` } as CSSProperties}
      data-testid="write-revision"
      data-kind={e.target.kind}
    >
      <button type="button" className="kiln-revision__main" onClick={goto}>
        <span className="kiln-revision__head">
          <span className="kiln-revision__author">{e.author}</span>
          <span className="kiln-revision__verb">{VERB[e.target.kind]}</span>
          <time className="kiln-revision__when">{whenLabel(e.date)}</time>
        </span>
        <span className={`kiln-revision__text kiln-revision__text--${e.target.kind}`}>{e.text || '¶'}</span>
        {e.story !== 'Main Document' ? <span className="kiln-revision__story">{e.story}</span> : null}
      </button>
      <span className="kiln-revision__actions">
        <button
          type="button"
          className="kiln-revision__act"
          aria-label={`Accept ${VERB[e.target.kind].toLowerCase()} by ${e.author}`}
          title="Accept"
          onClick={() => void model.run('write.review.resolve', { target: e.target, accept: true })}
        >
          <Icon icon={Check} size={14} />
        </button>
        <button
          type="button"
          className="kiln-revision__act"
          aria-label={`Reject ${VERB[e.target.kind].toLowerCase()} by ${e.author}`}
          title="Reject"
          onClick={() => void model.run('write.review.resolve', { target: e.target, accept: false })}
        >
          <Icon icon={X} size={14} />
        </button>
      </span>
    </li>
  );
}

export function ReviewingPane({ model }: { model: WriteEditorModel }) {
  const features = featuresOf(model);
  const info = useStore(features.store, (s) => s.revisions);
  if (!info) return <p className="kiln-comments-pane__empty">Reading the revisions…</p>;
  const total = info.insertions + info.deletions + info.formatting;
  return (
    <div className="kiln-reviewing-pane" data-testid="write-reviewing-pane">
      <p className="kiln-reviewing-pane__summary">
        <strong>{total.toLocaleString()}</strong> {total === 1 ? 'revision' : 'revisions'}
      </p>
      <dl className="kiln-reviewing-pane__counts">
        <div>
          <dt>Insertions</dt>
          <dd>{info.insertions}</dd>
        </div>
        <div>
          <dt>Deletions</dt>
          <dd>{info.deletions}</dd>
        </div>
        <div>
          <dt>Formatting</dt>
          <dd>{info.formatting}</dd>
        </div>
        <div>
          <dt>Comments</dt>
          <dd>{info.comments}</dd>
        </div>
      </dl>
      {info.reviewers.length > 0 ? (
        <ul className="kiln-reviewing-pane__reviewers" aria-label="Reviewers">
          {info.reviewers.map((r) => (
            <li key={r.name} style={{ '--author': `#${r.color}` } as CSSProperties}>
              <span className="kiln-reviewing-pane__swatch" aria-hidden="true" />
              {r.name}
              <span className="kiln-reviewing-pane__n">{r.changes}</span>
            </li>
          ))}
        </ul>
      ) : null}
      {info.entries.length === 0 ? (
        <p className="kiln-comments-pane__empty">
          No tracked changes. Turn on Track Changes (Ctrl+Shift+E) to record edits.
        </p>
      ) : (
        <ol className="kiln-reviewing-pane__list">
          {info.entries.map((e) => (
            <EntryRow
              key={`${e.target.kind}-${e.target.node}-${e.target.start}-${e.target.end}`}
              model={model}
              e={e}
            />
          ))}
        </ol>
      )}
    </div>
  );
}
