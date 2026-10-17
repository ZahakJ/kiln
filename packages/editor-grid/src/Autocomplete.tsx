/**
 * Formula autocomplete with inline docs (★) and the argument tooltip (PLAN.md §2.3): the core's
 * suggestions for what is being typed (functions with their one-line summary and signature,
 * defined names and tables), the highlighted one documented beside the list, the pick list of
 * Alt+Down, and the hint for the function call around the caret with the current argument in bold.
 * Placed under the cell being edited (or under the formula bar when it is the one editing).
 */
import type { GridCompletion, GridEditor, GridFormulaHint } from '@kiln/protocol';
import { type CSSProperties, Fragment, useLayoutEffect, useRef, useState } from 'react';
import { useGrid } from './hooks.ts';
import type { GridEditorModel } from './model.ts';

/** The signature with the argument at the caret marked. */
export function hintParts(h: GridFormulaHint): { text: string; current: boolean }[] {
  const out: { text: string; current: boolean }[] = [{ text: `${h.function}(`, current: false }];
  h.args.forEach((a, i) => {
    if (i > 0) out.push({ text: ', ', current: false });
    out.push({ text: a.optional ? `[${a.name}]` : a.name, current: i === h.current });
  });
  out.push({ text: ')', current: false });
  return out;
}

function Item({
  c,
  index,
  selected,
  onPick,
}: {
  c: GridCompletion;
  index: number;
  selected: boolean;
  onPick: (i: number) => void;
}) {
  return (
    <div
      role="option"
      tabIndex={-1}
      aria-selected={selected}
      className={`kiln-autocomplete__item${selected ? ' kiln-autocomplete__item--selected' : ''}`}
      onMouseDown={(e) => {
        // Keep the keyboard in the cell.
        e.preventDefault();
        onPick(index);
      }}
    >
      <span
        className={`kiln-autocomplete__kind kiln-autocomplete__kind--${c.kind.toLowerCase()}`}
        aria-hidden="true"
      >
        {c.kind === 'Function' ? 'fx' : c.kind === 'Name' ? 'N' : '≡'}
      </span>
      <span className="kiln-autocomplete__label">{c.label}</span>
    </div>
  );
}

export function Autocomplete({
  model,
  offset,
}: {
  model: GridEditorModel;
  offset?: { x: number; y: number };
}) {
  const ed = useGrid(model, (s) => s.grid?.editor ?? null);
  if (!ed) return null;
  return <AutocompleteFor model={model} ed={ed} offset={offset} />;
}

function AutocompleteFor({
  model,
  ed,
  offset,
}: {
  model: GridEditorModel;
  ed: GridEditor;
  offset?: { x: number; y: number };
}) {
  const list = ed.pick_list ?? null;
  const items = ed.completions;
  const hint = ed.hint;
  const box = useRef<HTMLDivElement>(null);
  // Under the cell; above it when the sheet has no room below (a cell near the bottom), and
  // pulled left when it would leave the sheet on the right.
  const below = Math.round(ed.rect.y + ed.rect.height + 4 + (offset?.y ?? 0));
  const left0 = Math.round(ed.rect.x + (offset?.x ?? 0));
  const [place, setPlace] = useState<{ top: number; left: number } | null>(null);
  const shown = !!list || items.length > 0 || !!hint;
  const key = `${below}:${left0}:${items.length}:${list?.length ?? 0}:${ed.completion_index}:${hint?.current ?? -1}`;
  useLayoutEffect(() => {
    const el = box.current;
    const host = el?.offsetParent as HTMLElement | null | undefined;
    if (!el || !host || ed.formula_bar) {
      setPlace(null);
      return;
    }
    const h = el.offsetHeight;
    const w = el.offsetWidth;
    const room = host.clientHeight;
    const above = Math.round(ed.rect.y + (offset?.y ?? 0)) - 4 - h;
    const top = below + h <= room || above < 0 ? below : above;
    const left = Math.max(0, Math.min(left0, host.clientWidth - w - 4));
    setPlace((p) => (p && p.top === top && p.left === left ? p : { top, left }));
    // `key` sums up what changes the box's size and anchor.
  }, [key, ed.formula_bar]);
  if (!shown) return null;
  const style: CSSProperties = ed.formula_bar
    ? { left: 8, top: 4 }
    : { left: place?.left ?? left0, top: place?.top ?? below };
  const pick = (index: number) => void model.core('grid.editing.accept_completion', { index });
  const selected = items[ed.completion_index] ?? null;
  return (
    <div ref={box} className="kiln-autocomplete" style={style} data-kiln-autocomplete="">
      {hint && items.length === 0 && !list ? (
        <div className="kiln-autocomplete__hint" role="tooltip">
          <div className="kiln-autocomplete__syntax">
            {hintParts(hint).map((p, i) =>
              p.current ? <b key={i}>{p.text}</b> : <Fragment key={i}>{p.text}</Fragment>,
            )}
          </div>
          {hint.args[hint.current] ? (
            <div className="kiln-autocomplete__arg">
              <b>{hint.args[hint.current]?.name}</b> — {hint.args[hint.current]?.description}
            </div>
          ) : null}
          <div className="kiln-autocomplete__summary">{hint.summary}</div>
        </div>
      ) : null}
      {list ? (
        <div className="kiln-autocomplete__list" role="listbox" aria-label="Pick from list">
          {list.map((v, i) => (
            <Item
              key={i}
              c={{ label: v, kind: 'Value', detail: null, syntax: null }}
              index={i}
              selected={i === ed.completion_index}
              onPick={pick}
            />
          ))}
        </div>
      ) : null}
      {!list && items.length > 0 ? (
        <div className="kiln-autocomplete__panel">
          <div className="kiln-autocomplete__list" role="listbox" aria-label="Suggestions">
            {items.map((c, i) => (
              <Item
                key={`${c.kind}:${c.label}`}
                c={c}
                index={i}
                selected={i === ed.completion_index}
                onPick={pick}
              />
            ))}
          </div>
          {selected?.detail ? (
            <div className="kiln-autocomplete__doc" aria-live="polite">
              {selected.syntax ? <div className="kiln-autocomplete__docsyntax">{selected.syntax}</div> : null}
              <div>{selected.detail}</div>
              <div className="kiln-autocomplete__tip">Tab to insert</div>
            </div>
          ) : null}
        </div>
      ) : null}
    </div>
  );
}
