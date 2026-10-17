/**
 * Grid's status bar (PLAN.md §1.4 "Bottom"; Grid mockup): the cell mode (Ready, Enter, Edit,
 * Point), the selection's Average, Count and Sum — formatted by the core like the active cell, with
 * Numerical Count, Minimum and Maximum in the customise menu — the view buttons and the zoom.
 */
import type { GridMode, GridStats, ViewMode } from '@kiln/protocol';
import { StatusBar, type StatusBarItem, type StatusBarView } from '@kiln/ui-kit';
import { useMemo } from 'react';
import { useGrid } from './hooks.ts';
import type { GridEditorModel } from './model.ts';

const VIEWS: readonly StatusBarView[] = [
  { id: 'Normal', label: 'Normal', icon: 'NormalView' },
  { id: 'PageLayout', label: 'Page Layout', icon: 'PageLayoutView' },
  { id: 'PageBreakPreview', label: 'Page Break Preview', icon: 'PageBreakPreview' },
];

const MODE: Readonly<Record<GridMode, string>> = {
  Ready: 'Ready',
  Enter: 'Enter',
  Edit: 'Edit',
  Point: 'Point',
};

/** The left items: the mode, and read-only when it applies. */
export function modeItems(mode: GridMode | undefined, readOnly: boolean): StatusBarItem[] {
  const out: StatusBarItem[] = [{ id: 'mode', label: 'Cell mode', content: MODE[mode ?? 'Ready'] }];
  if (readOnly) out.push({ id: 'readonly', label: 'Read-only', content: 'Read-only' });
  return out;
}

/** The statistics items (Excel's defaults: Average, Count, Sum). */
export function statItems(stats: GridStats | null | undefined): StatusBarItem[] {
  if (!stats) return [];
  const item = (
    id: string,
    label: string,
    value: string | null | undefined,
    hidden = false,
  ): StatusBarItem[] =>
    value === null || value === undefined
      ? []
      : [
          {
            id,
            label,
            content: (
              <span className="kiln-gridstat">
                {label === 'Numerical Count' ? 'Numerical Count' : label} <b>{value}</b>
              </span>
            ),
            defaultHidden: hidden,
          },
        ];
  return [
    ...item('average', 'Average', stats.average),
    ...item('count', 'Count', stats.count.toLocaleString()),
    ...item('numerical', 'Numerical Count', stats.numerical_count.toLocaleString(), true),
    ...item('min', 'Minimum', stats.min, true),
    ...item('max', 'Maximum', stats.max, true),
    ...item('sum', 'Sum', stats.sum),
  ];
}

export function GridStatusBar({ model, readOnly }: { model: GridEditorModel; readOnly: boolean }) {
  const mode = useGrid(model, (s) => s.grid?.mode);
  const stats = useGrid(model, (s) => s.grid?.stats);
  const status = useGrid(model, (s) => s.status);
  const left = useMemo(() => modeItems(mode, readOnly), [mode, readOnly]);
  const right = useMemo(() => statItems(stats), [stats]);
  const view = status?.view ?? 'Normal';
  return (
    <StatusBar
      // The defaults (Numerical Count, Minimum, Maximum hidden) apply when the statistics appear.
      key={stats ? 'range' : 'cell'}
      left={left}
      right={right}
      storageKey="kiln.statusbar.grid"
      views={{
        value: VIEWS.some((v) => v.id === view) ? view : 'Normal',
        options: VIEWS,
        onChange: (m) => void model.run('suite.view.mode', { mode: m as ViewMode }),
      }}
      zoom={{
        value: Math.round((status?.zoom ?? 1) * 100),
        onChange: (pct) => void model.run('suite.view.zoom', { zoom: pct / 100 }),
        presets: [400, 200, 150, 100, 75, 50, 25],
        fits: [
          {
            id: 'selection',
            label: 'Zoom to Selection',
            onSelect: () => void model.run('grid.view.zoom_to_selection'),
          },
        ],
      }}
    />
  );
}
