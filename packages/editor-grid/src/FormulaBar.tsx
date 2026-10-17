/**
 * The formula bar (PLAN.md §1.4 "Below ribbon"; Grid mockup): the name box (with the workbook's
 * names), Cancel and Enter, the fx button (Insert Function) and the formula box. The formula box is
 * a real text field over a mirror that colours the references in the colours the core gives them
 * (the same colours as the ranges outlined on the sheet). Typing in it edits the active cell
 * (Edit mode); Enter, Tab and Esc work as in the cell.
 */
import type { GridRefSpan, GridState } from '@kiln/protocol';
import { Icon, IconButton, Menu, MenuContent, MenuItem, MenuTrigger } from '@kiln/ui-kit';
import { Check, ChevronDown, ChevronUp, X } from 'lucide-react';
import { type KeyboardEvent, type ReactNode, useEffect, useRef, useState } from 'react';
import { useGrid } from './hooks.ts';
import type { GridEditorModel } from './model.ts';

/** The formula text split into plain runs and coloured references (UTF-16 offsets). */
export function colourRuns(
  text: string,
  refs: readonly GridRefSpan[],
): { text: string; color: string | null }[] {
  const out: { text: string; color: string | null }[] = [];
  let at = 0;
  for (const r of [...refs].sort((a, b) => a.start - b.start)) {
    if (r.start < at || r.end > text.length) continue;
    if (r.start > at) out.push({ text: text.slice(at, r.start), color: null });
    out.push({ text: text.slice(r.start, r.end), color: r.color });
    at = r.end;
  }
  if (at < text.length) out.push({ text: text.slice(at), color: null });
  return out;
}

function NameBox({ model, state }: { model: GridEditorModel; state: GridState | null }) {
  const [value, setValue] = useState(state?.name_box ?? '');
  const [editing, setEditing] = useState(false);
  useEffect(() => {
    if (!editing) setValue(state?.name_box ?? '');
  }, [state?.name_box, editing]);
  const go = async (reference: string) => {
    setEditing(false);
    // A name that does not exist yet names the selection (Excel's name box).
    if (reference.trim()) await model.run('grid.nav.go_to', { reference, define: true });
    model.surface?.focus();
  };
  return (
    <div className="kiln-namebox">
      <input
        className="kiln-namebox__input"
        aria-label="Name Box"
        value={value}
        spellCheck={false}
        onFocus={(e) => {
          setEditing(true);
          e.currentTarget.select();
        }}
        onBlur={() => setEditing(false)}
        onChange={(e) => setValue(e.target.value)}
        onKeyDown={(e) => {
          if (e.key === 'Enter') {
            e.preventDefault();
            void go(value);
          } else if (e.key === 'Escape') {
            e.preventDefault();
            setEditing(false);
            setValue(state?.name_box ?? '');
            model.surface?.focus();
          }
          e.stopPropagation();
        }}
      />
      <Menu>
        <MenuTrigger asChild>
          <button type="button" className="kiln-namebox__menu" aria-label="Defined names">
            <Icon icon={ChevronDown} size={14} />
          </button>
        </MenuTrigger>
        <MenuContent align="start">
          {(state?.names.length ?? 0) === 0 ? (
            <MenuItem disabled>No defined names</MenuItem>
          ) : (
            state?.names.map((n) => (
              <MenuItem key={n} onSelect={() => void go(n)}>
                {n}
              </MenuItem>
            ))
          )}
        </MenuContent>
      </Menu>
    </div>
  );
}

/** The formula box: a text field over the coloured mirror. */
function FormulaBox({
  model,
  state,
  expanded,
}: {
  model: GridEditorModel;
  state: GridState | null;
  expanded: boolean;
}) {
  const area = useRef<HTMLTextAreaElement>(null);
  const mirror = useRef<HTMLDivElement>(null);
  const ed = state?.editor ?? null;
  const own = !!ed?.formula_bar;
  const shown = ed ? ed.text : (state?.formula ?? '');
  const [text, setText] = useState(shown);
  const sent = useRef(shown);
  // The core's text wins unless it is what this box just sent (typing stays smooth).
  useEffect(() => {
    if (shown !== sent.current || !own) {
      setText(shown);
      sent.current = shown;
      if (own && area.current && ed && document.activeElement === area.current) {
        const el = area.current;
        requestAnimationFrame(() => el.setSelectionRange(ed.start, ed.end));
      }
    }
  }, [shown, own, ed]);
  const send = (value: string, el: HTMLTextAreaElement) => {
    sent.current = value;
    setText(value);
    void model.core('grid.editing.set_text', {
      text: value,
      start: el.selectionStart ?? value.length,
      end: el.selectionEnd ?? value.length,
      formula_bar: true,
    });
  };
  const onKeyDown = (e: KeyboardEvent<HTMLTextAreaElement>) => {
    e.stopPropagation();
    const editing = state?.mode !== 'Ready';
    if (e.key === 'Enter' && !e.altKey) {
      e.preventDefault();
      if (e.ctrlKey || e.metaKey) void model.core('grid.editing.fill_selection');
      else void model.core('grid.editing.commit', { advance: e.shiftKey ? 'Up' : 'Down' });
      model.surface?.focus();
    } else if (e.key === 'Tab') {
      e.preventDefault();
      void model.core('grid.editing.commit', { advance: e.shiftKey ? 'Left' : 'Right' });
      model.surface?.focus();
    } else if (e.key === 'Escape') {
      e.preventDefault();
      void model.core('grid.editing.cancel_edit');
      setText(state?.formula ?? '');
      model.surface?.focus();
    } else if (e.key === 'F4' && editing) {
      e.preventDefault();
      void model.core('grid.editing.toggle_absolute');
    } else if (e.key === 'Enter' && e.altKey) {
      // Alt+Enter: a line break, as typed.
    }
  };
  const runs = colourRuns(text, ed?.refs ?? []);
  return (
    <div className={`kiln-formulabox${expanded ? ' kiln-formulabox--expanded' : ''}`}>
      <div ref={mirror} className="kiln-formulabox__mirror" aria-hidden="true">
        {runs.map((r, i) =>
          r.color ? (
            <span
              key={i}
              className="kiln-formulabox__ref"
              style={{ color: `#${r.color}`, background: `#${r.color}1f` }}
            >
              {r.text}
            </span>
          ) : (
            <span key={i}>{r.text}</span>
          ),
        )}
        {'​'}
      </div>
      <textarea
        ref={area}
        className="kiln-formulabox__input"
        aria-label="Formula Bar"
        spellCheck={false}
        value={text}
        rows={expanded ? 4 : 1}
        onFocus={(e) => {
          if (state?.mode === 'Ready') send(e.currentTarget.value, e.currentTarget);
        }}
        onChange={(e) => send(e.target.value, e.target)}
        onSelect={(e) => {
          const el = e.currentTarget;
          if (state?.mode !== 'Ready' && own)
            void model.core('grid.editing.set_text', {
              text: el.value,
              start: el.selectionStart ?? 0,
              end: el.selectionEnd ?? 0,
              formula_bar: true,
            });
        }}
        onScroll={(e) => {
          if (mirror.current) mirror.current.scrollTop = e.currentTarget.scrollTop;
        }}
        onKeyDown={onKeyDown}
      />
    </div>
  );
}

export function FormulaBar({ model, trailing }: { model: GridEditorModel; trailing?: ReactNode }) {
  const state = useGrid(model, (s) => s.grid);
  const expanded = useGrid(model, (s) => s.formulaBarExpanded);
  const editing = !!state && state.mode !== 'Ready';
  return (
    <div className="kiln-formulabar" role="toolbar" aria-label="Formula bar">
      <NameBox model={model} state={state} />
      <div className="kiln-formulabar__buttons">
        <IconButton
          label="Cancel"
          icon={<Icon icon={X} size={14} />}
          size="sm"
          variant="ghost"
          disabled={!editing}
          onMouseDown={(e) => e.preventDefault()}
          onClick={() => {
            void model.core('grid.editing.cancel_edit');
            model.surface?.focus();
          }}
        />
        <IconButton
          label="Enter"
          icon={<Icon icon={Check} size={14} />}
          size="sm"
          variant="ghost"
          disabled={!editing}
          onMouseDown={(e) => e.preventDefault()}
          onClick={() => {
            void model.core('grid.editing.commit', { advance: null });
            model.surface?.focus();
          }}
        />
      </div>
      <div className="kiln-formulabar__field">
        <button
          type="button"
          className="kiln-formulabar__fx"
          aria-label="fx Insert Function"
          title="Insert Function"
          onClick={() => model.openDialog('insertFunction')}
        >
          fx
        </button>
        <FormulaBox model={model} state={state} expanded={expanded} />
        {trailing}
        <button
          type="button"
          className="kiln-formulabar__expand"
          aria-label={expanded ? 'Collapse Formula Bar' : 'Expand Formula Bar'}
          aria-expanded={expanded}
          onClick={() => model.set({ formulaBarExpanded: !expanded })}
        >
          <Icon icon={expanded ? ChevronUp : ChevronDown} size={14} />
        </button>
      </div>
    </div>
  );
}
