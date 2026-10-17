/**
 * Basic find (Ctrl+F): a bar over the sheet that selects the next cell whose value (or formula)
 * contains the text, through the core's `grid.editing.find_next`. Enter finds the next match,
 * Shift+Enter the previous one, Esc closes the bar and returns to the sheet.
 */
import { Icon, IconButton } from '@kiln/ui-kit';
import { ChevronDown, ChevronUp, X } from 'lucide-react';
import { useEffect, useRef } from 'react';
import { useGrid } from './hooks.ts';
import type { GridEditorModel } from './model.ts';

export function FindBar({ model }: { model: GridEditorModel }) {
  const find = useGrid(model, (s) => s.find);
  const input = useRef<HTMLInputElement>(null);
  useEffect(() => {
    if (find.open) {
      input.current?.focus();
      input.current?.select();
    }
  }, [find.open]);
  if (!find.open) return null;
  const patch = (p: Partial<typeof find>) => model.set({ find: { ...model.state.find, ...p } });
  const next = (backward: boolean) => {
    const f = model.state.find;
    if (!f.text) return;
    void model.run('grid.editing.find_next', {
      text: f.text,
      match_case: f.matchCase,
      whole_cell: f.wholeCell,
      in_formulas: f.inFormulas,
      backward,
    });
  };
  const close = () => {
    patch({ open: false });
    model.surface?.focus();
  };
  return (
    // A non-modal dialog, as Excel's Find is: Esc closes it and returns to the sheet.
    <div role="dialog" aria-modal="false" className="kiln-findbar" aria-label="Find" data-testid="grid-find">
      <input
        ref={input}
        className="kiln-findbar__input"
        aria-label="Find what"
        placeholder="Find in sheet"
        value={find.text}
        spellCheck={false}
        onChange={(e) => patch({ text: e.target.value })}
        onKeyDown={(e) => {
          e.stopPropagation();
          if (e.key === 'Enter') {
            e.preventDefault();
            next(e.shiftKey);
          } else if (e.key === 'Escape') {
            e.preventDefault();
            close();
          }
        }}
      />
      <label className="kiln-findbar__opt">
        <input
          type="checkbox"
          checked={find.matchCase}
          onChange={(e) => patch({ matchCase: e.target.checked })}
        />
        Match case
      </label>
      <label className="kiln-findbar__opt">
        <input
          type="checkbox"
          checked={find.wholeCell}
          onChange={(e) => patch({ wholeCell: e.target.checked })}
        />
        Entire cell
      </label>
      <label className="kiln-findbar__opt">
        <input
          type="checkbox"
          checked={find.inFormulas}
          onChange={(e) => patch({ inFormulas: e.target.checked })}
        />
        Formulas
      </label>
      <IconButton
        label="Find Previous"
        icon={<Icon icon={ChevronUp} size={14} />}
        size="sm"
        variant="ghost"
        onClick={() => next(true)}
      />
      <IconButton
        label="Find Next"
        icon={<Icon icon={ChevronDown} size={14} />}
        size="sm"
        variant="ghost"
        onClick={() => next(false)}
      />
      <IconButton
        label="Close Find"
        icon={<Icon icon={X} size={14} />}
        size="sm"
        variant="ghost"
        onClick={close}
      />
    </div>
  );
}
