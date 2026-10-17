/**
 * Charts in Kiln Grid (WP-73's chart chrome, hosted by Grid): the Chart Design and Format
 * contextual tabs while a chart is selected, the Format Chart pane beside the sheet, and the
 * Recommended Charts dialog of Insert ▸ Charts. Every control runs a `SuiteCommand::Chart*` on
 * the core through {@link GridEditorModel.chart}; the chart's state is the core's
 * (`GridState.drawing.chart`).
 */
import type { ChartKind } from '@kiln/protocol';
import {
  type ChartUiHost,
  chartTabs,
  EMPTY_CHART_STATE,
  FormatChartPane,
  Icon,
  RecommendedChartsDialog,
  type RibbonTab,
} from '@kiln/ui-kit';
import { X } from 'lucide-react';
import { useEffect, useMemo, useState } from 'react';
import { useGrid } from './hooks.ts';
import type { GridEditorModel } from './model.ts';

/** The chart host of the selected chart (an empty state while none is). */
export function useChartHost(model: GridEditorModel): ChartUiHost {
  const chart = useGrid(model, (s) => s.grid?.drawing?.chart ?? null);
  return useMemo(
    () => ({
      state: chart ?? EMPTY_CHART_STATE,
      run: (command) => void model.chart(command),
    }),
    [model, chart],
  );
}

/** Chart Design and Format, shown while a chart is selected (Office's Chart Tools). */
export function useChartTabs(model: GridEditorModel | null): readonly RibbonTab[] {
  const chart = useGrid(model ?? NO_MODEL, (s) => s.grid?.drawing?.chart ?? null);
  const sheets = useGrid(model ?? NO_MODEL, (s) => s.grid?.sheets ?? null);
  return useMemo(() => {
    if (!model) return [];
    const host: ChartUiHost = {
      state: chart ?? EMPTY_CHART_STATE,
      run: (command) => void model.chart(command),
    };
    return chartTabs(host, {
      sheets: (sheets ?? []).filter((s) => !s.hidden).map((s) => s.name),
      isChartSelected: () => !!model.state.grid?.drawing,
      onFormatSelection: () => model.set({ chartPane: true }),
    });
  }, [model, chart, sheets]);
}

/** The Format Chart pane, at the right of the sheet while a chart is selected. */
export function FormatChartArea({ model }: { model: GridEditorModel }) {
  const open = useGrid(model, (s) => s.chartPane && !!s.grid?.drawing);
  const name = useGrid(model, (s) => s.grid?.drawing?.name ?? '');
  const host = useChartHost(model);
  if (!open) return null;
  return (
    <aside className="kiln-grid__pane" aria-label="Format Chart Area" data-testid="grid-chart-pane">
      <header className="kiln-grid__pane-head">
        <h2>Format Chart Area</h2>
        <span className="kiln-grid__pane-sub">{name}</span>
        <button
          type="button"
          className="kiln-grid__pane-close"
          aria-label="Close Format Chart Area"
          onClick={() => {
            model.set({ chartPane: false });
            model.surface?.focus();
          }}
        >
          <Icon icon={X} size={14} />
        </button>
      </header>
      <div className="kiln-grid__pane-body">
        <FormatChartPane host={host} />
      </div>
    </aside>
  );
}

/** Insert ▸ Recommended Charts: the types that suit the selection, best first. */
export function RecommendedChartsForSelection({ model }: { model: GridEditorModel }) {
  const host = useChartHost(model);
  const [recommended, setRecommended] = useState<ChartKind[] | null>(null);
  useEffect(() => {
    let live = true;
    void model.recommendedCharts().then((r) => {
      if (live) setRecommended(r.length > 0 ? r : ['ColumnClustered']);
    });
    return () => {
      live = false;
    };
  }, [model]);
  if (!recommended) return null;
  return (
    <RecommendedChartsDialog
      host={host}
      recommended={recommended}
      open
      onOpenChange={(o) => {
        if (!o) model.closeDialog();
      }}
    />
  );
}

/** A stand-in store for the hooks while there is no model (hooks cannot be conditional). */
const NO_MODEL = {
  store: {
    getState: () => ({ grid: null, chartPane: false }),
    getInitialState: () => ({ grid: null, chartPane: false }),
    setState: () => undefined,
    subscribe: () => () => undefined,
  },
} as unknown as GridEditorModel;
