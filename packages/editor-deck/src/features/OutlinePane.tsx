/**
 * Outline View (View ▸ Outline View): the text of every slide in the rail's place — each slide's
 * number and title, then its body lines indented by level — edited as one outline. Typing in a
 * line sends the line's text; Enter, Tab, Shift+Tab and Backspace at the start of a line are the
 * core's outline edits (a title split or promoted line makes a new slide, a demoted title joins
 * the slide before). Joining away a slide that holds more than its outline text (pictures, other
 * shapes, notes) asks first, as PowerPoint does. The core's outline replaces the pane's once every
 * edit sent is answered.
 * Clicking into a slide's lines shows that slide on the stage.
 */
import { t } from '@kiln/i18n';
import type { DeckOutlineEdit, DeckOutlineEntry } from '@kiln/protocol';
import { Button, Dialog, DialogContent, Icon } from '@kiln/ui-kit';
import { X } from 'lucide-react';
import { useCallback, useEffect, useLayoutEffect, useRef, useState } from 'react';
import { useDeck } from '../hooks.ts';
import type { DeckEditorModel } from '../model.ts';

/** Where the caret goes after an edit. */
export interface OutlineSpot {
  slide: number;
  line: number;
  offset: number;
}

/** The caret's offset in a line (characters from its start), or `null` when it is elsewhere. */
export function caretOffset(el: HTMLElement): number | null {
  const sel = el.ownerDocument.getSelection();
  if (!sel || sel.rangeCount === 0) return null;
  const r = sel.getRangeAt(0);
  if (!el.contains(r.startContainer)) return null;
  const pre = el.ownerDocument.createRange();
  pre.selectNodeContents(el);
  pre.setEnd(r.startContainer, r.startOffset);
  return [...pre.toString()].length;
}

/** Puts the caret `offset` characters into a line. */
export function placeCaret(el: HTMLElement, offset: number): void {
  el.focus();
  const doc = el.ownerDocument;
  const sel = doc.getSelection();
  if (!sel) return;
  const text = el.firstChild;
  const r = doc.createRange();
  if (text && text.nodeType === Node.TEXT_NODE) {
    const s = text.textContent ?? '';
    const chars = [...s];
    const k = Math.max(0, Math.min(offset, chars.length));
    r.setStart(text, chars.slice(0, k).join('').length);
  } else {
    r.selectNodeContents(el);
    r.collapse(true);
  }
  r.collapse(true);
  sel.removeAllRanges();
  sel.addRange(r);
}

/** Where the caret goes after an edit (the outline before it is `o`, the line's text `text`). */
export function spotAfter(
  o: readonly DeckOutlineEntry[],
  edit: DeckOutlineEdit,
  text: string,
): OutlineSpot | null {
  if ('Split' in edit) {
    const { slide, line } = edit.Split;
    // A body line's rest is the next line; a title's rest (or, split at its start, the title
    // itself) is the next slide's title.
    return line > 0 ? { slide, line: line + 1, offset: 0 } : { slide: slide + 1, line: 0, offset: 0 };
  }
  if ('Indent' in edit) {
    const { slide, line, by } = edit.Indent;
    const e = o[slide];
    const offset = [...text].length;
    if (line === 0) {
      if (by < 0 || slide === 0) return { slide, line, offset };
      const prev = o[slide - 1];
      return { slide: slide - 1, line: (prev?.lines.length ?? 0) + 1, offset };
    }
    if (by < 0 && (e?.lines[line - 1]?.level ?? 0) === 0) return { slide: slide + 1, line: 0, offset };
    return { slide, line, offset };
  }
  if ('Join' in edit) {
    const { slide, line } = edit.Join;
    const e = o[slide];
    if (line > 1) return { slide, line: line - 1, offset: [...(e?.lines[line - 2]?.text ?? '')].length };
    if (line === 1) return { slide, line: 0, offset: [...(e?.title ?? '')].length };
    if (slide === 0) return null;
    const prev = o[slide - 1];
    const n = prev?.lines.length ?? 0;
    const last = n > 0 ? (prev?.lines[n - 1]?.text ?? '') : (prev?.title ?? '');
    return { slide: slide - 1, line: n, offset: [...last].length };
  }
  return null;
}

/** The slide an edit joins into the one before (Tab on a title, Backspace at its start). */
export function slideJoinedAway(edit: DeckOutlineEdit): number | null {
  if ('Indent' in edit) {
    const { slide, line, by } = edit.Indent;
    return line === 0 && by > 0 && slide > 0 ? slide : null;
  }
  if ('Join' in edit) {
    const { slide, line } = edit.Join;
    return line === 0 && slide > 0 ? slide : null;
  }
  return null;
}

interface LineProps {
  text: string;
  slide: number;
  line: number;
  level: number;
  editable: boolean;
  /** Edits are in flight: the line keeps what is typed in it. */
  busy: () => boolean;
  onText: (text: string) => void;
  onEdit: (edit: DeckOutlineEdit, text: string) => void;
  onFocus: () => void;
  onMove: (el: HTMLElement, up: boolean) => void;
  onUndo: (redo: boolean) => void;
}

function Line({
  text,
  slide,
  line,
  level,
  editable,
  busy,
  onText,
  onEdit,
  onFocus,
  onMove,
  onUndo,
}: LineProps) {
  const ref = useRef<HTMLDivElement>(null);
  const composing = useRef(false);
  useLayoutEffect(() => {
    const el = ref.current;
    if (!el || el.textContent === text) return;
    const focused = el.ownerDocument.activeElement === el;
    if (focused && busy()) return;
    const off = focused ? caretOffset(el) : null;
    el.textContent = text;
    if (focused && off !== null) placeCaret(el, Math.min(off, [...text].length));
  }, [text, busy]);
  const keyDown = (e: React.KeyboardEvent<HTMLDivElement>) => {
    const el = e.currentTarget;
    const mod = e.ctrlKey || e.metaKey;
    e.stopPropagation();
    if (composing.current) return;
    if (mod && (e.key === 'z' || e.key === 'Z' || e.key === 'y' || e.key === 'Y')) {
      e.preventDefault();
      onUndo(e.key.toLowerCase() === 'y' || (e.key.toLowerCase() === 'z' && e.shiftKey));
      return;
    }
    const at = caretOffset(el) ?? 0;
    const sel = el.ownerDocument.getSelection();
    const collapsed = !sel || sel.isCollapsed;
    const now = el.textContent ?? '';
    if (e.key === 'Enter' && !e.shiftKey) {
      e.preventDefault();
      onEdit({ Split: { slide, line, offset: at } }, now);
    } else if (e.key === 'Tab') {
      e.preventDefault();
      onEdit({ Indent: { slide, line, by: e.shiftKey ? -1 : 1 } }, now);
    } else if (e.key === 'Backspace' && at === 0 && collapsed && (line > 0 || slide > 0)) {
      e.preventDefault();
      onEdit({ Join: { slide, line } }, now);
    } else if ((e.key === 'ArrowUp' || e.key === 'ArrowDown') && !e.shiftKey) {
      const range = sel && sel.rangeCount > 0 ? sel.getRangeAt(0) : null;
      const caret = range?.getClientRects()[0] ?? range?.getBoundingClientRect();
      const box = el.getBoundingClientRect();
      const lh = Number.parseFloat(getComputedStyle(el).lineHeight) || 18;
      const up = e.key === 'ArrowUp';
      const edge = !caret || (up ? caret.top - box.top < lh * 0.8 : box.bottom - caret.bottom < lh * 0.8);
      if (edge) {
        e.preventDefault();
        onMove(el, up);
      }
    } else if (e.key === 'Escape') {
      e.preventDefault();
      el.blur();
    }
  };
  return (
    // biome-ignore lint/a11y/useSemanticElements: a line wraps and is spell-checked in place, as PowerPoint's outline
    <div
      ref={ref}
      className="kiln-outline__line"
      data-slide={slide}
      data-line={line}
      data-level={line === 0 ? undefined : level}
      data-title={line === 0 || undefined}
      style={line === 0 ? undefined : { paddingInlineStart: 20 + level * 20 }}
      contentEditable={editable}
      suppressContentEditableWarning
      role="textbox"
      aria-label={
        line === 0
          ? t('Slide {value} title', { value: slide + 1 })
          : t('Slide {value}, line {line}', { value: slide + 1, line })
      }
      aria-multiline="false"
      spellCheck
      tabIndex={editable ? 0 : -1}
      onFocus={onFocus}
      onKeyDown={keyDown}
      onCompositionStart={() => {
        composing.current = true;
      }}
      onCompositionEnd={(e) => {
        composing.current = false;
        onText(e.currentTarget.textContent ?? '');
      }}
      onInput={(e) => {
        if (!composing.current) onText(e.currentTarget.textContent ?? '');
      }}
      onPaste={(e) => {
        // Plain text, one line (Enter splits lines; a pasted break becomes a space).
        e.preventDefault();
        const t = e.clipboardData.getData('text/plain').replace(/\s*[\r\n]+\s*/g, ' ');
        e.currentTarget.ownerDocument.execCommand('insertText', false, t);
      }}
    />
  );
}

export function OutlinePane({ model }: { model: DeckEditorModel }) {
  const key = useDeck(model, (s) =>
    s.deck ? s.deck.slides.map((x) => `${x.id}:${x.version}`).join(',') : '',
  );
  const current = useDeck(model, (s) => s.deck?.current ?? 0);
  const [outline, setOutline] = useState<DeckOutlineEntry[]>([]);
  const root = useRef<HTMLDivElement>(null);
  const inflight = useRef(0);
  const focusAfter = useRef<OutlineSpot | null>(null);
  /** What is typed while an edit across lines is answered: it goes where the caret lands. */
  const typed = useRef<string | null>(null);
  /** An edit that would delete a slide's pictures, shapes or notes, waiting for an answer. */
  const [ask, setAsk] = useState<{ edit: DeckOutlineEdit; text: string; slide: number } | null>(null);
  const outlineRef = useRef(outline);
  outlineRef.current = outline;
  const busy = useCallback(() => inflight.current > 0, []);

  const reload = useCallback(async () => {
    const r = await model.query('OutlineView');
    if (inflight.current > 0 || !r || !('OutlineView' in r)) return;
    setOutline(r.OutlineView);
  }, [model]);
  useEffect(() => {
    void reload();
  }, [key, reload]);
  // After an edit, the caret goes where the edit leaves it.
  useLayoutEffect(() => {
    const f = focusAfter.current;
    const el = root.current?.querySelector<HTMLElement>(`[data-slide="${f?.slide}"][data-line="${f?.line}"]`);
    if (f && el) {
      focusAfter.current = null;
      placeCaret(el, f.offset);
      const t = typed.current;
      typed.current = null;
      if (t) el.ownerDocument.execCommand('insertText', false, t);
    } else if (f) {
      // The edit did not make that line (it was refused): the keys are free again.
      focusAfter.current = null;
      typed.current = null;
    }
  }, [outline]);

  const send = useCallback(
    async (edit: DeckOutlineEdit) => {
      inflight.current += 1;
      await model.core('deck.outline.edit', { edit });
      inflight.current -= 1;
      if (inflight.current === 0) await reload();
    },
    [model, reload],
  );
  const select = (slide: number) => {
    if (slide !== model.state.deck?.current)
      void model.core('deck.slides.select', { index: slide, extend: false, toggle: false });
  };
  const move = (el: HTMLElement, up: boolean) => {
    const lines = [
      ...(root.current?.querySelectorAll<HTMLElement>('.kiln-outline__line[contenteditable="true"]') ?? []),
    ];
    const i = lines.indexOf(el);
    const next = lines[up ? i - 1 : i + 1];
    if (next) placeCaret(next, up ? [...(next.textContent ?? '')].length : 0);
  };
  const undo = async (redo: boolean) => {
    inflight.current += 1;
    await model.run(redo ? 'suite.edit.redo' : 'suite.edit.undo');
    inflight.current -= 1;
    await reload();
  };
  const edit = (e: DeckOutlineEdit, text: string) => {
    focusAfter.current = spotAfter(outlineRef.current, e, text);
    if (focusAfter.current) typed.current = '';
    void send(e);
  };
  const answer = (yes: boolean) => {
    const a = ask;
    setAsk(null);
    if (!a) return;
    if (yes) edit(a.edit, a.text);
    else {
      const el = root.current?.querySelector<HTMLElement>(`[data-slide="${a.slide}"][data-line="0"]`);
      if (el) placeCaret(el, 0);
    }
  };
  const close = () => {
    model.set({ outline: false });
    model.stage?.focus();
  };

  return (
    <section
      className="kiln-outline"
      aria-label={t('Outline')}
      // An F6 stop, as PowerPoint's outline pane is.
      data-kiln-region="pane"
      data-testid="deck-outline"
      data-kiln-shortcuts="off"
      ref={root}
      onKeyDownCapture={(e) => {
        // Until the line the edit makes exists, keys are kept for it.
        if (typed.current === null) return;
        e.preventDefault();
        e.stopPropagation();
        if (e.key.length === 1 && !e.ctrlKey && !e.metaKey && !e.altKey) typed.current += e.key;
      }}
    >
      <div className="kiln-outline__head">
        <h2>{t('Outline')}</h2>
        <button
          type="button"
          className="kiln-sidepane__close"
          aria-label={t('Close Outline View')}
          onClick={close}
        >
          <Icon icon={X} size={14} />
        </button>
      </div>
      <div className="kiln-outline__body">
        {outline.map((e, slide) => (
          <section
            key={e.slide}
            className="kiln-outline__slide"
            data-current={slide === current || undefined}
            data-hidden={e.hidden || undefined}
            aria-label={t('Slide {value}', { value: slide + 1 })}
          >
            <span className="kiln-outline__number" aria-hidden="true">
              {slide + 1}
            </span>
            <span className="kiln-outline__icon" aria-hidden="true" />
            <div className="kiln-outline__lines">
              {[{ level: 0, text: e.title }, ...e.lines].map((l, line) => (
                <Line
                  key={line}
                  text={l.text}
                  slide={slide}
                  line={line}
                  level={l.level}
                  editable={line > 0 || e.has_title}
                  busy={busy}
                  onFocus={() => select(slide)}
                  onText={(text) => void send({ Text: { slide, line, text } })}
                  onEdit={(e, text) => {
                    const gone = slideJoinedAway(e);
                    if (gone !== null && outlineRef.current[gone]?.more)
                      setAsk({ edit: e, text, slide: gone });
                    else edit(e, text);
                  }}
                  onMove={move}
                  onUndo={(redo) => void undo(redo)}
                />
              ))}
            </div>
          </section>
        ))}
      </div>
      {ask ? (
        <Dialog open onOpenChange={(o) => (o ? undefined : answer(false))}>
          <DialogContent
            title={t('Join Slides')}
            size="sm"
            data-testid="deck-outline-join"
            data-kiln-shortcuts="off"
            onCloseAutoFocus={(e) => e.preventDefault()}
            footer={
              <>
                <Button onClick={() => answer(false)}>{t('Cancel')}</Button>
                <Button variant="primary" autoFocus onClick={() => answer(true)}>
                  {t('Join')}
                </Button>
              </>
            }
          >
            <p role="alert">
              {t(
                'Slide {value} has pictures, shapes or speaker notes that are not in the outline. Joining it to the slide before deletes them.',
                { value: ask.slide + 1 },
              )}
            </p>
          </DialogContent>
        </Dialog>
      ) : null}
    </section>
  );
}
