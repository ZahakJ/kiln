/**
 * Types of the chart editing chrome. The commands are the core's `SuiteCommand::Chart*` variants in
 * their JSON form, so a host passes them straight to `client.execute(doc, { Suite: command })`; the
 * chrome never edits a chart itself.
 */
import type {
  ChartAreaState,
  ChartAreaTarget,
  ChartAxisState,
  ChartAxisTarget,
  ChartBound,
  ChartElementKind,
  ChartKind,
  ChartLabelPosition,
  ChartLegendPosition,
  ChartSeriesState,
  ChartState,
  SuiteCommand,
} from '@kiln/protocol';

export type {
  ChartAreaState,
  ChartAreaTarget,
  ChartAxisTarget,
  ChartBound,
  ChartElementKind,
  ChartKind,
  ChartLabelPosition,
  ChartLegendPosition,
};

type ChartVariantKey =
  | 'ChartInsert'
  | 'ChartChangeType'
  | 'ChartSelectData'
  | 'ChartSetElement'
  | 'ChartSetLegend'
  | 'ChartSetDataLabels'
  | 'ChartQuickLayout'
  | 'ChartSetStyle'
  | 'ChartSetColors'
  | 'ChartSetTitle'
  | 'ChartMove'
  | 'ChartFormatSeries'
  | 'ChartFormatAxis'
  | 'ChartEditSeries'
  | 'ChartHiddenEmpty'
  | 'ChartFormatArea';

type Variant<K> = K extends string ? Extract<SuiteCommand, Record<K, unknown>> : never;

/** Every chart command, exactly as the core's protocol spells it. */
export type ChartCommand = Extract<SuiteCommand, 'ChartSwitchRowColumn'> | Variant<ChartVariantKey>;

/** One series as the chart chrome lists it (Select Data, Format pane); colours are `RRGGBB`. */
export type ChartSeriesInfo = ChartSeriesState;

/** One axis as the Format pane shows it, with the bounds the chart is drawn with. */
export type ChartAxisInfo = ChartAxisState;

/**
 * What the chrome shows about the selected chart. The core computes it
 * (`kiln_charts::state::chart_state`), so the chrome derives nothing from the chart itself.
 */
export type ChartUiState = ChartState;

/** The host the chart chrome talks to. */
export interface ChartUiHost {
  state: ChartUiState;
  run: (command: ChartCommand) => void;
}

export const EMPTY_CHART_STATE: ChartUiState = {
  kind: null,
  style: null,
  color_set: 0,
  color_sets: [],
  title: null,
  legend: null,
  data_labels: null,
  elements: [],
  data_range: '',
  series: [],
  categories: [],
  has_cells: false,
  axes: [],
  blanks: 'Gap',
  show_hidden: false,
  categories_range: '',
  chart_area: {
    fill: null,
    no_fill: false,
    line: null,
    no_line: false,
    line_width: 0,
    shadow: false,
    rounded: false,
  },
  plot_area: {
    fill: null,
    no_fill: false,
    line: null,
    no_line: false,
    line_width: 0,
    shadow: false,
    rounded: false,
  },
};

/** `ChartBound` helpers for the Format pane. */
export const bound = {
  auto: 'Auto' as ChartBound,
  fixed: (value: number): ChartBound => ({ Fixed: value }),
};
