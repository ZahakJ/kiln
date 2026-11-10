/**
 * The Comments pane (Review ▸ Show Comments, New Comment, the status bar's Comments button): the
 * current slide's threads with their replies, a box for a new comment on the selected object or
 * the slide, Reply, Resolve, Edit and Delete; and the threads' markers on the slide, which open
 * their thread. What they show is the core's (`DeckState.comments`); every change is a core
 * command.
 */
import { formatDate, t } from '@kiln/i18n';
import type { DeckComment } from '@kiln/protocol';
import { Button, Icon } from '@kiln/ui-kit';
import { Check, MessageSquare, Pencil, RotateCcw, Trash2, X } from 'lucide-react';
import { type KeyboardEvent, useEffect, useRef, useState } from 'react';
import { useDeck } from '../hooks.ts';
import type { DeckEditorModel } from '../model.ts';
import { openPane, reviewer } from './reviewCommands.ts';

/** "22 Oct 2026, 09:30" for an ISO time stamp (the text as given when it does not parse). */
export function commentDate(iso: string): string {
  const at = Date.parse(iso);
  if (Number.isNaN(at)) return iso;
  return formatDate(new Date(at), {
    day: 'numeric',
    month: 'short',
    year: 'numeric',
    hour: '2-digit',
    minute: '2-digit',
  });
}

/** A text box that posts with Ctrl+Enter (and the button) and cancels with Esc. */
function Composer({
  label,
  submit,
  initial = '',
  testId,
  onPost,
  onCancel,
}: {
  label: string;
  submit: string;
  initial?: string;
  testId: string;
  onPost: (text: string) => Promise<boolean>;
  onCancel: () => void;
}) {
  const [text, setText] = useState(initial);
  const ref = useRef<HTMLTextAreaElement>(null);
  useEffect(() => ref.current?.focus(), []);
  const post = async () => {
    if (!text.trim()) return;
    if (await onPost(text)) setText('');
  };
  const onKey = (e: KeyboardEvent<HTMLTextAreaElement>) => {
    e.stopPropagation();
    if (e.key === 'Enter' && (e.ctrlKey || e.metaKey)) {
      e.preventDefault();
      void post();
    } else if (e.key === 'Escape') {
      e.preventDefault();
      onCancel();
    }
  };
  return (
    <div className="kiln-deckcomment__composer">
      <textarea
        ref={ref}
        className="kiln-deckcomment__input"
        aria-label={label}
        data-testid={testId}
        placeholder={t('{label}… (Ctrl+Enter to post)', { label })}
        value={text}
        rows={3}
        onChange={(e) => setText(e.target.value)}
        onKeyDown={onKey}
      />
      <div className="kiln-deckcomment__actions">
        <Button size="sm" variant="ghost" onClick={onCancel}>
          {t('Cancel')}
        </Button>
        <Button size="sm" variant="primary" disabled={!text.trim()} onClick={() => void post()}>
          {submit}
        </Button>
      </div>
    </div>
  );
}

function Avatar({ initials, color }: { initials: string; color: string }) {
  return (
    <span className="kiln-deckcomment__avatar" style={{ background: `#${color}` }} aria-hidden="true">
      {initials}
    </span>
  );
}

function Thread({ model, c, active }: { model: DeckEditorModel; c: DeckComment; active: boolean }) {
  const [replying, setReplying] = useState(false);
  const [editing, setEditing] = useState<number | null>(null);
  const select = () => {
    if (!active) void model.core('deck.comment.select', { id: c.id });
  };
  return (
    <article
      className="kiln-deckcomment"
      data-active={active || undefined}
      data-resolved={c.resolved || undefined}
      data-testid="deck-comment"
      aria-label={t('Comment by {author}', { author: c.author })}
      onPointerDown={select}
      onFocusCapture={select}
    >
      <header className="kiln-deckcomment__head">
        <Avatar initials={c.initials} color={c.color} />
        <div className="kiln-deckcomment__who">
          <span className="kiln-deckcomment__author">{c.author || t('Author')}</span>
          <span className="kiln-deckcomment__date">{commentDate(c.date)}</span>
        </div>
        {c.resolved ? <span className="kiln-deckcomment__badge">{t('Resolved')}</span> : null}
      </header>
      {c.shape_name ? (
        <p className="kiln-deckcomment__on">{t('On {shape_name}', { shape_name: c.shape_name })}</p>
      ) : null}
      {editing === c.id ? (
        <Composer
          label={t('Edit comment')}
          submit="Save"
          initial={c.text}
          testId="deck-comment-edit"
          onPost={async (text) => {
            const ok = await model.core('deck.comment.edit', { id: c.id, text });
            if (ok) setEditing(null);
            return ok;
          }}
          onCancel={() => setEditing(null)}
        />
      ) : (
        <p className="kiln-deckcomment__text">{c.text}</p>
      )}
      {c.replies.map((r) => (
        <div key={r.id} className="kiln-deckcomment__reply" data-testid="deck-comment-reply">
          <div className="kiln-deckcomment__who">
            <span className="kiln-deckcomment__author">{r.author || t('Author')}</span>
            <span className="kiln-deckcomment__date">{commentDate(r.date)}</span>
          </div>
          {editing === r.id ? (
            <Composer
              label={t('Edit reply')}
              submit="Save"
              initial={r.text}
              testId="deck-comment-edit"
              onPost={async (text) => {
                const ok = await model.core('deck.comment.edit', { id: r.id, text });
                if (ok) setEditing(null);
                return ok;
              }}
              onCancel={() => setEditing(null)}
            />
          ) : (
            <p className="kiln-deckcomment__text">{r.text}</p>
          )}
          <div className="kiln-deckcomment__tools">
            <button type="button" aria-label={t('Edit reply')} onClick={() => setEditing(r.id)}>
              <Icon icon={Pencil} size={13} />
            </button>
            <button
              type="button"
              aria-label={t('Delete reply')}
              onClick={() => void model.core('deck.comment.delete', { id: r.id, scope: 'One' })}
            >
              <Icon icon={Trash2} size={13} />
            </button>
          </div>
        </div>
      ))}
      {replying ? (
        <Composer
          label={t('Reply')}
          submit="Reply"
          testId="deck-comment-reply-box"
          onPost={async (text) => {
            const ok = await model.core('deck.comment.reply', { id: c.id, text, author: reviewer().author });
            if (ok) setReplying(false);
            return ok;
          }}
          onCancel={() => setReplying(false)}
        />
      ) : (
        <div className="kiln-deckcomment__tools">
          <Button size="sm" variant="ghost" onClick={() => setReplying(true)}>
            {t('Reply')}
          </Button>
          <button
            type="button"
            aria-label={c.resolved ? t('Reopen thread') : t('Resolve thread')}
            title={c.resolved ? t('Reopen thread') : t('Resolve thread')}
            onClick={() => void model.core('deck.comment.resolve', { id: c.id, resolved: !c.resolved })}
          >
            <Icon icon={c.resolved ? RotateCcw : Check} size={14} />
          </button>
          <button
            type="button"
            aria-label={t('Edit comment')}
            title={t('Edit comment')}
            onClick={() => setEditing(c.id)}
          >
            <Icon icon={Pencil} size={14} />
          </button>
          <button
            type="button"
            aria-label={t('Delete thread')}
            title={t('Delete thread')}
            onClick={() => void model.core('deck.comment.delete', { id: c.id, scope: 'One' })}
          >
            <Icon icon={Trash2} size={14} />
          </button>
        </div>
      )}
    </article>
  );
}

export function CommentsPane({ model }: { model: DeckEditorModel }) {
  const comments = useDeck(model, (s) => s.deck?.comments ?? NONE);
  const active = useDeck(model, (s) => s.deck?.comment ?? null);
  const composer = useDeck(model, (s) => s.composer);
  const selected = useDeck(model, (s) => (s.deck?.selection.length ?? 0) > 0);
  const slide = useDeck(model, (s) => (s.deck?.current ?? 0) + 1);
  return (
    <aside className="kiln-deckpane" aria-label={t('Comments')} data-testid="deck-comments-pane">
      <header className="kiln-deckpane__head">
        <h2>{t('Comments')}</h2>
        <Button
          size="sm"
          variant="ghost"
          onClick={() => model.set({ composer: true })}
          data-testid="deck-comment-new"
        >
          {t('New')}
        </Button>
        <button
          type="button"
          className="kiln-deckpane__close"
          aria-label={t('Close Comments')}
          onClick={() => {
            openPane(model, null);
            model.stage?.focus();
          }}
        >
          <Icon icon={X} size={14} />
        </button>
      </header>
      <div className="kiln-deckpane__body kiln-deckcomments">
        {composer ? (
          <article className="kiln-deckcomment" data-active>
            <p className="kiln-deckcomment__on">
              {selected ? t('On the selected object') : t('On slide {slide}', { slide })}
            </p>
            <Composer
              label={t('New comment')}
              submit="Post"
              testId="deck-comment-composer"
              onPost={async (text) => {
                const ok = await model.core('deck.comment.new', { text, ...reviewer() });
                if (ok) model.set({ composer: false });
                return ok;
              }}
              onCancel={() => {
                model.set({ composer: false });
                model.stage?.focus();
              }}
            />
          </article>
        ) : null}
        {comments.length === 0 && !composer ? (
          <p className="kiln-deckcomments__empty">
            {t('No comments on this slide. Select an object or the slide and choose New (Ctrl+Alt+M).')}
          </p>
        ) : null}
        {comments.map((c) => (
          <Thread key={c.id} model={model} c={c} active={active === c.id} />
        ))}
      </div>
    </aside>
  );
}

const NONE: readonly DeckComment[] = [];

/** The markers of the slide's threads, over the stage where each comment is. */
export function CommentMarkers({ model }: { model: DeckEditorModel }) {
  const comments = useDeck(model, (s) => s.deck?.comments ?? NONE);
  const active = useDeck(model, (s) => s.deck?.comment ?? null);
  const [view, setView] = useState<{ zoom: number; left: number; top: number } | null>(null);
  // Markers follow the stage's scroll and zoom.
  useEffect(() => {
    if (comments.length === 0) return;
    let frame = 0;
    let last = '';
    const tick = () => {
      const v = model.stage?.view?.();
      const key = v ? `${v.zoom}:${v.left}:${v.top}` : '';
      if (key !== last) {
        last = key;
        setView(v ? { zoom: v.zoom, left: v.left, top: v.top } : null);
      }
      frame = requestAnimationFrame(tick);
    };
    tick();
    return () => cancelAnimationFrame(frame);
  }, [model, comments.length]);
  if (!view || comments.length === 0) return null;
  return (
    <div className="kiln-deckmarkers">
      {comments.map((c, i) => (
        <button
          key={c.id}
          type="button"
          className="kiln-deckmarker"
          data-active={active === c.id || undefined}
          data-resolved={c.resolved || undefined}
          data-testid="deck-comment-marker"
          aria-label={t('Comment {n, number} by {author}', { n: i + 1, author: c.author || t('Author') })}
          style={{
            left: view.left + c.x * view.zoom,
            top: view.top + c.y * view.zoom,
            borderColor: `#${c.color}`,
          }}
          onClick={() => {
            openPane(model, 'comments');
            void model.core('deck.comment.select', { id: c.id });
          }}
        >
          <Icon icon={MessageSquare} size={14} />
        </button>
      ))}
    </div>
  );
}
