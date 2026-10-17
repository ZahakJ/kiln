/**
 * The Comments pane (Review ▸ Comments Pane, the title bar's Comments button): every thread in
 * document order with the text it is on, replies, Reply/Resolve, a New Comment box at the top,
 * and a filter for resolved threads. Click a thread to select its text.
 */
import { Button } from '@kiln/ui-kit';
import { useState } from 'react';
import { useStore } from 'zustand';
import { CommentBox, CommentCard, threadsOf } from '../features/Comments.tsx';
import { featuresOf } from '../features/model.ts';
import { useWrite } from '../hooks.ts';
import type { WriteEditorModel } from '../model.ts';

export function CommentsPane({ model }: { model: WriteEditorModel }) {
  const features = featuresOf(model);
  const comments = useStore(features.store, (s) => s.comments);
  const composer = useStore(features.store, (s) => s.composer);
  const active = useStore(features.store, (s) => s.activeComment);
  const current = useWrite(model, (s) => s.format?.features.comment ?? null);
  const [showResolved, setShowResolved] = useState(true);
  const threads = threadsOf(comments ?? []);
  const shown = threads.filter((t) => showResolved || !t.root.resolved);
  const open = threads.filter((t) => !t.root.resolved).length;
  return (
    <div className="kiln-comments-pane" data-testid="write-comments-pane">
      <div className="kiln-comments-pane__bar">
        <span className="kiln-comments-pane__count">
          {open} open · {threads.length - open} resolved
        </span>
        <label className="kiln-write-check">
          <input type="checkbox" checked={showResolved} onChange={(e) => setShowResolved(e.target.checked)} />
          Show resolved
        </label>
      </div>
      {composer?.parent === null ? (
        <article className="kiln-comment kiln-comment--pane is-active">
          <CommentBox
            label="New comment"
            placeholder="Add a comment… (Ctrl+Enter to post)"
            submitLabel="Comment"
            testId="write-comment-pane-composer"
            onSubmit={async (text) => {
              const ok = await model.run('write.comment.new', { text });
              if (ok) features.set({ composer: null });
              return ok;
            }}
            onCancel={() => features.set({ composer: null })}
          />
        </article>
      ) : (
        <Button size="sm" variant="primary" onClick={() => features.set({ composer: { parent: null } })}>
          New Comment
        </Button>
      )}
      {shown.length === 0 && composer?.parent !== null ? (
        <p className="kiln-comments-pane__empty">
          No comments yet. Select text and choose New Comment (Ctrl+Alt+M) to start a conversation.
        </p>
      ) : null}
      <div className="kiln-comments-pane__list">
        {shown.map((t) => (
          <CommentCard
            key={t.root.id}
            model={model}
            thread={t}
            inPane
            active={active === t.root.id || current === t.root.id}
          />
        ))}
      </div>
      {threads.length > 0 ? (
        <Button size="sm" variant="ghost" onClick={() => void model.run('write.comment.delete_all')}>
          Delete all comments
        </Button>
      ) : null}
    </div>
  );
}
