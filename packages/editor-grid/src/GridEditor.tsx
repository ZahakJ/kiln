/**
 * Kiln Grid's editing area (the Grid mockup): the formula bar under the ribbon, the
 * sheet surface with the autocomplete popover and the find bar over it, the sheet tabs below, the
 * context menus and the dialogs. Every edit is a core command; this is presentation only.
 */
import { t } from '@kiln/i18n';
import { Icon, type RibbonSpec, useFormFactor } from '@kiln/ui-kit';
import { X } from 'lucide-react';
import { useCallback } from 'react';
import { Autocomplete } from './Autocomplete.tsx';
import { PivotFieldsPane } from './analysis/FieldsPane.tsx';
import { GridAnalysisLayer } from './analysis/Layer.tsx';
import type { GridSurfaceController } from './controller.ts';
import { FindBar } from './FindBar.tsx';
import { FormulaBar } from './FormulaBar.tsx';
import { GridFeatureLayer } from './features/Layer.tsx';
import { PageLayoutView } from './features/PageLayoutView.tsx';
import { FormatChartArea } from './GridCharts.tsx';
import { GridDialogs } from './GridDialogs.tsx';
import { GridMenus } from './GridMenus.tsx';
import { GridSurface } from './GridSurface.tsx';
import { GridHomeLayer } from './home/HomeLayer.tsx';
import { useGrid } from './hooks.ts';
import { GridTouchLayer } from './mobile/GridTouch.tsx';
import { GridQuickKeys, PhoneStats, PickListSheet } from './mobile/PhoneGrid.tsx';
import type { GridEditorModel } from './model.ts';
import { FeaturePanes } from './panes/FeaturePanes.tsx';
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
  /** Grid's ribbon, whose Home controls the mini toolbar reuses. */
  ribbon?: RibbonSpec;
}

export function GridEditor({ model, title, theme, blink, onController, ribbon }: GridEditorProps) {
  const showFormulaBar = useGrid(model, (s) => s.showFormulaBar);
  const notice = useGrid(model, (s) => s.notice);
  const mode = useGrid(model, (s) => s.grid?.mode ?? 'Ready');
  // the phone's formula bar keys, sheet chips with the statistics, and pick list sheet.
  const phone = useFormFactor() === 'phone';
  const onKeyDownCapture = useCallback(
    (e: React.KeyboardEvent<HTMLDivElement>) => {
      // Shift+F10 and the Menu key open the cell menu at the active cell's corner.
      if (mode !== 'Ready') return;
      // Alt+Shift+F10: the error indicator's menu of the active cell (Excel's smart tag key).
      if (e.altKey && e.shiftKey && e.key === 'F10') {
        if (!model.state.grid?.surface?.error) return;
        e.preventDefault();
        e.stopPropagation();
        const r = e.currentTarget.getBoundingClientRect();
        const l = model.state.grid?.layout;
        model.set({
          menu: { x: r.left + (l?.header_w ?? 44) + 24, y: r.top + (l?.header_h ?? 24) + 16, kind: 'error' },
        });
        return;
      }
      if ((e.shiftKey && e.key === 'F10') || e.key === 'ContextMenu') {
        e.preventDefault();
        e.stopPropagation();
        const r = e.currentTarget.getBoundingClientRect();
        const l = model.state.grid?.layout;
        // A selected chart or picture has its own menu, as a right-click on it shows.
        const d = model.state.grid?.drawing;
        model.set({
          menu: {
            x: r.left + (l?.header_w ?? 44) + 24,
            y: r.top + (l?.header_h ?? 24) + 16,
            kind: d ? (d.chart ? 'chart' : 'object') : 'cell',
          },
        });
      }
    },
    [model, mode],
  );
  return (
    <div className="kiln-grid" data-testid="grid-editor" data-mode={mode}>
      {showFormulaBar ? <FormulaBar model={model} /> : null}
      {phone ? <GridQuickKeys model={model} /> : null}
      <div className="kiln-grid__body">
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
          <GridFeatureLayer model={model} />
          <GridAnalysisLayer model={model} />
          <GridHomeLayer model={model} />
          <PageLayoutView model={model} />
          <GridTouchLayer model={model} />
          {notice ? (
            <div className="kiln-grid__notice" role="alert">
              <span>{notice}</span>
              <button type="button" aria-label={t('Dismiss')} onClick={() => model.set({ notice: null })}>
                <Icon icon={X} size={12} />
              </button>
            </div>
          ) : null}
        </div>
        <FormatChartArea model={model} />
        <FeaturePanes model={model} />
        <PivotFieldsPane model={model} />
      </div>
      {phone ? (
        <div className="kiln-grid-phone__tabs">
          <SheetTabs model={model} />
          <PhoneStats model={model} />
        </div>
      ) : (
        <SheetTabs model={model} />
      )}
      {phone ? <PickListSheet model={model} /> : null}
      <GridMenus model={model} ribbon={ribbon} />
      <GridDialogs model={model} />
    </div>
  );
}
