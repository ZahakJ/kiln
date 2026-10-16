/** The chart types as the Insert ▸ Charts galleries and Change Chart Type group them (Office's order). */
import type { ChartKind } from './types.ts';

export interface ChartFamily {
  id: 'column' | 'bar' | 'line' | 'area' | 'pie' | 'xy' | 'radar' | 'hierarchy' | 'combo';
  label: string;
  kinds: readonly ChartKind[];
}

/** English names (Office's) of every chart type. Keyed by the protocol enum, so a new kind fails typecheck. */
export const CHART_KIND_LABELS: Record<ChartKind, string> = {
  ColumnClustered: 'Clustered Column',
  ColumnStacked: 'Stacked Column',
  ColumnPercent: '100% Stacked Column',
  BarClustered: 'Clustered Bar',
  BarStacked: 'Stacked Bar',
  BarPercent: '100% Stacked Bar',
  Line: 'Line',
  LineMarkers: 'Line with Markers',
  LineStacked: 'Stacked Line',
  LinePercent: '100% Stacked Line',
  LineSmooth: 'Smooth Line',
  Area: 'Area',
  AreaStacked: 'Stacked Area',
  AreaPercent: '100% Stacked Area',
  Pie: 'Pie',
  PieExploded: 'Pie with Pulled-out Slice',
  Doughnut: 'Doughnut',
  ScatterMarkers: 'Scatter',
  ScatterLines: 'Scatter with Straight Lines and Markers',
  ScatterSmooth: 'Scatter with Smooth Lines',
  Bubble: 'Bubble',
  Radar: 'Radar',
  RadarMarkers: 'Radar with Markers',
  RadarFilled: 'Filled Radar',
  ComboColumnLine: 'Clustered Column – Line',
  ComboColumnLineSecondary: 'Clustered Column – Line on Secondary Axis',
  Treemap: 'Treemap',
  Sunburst: 'Sunburst',
};

/** One-line descriptions for tooltips (Office's ScreenTip style: what it is for). */
export const CHART_KIND_HINTS: Partial<Record<ChartKind, string>> = {
  ColumnClustered: 'Compare values across a few categories.',
  BarClustered: 'Compare values when the category names are long.',
  LineMarkers: 'Show trends over time or ordered categories.',
  AreaStacked: 'Show how parts add up to a total over time.',
  Pie: 'Show proportions of one whole.',
  Doughnut: 'Show proportions, with room for a label in the middle.',
  ScatterMarkers: 'Show the relationship between two sets of numbers.',
  Bubble: 'Like scatter, with a third value as the bubble size.',
  RadarMarkers: 'Compare several values across the same set of qualities.',
  ComboColumnLineSecondary: 'Compare values with very different scales.',
  Treemap: 'Show parts of a hierarchy as nested rectangles.',
  Sunburst: 'Show a hierarchy as rings, from the root outwards.',
};

export const CHART_FAMILIES: readonly ChartFamily[] = [
  { id: 'column', label: 'Column', kinds: ['ColumnClustered', 'ColumnStacked', 'ColumnPercent'] },
  { id: 'bar', label: 'Bar', kinds: ['BarClustered', 'BarStacked', 'BarPercent'] },
  { id: 'line', label: 'Line', kinds: ['Line', 'LineMarkers', 'LineStacked', 'LinePercent', 'LineSmooth'] },
  { id: 'area', label: 'Area', kinds: ['Area', 'AreaStacked', 'AreaPercent'] },
  { id: 'pie', label: 'Pie', kinds: ['Pie', 'PieExploded', 'Doughnut'] },
  { id: 'xy', label: 'X Y (Scatter)', kinds: ['ScatterMarkers', 'ScatterLines', 'ScatterSmooth', 'Bubble'] },
  { id: 'radar', label: 'Radar', kinds: ['Radar', 'RadarMarkers', 'RadarFilled'] },
  { id: 'hierarchy', label: 'Hierarchy', kinds: ['Treemap', 'Sunburst'] },
  { id: 'combo', label: 'Combo', kinds: ['ComboColumnLine', 'ComboColumnLineSecondary'] },
];

export const ALL_CHART_KINDS: readonly ChartKind[] = CHART_FAMILIES.flatMap((f) => f.kinds);

/** The family a kind belongs to. */
export function familyOf(kind: ChartKind): ChartFamily {
  return CHART_FAMILIES.find((f) => f.kinds.includes(kind)) ?? (CHART_FAMILIES[0] as ChartFamily);
}

/** Office's quick layouts 1–11, described for the gallery tooltips. */
export const QUICK_LAYOUTS: readonly { layout: number; label: string }[] = [
  { layout: 1, label: 'Layout 1: title, legend on the right' },
  { layout: 2, label: 'Layout 2: title, legend at the top, data labels' },
  { layout: 3, label: 'Layout 3: title, legend at the bottom' },
  { layout: 4, label: 'Layout 4: legend at the bottom, data labels' },
  { layout: 5, label: 'Layout 5: title and axis titles' },
  { layout: 6, label: 'Layout 6: title, axis titles, legend' },
  { layout: 7, label: 'Layout 7: axis titles, legend, minor gridlines' },
  { layout: 8, label: 'Layout 8: title and axis titles, no gridlines' },
  { layout: 9, label: 'Layout 9: everything labelled' },
  { layout: 10, label: 'Layout 10: title, legend, data labels' },
  { layout: 11, label: 'Layout 11: legend only' },
];

/** Change Colors: colourful, one accent in shades, or greys (`ChartSetColors.palette`). */
export const COLOR_SETS: readonly { palette: number; label: string; accent: string | null }[] = [
  { palette: 0, label: 'Colorful', accent: null },
  { palette: 1, label: 'Monochromatic: accent 1', accent: 'grid' },
  { palette: 2, label: 'Monochromatic: accent 2', accent: 'deck' },
  { palette: 3, label: 'Monochromatic: accent 3', accent: 'write' },
  { palette: 4, label: 'Monochromatic: accent 4', accent: 'notes' },
  { palette: 5, label: 'Monochromatic: accent 5', accent: 'clay' },
  { palette: 6, label: 'Monochromatic: accent 6', accent: 'plum' },
  { palette: 7, label: 'Grayscale', accent: 'grey' },
];
