/**
 * Comment cards: the balloons beside the page (Simple and All Markup) and the threads of the
 * Comments pane share them. A card shows the author's avatar in their review colour, the time,
 * the text and the replies; Reply and Resolve sit under it, Edit and Delete in its menu. The
 * composer is the same card with a text box (Ctrl+Enter posts, Esc cancels). Every action is a
 * core command (`write.comment.*`).
 */
import { t } from '@kiln/i18n';
import type { CommentInfo, NodeId } from '@kiln/protocol';
import { Button, Icon, Menu, MenuContent, MenuItem, MenuTrigger } from '@kiln/ui-kit';
import { Check, MoreHorizontal, RotateCcw } from 'lucide-react';
import { type CSSProperties, type KeyboardEvent, useEffect, useRef, useState } from 'react';
import type { WriteEditorModel } from '../model.ts';
import { avatarText, featuresOf, whenLabel } from './model.ts';

export interface Thread {
  root: CommentInfo;
  replies: CommentInfo[];
}

/** Groups the core's flat list (each top-level comment followed by its replies) into threads. */
export function threadsOf(list: readonly CommentInfo[]): Thread[] {
  const out: Thread[] = [];
  for (const c of list) {
    if (c.parent === null) out.push({ root: c, replies: [] });
    else out[out.length - 1]?.replies.push(c);
  }
  return out;
}

function Avatar({
  name,
  initials,
  color,
  small,
}: {
  name: string;
  initials: string;
  color: string;
  small?: boolean;
}) {
  return (
    <span
      className={`kiln-comment__avatar${small ? ' kiln-comment__avatar--small' : ''}`}
      style={{ '--avatar': `#${color}` } as CSSProperties}
      aria-hidden="true"
      title={name}
    >
      {avatarText(name, initials)}
    </span>
  );
}

/** Unsent comment text by composer, kept while the composer is away (Esc, scrolling). */
const drafts = new Map<string, string>();

/**
 * A text box that grows with its text; Ctrl+Enter submits. Esc goes back to the document and
 * keeps the draft (as Word does); only Cancel, or Esc on an empty box, discards it.
 */
export function CommentBox({
  initial = '',
  placeholder,
  label,
  submitLabel,
  onSubmit,
  onCancel,
  testId,
  draftKey,
}: {
  initial?: string;
  placeholder: string;
  label: string;
  submitLabel: string;
  onSubmit: (text: string) => Promise<boolean> | boolean;
  onCancel: () => void;
  testId?: string;
  /** Where the unsent text is kept (`new`, `reply:<id>`, `edit:<id>`). */
  draftKey?: string;
}) {
  const [text, setTextState] = useState(() => (draftKey ? drafts.get(draftKey) : undefined) ?? initial);
  const setText = (t: string) => {
    setTextState(t);
    if (draftKey) {
      if (t && t !== initial) drafts.set(draftKey, t);
      else drafts.delete(draftKey);
    }
  };
  const [busy, setBusy] = useState(false);
  const ref = useRef<HTMLTextAreaElement>(null);
  useEffect(() => {
    ref.current?.focus();
    const el = ref.current;
    if (el) el.setSelectionRange(el.value.length, el.value.length);
  }, []);
  const submit = async () => {
    if (!text.trim() || busy) return;
    setBusy(true);
    const ok = await onSubmit(text.trim());
    setBusy(false);
    if (ok) setText('');
  };
  const cancel = () => {
    if (draftKey) drafts.delete(draftKey);
    onCancel();
  };
  const onKey = (e: KeyboardEvent<HTMLTextAreaElement>) => {
    e.stopPropagation();
    if (e.key === 'Enter' && (e.ctrlKey || e.metaKey)) {
      e.preventDefault();
      void submit();
    } else if (e.key === 'Escape') {
      e.preventDefault();
      if (text.trim() && text !== initial) {
        // Back to the document; the draft stays in its box.
        window.dispatchEvent(new CustomEvent('kiln:return-to-document'));
      } else {
        cancel();
      }
    }
  };
  return (
    <div className="kiln-comment__compose" data-testid={testId}>
      <textarea
        ref={ref}
        className="kiln-comment__input"
        aria-label={label}
        placeholder={placeholder}
        value={text}
        rows={Math.min(8, Math.max(2, text.split('\n').length))}
        onChange={(e) => setText(e.target.value)}
        onKeyDown={onKey}
      />
      <div className="kiln-comment__actions">
        <Button size="sm" variant="primary" disabled={!text.trim() || busy} onClick={() => void submit()}>
          {submitLabel}
        </Button>
        <Button size="sm" onClick={cancel}>
          {t('Cancel')}
        </Button>
      </div>
    </div>
  );
}

function Entry({
  model,
  c,
  reply,
  onEdit,
}: {
  model: WriteEditorModel;
  c: CommentInfo;
  reply?: boolean;
  onEdit: (id: NodeId) => void;
}) {
  return (
    <div className={`kiln-comment__entry${reply ? ' kiln-comment__entry--reply' : ''}`}>
      <header className="kiln-comment__head">
        <Avatar name={c.author} initials={c.initials} color={c.color} small={reply} />
        <span className="kiln-comment__author">{c.author}</span>
        <time className="kiln-comment__when" dateTime={c.date ?? undefined} title={c.date ?? undefined}>
          {whenLabel(c.date)}
        </time>
        <Menu modal={false}>
          <MenuTrigger asChild>
            <button
              type="button"
              className="kiln-comment__more"
              aria-label={t('More actions for {author}’s comment', { author: c.author })}
              onClick={(e) => e.stopPropagation()}
            >
              <Icon icon={MoreHorizontal} size={14} />
            </button>
          </MenuTrigger>
          <MenuContent align="end">
            <MenuItem onSelect={() => onEdit(c.id)}>{t('Edit comment')}</MenuItem>
            <MenuItem onSelect={() => void model.run('write.comment.delete', { id: c.id })}>
              {reply ? t('Delete reply') : t('Delete thread')}
            </MenuItem>
          </MenuContent>
        </Menu>
      </header>
      <p className="kiln-comment__text">
        {c.text || <span className="kiln-comment__empty">{t('(no text)')}</span>}
      </p>
    </div>
  );
}

/** One thread as a card. */
export function CommentCard({
  model,
  thread,
  active,
  style,
  inPane,
  cardRef,
}: {
  model: WriteEditorModel;
  thread: Thread;
  active: boolean;
  style?: CSSProperties;
  inPane?: boolean;
  cardRef?: (el: HTMLElement | null) => void;
}) {
  const features = featuresOf(model);
  const [replying, setReplying] = useState(false);
  const [editing, setEditing] = useState<NodeId | null>(null);
  const { root, replies } = thread;
  const resolved = root.resolved;
  const all = [root, ...replies];
  const editingComment = all.find((c) => c.id === editing) ?? null;
  return (
    <article
      ref={cardRef}
      className={`kiln-comment${active ? ' is-active' : ''}${resolved ? ' is-resolved' : ''}${inPane ? ' kiln-comment--pane' : ''}`}
      style={style}
      data-testid="write-comment-card"
      data-comment={root.id}
      aria-label={
        resolved
          ? t('Comment by {author}, resolved', { author: root.author })
          : t('Comment by {author}', { author: root.author })
      }
      tabIndex={-1}
      onMouseDown={(e) => {
        if ((e.target as HTMLElement).closest('button, textarea, [role="menuitem"]')) return;
        features.set({ activeComment: root.id });
        void model.runCore('write.comment.select', { id: root.id });
      }}
      // The keys entering a thread select it, as a click does (its text is selected in the page).
      onFocus={(e) => {
        if (e.currentTarget.contains(e.relatedTarget as Node | null)) return;
        features.set({ activeComment: root.id });
        void model.runCore('write.comment.select', { id: root.id });
      }}
    >
      {inPane && root.anchor_text ? (
        <blockquote className="kiln-comment__quote">{root.anchor_text}</blockquote>
      ) : null}
      {editingComment ? (
        <CommentBox
          initial={editingComment.text}
          draftKey={`edit:${editingComment.id}`}
          label={t('Edit comment')}
          placeholder={t('Comment')}
          submitLabel={t('Save')}
          onSubmit={async (text) => {
            const ok = await model.run('write.comment.edit', { id: editingComment.id, text });
            if (ok) setEditing(null);
            return ok;
          }}
          onCancel={() => setEditing(null)}
        />
      ) : (
        <>
          <Entry model={model} c={root} onEdit={setEditing} />
          {replies.map((r) => (
            <Entry key={r.id} model={model} c={r} reply onEdit={setEditing} />
          ))}
        </>
      )}
      {replying ? (
        <CommentBox
          label={t('Reply')}
          placeholder={t('Reply…')}
          submitLabel={t('Reply')}
          testId="write-comment-reply"
          draftKey={`reply:${root.id}`}
          onSubmit={async (text) => {
            const ok = await model.run('write.comment.reply', { parent: root.id, text });
            if (ok) setReplying(false);
            return ok;
          }}
          onCancel={() => setReplying(false)}
        />
      ) : editing ? null : (
        <div className="kiln-comment__actions">
          {resolved ? (
            <Button
              size="sm"
              onClick={() => void model.run('write.comment.resolve', { id: root.id, resolved: false })}
            >
              <Icon icon={RotateCcw} size={13} /> {t('Reopen')}
            </Button>
          ) : (
            <>
              <Button size="sm" variant={active ? 'primary' : 'default'} onClick={() => setReplying(true)}>
                {t('Reply')}
              </Button>
              <Button
                size="sm"
                onClick={() => void model.run('write.comment.resolve', { id: root.id, resolved: true })}
              >
                <Icon icon={Check} size={13} /> {t('Resolve')}
              </Button>
            </>
          )}
        </div>
      )}
    </article>
  );
}
