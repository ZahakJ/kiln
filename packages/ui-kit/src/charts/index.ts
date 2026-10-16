/**
 * Chart editing chrome shared by Grid, Write and Deck (WP-73): the chart type galleries, Change
 * Chart Type and Recommended Charts, Select Data, Add Chart Element, Quick Layout, styles and
 * colours, Move Chart, the Format Chart pane and the Chart Design / Format contextual tabs. Every
 * control runs a `SuiteCommand::Chart*` through a {@link ChartUiHost}; charts themselves are laid
 * out and drawn by the core (`kiln-charts`).
 */
import './charts.css';

export {
  ChangeChartTypeDialog,
  RANGE_PATTERN,
  RecommendedChartsDialog,
  SelectDataDialog,
} from './ChartDialogs.tsx';
export { ChangeColorsMenu, ChartElementsMenu, ChartStylesMenu, QuickLayoutMenu } from './ChartMenus.tsx';
export { ChartThumb } from './ChartThumb.tsx';
export { ChartTypeGallery, type ChartTypeGalleryProps } from './ChartTypeGallery.tsx';
export { type ChartTabsOptions, chartTabs, MoveChartDialog } from './chartRibbon.tsx';
export { FormatChartPane, SERIES_SWATCHES } from './FormatChartPane.tsx';
export {
  ALL_CHART_KINDS,
  CHART_FAMILIES,
  CHART_KIND_HINTS,
  CHART_KIND_LABELS,
  type ChartFamily,
  COLOR_SETS,
  familyOf,
  QUICK_LAYOUTS,
} from './kinds.ts';
export {
  bound,
  type ChartAxisInfo,
  type ChartAxisTarget,
  type ChartBound,
  type ChartCommand,
  type ChartElementKind,
  type ChartKind,
  type ChartLabelPosition,
  type ChartLegendPosition,
  type ChartSeriesInfo,
  type ChartUiHost,
  type ChartUiState,
  EMPTY_CHART_STATE,
} from './types.ts';
