import {
  type CaretMotion,
  type Command,
  type DocSummary,
  type KilnClient,
  KilnError,
  type ParagraphText,
  type Selection,
  type StatusBar,
} from '@kiln/protocol';
import { type KeyboardEvent, useCallback, useEffect, useRef, useState } from 'react';
import { useClient } from '../../client/useClient';
import { BridgeError, bridgeName, DevShell } from './DevShell';

/** Keys that move the caret, with their Ctrl variants. */
function motionFor(key: string, ctrl: boolean): CaretMotion | null {
  switch (key) {
    case 'ArrowLeft':
      return ctrl ? 'WordBackward' : 'Backward';
    case 'ArrowRight':
      return ctrl ? 'WordForward' : 'Forward';
    case 'ArrowUp':
      return 'LineUp';
    case 'ArrowDown':
      return 'LineDown';
    case 'Home':
      return ctrl ? 'DocumentStart' : 'LineStart';
    case 'End':
      return ctrl ? 'DocumentEnd' : 'LineEnd';
    default:
      return null;
  }
}

/**
 * Maps a key press to a command. Presentation only: which key means what (the keymap); what the
 * command does is decided by the core.
 */
export function commandForKey(
  e: Pick<KeyboardEvent, 'key' | 'ctrlKey' | 'metaKey' | 'shiftKey' | 'altKey'>,
): Command | null {
  const ctrl = e.ctrlKey || e.metaKey;
  if (ctrl && !e.altKey) {
    const k = e.key.toLowerCase();
    if (k === 'z' && !e.shiftKey) return { Suite: 'Undo' };
    if (k === 'y' || (k === 'z' && e.shiftKey)) return { Suite: 'Redo' };
  }
  const motion = motionFor(e.key, ctrl);
  if (motion) return { Write: { MoveCaret: { motion, extend: e.shiftKey } } };
  if (e.key === 'Backspace') return { Write: 'DeleteBackward' };
  if (e.key === 'Enter') return { Write: { InsertText: { text: '\n' } } };
  if (e.key === 'Tab') return { Write: { InsertText: { text: '\t' } } };
  // Printable characters arrive as `beforeinput` (see commandForInput), which also covers dead
  // keys, emoji pickers and text inserted by assistive technology.
  return null;
}

/** Maps a `beforeinput` event to a command (text insertion and deletion). */
export function commandForInput(inputType: string, data: string | null): Command | null {
  switch (inputType) {
    case 'insertText':
    case 'insertReplacementText':
      return data ? { Write: { InsertText: { text: data } } } : null;
    case 'insertParagraph':
    case 'insertLineBreak':
      return { Write: { InsertText: { text: '\n' } } };
    case 'deleteContentBackward':
      return { Write: 'DeleteBackward' };
    default:
      return null;
  }
}

/** Selected `[from, to)` char range of a paragraph, and the caret offset if it is there. */
function marks(paras: ParagraphText[], index: number, selection: Selection | null) {
  const kind = selection?.kind;
  if (!kind || typeof kind !== 'object' || !('Text' in kind)) return { range: null, caret: null };
  const { anchor, head } = kind.Text;
  const pos = (p: { block: number; offset: number }) => ({
    i: paras.findIndex((x) => x.id === p.block),
    o: p.offset,
  });
  const a = pos(anchor);
  const h = pos(head);
  const caret = h.i === index ? h.o : null;
  const [s, e] = a.i < h.i || (a.i === h.i && a.o <= h.o) ? [a, h] : [h, a];
  if (s.i === e.i && s.o === e.o) return { range: null, caret };
  if (index < s.i || index > e.i) return { range: null, caret };
  const len = [...(paras[index]?.text ?? '')].length;
  return { range: [index === s.i ? s.o : 0, index === e.i ? e.o : len] as const, caret };
}

function Paragraph({
  para,
  index,
  all,
  selection,
  focused,
}: {
  para: ParagraphText;
  index: number;
  all: ParagraphText[];
  selection: Selection | null;
  focused: boolean;
}) {
  const chars = [...para.text];
  const { range, caret } = marks(all, index, selection);
  const cut = (from: number, to: number) => chars.slice(from, to).join('');
  const parts: { text: string; selected: boolean }[] = range
    ? [
        { text: cut(0, range[0]), selected: false },
        { text: cut(range[0], range[1]), selected: true },
        { text: cut(range[1], chars.length), selected: false },
      ]
    : [{ text: para.text, selected: false }];
  // Split the part containing the caret so the caret bar sits between two characters.
  let offset = 0;
  const nodes = parts.flatMap((part, i) => {
    const len = [...part.text].length;
    const out = [];
    if (
      caret !== null &&
      caret >= offset &&
      caret <= offset + len &&
      (caret < offset + len || i === parts.length - 1 || len > 0)
    ) {
      const at = caret - offset;
      const before = [...part.text].slice(0, at).join('');
      const after = [...part.text].slice(at).join('');
      out.push(
        <span key={`${i}a`} className={part.selected ? 'bg-(--kiln-accent-write)/25' : undefined}>
          {before}
        </span>,
        <span
          key={`${i}c`}
          aria-hidden="true"
          data-testid="scratch-caret"
          className={`relative -mx-px inline-block h-[1.15em] w-0.5 translate-y-[0.18em] rounded-full bg-(--kiln-accent-write) ${focused ? 'animate-pulse' : 'opacity-40'}`}
        />,
        <span key={`${i}b`} className={part.selected ? 'bg-(--kiln-accent-write)/25' : undefined}>
          {after}
        </span>,
      );
      offset = Number.POSITIVE_INFINITY; // caret placed
    } else {
      out.push(
        <span key={i} className={part.selected ? 'bg-(--kiln-accent-write)/25' : undefined}>
          {part.text}
        </span>,
      );
      offset += len;
    }
    return out;
  });
  return (
    <p
      className="m-0 min-h-[1.6em] whitespace-pre-wrap break-words text-[15px] leading-[1.6]"
      data-testid="scratch-paragraph"
      data-text={para.text}
    >
      {nodes}
    </p>
  );
}

interface DocState {
  summary: DocSummary;
  paragraphs: ParagraphText[];
  selection: Selection | null;
  status: StatusBar | null;
}

async function load(client: KilnClient, summary: DocSummary): Promise<DocState> {
  const [paras, sel, status] = await Promise.all([
    client.query(summary.doc, 'Paragraphs'),
    client.query(summary.doc, 'Selection'),
    client.query(summary.doc, 'Status'),
  ]);
  return {
    summary,
    paragraphs: typeof paras === 'object' && 'Paragraphs' in paras ? paras.Paragraphs : [],
    selection: typeof sel === 'object' && 'Selection' in sel ? sel.Selection : null,
    status: typeof status === 'object' && 'Status' in status ? status.Status : null,
  };
}

/**
 * A temporary editor over a scratch document (plain paragraphs, no layout), rendered as plain DOM
 * until the canvas surface arrives (WP-54). Every key press becomes a command; the paragraphs
 * shown are always the core's.
 */
export function ScratchRoute() {
  const state = useClient();
  const client = state.status === 'ready' ? state.client : null;
  const [doc, setDoc] = useState<DocState | null>(null);
  const [error, setError] = useState<string | null>(null);
  const [focused, setFocused] = useState(false);
  const editor = useRef<HTMLDivElement>(null);
  const created = useRef(false);

  useEffect(() => {
    if (!client || created.current) return;
    created.current = true;
    let live = true;
    const off = client.subscribe((event) => {
      if (typeof event !== 'object' || !('Changed' in event)) return;
      const { summary, selection, status } = event.Changed;
      setDoc((d) => (d && d.summary.doc === summary.doc ? { ...d, summary, selection, status } : d));
      if (live) {
        client.query(summary.doc, 'Paragraphs').then(
          (r) => {
            if (live && typeof r === 'object' && 'Paragraphs' in r) {
              setDoc((d) => (d && d.summary.doc === summary.doc ? { ...d, paragraphs: r.Paragraphs } : d));
            }
          },
          (e: unknown) => setError(KilnError.from(e).message),
        );
      }
    });
    client
      .new('Scratch')
      .then((summary) => load(client, summary))
      .then((d) => live && setDoc(d))
      .catch((e: unknown) => setError(KilnError.from(e).message));
    return () => {
      live = false;
      off();
    };
  }, [client]);

  const run = useCallback(
    (command: Command) => {
      if (!client || !doc) return;
      client.execute(doc.summary.doc, command).catch((e: unknown) => {
        const err = KilnError.from(e);
        // "Nothing to undo" is not worth a message; anything else is.
        if (err.kind !== 'Unavailable') setError(err.message);
      });
    },
    [client, doc],
  );

  // `beforeinput` is not a React event; listen natively and keep the DOM untouched.
  useEffect(() => {
    const el = editor.current;
    if (!el) return;
    const onBeforeInput = (e: InputEvent) => {
      if (e.isComposing) return; // IME composition arrives with the input proxy (WP-54)
      e.preventDefault();
      const command = commandForInput(e.inputType, e.data);
      if (command) run(command);
    };
    el.addEventListener('beforeinput', onBeforeInput);
    return () => el.removeEventListener('beforeinput', onBeforeInput);
  }, [run]);

  const onKeyDown = (e: KeyboardEvent<HTMLDivElement>) => {
    if (e.nativeEvent.isComposing) return;
    const command = commandForKey(e);
    if (!command) return;
    e.preventDefault();
    run(command);
  };

  const s = doc?.summary;
  const words = doc?.status?.words ?? 0;
  const chars = doc?.status?.chars ?? 0;
  return (
    <DevShell bridge={client ? bridgeName(client.bridge) : null}>
      {state.status === 'error' && <BridgeError message={state.error.message} />}
      {error && <BridgeError message={error} />}
      <div className="flex flex-wrap items-end justify-between gap-3">
        <div className="flex flex-col gap-1">
          <p className="m-0 text-[12px] font-semibold uppercase tracking-wide text-muted">Scratch document</p>
          <h1
            className="m-0 flex items-center gap-2 text-[24px] font-semibold tracking-tight"
            data-testid="scratch-title"
          >
            {s?.title ?? 'Opening…'}
            {s?.dirty && (
              <span
                className="size-2 rounded-full bg-(--kiln-accent-write)"
                role="img"
                aria-label="Unsaved changes"
              />
            )}
          </h1>
        </div>
        <div className="flex items-center gap-2">
          <button
            type="button"
            onClick={() => run({ Suite: 'Undo' })}
            disabled={!s?.can_undo}
            className="rounded-md border border-line bg-surface px-3 py-1.5 text-[13px] font-medium shadow-(--kiln-shadow-1) hover:bg-surface-2 disabled:opacity-40"
          >
            Undo
          </button>
          <button
            type="button"
            onClick={() => run({ Suite: 'Redo' })}
            disabled={!s?.can_redo}
            className="rounded-md border border-line bg-surface px-3 py-1.5 text-[13px] font-medium shadow-(--kiln-shadow-1) hover:bg-surface-2 disabled:opacity-40"
          >
            Redo
          </button>
        </div>
      </div>
      {/* biome-ignore lint/a11y/useSemanticElements: a custom editing surface; the core owns the text, so a native textarea's own editing must not run */}
      <div
        ref={editor}
        role="textbox"
        aria-multiline="true"
        aria-label="Scratch document"
        tabIndex={0}
        contentEditable
        suppressContentEditableWarning
        spellCheck={false}
        data-testid="scratch-editor"
        data-ready={doc ? 'true' : 'false'}
        onKeyDown={onKeyDown}
        onFocus={() => setFocused(true)}
        onBlur={() => setFocused(false)}
        onPaste={(e) => {
          e.preventDefault();
          const text = e.clipboardData.getData('text/plain');
          if (text) run({ Write: { InsertText: { text } } });
        }}
        className="min-h-64 cursor-text rounded-md [caret-color:transparent] border border-line bg-surface px-8 py-7 shadow-(--kiln-shadow-2) outline-none ring-(--kiln-accent-write)/40 focus-visible:ring-2"
      >
        {doc?.paragraphs.map((p, i) => (
          <Paragraph
            key={p.id}
            para={p}
            index={i}
            all={doc.paragraphs}
            selection={doc.selection}
            focused={focused}
          />
        ))}
      </div>
      <footer className="flex flex-wrap items-center gap-x-4 gap-y-1 text-[12px] text-muted [font-feature-settings:'tnum']">
        <span data-testid="scratch-words">
          {words} {words === 1 ? 'word' : 'words'}
        </span>
        <span>
          {chars} {chars === 1 ? 'character' : 'characters'}
        </span>
        <span data-testid="scratch-revision">Revision {s?.revision ?? 0}</span>
        <span className="ml-auto hidden sm:inline">
          Typing goes through the core: every key is a command.
        </span>
      </footer>
    </DevShell>
  );
}
