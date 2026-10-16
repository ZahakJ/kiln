/**
 * Types of the chart editing chrome. The commands are the core's `SuiteCommand::Chart*` variants in
 * their JSON form, so a host passes them straight to `client.execute(doc, { Suite: command })`; the
 * chrome never edits a chart itself (PLAN.md §3.7).
 */
import type {
  ChartAxisTarget,
  ChartBound,
  ChartElementKind,
  ChartKind,
  ChartLabelPosition,
  ChartLegendPosition,
  SuiteCommand,
} from '@kiln/protocol';

export type {
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
  | 'ChartFormatAxis';

type Variant<K> = K extends string ? Extract<SuiteCommand, Record<K, unknown>> : never;

/** Every chart command, exactly as the core's protocol spells it. */
export type ChartCommand = Extract<SuiteCommand, 'ChartSwitchRowColumn'> | Variant<ChartVariantKey>;

/** One series as the chart chrome lists it (Select Data, Format pane). */
export interface ChartSeriesInfo {
  /** The series' `idx` (what `ChartFormatSeries` addresses). */
  idx: number;
  name: string;
  /** Resolved colour `#RRGGBB` for swatches. */
  color: string;
  secondary: boolean;
}

/** What the chrome shows about the selected chart; the host derives it from the core. */
export interface ChartUiState {
  kind: ChartKind | null;
  /** Office chart style number (1–48), when set. */
  style: number | null;
  /** Title text; `null` when the chart has no title. */
  title: string | null;
  legend: ChartLegendPosition | null;
  dataLabels: ChartLabelPosition | null;
  /** Which elements are on (Add Chart Element check marks). */
  elements: Partial<Record<ChartElementKind, boolean>>;
  /** Select Data's "Chart data range" (`Sheet1!$A$1:$D$5`), empty for literal data. */
  dataRange: string;
  series: readonly ChartSeriesInfo[];
  /** Horizontal (category) axis labels. */
  categories: readonly string[];
  /** Whether the chart reads cells (Write and Deck charts without a workbook do not). */
  hasCells: boolean;
}

/** The host the chart chrome talks to. */
export interface ChartUiHost {
  state: ChartUiState;
  run: (command: ChartCommand) => void;
}

export const EMPTY_CHART_STATE: ChartUiState = {
  kind: null,
  style: null,
  title: null,
  legend: null,
  dataLabels: null,
  elements: {},
  dataRange: '',
  series: [],
  categories: [],
  hasCells: false,
};

/** `ChartBound` helpers for the Format pane. */
export const bound = {
  auto: 'Auto' as ChartBound,
  fixed: (value: number): ChartBound => ({ Fixed: value }),
};
