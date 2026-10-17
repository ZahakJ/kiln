/**
 * The Navigation pane (View ▸ Navigation Pane, Ctrl+F): search the document, browse its headings
 * and pages, and jump there. Everything listed comes from the core (`Headings`, `Pages`,
 * `FindAll`); a click runs `write.find.go_to` or selects a match.
 */
import type { FindHit, HeadingInfo } from '@kiln/protocol';
import { Button, Icon, Tabs, TabsContent, TabsList, TabsTrigger } from '@kiln/ui-kit';
import { ChevronDown, ChevronRight, ChevronUp, Search, X } from 'lucide-react';
import { type KeyboardEvent, useEffect, useMemo, useRef, useState } from 'react';
import { useWrite } from '../hooks.ts';
import type { NavTab, WriteEditorModel } from '../model.ts';

const fmt = (n: number) => n.toLocaleString();

function Headings({ model, headings }: { model: WriteEditorModel; headings: HeadingInfo[] | null }) {
  const [collapsed, setCollapsed] = useState<Set<number>>(new Set());
  if (!headings) return <p className="kiln-write-nav__empty">Reading the headings…</p>;
  if (headings.length === 0)
    return (
      <p className="kiln-write-nav__empty">
        Headings you add with the Heading styles show here, so you can jump to them.
      </p>
    );
  // A heading is hidden when an earlier, shallower heading is collapsed.
  const visible: { h: HeadingInfo; i: number; hasChildren: boolean }[] = [];
  let hideBelow: number | null = null;
  headings.forEach((h, i) => {
    if (hideBelow !== null && h.level > hideBelow) return;
    hideBelow = null;
    const next = headings[i + 1];
    const hasChildren = !!next && next.level > h.level;
    visible.push({ h, i, hasChildren });
    if (collapsed.has(i) && hasChildren) hideBelow = h.level;
  });
  return (
    <ul className="kiln-write-nav__headings" aria-label="Headings">
      {visible.map(({ h, i, hasChildren }) => (
        <li key={`${h.para}`} style={{ paddingInlineStart: `${(h.level - 1) * 14}px` }}>
          {hasChildren ? (
            <button
              type="button"
              className="kiln-write-nav__twisty"
              aria-label={collapsed.has(i) ? `Expand ${h.text}` : `Collapse ${h.text}`}
              aria-expanded={!collapsed.has(i)}
              onClick={() =>
                setCollapsed((c) => {
                  const n = new Set(c);
                  if (n.has(i)) n.delete(i);
                  else n.add(i);
                  return n;
                })
              }
            >
              <Icon icon={collapsed.has(i) ? ChevronRight : ChevronDown} size={12} />
            </button>
          ) : (
            <span className="kiln-write-nav__twisty" aria-hidden="true" />
          )}
          <button
            type="button"
            className="kiln-write-nav__heading"
            data-current={h.current || undefined}
            aria-current={h.current ? 'location' : undefined}
            onClick={() => void model.run('write.find.go_to', { target: { Paragraph: { para: h.para } } })}
          >
            {h.text}
          </button>
        </li>
      ))}
    </ul>
  );
}

function Pages({ model }: { model: WriteEditorModel }) {
  const pages = useWrite(model, (s) => s.pages);
  if (!pages) return <p className="kiln-write-nav__empty">Laying out the pages…</p>;
  return (
    <ol className="kiln-write-nav__pages" aria-label="Pages">
      {pages.rects.map((r, i) => {
        const w = 76;
        const h = Math.round((r.height / r.width) * w);
        return (
          <li key={`${r.x}-${r.y}`}>
            <button
              type="button"
              className="kiln-write-nav__page"
              data-current={i === pages.current || undefined}
              aria-current={i === pages.current ? 'page' : undefined}
              aria-label={`Page ${i + 1}`}
              onClick={() =>
                void model.run('write.find.go_to', { target: { Page: { number: i + 1, relative: false } } })
              }
            >
              <span className="kiln-write-nav__sheet" style={{ width: w, height: h }} />
              <span className="kiln-write-nav__page-number">{i + 1}</span>
            </button>
          </li>
        );
      })}
    </ol>
  );
}

function Results({
  model,
  hits,
  total,
  error,
}: {
  model: WriteEditorModel;
  hits: FindHit[];
  total: number;
  error: string | null;
}) {
  const q = useWrite(model, (s) => s.findQuery);
  if (error)
    return (
      <p className="kiln-write-nav__empty" role="alert">
        {error}
      </p>
    );
  if (!q.text)
    return <p className="kiln-write-nav__empty">Search for text, then pick a result to go there.</p>;
  if (total === 0) return <p className="kiln-write-nav__empty">No results for “{q.text}”.</p>;
  return (
    <>
      <p className="kiln-write-nav__count" aria-live="polite">
        {total === 1 ? '1 result' : `${fmt(total)} results`}
        {hits.length < total ? ` (first ${fmt(hits.length)} shown)` : ''}
      </p>
      <ul className="kiln-write-nav__results" aria-label="Search results">
        {hits.map((h) => (
          <li key={`${h.start.block}:${h.start.offset}`}>
            <button
              type="button"
              className="kiln-write-nav__result"
              onClick={async () => {
                await model.run('write.caret.select_to', { pos: h.start, extend: false });
                await model.run('write.caret.select_to', { pos: h.end, extend: true });
              }}
            >
              <span className="kiln-write-nav__context">
                {h.before}
                <mark>{h.text}</mark>
                {h.after}
              </span>
              {h.page ? <span className="kiln-write-nav__result-page">Page {h.page}</span> : null}
            </button>
          </li>
        ))}
      </ul>
    </>
  );
}

/** Word count and reading time (the mockup's card at the foot of the pane). */
function Facts({ model }: { model: WriteEditorModel }) {
  const status = useWrite(model, (s) => s.status);
  const n = status?.words ?? 0;
  const pages = status?.page?.total ?? 1;
  const minutes = Math.max(1, Math.round(n / 230));
  return (
    <section className="kiln-write-nav__facts" aria-label="Document facts">
      <div>
        <strong className="kiln-write-nav__fact">{fmt(n)}</strong>
        <span>words</span>
      </div>
      <div>
        <strong className="kiln-write-nav__fact">{fmt(pages)}</strong>
        <span>{pages === 1 ? 'page' : 'pages'}</span>
      </div>
      <div>
        <strong className="kiln-write-nav__fact">{minutes}</strong>
        <span>min to read</span>
      </div>
    </section>
  );
}

export function NavigationPane({ model }: { model: WriteEditorModel }) {
  const tab = useWrite(model, (s) => s.navTab);
  const headings = useWrite(model, (s) => s.headings);
  const results = useWrite(model, (s) => s.findResults);
  const query = useWrite(model, (s) => s.findQuery);
  const focus = useWrite(model, (s) => s.searchFocus);
  const [text, setText] = useState(query.text);
  const input = useRef<HTMLInputElement>(null);
  const timer = useRef<ReturnType<typeof setTimeout> | null>(null);

  useEffect(() => {
    if (focus > 0) {
      input.current?.focus();
      input.current?.select();
    }
  }, [focus]);
  useEffect(
    () => () => {
      if (timer.current) clearTimeout(timer.current);
    },
    [],
  );

  const search = (value: string) => {
    setText(value);
    if (timer.current) clearTimeout(timer.current);
    timer.current = setTimeout(() => {
      void model.find({ ...model.state.findQuery, text: value });
      if (value) model.set({ navTab: 'results' });
    }, 160);
  };
  const step = (backward: boolean) => {
    if (!text) return;
    void model.run('write.find.next', { query: { ...model.state.findQuery, text }, backward });
  };
  const onKey = (e: KeyboardEvent<HTMLInputElement>) => {
    if (e.key === 'Enter') {
      e.preventDefault();
      step(e.shiftKey);
    } else if (e.key === 'Escape') {
      e.preventDefault();
      model.controller?.focus();
    }
  };
  const hits = useMemo(() => results?.hits ?? [], [results]);

  return (
    <div className="kiln-write-nav" data-testid="write-navigation">
      <div className="kiln-write-nav__search">
        <Icon icon={Search} size={14} />
        <input
          ref={input}
          type="search"
          value={text}
          placeholder="Search document"
          aria-label="Search document"
          onChange={(e) => search(e.target.value)}
          onKeyDown={onKey}
        />
        {text ? (
          <button
            type="button"
            className="kiln-write-nav__clear"
            aria-label="Clear search"
            onClick={() => search('')}
          >
            <Icon icon={X} size={12} />
          </button>
        ) : null}
      </div>
      <div className="kiln-write-nav__steps">
        <Button
          size="sm"
          variant="ghost"
          aria-label="Previous result"
          onClick={() => step(true)}
          disabled={!text}
        >
          <Icon icon={ChevronUp} size={14} />
        </Button>
        <Button
          size="sm"
          variant="ghost"
          aria-label="Next result"
          onClick={() => step(false)}
          disabled={!text}
        >
          <Icon icon={ChevronDown} size={14} />
        </Button>
        <label className="kiln-write-nav__option">
          <input
            type="checkbox"
            checked={query.match_case}
            onChange={(e) =>
              void model.find({ ...model.state.findQuery, text, match_case: e.target.checked })
            }
          />
          Match case
        </label>
        <label className="kiln-write-nav__option">
          <input
            type="checkbox"
            checked={query.whole_word}
            onChange={(e) =>
              void model.find({ ...model.state.findQuery, text, whole_word: e.target.checked })
            }
          />
          Whole words
        </label>
      </div>
      <Tabs
        value={tab}
        onValueChange={(v) => {
          model.set({ navTab: v as NavTab });
          model.refresh({ content: true });
        }}
      >
        <TabsList variant="segmented" aria-label="Show" className="kiln-write-nav__tabs">
          <TabsTrigger value="headings">Headings</TabsTrigger>
          <TabsTrigger value="pages">Pages</TabsTrigger>
          <TabsTrigger value="results">Results</TabsTrigger>
        </TabsList>
        <TabsContent value="headings" className="kiln-write-nav__list">
          <Headings model={model} headings={headings} />
        </TabsContent>
        <TabsContent value="pages" className="kiln-write-nav__list">
          <Pages model={model} />
        </TabsContent>
        <TabsContent value="results" className="kiln-write-nav__list">
          <Results model={model} hits={hits} total={results?.total ?? 0} error={results?.error ?? null} />
        </TabsContent>
      </Tabs>
      <Facts model={model} />
    </div>
  );
}
