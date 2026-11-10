/**
 * Grid's status bar (Grid mockup): the cell mode (Ready, Enter, Edit,
 * Point), the selection's Average, Count and Sum — formatted by the core like the active cell, with
 * Numerical Count, Minimum and Maximum in the customise menu — the view buttons and the zoom.
 */
import { formatCount, localizeNumberText, msg, t, tx } from '@kiln/i18n';
import { useLanguage } from '@kiln/i18n/react';
import type { GridMode, GridSelectMode, GridStats, ViewMode } from '@kiln/protocol';
import { StatusBar, type StatusBarItem, type StatusBarView } from '@kiln/ui-kit';
import { useMemo } from 'react';
import { useGrid } from './hooks.ts';
import type { GridEditorModel } from './model.ts';

const VIEWS: readonly StatusBarView[] = [
  { id: 'Normal', label: msg('Normal'), icon: 'NormalView' },
  { id: 'PageLayout', label: msg('Page Layout'), icon: 'PageLayoutView' },
  { id: 'PageBreakPreview', label: msg('Page Break Preview'), icon: 'PageBreakPreview' },
];

const MODE: Readonly<Record<GridMode, string>> = {
  Ready: msg('Ready'),
  Enter: msg('Enter'),
  Edit: msg('Edit'),
  Point: msg('Point'),
};

/** F8 and Shift+F8 as Excel's status bar names them. */
const SELECT_MODE: Readonly<Record<GridSelectMode, string | null>> = {
  Normal: null,
  Extend: msg('Extend Selection'),
  Add: msg('Add or Remove Selection'),
};

/** The left items: the mode, the selection mode (F8, Shift+F8), and read-only when it applies. */
export function modeItems(
  mode: GridMode | undefined,
  readOnly: boolean,
  selectMode: GridSelectMode = 'Normal',
): StatusBarItem[] {
  const shown = tx(MODE[mode ?? 'Ready']);
  const out: StatusBarItem[] = [
    { id: 'mode', label: t('Cell mode'), content: shown, announce: t('{mode} mode', { mode: shown }) },
  ];
  const sel = SELECT_MODE[selectMode];
  if (sel)
    out.push({
      id: 'selectmode',
      label: t('Selection mode'),
      content: tx(sel),
      announce: t('{mode} on', { mode: tx(sel) }),
    });
  if (readOnly) out.push({ id: 'readonly', label: t('Read-only'), content: t('Read-only'), announce: true });
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
                {label} <b>{value}</b>
              </span>
            ),
            defaultHidden: hidden,
          },
        ];
  // The core formats the figures like the active cell; the chrome shows them with the interface
  // language's separators and digits (text, dates and currencies stay as the core wrote them).
  const num = (v: string | null) => (v === null ? null : localizeNumberText(v));
  return [
    ...item('average', t('Average'), num(stats.average)),
    ...item('count', t('Count'), formatCount(stats.count)),
    ...item('numerical', t('Numerical Count'), formatCount(stats.numerical_count), true),
    ...item('min', t('Minimum'), num(stats.min), true),
    ...item('max', t('Maximum'), num(stats.max), true),
    ...item('sum', t('Sum'), num(stats.sum)),
  ];
}

export function GridStatusBar({ model, readOnly }: { model: GridEditorModel; readOnly: boolean }) {
  const mode = useGrid(model, (s) => s.grid?.mode);
  const stats = useGrid(model, (s) => s.grid?.stats);
  const selectMode = useGrid(model, (s) => s.grid?.surface?.select_mode ?? 'Normal');
  const status = useGrid(model, (s) => s.status);
  const circular = useGrid(model, (s) =>
    s.grid?.features.iterative[0] ? '' : (s.grid?.features.circular[0] ?? ''),
  );
  const pending = useGrid(model, (s) => !!s.grid?.features.calc_pending);
  const language = useLanguage();
  const left = useMemo(() => {
    const items = modeItems(mode, readOnly, selectMode);
    // Excel's "Circular References: A1" and "Calculate" (manual calculation waiting for F9).
    if (circular)
      items.push({
        id: 'circular',
        label: t('Circular References'),
        content: t('Circular References: {circular}', { circular: circular.replace(/^.*!/, '') }),
      });
    if (pending) items.push({ id: 'calculate', label: t('Calculate'), content: t('Calculate') });
    return items;
  }, [mode, readOnly, selectMode, circular, pending, language]);
  const right = useMemo(() => statItems(stats), [stats, language]);
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
            label: t('Zoom to Selection'),
            onSelect: () => void model.run('grid.view.zoom_to_selection'),
          },
        ],
      }}
    />
  );
}
