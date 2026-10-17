/**
 * Kiln Grid's editing area (PLAN.md §1.3; the Grid mockup): the formula bar under the ribbon, the
 * sheet surface with the autocomplete popover and the find bar over it, the sheet tabs below, the
 * context menus and the dialogs. Every edit is a core command; this is presentation only.
 */
import { Icon } from '@kiln/ui-kit';
import { X } from 'lucide-react';
import { useCallback } from 'react';
import { Autocomplete } from './Autocomplete.tsx';
import type { GridSurfaceController } from './controller.ts';
import { FindBar } from './FindBar.tsx';
import { FormulaBar } from './FormulaBar.tsx';
import { GridDialogs } from './GridDialogs.tsx';
import { GridMenus } from './GridMenus.tsx';
import { GridSurface } from './GridSurface.tsx';
import { useGrid } from './hooks.ts';
import type { GridEditorModel } from './model.ts';
import { SheetTabs } from './SheetTabs.tsx';
import './grid.css';

export interface GridEditorProps {
  model: GridEditorModel;
  /** The workbook's title (the surface's accessible name). */
  title: string;
  /** The sheet colours follow the chrome's resolved theme. */
  theme: 'light' | 'dark';
  /** Blink the caret (screenshots turn it off). */
  blink?: boolean;
  onController?: (c: GridSurfaceController | null) => void;
}

export function GridEditor({ model, title, theme, blink, onController }: GridEditorProps) {
  const showFormulaBar = useGrid(model, (s) => s.showFormulaBar);
  const notice = useGrid(model, (s) => s.notice);
  const mode = useGrid(model, (s) => s.grid?.mode ?? 'Ready');
  const onKeyDownCapture = useCallback(
    (e: React.KeyboardEvent<HTMLDivElement>) => {
      // Shift+F10 and the Menu key open the cell menu at the active cell's corner.
      if (mode !== 'Ready') return;
      if ((e.shiftKey && e.key === 'F10') || e.key === 'ContextMenu') {
        e.preventDefault();
        e.stopPropagation();
        const r = e.currentTarget.getBoundingClientRect();
        const l = model.state.grid?.layout;
        model.set({
          menu: { x: r.left + (l?.header_w ?? 44) + 24, y: r.top + (l?.header_h ?? 24) + 16, kind: 'cell' },
        });
      }
    },
    [model, mode],
  );
  return (
    <div className="kiln-grid" data-testid="grid-editor" data-mode={mode}>
      {showFormulaBar ? <FormulaBar model={model} /> : null}
      <div className="kiln-grid__sheet" onKeyDownCapture={onKeyDownCapture}>
        <GridSurface
          client={model.client}
          model={model}
          theme={theme}
          blink={blink}
          label={title}
          onController={onController}
        />
        <Autocomplete model={model} />
        <FindBar model={model} />
        {notice ? (
          <div className="kiln-grid__notice" role="alert">
            <span>{notice}</span>
            <button type="button" aria-label="Dismiss" onClick={() => model.set({ notice: null })}>
              <Icon icon={X} size={12} />
            </button>
          </div>
        ) : null}
      </div>
      <SheetTabs model={model} />
      <GridMenus model={model} />
      <GridDialogs model={model} />
    </div>
  );
}
