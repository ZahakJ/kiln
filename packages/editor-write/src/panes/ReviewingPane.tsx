/**
 * The Reviewing pane (Review ▸ Reviewing Pane): a summary of the revisions (insertions,
 * deletions, formatting, comments), the reviewers in their colours, and every tracked change in
 * document order with its author, time and text — Accept or Reject each one, or go to it.
 */
import { formatNumber, msg, t, tx } from '@kiln/i18n';
import type { RevisionEntryInfo, RevisionKindInfo } from '@kiln/protocol';
import { Icon } from '@kiln/ui-kit';
import { Check, X } from 'lucide-react';
import type { CSSProperties } from 'react';
import { useStore } from 'zustand';
import { featuresOf, whenLabel } from '../features/model.ts';
import { useWrite } from '../hooks.ts';
import type { WriteEditorModel } from '../model.ts';

const NONE: readonly string[] = [];

/** The kind of change a revision's Accept and Reject buttons name. */
const VERB_KIND: Record<RevisionKindInfo, 'inserted' | 'deleted' | 'movedFrom' | 'movedTo' | 'formatted'> = {
  Inserted: 'inserted',
  Deleted: 'deleted',
  MovedFrom: 'movedFrom',
  MovedTo: 'movedTo',
  Formatted: 'formatted',
  ParagraphFormatted: 'formatted',
  ParagraphInserted: 'inserted',
  ParagraphDeleted: 'deleted',
  RowInserted: 'inserted',
  RowDeleted: 'deleted',
};

const VERB: Record<RevisionKindInfo, string> = {
  Inserted: msg('Inserted'),
  Deleted: msg('Deleted'),
  MovedFrom: msg('Moved from'),
  MovedTo: msg('Moved to'),
  Formatted: msg('Formatted'),
  ParagraphFormatted: msg('Formatted'),
  ParagraphInserted: msg('Inserted'),
  ParagraphDeleted: msg('Deleted'),
  RowInserted: msg('Inserted'),
  RowDeleted: msg('Deleted'),
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
          <span className="kiln-revision__verb">{tx(VERB[e.target.kind])}</span>
          <time className="kiln-revision__when">{whenLabel(e.date)}</time>
        </span>
        <span className={`kiln-revision__text kiln-revision__text--${e.target.kind}`}>{e.text || '¶'}</span>
        {e.story !== 'Main Document' ? <span className="kiln-revision__story">{e.story}</span> : null}
      </button>
      <span className="kiln-revision__actions">
        <button
          type="button"
          className="kiln-revision__act"
          aria-label={t('write.acceptChange', { kind: VERB_KIND[e.target.kind], author: e.author })}
          title={t('Accept')}
          onClick={() => void model.run('write.review.resolve', { target: e.target, accept: true })}
        >
          <Icon icon={Check} size={14} />
        </button>
        <button
          type="button"
          className="kiln-revision__act"
          aria-label={t('write.rejectChange', { kind: VERB_KIND[e.target.kind], author: e.author })}
          title={t('Reject')}
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
  const hidden = useWrite(model, (s) => s.format?.features.hidden_reviewers ?? NONE);
  if (!info) return <p className="kiln-comments-pane__empty">{t('Reading the revisions…')}</p>;
  const total = info.insertions + info.deletions + info.formatting;
  return (
    <div className="kiln-reviewing-pane" data-testid="write-reviewing-pane">
      <p className="kiln-reviewing-pane__summary">
        <strong>{formatNumber(total)}</strong> {t('write.revisionsNoun', { count: total })}
      </p>
      <dl className="kiln-reviewing-pane__counts">
        <div>
          <dt>{t('Insertions')}</dt>
          <dd>{info.insertions}</dd>
        </div>
        <div>
          <dt>{t('Deletions')}</dt>
          <dd>{info.deletions}</dd>
        </div>
        <div>
          <dt>{t('Formatting')}</dt>
          <dd>{info.formatting}</dd>
        </div>
        <div>
          <dt>{t('Comments')}</dt>
          <dd>{info.comments}</dd>
        </div>
      </dl>
      {info.reviewers.length > 0 ? (
        <>
          <p className="kiln-reviewing-pane__label" id="kiln-reviewers-label">
            {t('Show markup from')}
          </p>
          <ul className="kiln-reviewing-pane__reviewers" aria-labelledby="kiln-reviewers-label">
            {info.reviewers.map((r) => {
              const shown = !hidden.includes(r.name);
              return (
                <li key={r.name} style={{ '--author': `#${r.color}` } as CSSProperties}>
                  <button
                    type="button"
                    className="kiln-reviewing-pane__reviewer"
                    aria-pressed={shown}
                    title={
                      shown
                        ? t('Hide {name}’s changes and comments', { name: r.name })
                        : t('Show {name}’s changes and comments', { name: r.name })
                    }
                    data-testid="write-reviewer"
                    onClick={() =>
                      void model.run('write.review.show_reviewer', { name: r.name, show: !shown })
                    }
                  >
                    <span className="kiln-reviewing-pane__swatch" aria-hidden="true" />
                    {r.name}
                    <span className="kiln-reviewing-pane__n">{r.changes}</span>
                  </button>
                </li>
              );
            })}
          </ul>
        </>
      ) : null}
      {info.entries.length === 0 ? (
        <p className="kiln-comments-pane__empty">
          {t('No tracked changes. Turn on Track Changes (Ctrl+Shift+E) to record edits.')}
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
