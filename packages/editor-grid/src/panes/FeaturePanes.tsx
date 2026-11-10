/**
 * The Grid feature panes beside the sheet (one at a time): Formula Lab (the active cell's
 * formula as a tree with each part's value, and its steps), the Watch Window, the Comments pane
 * (threads with replies, resolve, and the notes) and Spelling (F7). What they show are the
 * core's answers; every change is a core command.
 */
import { formatDate, t } from '@kiln/i18n';
import type { GridCommentInfo, GridFormulaTrace, GridSpellingHit, GridTraceNode } from '@kiln/protocol';
import { Button, Icon } from '@kiln/ui-kit';
import { Check as CheckIcon, MessageSquare, PencilLine, Plus, StickyNote, Trash2, X } from 'lucide-react';
import { useEffect, useState } from 'react';
import { useStore } from 'zustand';
import affUrl from '../../../../assets/dictionaries/en-US/en_US.aff?url';
import dicUrl from '../../../../assets/dictionaries/en-US/en_US.dic?url';
import { authorName, featuresOf, type GridFeatures } from '../features/model.ts';
import { useGrid } from '../hooks.ts';
import type { GridEditorModel } from '../model.ts';

function PaneFrame({
  features,
  title,
  children,
  testId,
}: {
  features: GridFeatures;
  title: string;
  children: React.ReactNode;
  testId: string;
}) {
  return (
    <aside className="kiln-gpane" aria-label={title} data-testid={testId} data-kiln-shortcuts="off">
      <header className="kiln-gpane__head">
        <h2>{title}</h2>
        <button
          type="button"
          className="kiln-gpane__close"
          aria-label={t('Close {title}', { title })}
          onClick={() => features.closePane()}
        >
          <Icon icon={X} size={16} />
        </button>
      </header>
      <div className="kiln-gpane__body">{children}</div>
    </aside>
  );
}

export function FeaturePanes({ model }: { model: GridEditorModel }) {
  const features = featuresOf(model);
  const pane = useStore(features.store, (s) => s.pane);
  switch (pane) {
    case 'lab':
      return <FormulaLabPane features={features} />;
    case 'watch':
      return <WatchPane features={features} />;
    case 'comments':
      return <CommentsPane features={features} />;
    case 'spelling':
      return <SpellingPane features={features} />;
    default:
      return null;
  }
}

/** One node of the evaluation tree. */
function TraceNode({ node, depth }: { node: GridTraceNode; depth: number }) {
  const [open, setOpen] = useState(depth < 3);
  const kids = node.children.filter((c) => c.kind !== 'group' || c.children.length > 0);
  return (
    <li className={node.evaluated ? 'kiln-lab__node' : 'kiln-lab__node kiln-lab__node--skipped'}>
      <div className="kiln-lab__line">
        {kids.length ? (
          <button
            type="button"
            className="kiln-lab__toggle"
            aria-expanded={open}
            onClick={() => setOpen(!open)}
          >
            {open ? '▾' : '▸'}
          </button>
        ) : (
          <span className="kiln-lab__toggle" />
        )}
        <span className={`kiln-lab__label kiln-lab__label--${node.kind}`}>
          {node.kind === 'call' ? `${node.label}( )` : node.text}
        </span>
        <span className="kiln-lab__arrow">→</span>
        <span className="kiln-lab__value">{node.evaluated ? node.value : t('not evaluated')}</span>
      </div>
      {node.summary && node.kind === 'call' ? <div className="kiln-lab__doc">{node.summary}</div> : null}
      {open && kids.length ? (
        <ul>
          {kids.map((c, i) => (
            <TraceNode key={i} node={c} depth={depth + 1} />
          ))}
        </ul>
      ) : null}
    </li>
  );
}

function FormulaLabPane({ features }: { features: GridFeatures }) {
  const grid = useGrid(features.model, (s) => s.grid);
  const [trace, setTrace] = useState<GridFormulaTrace | null>(null);
  // The core keeps the trace of the active cell while the lab is open.
  useEffect(() => {
    void features.ask('FormulaTrace').then((r) => {
      if (r && 'Trace' in r) setTrace(r.Trace);
    });
  }, [features, grid]);
  return (
    <PaneFrame features={features} title={t('Formula Lab')} testId="grid-formula-lab">
      <div className="kiln-lab__cell">{trace?.cell}</div>
      {trace?.formula ? <code className="kiln-lab__formula">{trace.formula}</code> : null}
      {trace?.message ? <p className="kiln-gfx__hint">{trace.message}</p> : null}
      {trace?.root ? (
        <>
          <div className="kiln-lab__result">
            = <strong>{trace.value}</strong>
          </div>
          <ul className="kiln-lab__tree" aria-label={t('Evaluation tree')}>
            <TraceNode node={trace.root} depth={0} />
          </ul>
          <h3 className="kiln-lab__h">{t('Steps')}</h3>
          <ol className="kiln-lab__steps" data-testid="grid-lab-steps">
            {trace.steps.map((s, i) => (
              <li key={i}>
                <code>
                  {s.text.slice(0, s.start)}
                  <u>{s.text.slice(s.start, s.end)}</u>
                  {s.text.slice(s.end)}
                </code>
                {s.end > s.start ? (
                  <span className="kiln-lab__gives"> {t('gives {value}', { value: s.value })}</span>
                ) : null}
              </li>
            ))}
          </ol>
        </>
      ) : null}
    </PaneFrame>
  );
}

const NO_WATCHES: never[] = [];

function WatchPane({ features }: { features: GridFeatures }) {
  const watches = useGrid(features.model, (s) => s.grid?.features.watches ?? NO_WATCHES);
  const sheet = useGrid(features.model, (s) => s.grid?.active_sheet ?? 0);
  return (
    <PaneFrame features={features} title={t('Watch Window')} testId="grid-watch-window">
      <div className="kiln-gfx__toolbar">
        <Button size="sm" onClick={() => features.model.run('ui.grid.watch_add')}>
          <Icon icon={Plus} size={14} /> {t('Add Watch')}
        </Button>
      </div>
      {watches.length === 0 ? (
        <p className="kiln-gfx__hint">{t('Select cells and add a watch to follow their values.')}</p>
      ) : null}
      <div className="kiln-gfx__table kiln-gfx__watch">
        {watches.length ? (
          <div className="kiln-gfx__thead">
            <span>{t('Sheet')}</span>
            <span>{t('Name')}</span>
            <span>{t('Cell')}</span>
            <span>{t('Value')}</span>
            <span>{t('Formula')}</span>
            <span />
          </div>
        ) : null}
        {watches.map((w) => (
          <div key={`${w.sheet_index}-${w.row}-${w.col}`} className="kiln-gfx__level">
            <span>{w.sheet}</span>
            <span>{w.name}</span>
            <button
              type="button"
              className="kiln-gfx__link"
              onClick={async () => {
                if (w.sheet_index !== sheet)
                  await features.model.run('grid.sheet.activate', { index: w.sheet_index });
                await features.model.core('grid.nav.select_cell', { row: w.row, col: w.col });
              }}
            >
              {w.cell}
            </button>
            <span>{w.value}</span>
            <span className="kiln-gfx__mono">{w.formula}</span>
            <button
              type="button"
              aria-label={t('Delete watch {cell}', { cell: w.cell })}
              className="kiln-gfx__icon"
              onClick={() =>
                features.exec({ Watch: { add: false, sheet: w.sheet_index, row: w.row, col: w.col } })
              }
            >
              <Icon icon={Trash2} size={14} />
            </button>
          </div>
        ))}
      </div>
    </PaneFrame>
  );
}

function when(date: string): string {
  if (!date) return '';
  const d = new Date(date);
  return Number.isNaN(d.getTime()) ? date : formatDate(d, { dateStyle: 'medium', timeStyle: 'short' });
}

function CommentsPane({ features }: { features: GridFeatures }) {
  const grid = useGrid(features.model, (s) => s.grid);
  const composer = useStore(features.store, (s) => s.composer);
  const [list, setList] = useState<GridCommentInfo[]>([]);
  const [draft, setDraft] = useState('');
  const [replies, setReplies] = useState<Record<string, string>>({});
  const [editing, setEditing] = useState<{ id: string; text: string } | null>(null);
  useEffect(() => {
    void features.ask('Comments').then((r) => {
      if (r && 'Comments' in r) setList(r.Comments);
    });
  }, [features, grid]);
  const sheet = grid?.active_sheet ?? 0;
  const roots = list.filter((c) => !c.note && !c.parent);
  const notes = list.filter((c) => c.note);
  const go = async (c: GridCommentInfo) => {
    if (c.sheet !== sheet) await features.model.run('grid.sheet.activate', { index: c.sheet });
    await features.model.core('grid.nav.select_cell', { row: c.row, col: c.col });
  };
  const post = async (text: string, reply: string | null) => {
    if (!text.trim()) return;
    await features.exec({ AddComment: { text, author: authorName(), reply_to: reply, now_ms: Date.now() } });
  };
  // Posting or cancelling hands the keys back to the sheet, as Excel does.
  const closeComposer = () => {
    features.set({ composer: null });
    features.model.surface?.focus();
  };
  return (
    <PaneFrame features={features} title={t('Comments')} testId="grid-comments-pane">
      <div className="kiln-gfx__toolbar">
        <Button size="sm" onClick={() => features.set({ composer: { reply: null } })}>
          <Icon icon={MessageSquare} size={14} /> {t('New')}
        </Button>
        <Button size="sm" onClick={() => features.exec({ NextComment: { backward: true } })}>
          {t('Previous')}
        </Button>
        <Button size="sm" onClick={() => features.exec({ NextComment: { backward: false } })}>
          {t('Next')}
        </Button>
      </div>
      {composer?.reply === null ? (
        <form
          className="kiln-cmt__composer"
          data-testid="grid-comment-composer"
          onSubmit={async (e) => {
            e.preventDefault();
            await post(draft, null);
            setDraft('');
            closeComposer();
          }}
        >
          <div className="kiln-cmt__where">
            {t('New comment at')} {grid?.name_box}
          </div>
          <textarea
            // biome-ignore lint/a11y/noAutofocus: the composer opens to be typed in.
            autoFocus
            aria-label={t('Comment')}
            placeholder={t('Start a conversation')}
            value={draft}
            data-testid="grid-comment-draft"
            onChange={(e) => setDraft(e.target.value)}
            onKeyDown={(e) => {
              if (e.key === 'Enter' && (e.ctrlKey || e.metaKey)) e.currentTarget.form?.requestSubmit();
              if (e.key === 'Escape') {
                e.preventDefault();
                closeComposer();
              }
            }}
          />
          <div className="kiln-gfx__toolbar">
            <span className="kiln-gfx__grow" />
            <Button size="sm" onClick={closeComposer}>
              {t('Cancel')}
            </Button>
            <Button
              size="sm"
              variant="primary"
              type="submit"
              disabled={!draft.trim()}
              data-testid="grid-comment-post"
            >
              {t('Post')}
            </Button>
          </div>
        </form>
      ) : null}
      {roots.length === 0 && notes.length === 0 && !composer ? (
        <p className="kiln-gfx__hint">{t('No comments yet. Select a cell and choose New.')}</p>
      ) : null}
      {roots.map((c) => {
        const thread = list.filter((x) => x.parent === c.id);
        const name = grid?.sheets[c.sheet]?.name ?? '';
        return (
          <article
            key={c.id}
            className={c.done ? 'kiln-cmt kiln-cmt--done' : 'kiln-cmt'}
            data-testid="grid-comment"
          >
            <button type="button" className="kiln-cmt__where" onClick={() => go(c)}>
              {name}!{c.cell}
            </button>
            {[c, ...thread].map((m) => (
              <div key={m.id} className="kiln-cmt__msg">
                <div className="kiln-cmt__meta">
                  <span className="kiln-cmt__avatar" aria-hidden="true">
                    {m.author.slice(0, 1).toUpperCase()}
                  </span>
                  <strong>{m.author}</strong>
                  <span className="kiln-cmt__date">{when(m.date)}</span>
                  <button
                    type="button"
                    className="kiln-gfx__icon"
                    aria-label={m.id === c.id ? t('Edit comment') : t('Edit reply')}
                    onClick={() => setEditing({ id: m.id, text: m.text })}
                  >
                    <Icon icon={PencilLine} size={12} />
                  </button>
                  {m.id !== c.id ? (
                    <button
                      type="button"
                      className="kiln-gfx__icon"
                      aria-label={t('Delete reply')}
                      onClick={() => features.exec({ DeleteComment: { id: m.id } })}
                    >
                      <Icon icon={Trash2} size={12} />
                    </button>
                  ) : null}
                </div>
                {editing?.id === m.id ? (
                  // Excel's Edit comment: the text in a box, Enter saves it, Esc leaves it.
                  <input
                    className="kiln-cmt__edit"
                    aria-label={t('Edit comment text')}
                    value={editing.text}
                    // biome-ignore lint/a11y/noAutofocus: the box the Edit button opened.
                    autoFocus
                    onChange={(e) => setEditing({ id: m.id, text: e.target.value })}
                    onKeyDown={async (e) => {
                      if (e.key === 'Enter' && editing.text.trim()) {
                        e.preventDefault();
                        await features.exec({ EditComment: { id: m.id, text: editing.text.trim() } });
                        setEditing(null);
                      } else if (e.key === 'Escape') {
                        e.preventDefault();
                        e.stopPropagation();
                        setEditing(null);
                      }
                    }}
                  />
                ) : (
                  <p>{m.text}</p>
                )}
              </div>
            ))}
            <div className="kiln-cmt__actions">
              <input
                aria-label={t('Reply')}
                placeholder={t('Reply')}
                value={replies[c.id] ?? ''}
                onChange={(e) => setReplies({ ...replies, [c.id]: e.target.value })}
                onKeyDown={async (e) => {
                  if (e.key === 'Enter') {
                    e.preventDefault();
                    await post(replies[c.id] ?? '', c.id);
                    setReplies({ ...replies, [c.id]: '' });
                    features.model.surface?.focus();
                  }
                }}
              />
              <button
                type="button"
                className="kiln-gfx__icon"
                aria-label={c.done ? t('Reopen thread') : t('Resolve thread')}
                title={c.done ? t('Reopen') : t('Resolve')}
                onClick={() => features.exec({ ResolveComment: { id: c.id, done: !c.done } })}
              >
                <Icon icon={CheckIcon} size={14} />
              </button>
              <button
                type="button"
                className="kiln-gfx__icon"
                aria-label={t('Delete thread')}
                onClick={() => features.exec({ DeleteComment: { id: c.id } })}
              >
                <Icon icon={Trash2} size={14} />
              </button>
            </div>
          </article>
        );
      })}
      {notes.length ? <h3 className="kiln-lab__h">{t('Notes')}</h3> : null}
      {notes.map((n) => (
        <article key={`${n.sheet}-${n.cell}`} className="kiln-cmt kiln-cmt--note">
          <button type="button" className="kiln-cmt__where" onClick={() => go(n)}>
            <Icon icon={StickyNote} size={12} /> {grid?.sheets[n.sheet]?.name}!{n.cell}
          </button>
          <div className="kiln-cmt__meta">
            <strong>{n.author}</strong>
          </div>
          <p>{n.text}</p>
        </article>
      ))}
    </PaneFrame>
  );
}

function SpellingPane({ features }: { features: GridFeatures }) {
  const [hit, setHit] = useState<GridSpellingHit | null | undefined>(undefined);
  const [change, setChange] = useState('');
  const next = async () => {
    const r = await features.ask('Spelling');
    const h = r && 'Spelling' in r ? r.Spelling : null;
    setHit(h);
    setChange(h?.suggestions[0] ?? '');
    if (h) await features.model.core('grid.nav.select_cell', { row: h.row, col: h.col });
  };
  useEffect(() => {
    void (async () => {
      await ensureGridDictionary(features);
      await next();
    })();
  }, [features]); // eslint-disable-line react-hooks/exhaustive-deps
  const act = async (with_: string, all: boolean) => {
    if (!hit) return;
    await features.exec({
      SpellChange: {
        sheet: hit.sheet,
        row: hit.row,
        col: hit.col,
        offset: hit.offset,
        word: hit.word,
        with: with_,
        all,
      },
    });
    await next();
  };
  return (
    <PaneFrame features={features} title={t('Spelling')} testId="grid-spelling">
      {hit === undefined ? <p className="kiln-gfx__hint">{t('Checking…')}</p> : null}
      {hit === null ? (
        <p className="kiln-gfx__prompt">{t("Spell check complete. You're good to go!")}</p>
      ) : null}
      {hit ? (
        <>
          <p className="kiln-gfx__prompt">{t('Not in dictionary:')}</p>
          <div className="kiln-spell__word">{hit.word}</div>
          <p className="kiln-gfx__hint">{hit.text}</p>
          <div className="kiln-griddlg__list" role="listbox" aria-label={t('Suggestions')}>
            {hit.suggestions.map((s) => (
              <div
                key={s}
                role="option"
                tabIndex={-1}
                aria-selected={s === change}
                className={
                  s === change ? 'kiln-griddlg__option kiln-griddlg__option--on' : 'kiln-griddlg__option'
                }
                onClick={() => setChange(s)}
                onKeyDown={(e) => e.key === 'Enter' && setChange(s)}
                onDoubleClick={() => act(s, false)}
              >
                {s}
              </div>
            ))}
            {hit.suggestions.length === 0 ? <p className="kiln-gfx__hint">{t('(No suggestions)')}</p> : null}
          </div>
          <div className="kiln-gfx__grid2">
            <Button size="sm" onClick={() => act(hit.word, false)}>
              {t('Ignore Once')}
            </Button>
            <Button
              size="sm"
              onClick={async () => {
                await features.exec({ SpellIgnore: { word: hit.word, add: false } });
                await next();
              }}
            >
              {t('Ignore All')}
            </Button>
            <Button
              size="sm"
              onClick={async () => {
                await features.exec({ SpellIgnore: { word: hit.word, add: true } });
                await next();
              }}
            >
              {t('Add to Dictionary')}
            </Button>
            <span />
            <Button
              size="sm"
              variant="primary"
              disabled={!change}
              onClick={() => act(change, false)}
              data-testid="grid-spell-change"
            >
              {t('Change')}
            </Button>
            <Button size="sm" disabled={!change} onClick={() => act(change, true)}>
              {t('Change All')}
            </Button>
          </div>
        </>
      ) : null}
    </PaneFrame>
  );
}

let dictionary: Promise<void> | null = null;

/** The web core gets the en-US dictionary from the page (native builds embed it). */
function ensureGridDictionary(features: GridFeatures): Promise<void> {
  if (dictionary) return dictionary;
  dictionary = (async () => {
    try {
      const [aff, dic] = await Promise.all([
        fetch(affUrl).then((r) => (r.ok ? r.text() : '')),
        fetch(dicUrl).then((r) => (r.ok ? r.text() : '')),
      ]);
      if (aff && dic)
        await features.model.client.execute(features.model.doc, { Grid: { LoadDictionary: { aff, dic } } });
    } catch {
      dictionary = null;
    }
  })();
  return dictionary;
}
