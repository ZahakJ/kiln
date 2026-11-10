/** The chart types as the Insert ▸ Charts galleries and Change Chart Type group them (Office's order). */
import { msg } from '@kiln/i18n';
import type { ChartKind } from './types.ts';

export interface ChartFamily {
  id:
    | 'column'
    | 'bar'
    | 'line'
    | 'area'
    | 'pie'
    | 'xy'
    | 'radar'
    | 'hierarchy'
    | 'statistic'
    | 'waterfall'
    | 'combo';
  label: string;
  kinds: readonly ChartKind[];
}

/** English names (Office's) of every chart type. Keyed by the protocol enum, so a new kind fails typecheck. */
export const CHART_KIND_LABELS: Record<ChartKind, string> = {
  ColumnClustered: msg('Clustered Column'),
  ColumnStacked: msg('Stacked Column'),
  ColumnPercent: msg('100% Stacked Column'),
  BarClustered: msg('Clustered Bar'),
  BarStacked: msg('Stacked Bar'),
  BarPercent: msg('100% Stacked Bar'),
  Line: msg('Line'),
  LineMarkers: msg('Line with Markers'),
  LineStacked: msg('Stacked Line'),
  LinePercent: msg('100% Stacked Line'),
  LineSmooth: msg('Smooth Line'),
  Area: msg('Area'),
  AreaStacked: msg('Stacked Area'),
  AreaPercent: msg('100% Stacked Area'),
  Pie: msg('Pie'),
  PieExploded: msg('Pie with Pulled-out Slice'),
  Doughnut: msg('Doughnut'),
  ScatterMarkers: msg('Scatter'),
  ScatterLines: msg('Scatter with Straight Lines and Markers'),
  ScatterSmooth: msg('Scatter with Smooth Lines'),
  Bubble: msg('Bubble'),
  Radar: msg('Radar'),
  RadarMarkers: msg('Radar with Markers'),
  RadarFilled: msg('Filled Radar'),
  ComboColumnLine: msg('Clustered Column – Line'),
  ComboColumnLineSecondary: msg('Clustered Column – Line on Secondary Axis'),
  Treemap: msg('Treemap'),
  Sunburst: msg('Sunburst'),
  Histogram: msg('Histogram'),
  Pareto: msg('Pareto'),
  BoxWhisker: msg('Box and Whisker'),
  Waterfall: msg('Waterfall'),
  Funnel: msg('Funnel'),
};

/** One-line descriptions for tooltips (Office's ScreenTip style: what it is for). */
export const CHART_KIND_HINTS: Partial<Record<ChartKind, string>> = {
  ColumnClustered: msg('Compare values across a few categories.'),
  BarClustered: msg('Compare values when the category names are long.'),
  LineMarkers: msg('Show trends over time or ordered categories.'),
  AreaStacked: msg('Show how parts add up to a total over time.'),
  Pie: msg('Show proportions of one whole.'),
  Doughnut: msg('Show proportions, with room for a label in the middle.'),
  ScatterMarkers: msg('Show the relationship between two sets of numbers.'),
  Bubble: msg('Like scatter, with a third value as the bubble size.'),
  RadarMarkers: msg('Compare several values across the same set of qualities.'),
  ComboColumnLineSecondary: msg('Compare values with very different scales.'),
  Treemap: msg('Show parts of a hierarchy as nested rectangles.'),
  Sunburst: msg('Show a hierarchy as rings, from the root outwards.'),
  Histogram: msg('Show how values are spread over ranges (bins).'),
  Pareto: msg('Show the biggest contributors first, with their running share of the total.'),
  BoxWhisker: msg('Show the spread of values: quartiles, median, mean and range.'),
  Waterfall: msg('Show how a starting value rises and falls to a total.'),
  Funnel: msg('Show values that shrink stage by stage.'),
};

export const CHART_FAMILIES: readonly ChartFamily[] = [
  { id: 'column', label: msg('Column'), kinds: ['ColumnClustered', 'ColumnStacked', 'ColumnPercent'] },
  { id: 'bar', label: msg('Bar'), kinds: ['BarClustered', 'BarStacked', 'BarPercent'] },
  {
    id: 'line',
    label: msg('Line'),
    kinds: ['Line', 'LineMarkers', 'LineStacked', 'LinePercent', 'LineSmooth'],
  },
  { id: 'area', label: msg('Area'), kinds: ['Area', 'AreaStacked', 'AreaPercent'] },
  { id: 'pie', label: msg('Pie'), kinds: ['Pie', 'PieExploded', 'Doughnut'] },
  {
    id: 'xy',
    label: msg('X Y (Scatter)'),
    kinds: ['ScatterMarkers', 'ScatterLines', 'ScatterSmooth', 'Bubble'],
  },
  { id: 'radar', label: msg('Radar'), kinds: ['Radar', 'RadarMarkers', 'RadarFilled'] },
  { id: 'hierarchy', label: msg('Hierarchy'), kinds: ['Treemap', 'Sunburst'] },
  { id: 'statistic', label: msg('Statistic'), kinds: ['Histogram', 'Pareto', 'BoxWhisker'] },
  { id: 'waterfall', label: msg('Waterfall and Funnel'), kinds: ['Waterfall', 'Funnel'] },
  { id: 'combo', label: msg('Combo'), kinds: ['ComboColumnLine', 'ComboColumnLineSecondary'] },
];

export const ALL_CHART_KINDS: readonly ChartKind[] = CHART_FAMILIES.flatMap((f) => f.kinds);

/** The family a kind belongs to. */
export function familyOf(kind: ChartKind): ChartFamily {
  return CHART_FAMILIES.find((f) => f.kinds.includes(kind)) ?? (CHART_FAMILIES[0] as ChartFamily);
}

/** Office's quick layouts 1–11, described for the gallery tooltips. */
export const QUICK_LAYOUTS: readonly { layout: number; label: string }[] = [
  { layout: 1, label: msg('Layout 1: title, legend on the right') },
  { layout: 2, label: msg('Layout 2: title, legend at the top, data labels') },
  { layout: 3, label: msg('Layout 3: title, legend at the bottom') },
  { layout: 4, label: msg('Layout 4: legend at the bottom, data labels') },
  { layout: 5, label: msg('Layout 5: title and axis titles') },
  { layout: 6, label: msg('Layout 6: title, axis titles, legend') },
  { layout: 7, label: msg('Layout 7: axis titles, legend, minor gridlines') },
  { layout: 8, label: msg('Layout 8: title and axis titles, no gridlines') },
  { layout: 9, label: msg('Layout 9: everything labelled') },
  { layout: 10, label: msg('Layout 10: title, legend, data labels') },
  { layout: 11, label: msg('Layout 11: legend only') },
];

/** Change Colors: colourful, one accent in shades, or greys (`ChartSetColors.palette`). */
export const COLOR_SETS: readonly { palette: number; label: string; accent: string | null }[] = [
  { palette: 0, label: msg('Colorful'), accent: null },
  { palette: 1, label: msg('Monochromatic: accent 1'), accent: 'grid' },
  { palette: 2, label: msg('Monochromatic: accent 2'), accent: 'deck' },
  { palette: 3, label: msg('Monochromatic: accent 3'), accent: 'write' },
  { palette: 4, label: msg('Monochromatic: accent 4'), accent: 'notes' },
  { palette: 5, label: msg('Monochromatic: accent 5'), accent: 'clay' },
  { palette: 6, label: msg('Monochromatic: accent 6'), accent: 'plum' },
  { palette: 7, label: msg('Grayscale'), accent: 'grey' },
];
