/**
 * The Accessible text view (View ▸ Accessibility ▸ Text View): the document as plain
 * semantic DOM for screen reader users who prefer it, over the same model as the canvas. Write
 * shows headings, paragraphs, lists, tables with their header cells, pictures by their alt text,
 * text boxes, headers and footers; Grid the active sheet's used cells as a table with row and
 * column headers; Deck every slide as a section with its title, shapes and notes.
 *
 * It edits through the core's commands, as the canvas does: typing, Enter and Backspace become the
 * same `InsertText`/`Delete…` commands, the caret and selection follow into the model (so Ctrl+B
 * and every other shortcut acts on what is selected here), and the view re-reads the model after
 * every change wherever it came from.
 */
import { msg, t } from '@kiln/i18n';
import type { A11yNode, A11yTree, Command, DocId, KilnClient } from '@kiln/protocol';
import { commandForInput } from '@kiln/surface';
import {
  Fragment,
  type KeyboardEvent as ReactKeyboardEvent,
  type ReactNode,
  useCallback,
  useEffect,
  useLayoutEffect,
  useMemo,
  useRef,
  useState,
} from 'react';
import { Button } from '../primitives/Button.tsx';
import './a11y.css';

export type TextViewApp = 'write' | 'grid' | 'deck';

export interface AccessibleTextViewProps {
  client: KilnClient;
  doc: DocId;
  app: TextViewApp;
  /** Turns the view off (the toggle and the Close button). */
  onClose: () => void;
}

const OBJECT = '￼';

/** Unicode scalar values in a string (the core's offsets). */
function scalars(s: string): number {
  let n = 0;
  for (const _ of s) n += 1;
  return n;
}

/** The UTF-16 index of scalar offset `n` in `s`. */
function utf16At(s: string, n: number): number {
  let i = 0;
  let k = 0;
  for (const ch of s) {
    if (k === n) return i;
    i += ch.length;
    k += 1;
  }
  return s.length;
}

// ── Offsets between the DOM and the model ─────────────────────────────────────────────────────

/** Characters a DOM subtree stands for: labels count none, an object one. */
function lengthOf(n: Node): number {
  if (n.nodeType === Node.TEXT_NODE) return scalars(n.nodeValue ?? '');
  if (!(n instanceof HTMLElement)) return 0;
  if (n.dataset.kilnLabel !== undefined) return 0;
  if (n.dataset.kilnObject !== undefined) return 1;
  let k = 0;
  for (const c of n.childNodes) k += lengthOf(c);
  return k;
}

/** The model offset of a DOM point inside a text element (a paragraph). */
export function offsetIn(host: HTMLElement, node: Node, offset: number): number {
  let n = 0;
  const walk = (el: Node): boolean => {
    if (el === node) {
      if (el.nodeType === Node.TEXT_NODE) {
        n += scalars((el.nodeValue ?? '').slice(0, offset));
      } else {
        for (let i = 0; i < offset && i < el.childNodes.length; i++) n += lengthOf(el.childNodes[i] as Node);
      }
      return true;
    }
    if (el instanceof HTMLElement && el !== host) {
      if (el.dataset.kilnLabel !== undefined) return el.contains(node);
      if (el.dataset.kilnObject !== undefined) {
        if (el.contains(node)) return true;
        n += 1;
        return false;
      }
    }
    if (el.nodeType === Node.TEXT_NODE) {
      n += scalars(el.nodeValue ?? '');
      return false;
    }
    for (const c of el.childNodes) if (walk(c)) return true;
    return false;
  };
  walk(host);
  return n;
}

/** The DOM point of a model offset inside a text element. */
export function pointIn(host: HTMLElement, offset: number): { node: Node; offset: number } {
  let left = offset;
  let found: { node: Node; offset: number } | null = null;
  const walk = (el: Node): boolean => {
    if (el instanceof HTMLElement && el !== host) {
      if (el.dataset.kilnLabel !== undefined) return false;
      if (el.dataset.kilnObject !== undefined) {
        if (left === 0) {
          const parent = el.parentNode as Node;
          found = { node: parent, offset: Array.prototype.indexOf.call(parent.childNodes, el) };
          return true;
        }
        left -= 1;
        return false;
      }
    }
    if (el.nodeType === Node.TEXT_NODE) {
      const text = el.nodeValue ?? '';
      const len = scalars(text);
      if (left <= len) {
        found = { node: el, offset: utf16At(text, left) };
        return true;
      }
      left -= len;
      return false;
    }
    for (const c of el.childNodes) if (walk(c)) return true;
    return false;
  };
  walk(host);
  return found ?? { node: host, offset: host.childNodes.length };
}

// ── Rendering ─────────────────────────────────────────────────────────────────────────────────

type ById = ReadonlyMap<number, A11yNode>;

function kids(node: A11yNode, byId: ById): A11yNode[] {
  return node.children.map((c) => byId.get(c)).filter((c): c is A11yNode => !!c);
}

/**
 * A paragraph's text with its label, objects and tracked changes, as offsets count them: every
 * model character stays in the DOM (a change the review display hides is a hidden span), so a
 * DOM point maps to its model offset.
 */
export function Inline({ node }: { node: A11yNode }) {
  const text = node.text ?? '';
  const objects = node.objects ?? [];
  const revisions = node.revisions ?? [];
  const kindAt = (i: number): string => {
    const r = revisions.find((x) => i >= x.start && i < x.end);
    if (!r || r.display === 'Plain') return 'plain';
    if (r.display === 'Hidden') return 'hidden';
    return r.inserted ? 'ins' : 'del';
  };
  // Runs of one kind, each a list of text and object pieces.
  const runs: { kind: string; at: number; pieces: ReactNode[] }[] = [];
  let k = 0;
  let at = 0;
  let buf = '';
  const flush = () => {
    const last = runs[runs.length - 1];
    if (buf && last) last.pieces.push(buf);
    buf = '';
  };
  for (const ch of text) {
    const kind = kindAt(at);
    if (runs[runs.length - 1]?.kind !== kind) {
      flush();
      runs.push({ kind, at, pieces: [] });
    }
    if (ch === OBJECT) {
      flush();
      const label = objects[k] ?? '';
      runs[runs.length - 1]?.pieces.push(
        <span key={`o${at}`} data-kiln-object="" contentEditable={false} className="kiln-textview__object">
          {label.trim() ? label : null}
        </span>,
      );
      k += 1;
    } else buf += ch;
    at += 1;
  }
  flush();
  return (
    <>
      {node.role === 'ListItem' && node.name ? (
        <span data-kiln-label="" contentEditable={false} className="kiln-textview__label">
          {`${node.name} `}
        </span>
      ) : null}
      {runs.map((r) =>
        r.kind === 'ins' ? (
          <ins key={r.at}>{r.pieces}</ins>
        ) : r.kind === 'del' ? (
          <del key={r.at}>{r.pieces}</del>
        ) : r.kind === 'hidden' ? (
          <span key={r.at} hidden data-kiln-hidden="">
            {r.pieces}
          </span>
        ) : (
          <Fragment key={r.at}>{r.pieces}</Fragment>
        ),
      )}
      {/* An empty paragraph still takes the caret. */}
      {text === '' ? <br /> : null}
    </>
  );
}

function textKey(n: A11yNode): string {
  const revs = (n.revisions ?? []).map((r) => `${r.start}-${r.end}${r.display}`).join(',');
  return `${n.id}:${n.text ?? ''}:${(n.objects ?? []).join('\u001f')}:${n.name ?? ''}:${revs}`;
}

function WriteNode({ node, byId, depth }: { node: A11yNode; byId: ById; depth: number }): ReactNode {
  const children = depth > 40 ? [] : kids(node, byId);
  const sub = (n: A11yNode) => <WriteNode key={textKey(n)} node={n} byId={byId} depth={depth + 1} />;
  const data = { 'data-kiln-node': node.id };
  switch (node.role) {
    case 'Heading': {
      const level = Math.min(9, Math.max(1, node.level ?? 1));
      const Tag = (level <= 6 ? `h${level}` : 'p') as 'h1';
      return (
        <>
          <Tag {...data} {...(level > 6 ? { role: 'heading', 'aria-level': level } : {})}>
            <Inline node={node} />
          </Tag>
          {children.filter((c) => c.role === 'TextBox').map(sub)}
        </>
      );
    }
    case 'Paragraph':
      return (
        <>
          <p {...data}>
            <Inline node={node} />
          </p>
          {children.filter((c) => c.role === 'TextBox').map(sub)}
        </>
      );
    case 'ListItem':
      return (
        <li {...data}>
          <Inline node={node} />
          {children.filter((c) => c.role === 'List' || c.role === 'TextBox').map(sub)}
        </li>
      );
    case 'List':
      return <ul className="kiln-textview__list">{children.map(sub)}</ul>;
    case 'Table':
      return (
        <table className="kiln-textview__table">
          {node.name ? <caption>{node.name}</caption> : null}
          <tbody>{children.map(sub)}</tbody>
        </table>
      );
    case 'Row':
      return <tr>{children.map(sub)}</tr>;
    case 'ColumnHeader':
      return <th scope="col">{children.map(sub)}</th>;
    case 'Cell':
      return <td>{children.map(sub)}</td>;
    case 'TextBox':
      return (
        <section className="kiln-textview__box" aria-label={node.name ?? t('Text box')}>
          {children.map(sub)}
        </section>
      );
    case 'Group':
      return (
        <section className="kiln-textview__part" aria-label={node.name ?? undefined}>
          {children.map(sub)}
        </section>
      );
    case 'Image':
      return <span role="img" aria-label={node.name ?? t('Picture')} />;
    default:
      return <div>{children.map(sub)}</div>;
  }
}

// ── The view ──────────────────────────────────────────────────────────────────────────────────

const TITLES: Readonly<Record<TextViewApp, string>> = {
  write: msg('Text view'),
  grid: msg('Text view of the sheet'),
  deck: msg('Text view of the slides'),
};

/** Fetches the whole-document tree, again after every change of the document. */
function useDocumentTree(client: KilnClient, doc: DocId): [A11yTree | null, () => void] {
  const [tree, setTree] = useState<A11yTree | null>(null);
  const pending = useRef<ReturnType<typeof setTimeout> | null>(null);
  const live = useRef(true);
  const fetchTree = useCallback(() => {
    if (pending.current) return;
    pending.current = setTimeout(() => {
      pending.current = null;
      void client.query(doc, 'A11yDocument').then(
        (r) => {
          if (live.current && typeof r === 'object' && r !== null && 'A11y' in r) setTree(r.A11y);
        },
        () => undefined,
      );
    }, 16);
  }, [client, doc]);
  useEffect(() => {
    live.current = true;
    fetchTree();
    const stop = client.subscribe((e) => {
      if (typeof e === 'object' && e !== null && 'Changed' in e && e.Changed.summary.doc === doc) fetchTree();
    });
    return () => {
      live.current = false;
      stop();
      if (pending.current) clearTimeout(pending.current);
      pending.current = null;
    };
  }, [client, doc, fetchTree]);
  return [tree, fetchTree];
}

export function AccessibleTextView({ client, doc, app, onClose }: AccessibleTextViewProps) {
  const [tree, refetch] = useDocumentTree(client, doc);
  const root = useRef<HTMLElement>(null);
  // The keys go to the document once it is there (the toggle took them from the ribbon).
  const ready = tree !== null;
  useEffect(() => {
    if (!ready) return;
    root.current?.querySelector<HTMLElement>('[data-kiln-text-view-focus], td[tabindex="0"]')?.focus();
  }, [ready]);
  const exec = useCallback(
    (command: Command) => client.execute(doc, command).catch(() => undefined),
    [client, doc],
  );
  return (
    <section
      ref={root}
      className="kiln-textview"
      aria-label={t('Accessible text view')}
      data-testid="text-view"
    >
      <div className="kiln-textview__bar">
        <h2 className="kiln-textview__title">{t(TITLES[app])}</h2>
        <p className="kiln-textview__hint">
          {app === 'grid'
            ? t('Arrow keys move between cells; Enter or F2 edits a cell, Enter again saves it.')
            : t('Edits here change the document. The ribbon and every shortcut work on what you select.')}
        </p>
        <Button size="sm" onClick={onClose}>
          {t('Close Text View')}
        </Button>
      </div>
      <div className="kiln-textview__body">
        {tree === null ? (
          <p className="kiln-textview__loading">{t('Reading the document…')}</p>
        ) : app === 'write' ? (
          <WriteText tree={tree} exec={exec} refetch={refetch} />
        ) : app === 'grid' ? (
          <GridText tree={tree} exec={exec} client={client} doc={doc} />
        ) : (
          <DeckText tree={tree} exec={exec} refetch={refetch} />
        )}
      </div>
    </section>
  );
}

type Exec = (command: Command) => Promise<unknown>;

/** The paragraph element and model offset of a DOM point in the view. */
function locate(root: HTMLElement, node: Node | null, offset: number): { id: number; offset: number } | null {
  if (!node) return null;
  const el = (node instanceof HTMLElement ? node : node.parentElement)?.closest<HTMLElement>(
    '[data-kiln-node]',
  );
  if (!el || !root.contains(el)) return null;
  return { id: Number(el.dataset.kilnNode), offset: offsetIn(el, node, offset) };
}

function WriteText({ tree, exec, refetch }: { tree: A11yTree; exec: Exec; refetch: () => void }) {
  const ref = useRef<HTMLElement>(null);
  const byId = useMemo(() => new Map(tree.nodes.map((n) => [n.id, n])), [tree]);
  const root = tree.nodes[0];
  /** The selection last sent to the core (anchor, head), to send only changes. */
  const sent = useRef<string>('');
  const placing = useRef(false);

  const syncSelection = useCallback(() => {
    const el = ref.current;
    const sel = typeof window === 'undefined' ? null : window.getSelection();
    if (!el || !sel || placing.current || !el.contains(sel.anchorNode)) return;
    const a = locate(el, sel.anchorNode, sel.anchorOffset);
    const h = locate(el, sel.focusNode, sel.focusOffset);
    if (!a || !h) return;
    const key = `${a.id}:${a.offset}:${h.id}:${h.offset}`;
    if (key === sent.current) return;
    sent.current = key;
    void exec({ Write: { SelectTo: { pos: { block: a.id, offset: a.offset }, extend: false } } });
    if (a.id !== h.id || a.offset !== h.offset)
      void exec({ Write: { SelectTo: { pos: { block: h.id, offset: h.offset }, extend: true } } });
  }, [exec]);

  useEffect(() => {
    document.addEventListener('selectionchange', syncSelection);
    return () => document.removeEventListener('selectionchange', syncSelection);
  }, [syncSelection]);

  // After the model changed: the caret where the model has it (when the view has the keys).
  useLayoutEffect(() => {
    const el = ref.current;
    const caret = tree.caret;
    if (!el || !caret || !el.contains(document.activeElement)) return;
    const sel = window.getSelection();
    const host = el.querySelector<HTMLElement>(`[data-kiln-node="${caret.block}"]`);
    if (!sel || !host) return;
    const now = locate(el, sel.focusNode, sel.focusOffset);
    if (now && now.id === caret.block && now.offset === caret.offset && sel.isCollapsed) return;
    // A range the user is making stays; a caret follows the model.
    if (!sel.isCollapsed) return;
    const p = pointIn(host, caret.offset);
    placing.current = true;
    sel.collapse(p.node, p.offset);
    sent.current = `${caret.block}:${caret.offset}:${caret.block}:${caret.offset}`;
    queueMicrotask(() => {
      placing.current = false;
    });
  }, [tree]);

  const onBeforeInput = (e: InputEvent) => {
    e.preventDefault();
    syncSelection();
    const command = commandForInput(e.inputType, e.data);
    if (command) void exec(command).then(refetch);
  };

  // React's synthetic beforeinput lacks `inputType`; listen natively.
  useEffect(() => {
    const el = ref.current;
    if (!el) return;
    const handler = (e: Event) => onBeforeInput(e as InputEvent);
    el.addEventListener('beforeinput', handler);
    // A shortcut acts on the selection made here: it goes to the core before the key does.
    const keydown = () => syncSelection();
    el.addEventListener('keydown', keydown, true);
    const compose = (e: CompositionEvent) => {
      if (e.data) void exec({ Write: { InsertText: { text: e.data } } }).then(refetch);
    };
    el.addEventListener('compositionend', compose);
    return () => {
      el.removeEventListener('beforeinput', handler);
      el.removeEventListener('keydown', keydown, true);
      el.removeEventListener('compositionend', compose);
    };
  });

  if (!root) return null;
  return (
    <article
      ref={ref}
      className="kiln-textview__doc"
      contentEditable
      suppressContentEditableWarning
      aria-label={root.name ?? t('Document')}
      spellCheck={false}
      // biome-ignore lint/a11y/noNoninteractiveTabindex: the editable document takes the keys (a Tab stop)
      tabIndex={0}
      data-kiln-text-view-focus=""
    >
      {kids(root, byId).map((n) => (
        <WriteNode key={textKey(n)} node={n} byId={byId} depth={0} />
      ))}
    </article>
  );
}

// ── Grid ──────────────────────────────────────────────────────────────────────────────────────

/** A cell of the table by its place (row and column index in the view). */
interface Place {
  r: number;
  c: number;
}

/**
 * The sheet as a table: the core's tree gives every cell's name and text, the used range and a
 * row and column past it. Cells are found by their place in the table and selected in the sheet
 * by their names (Go To), so nothing here decodes addresses.
 */
function GridText({
  tree,
  exec,
  client,
  doc,
}: {
  tree: A11yTree;
  exec: Exec;
  client: KilnClient;
  doc: DocId;
}) {
  const byId = useMemo(() => new Map(tree.nodes.map((n) => [n.id, n])), [tree]);
  const grid = tree.nodes[0];
  const tableRef = useRef<HTMLTableElement>(null);
  const [editing, setEditing] = useState<{ at: Place; value: string; touched: boolean } | null>(null);
  const [focusAt, setFocusAt] = useState<Place | null>(null);
  // A right-to-left sheet (Page Layout ▸ Sheet Right-to-Left) reads from the right here too.
  const [rtl, setRtl] = useState(false);
  useEffect(() => {
    let live = true;
    void tree;
    void client.query(doc, { Grid: 'State' }).then(
      (r) => {
        const st =
          r && typeof r === 'object' && 'Grid' in r
            ? (r.Grid as { State?: { layout?: { rtl?: boolean } } })
            : null;
        if (live) setRtl(st?.State?.layout?.rtl === true);
      },
      () => undefined,
    );
    return () => {
      live = false;
    };
  }, [client, doc, tree]);
  const rows = grid ? kids(grid, byId) : [];
  const [headerRow, ...bodyRows] = rows;
  const table = bodyRows.map((row) => kids(row, byId).slice(1));
  const activeAt = (): Place | null => {
    for (let r = 0; r < table.length; r++) {
      const c = (table[r] ?? []).findIndex((n) => n.id === tree.active);
      if (c >= 0) return { r, c };
    }
    return null;
  };
  const active = focusAt ?? activeAt() ?? { r: 0, c: 0 };
  /** A place to give the keys to once it is drawn (after a commit adds a row). */
  const want = useRef<Place | null>(null);
  const focusWanted = useCallback(() => {
    const p = want.current;
    const el = p ? tableRef.current?.querySelector<HTMLElement>(`[data-kiln-place="${p.r}:${p.c}"]`) : null;
    if (el) {
      el.focus();
      want.current = null;
    }
  }, []);
  // Each new tree may hold the cell.
  useEffect(focusWanted, [tree]);
  if (!grid) return null;

  const nameAt = (p: Place) => table[p.r]?.[p.c]?.name ?? '';
  const select = (p: Place) => {
    const name = nameAt(p);
    if (name) void exec({ Grid: { GoTo: { reference: name, define: false } } });
  };
  const focusPlace = (p: Place) => {
    setFocusAt(p);
    want.current = p;
    select(p);
    requestAnimationFrame(focusWanted);
  };

  const startEdit = (p: Place, typed?: string) => {
    select(p);
    // The entry opens at once, so no key typed after the first is lost.
    setEditing({ at: p, value: typed ?? table[p.r]?.[p.c]?.text ?? '', touched: typed !== undefined });
    if (typed !== undefined) return;
    // F2 and Enter edit what the formula bar shows (the formula, not its value).
    void client.query(doc, { Grid: 'State' }).then(
      (r) => {
        const st =
          r && typeof r === 'object' && 'Grid' in r ? (r.Grid as { State?: { formula?: string } }) : null;
        const formula = st?.State?.formula;
        if (formula !== undefined)
          setEditing((e) =>
            e && e.at.r === p.r && e.at.c === p.c && !e.touched ? { ...e, value: formula } : e,
          );
      },
      () => undefined,
    );
  };

  const commit = async (p: Place, value: string, advance: 'Down' | null) => {
    setEditing(null);
    // The whole entry, as typed into the cell (Enter mode), then Enter.
    await exec({ Grid: { BeginEdit: { text: value } } });
    await exec({ Grid: { CommitEdit: { advance } } });
    // The sheet moved its active cell itself; the view follows when the row is drawn.
    const next = { r: p.r + (advance ? 1 : 0), c: p.c };
    setFocusAt(next);
    want.current = next;
    requestAnimationFrame(focusWanted);
  };

  const onCellKey = (e: ReactKeyboardEvent<HTMLElement>, p: Place) => {
    const move: Record<string, [number, number]> = {
      ArrowUp: [-1, 0],
      ArrowDown: [1, 0],
      // Left and Right follow the screen: a right-to-left sheet has column A at the right.
      ArrowLeft: [0, rtl ? 1 : -1],
      ArrowRight: [0, rtl ? -1 : 1],
    };
    const d = move[e.key];
    if (d && !e.altKey && !e.ctrlKey && !e.metaKey) {
      e.preventDefault();
      const next = { r: p.r + d[0], c: p.c + d[1] };
      if (table[next.r]?.[next.c]) focusPlace(next);
      return;
    }
    if (e.key === 'Enter' || e.key === 'F2') {
      e.preventDefault();
      e.stopPropagation();
      startEdit(p);
      return;
    }
    if (e.key.length === 1 && !e.ctrlKey && !e.metaKey && !e.altKey) {
      e.preventDefault();
      e.stopPropagation();
      startEdit(p, e.key);
    }
  };

  return (
    <table
      ref={tableRef}
      className="kiln-textview__table kiln-textview__sheet"
      dir={rtl ? 'rtl' : 'ltr'}
      aria-label={grid.name ?? t('Sheet')}
    >
      <thead>
        <tr>
          <td />
          {(headerRow ? kids(headerRow, byId) : []).map((h) => (
            <th key={h.id} scope="col">
              {h.name}
            </th>
          ))}
        </tr>
      </thead>
      <tbody>
        {bodyRows.map((row, r) => {
          const head = kids(row, byId)[0];
          return (
            <tr key={row.id}>
              <th scope="row">{head?.name}</th>
              {(table[r] ?? []).map((cell, c) => {
                const at = { r, c };
                const isEditing = editing?.at.r === r && editing.at.c === c;
                return (
                  <td
                    key={cell.id}
                    data-kiln-place={`${r}:${c}`}
                    tabIndex={active.r === r && active.c === c ? 0 : -1}
                    aria-label={
                      isEditing
                        ? undefined
                        : cell.text
                          ? t('{name}, {text}', { name: cell.name, text: cell.text })
                          : t('{name}, blank', { name: cell.name })
                    }
                    onKeyDown={isEditing ? undefined : (e) => onCellKey(e, at)}
                    onFocus={() => setFocusAt(at)}
                  >
                    {isEditing ? (
                      <input
                        className="kiln-textview__input"
                        aria-label={t('Edit {name}', { name: cell.name })}
                        value={editing.value}
                        onChange={(e) => {
                          const value = e.currentTarget.value;
                          setEditing((x) => (x ? { ...x, value, touched: true } : x));
                        }}
                        // biome-ignore lint/a11y/noAutofocus: editing a cell puts the keys in its entry, as in Excel
                        autoFocus
                        data-kiln-shortcuts="off"
                        onKeyDown={(e) => {
                          if (e.key === 'Enter') {
                            e.preventDefault();
                            void commit(at, e.currentTarget.value, e.shiftKey ? null : 'Down');
                          } else if (e.key === 'Escape') {
                            e.preventDefault();
                            e.stopPropagation();
                            setEditing(null);
                            focusPlace(at);
                          }
                        }}
                      />
                    ) : (
                      cell.text
                    )}
                  </td>
                );
              })}
            </tr>
          );
        })}
      </tbody>
    </table>
  );
}

// ── Deck ──────────────────────────────────────────────────────────────────────────────────────

/** A text point of a slide: the shape, the paragraph's index and the model offset in it. */
interface DeckPoint {
  shape: number;
  para: number;
  offset: number;
}

/** Where a DOM point is in the text of the shapes (null outside them). */
function deckPoint(node: Node | null, offset: number): DeckPoint | null {
  if (!node) return null;
  const el = node instanceof HTMLElement ? node : node.parentElement;
  const para = el?.closest<HTMLElement>('[data-kiln-para]');
  const shape = el?.closest<HTMLElement>('[data-kiln-shape]');
  if (!para || !shape) return null;
  return {
    shape: Number(shape.dataset.kilnShape),
    para: Number(para.dataset.kilnPara),
    offset: offsetIn(para, node, offset),
  };
}

function DeckText({ tree, exec, refetch }: { tree: A11yTree; exec: Exec; refetch: () => void }) {
  const byId = useMemo(() => new Map(tree.nodes.map((n) => [n.id, n])), [tree]);
  const root = tree.nodes[0];
  const bodyRef = useRef<HTMLDivElement>(null);
  /** The selection last sent to the core. */
  const sent = useRef('');

  /** Puts the core's text selection where the DOM has it (one command, the core's offsets). */
  const sync = useCallback(() => {
    const sel = window.getSelection();
    if (!sel || !bodyRef.current?.contains(sel.anchorNode)) return null;
    const a = deckPoint(sel.anchorNode, sel.anchorOffset);
    const h = deckPoint(sel.focusNode, sel.focusOffset);
    if (!a || !h || a.shape !== h.shape) return null;
    const key = `${a.shape}:${a.para}:${a.offset}:${h.para}:${h.offset}`;
    if (key !== sent.current) {
      sent.current = key;
      void exec({
        Deck: {
          SelectText: {
            shape: a.shape,
            anchor_para: a.para,
            anchor_offset: a.offset,
            head_para: h.para,
            head_offset: h.offset,
          },
        },
      });
    }
    return h;
  }, [exec]);

  // The selection follows into the core as it changes, so shortcuts and the ribbon act on it.
  useEffect(() => {
    document.addEventListener('selectionchange', sync);
    return () => document.removeEventListener('selectionchange', sync);
  }, [sync]);

  useEffect(() => {
    const el = bodyRef.current;
    if (!el) return;
    const onInput = (ev: Event) => {
      const e = ev as InputEvent;
      e.preventDefault();
      if (!sync()) return;
      let command: Command | null = null;
      switch (e.inputType) {
        case 'insertText':
        case 'insertReplacementText':
          if (e.data) command = { Deck: { InsertText: { text: e.data } } };
          break;
        case 'insertParagraph':
          command = { Deck: 'NewParagraph' };
          break;
        case 'insertLineBreak':
          command = { Deck: 'LineBreak' };
          break;
        case 'deleteContentBackward':
        case 'deleteWordBackward':
          command = { Deck: { DeleteText: { forward: false, word: e.inputType === 'deleteWordBackward' } } };
          break;
        case 'deleteContentForward':
        case 'deleteWordForward':
          command = { Deck: { DeleteText: { forward: true, word: e.inputType === 'deleteWordForward' } } };
          break;
        default:
          break;
      }
      // After the edit the core's caret is the truth (the tree brings it back).
      if (command) void exec(command).then(refetch);
    };
    const onKey = () => sync();
    el.addEventListener('beforeinput', onInput);
    el.addEventListener('keydown', onKey, true);
    return () => {
      el.removeEventListener('beforeinput', onInput);
      el.removeEventListener('keydown', onKey, true);
    };
  }, [exec, refetch, sync]);

  // After a change: the DOM caret where the core has it (the shapes keep their elements, so the
  // keys stay in the shape).
  useLayoutEffect(() => {
    const el = bodyRef.current;
    const caret = tree.caret;
    if (!el || !caret || !el.contains(document.activeElement)) return;
    const p = el.querySelector<HTMLElement>(`[data-kiln-node="${caret.block}"]`);
    const sel = window.getSelection();
    if (!p || !sel || !sel.isCollapsed) return;
    const now = deckPoint(sel.focusNode, sel.focusOffset);
    const want = { para: Number(p.dataset.kilnPara), offset: caret.offset };
    if (now && now.para === want.para && now.offset === want.offset) return;
    const point = pointIn(p, caret.offset);
    sel.collapse(point.node, point.offset);
    const shape = p.closest<HTMLElement>('[data-kiln-shape]');
    sent.current = `${shape?.dataset.kilnShape}:${want.para}:${want.offset}:${want.para}:${want.offset}`;
  }, [tree]);

  if (!root) return null;
  return (
    <div ref={bodyRef} className="kiln-textview__slides" data-kiln-text-view-focus="" tabIndex={-1}>
      {kids(root, byId).map((slide) => (
        <section key={slide.id} className="kiln-textview__slide" aria-label={slideName(slide, root.name)}>
          <h2 className="kiln-textview__slide-title">{slideName(slide, root.name)}</h2>
          {kids(slide, byId).map((sh) => (
            <DeckShape key={sh.id} node={sh} byId={byId} />
          ))}
          {slide.text ? (
            <section className="kiln-textview__notes" aria-label={t('Notes')}>
              {slide.text.split('\n').map((line, i) => (
                <p key={`${slide.id}:${i}`}>{line}</p>
              ))}
            </section>
          ) : null}
        </section>
      ))}
    </div>
  );
}

/** "Slide 2 of 5: Title" (the tree's name starts with the presentation's). */
function slideName(slide: A11yNode, title: string | null): string {
  let n = slide.name ?? 'Slide';
  if (title && n.startsWith(`${title}, `)) n = n.slice(title.length + 2);
  return n.charAt(0).toUpperCase() + n.slice(1);
}

function DeckShape({ node, byId }: { node: A11yNode; byId: ById }): ReactNode {
  switch (node.role) {
    case 'TextBox': {
      const paras = kids(node, byId);
      return (
        // biome-ignore lint/a11y/useSemanticElements: a rich text box (its paragraphs stay paragraphs), as the slide's shape is
        <div
          className="kiln-textview__shape"
          data-kiln-shape={node.id}
          role="textbox"
          aria-multiline="true"
          aria-label={node.name ?? t('Text')}
          tabIndex={0}
          contentEditable
          suppressContentEditableWarning
          spellCheck={false}
        >
          {paras.map((p, i) => (
            <p key={p.id} data-kiln-para={i} data-kiln-node={p.id}>
              <Inline node={p} />
            </p>
          ))}
        </div>
      );
    }
    case 'Table': {
      const rows = (node.text ?? '').split('\n').map((r) => r.split('\t'));
      return (
        <table className="kiln-textview__table" aria-label={node.name ?? t('Table')}>
          <tbody>
            {rows.map((cells, r) => (
              <tr key={r}>
                {cells.map((c, k) => (
                  <td key={k}>{c}</td>
                ))}
              </tr>
            ))}
          </tbody>
        </table>
      );
    }
    case 'Group':
      return (
        <section className="kiln-textview__box" aria-label={node.name ?? t('Group')}>
          {kids(node, byId).map((c) => (
            <DeckShape key={`${c.id}:${c.text ?? ''}`} node={c} byId={byId} />
          ))}
        </section>
      );
    default:
      return <span role="img" className="kiln-textview__picture" aria-label={node.name ?? t('Picture')} />;
  }
}
