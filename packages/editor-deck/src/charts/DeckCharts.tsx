/**
 * Charts on slides (the chart chrome, hosted by Deck): Insert ▸ Chart's dialog of every chart
 * type, the Chart Design and Format contextual tabs while a chart is selected, the Format Chart
 * pane beside the slide, and Edit Data, the chart's data sheet. Every control runs a
 * `SuiteCommand::Chart*` or `DeckCommand::SetChartData` on the core; what they show is the core's
 * (`DeckState.chart`, `DeckQuery::ChartData`).
 */
import { t } from '@kiln/i18n';
import type { DeckChartData } from '@kiln/protocol';
import {
  Button,
  ChangeChartTypeDialog,
  type ChartUiHost,
  chartTabs,
  EMPTY_CHART_STATE,
  FormatChartPane,
  Icon,
  type RibbonTab,
} from '@kiln/ui-kit';
import { X } from 'lucide-react';
import { type KeyboardEvent, useEffect, useMemo, useRef, useState } from 'react';
import { DeckDialogFrame } from '../dialogs/DeckDialogs.tsx';
import { useDeck } from '../hooks.ts';
import type { DeckEditorModel } from '../model.ts';

/** The chart host of the selected chart (an empty state while none is). */
export function useDeckChartHost(model: DeckEditorModel): ChartUiHost {
  const chart = useDeck(model, (s) => s.deck?.chart ?? null);
  return useMemo(
    () => ({ state: chart ?? EMPTY_CHART_STATE, run: (command) => void model.chart(command) }),
    [model, chart],
  );
}

/** Chart Design and Format, shown while a chart is selected (PowerPoint's Chart Tools). */
export function useDeckChartTabs(model: DeckEditorModel | null): readonly RibbonTab[] {
  const chart = useDeck(model ?? NO_MODEL, (s) => s.deck?.chart ?? null);
  return useMemo(() => {
    if (!model) return [];
    const host: ChartUiHost = {
      state: chart ?? EMPTY_CHART_STATE,
      run: (command) => void model.chart(command),
    };
    return chartTabs(host, {
      isChartSelected: () => !!model.state.deck?.chart,
      onFormatSelection: () => model.set({ chartPane: true }),
      onEditData: () => model.openDialog('chartData'),
    });
  }, [model, chart]);
}

/** The Format Chart pane, at the right of the slide while a chart is selected. */
export function FormatChartArea({ model }: { model: DeckEditorModel }) {
  const open = useDeck(model, (s) => s.chartPane && !!s.deck?.chart);
  const host = useDeckChartHost(model);
  if (!open) return null;
  return (
    <aside className="kiln-deckpane" aria-label={t('Format Chart Area')} data-testid="deck-chart-pane">
      <header className="kiln-deckpane__head">
        <h2>{t('Format Chart Area')}</h2>
        <button
          type="button"
          className="kiln-deckpane__close"
          aria-label={t('Close Format Chart Area')}
          onClick={() => {
            model.set({ chartPane: false });
            model.stage?.focus();
          }}
        >
          <Icon icon={X} size={14} />
        </button>
      </header>
      <div className="kiln-deckpane__body">
        <FormatChartPane host={host} />
      </div>
    </aside>
  );
}

/** Insert ▸ Chart: every chart type, as PowerPoint's Insert Chart dialog lists them. */
export function InsertChartDialog({ model }: { model: DeckEditorModel }) {
  const host = useDeckChartHost(model);
  return (
    <ChangeChartTypeDialog
      host={{ state: EMPTY_CHART_STATE, run: host.run }}
      mode="insert"
      open
      onOpenChange={(o) => {
        if (!o) model.closeDialog();
      }}
    />
  );
}

/** Column letters of a sheet (A … Z, AA …). */
export function columnName(index: number): string {
  let n = index + 1;
  let out = '';
  while (n > 0) {
    const r = (n - 1) % 26;
    out = String.fromCharCode(65 + r) + out;
    n = Math.floor((n - 1) / 26);
  }
  return out;
}

/** The table with a row or column more or less (at least two of each). */
export function resized(cells: readonly (readonly string[])[], rows: number, cols: number): string[][] {
  const r = Math.max(2, rows);
  const c = Math.max(2, cols);
  return Array.from({ length: r }, (_, i) => Array.from({ length: c }, (_, j) => cells[i]?.[j] ?? ''));
}

/**
 * Edit Data: the chart's data range on its data sheet, as a small sheet. A cell typed into
 * changes the chart as soon as it is left (Enter goes down, Tab right); rows and columns added or
 * removed make the chart's range larger or smaller, as dragging the range outline does in
 * PowerPoint's data sheet.
 */
export function ChartDataDialog({ model }: { model: DeckEditorModel }) {
  const [data, setData] = useState<DeckChartData | null>(null);
  const [cells, setCells] = useState<string[][]>([]);
  const table = useRef<HTMLTableElement>(null);
  useEffect(() => {
    let live = true;
    void model.query('ChartData').then((r) => {
      const d = r && 'ChartData' in r ? r.ChartData : null;
      if (!live) return;
      setData(d);
      setCells(d ? d.cells.map((row) => [...row]) : []);
    });
    return () => {
      live = false;
    };
  }, [model]);
  const send = (next: string[][]) => {
    setCells(next);
    void model.core('deck.chart.set_data', { cells: next });
  };
  const sent = useRef<string>('');
  const commit = (next: string[][]) => {
    const key = JSON.stringify(next);
    if (key === sent.current) return;
    sent.current = key;
    send(next);
  };
  useEffect(() => {
    if (data) sent.current = JSON.stringify(data.cells);
  }, [data]);
  const focusCell = (r: number, c: number) => {
    const input = table.current?.querySelector<HTMLInputElement>(`input[data-cell="${r}:${c}"]`);
    input?.focus();
    input?.select();
  };
  const onKey = (e: KeyboardEvent<HTMLInputElement>, r: number, c: number) => {
    const move = (dr: number, dc: number) => {
      e.preventDefault();
      commit(cells);
      focusCell(
        Math.min(Math.max(r + dr, 0), cells.length - 1),
        Math.min(Math.max(c + dc, 0), (cells[0]?.length ?? 1) - 1),
      );
    };
    const cols = cells[0]?.length ?? 1;
    if (e.key === 'Enter') move(e.shiftKey ? -1 : 1, 0);
    else if (e.key === 'Tab') {
      // Across the row, then on to the next one; past the last cell (or before the first) the
      // keys leave the sheet for its buttons, as Tab does anywhere else.
      const at = r * cols + c + (e.shiftKey ? -1 : 1);
      if (at < 0 || at >= cells.length * cols) {
        commit(cells);
        return;
      }
      e.preventDefault();
      commit(cells);
      focusCell(Math.floor(at / cols), at % cols);
    } else if (e.key === 'ArrowDown') move(1, 0);
    else if (e.key === 'ArrowUp') move(-1, 0);
  };
  const rows = cells.length;
  const cols = cells[0]?.length ?? 0;
  return (
    <DeckDialogFrame model={model} title={t('Edit Data')} size="lg" testId="deck-chart-data">
      {data === null ? (
        <p className="kiln-chartdata__empty">{t('Select a chart first.')}</p>
      ) : data.unreadable ? (
        <p className="kiln-chartdata__empty" role="alert">
          {data.unreadable}
        </p>
      ) : (
        <>
          <div className="kiln-chartdata__sheet">
            <table
              ref={table}
              className="kiln-chartdata"
              aria-label={t('Chart data on {sheet}', { sheet: data.sheet })}
            >
              <thead>
                <tr>
                  <th />
                  {cells[0]?.map((_, c) => (
                    <th key={columnName(data.col + c)} scope="col">
                      {columnName(data.col + c)}
                    </th>
                  ))}
                </tr>
              </thead>
              <tbody>
                {cells.map((row, r) => (
                  <tr key={r}>
                    <th scope="row">{data.row + r + 1}</th>
                    {row.map((v, c) => (
                      <td key={c} className={r === 0 || c === 0 ? 'is-label' : undefined}>
                        <input
                          value={v}
                          data-cell={`${r}:${c}`}
                          aria-label={`${columnName(data.col + c)}${data.row + r + 1}`}
                          inputMode={r === 0 || c === 0 ? 'text' : 'decimal'}
                          onChange={(e) => {
                            const next = cells.map((x) => [...x]);
                            const target = next[r];
                            if (target) target[c] = e.target.value;
                            setCells(next);
                          }}
                          onBlur={() => commit(cells)}
                          onKeyDown={(e) => onKey(e, r, c)}
                        />
                      </td>
                    ))}
                  </tr>
                ))}
              </tbody>
            </table>
          </div>
          <div className="kiln-chartdata__tools">
            <Button
              size="sm"
              data-testid="chart-data-add-row"
              onClick={() => commit(resized(cells, rows + 1, cols))}
            >
              {t('Add Category')}
            </Button>
            <Button
              size="sm"
              data-testid="chart-data-add-col"
              onClick={() => commit(resized(cells, rows, cols + 1))}
            >
              {t('Add Series')}
            </Button>
            <Button size="sm" disabled={rows <= 2} onClick={() => commit(resized(cells, rows - 1, cols))}>
              {t('Remove Last Category')}
            </Button>
            <Button size="sm" disabled={cols <= 2} onClick={() => commit(resized(cells, rows, cols - 1))}>
              {t('Remove Last Series')}
            </Button>
          </div>
        </>
      )}
    </DeckDialogFrame>
  );
}

/** A stand-in store for the hooks while there is no model (hooks cannot be conditional). */
const NO_MODEL = {
  store: {
    getState: () => ({ deck: null, chartPane: false }),
    getInitialState: () => ({ deck: null, chartPane: false }),
    setState: () => undefined,
    subscribe: () => () => undefined,
  },
} as unknown as DeckEditorModel;
