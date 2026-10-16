/**
 * Sheet and workbook metadata: defined names, tables (ListObjects), conditional formatting, data
 * validation, hyperlinks, notes and threaded comments, sheet views, print settings, AutoFilter and
 * drawing anchors. All of it is plain JSON data so it snapshots, serialises and diffs trivially.
 *
 * Formulas inside metadata (names, CF rules, validation criteria, table column formulas) are stored
 * without the leading `=`, as in SpreadsheetML.
 */
import type { CellRange, CellRef } from './address.ts';
import type { Color } from './styles.ts';

// ── names ─────────────────────────────────────────────────────────────────

export interface DefinedName {
  name: string;
  /** Formula text without `=`, e.g. `Sheet1!$A$1:$B$10`. */
  formula: string;
  /** Sheet id for a sheet-scoped (local) name; null = workbook scope. */
  scope: number | null;
  hidden?: boolean;
  comment?: string;
}

/** Excel's reserved built-in names. */
export const BUILTIN_NAMES = {
  printArea: '_xlnm.Print_Area',
  printTitles: '_xlnm.Print_Titles',
  filterDatabase: '_xlnm._FilterDatabase',
  criteria: '_xlnm.Criteria',
  extract: '_xlnm.Extract',
} as const;

// ── AutoFilter and sort state ─────────────────────────────────────────────

export type FilterOperator =
  | 'equal'
  | 'notEqual'
  | 'greaterThan'
  | 'greaterThanOrEqual'
  | 'lessThan'
  | 'lessThanOrEqual';

export type DynamicFilterType =
  | 'aboveAverage'
  | 'belowAverage'
  | 'today'
  | 'yesterday'
  | 'tomorrow'
  | 'thisWeek'
  | 'lastWeek'
  | 'nextWeek'
  | 'thisMonth'
  | 'lastMonth'
  | 'nextMonth'
  | 'thisQuarter'
  | 'lastQuarter'
  | 'nextQuarter'
  | 'thisYear'
  | 'lastYear'
  | 'nextYear'
  | 'yearToDate'
  | `Q${1 | 2 | 3 | 4}`
  | `M${1 | 2 | 3 | 4 | 5 | 6 | 7 | 8 | 9 | 10 | 11 | 12}`;

export interface DateGroupItem {
  grouping: 'year' | 'month' | 'day' | 'hour' | 'minute' | 'second';
  year: number;
  month?: number;
  day?: number;
  hour?: number;
  minute?: number;
  second?: number;
}

export type ColumnFilter =
  | { type: 'values'; values: string[]; blank?: boolean; dateGroups?: DateGroupItem[] }
  | { type: 'custom'; and?: boolean; criteria: { operator: FilterOperator; value: string }[] }
  | { type: 'top10'; top: boolean; percent: boolean; value: number }
  | { type: 'dynamic'; dynamicType: DynamicFilterType }
  | { type: 'color'; cellColor: boolean; dxf: number }
  | { type: 'icon'; iconSet: IconSetName; iconId: number };

export interface FilterColumn {
  /** Column offset within the filter range. */
  colId: number;
  hiddenButton?: boolean;
  showButton?: boolean;
  filter?: ColumnFilter;
}

export interface SortCondition {
  range: CellRange;
  descending?: boolean;
  sortBy?: 'value' | 'cellColor' | 'fontColor' | 'icon';
  customList?: string[];
  dxf?: number;
}

export interface SortState {
  range: CellRange;
  caseSensitive?: boolean;
  columnSort?: boolean;
  conditions: SortCondition[];
}

export interface AutoFilter {
  range: CellRange;
  columns: FilterColumn[];
  sort?: SortState;
}

// ── tables ────────────────────────────────────────────────────────────────

export type TotalsFunction =
  | 'none'
  | 'sum'
  | 'min'
  | 'max'
  | 'average'
  | 'count'
  | 'countNums'
  | 'stdDev'
  | 'var'
  | 'custom';

export interface TableColumn {
  name: string;
  totalsFunction?: TotalsFunction;
  totalsLabel?: string;
  /** Totals-row formula when `totalsFunction` is `custom`. */
  totalsFormula?: string;
  /** Calculated-column formula. */
  calculatedFormula?: string;
  dataDxf?: number;
  headerDxf?: number;
  totalsDxf?: number;
}

export interface TableStyleInfo {
  name: string;
  showFirstColumn?: boolean;
  showLastColumn?: boolean;
  showRowStripes?: boolean;
  showColumnStripes?: boolean;
}

/** An Excel table (ListObject). */
export interface Table {
  /** Workbook-unique numeric id (`table/@id`). */
  id: number;
  /** Name used by structured references; unique in the workbook (case-insensitive). */
  name: string;
  displayName?: string;
  /** Full range including the header and totals rows. */
  range: CellRange;
  headerRow: boolean;
  totalsRow: boolean;
  columns: TableColumn[];
  style?: TableStyleInfo;
  /** The table's own filter (relative to the table, excluding the totals row). */
  autoFilter?: Omit<AutoFilter, 'range'>;
  comment?: string;
}

// ── conditional formatting ────────────────────────────────────────────────

export type CfOperator =
  | 'between'
  | 'notBetween'
  | 'equal'
  | 'notEqual'
  | 'greaterThan'
  | 'lessThan'
  | 'greaterThanOrEqual'
  | 'lessThanOrEqual';

export type TimePeriod =
  | 'today'
  | 'yesterday'
  | 'tomorrow'
  | 'last7Days'
  | 'thisWeek'
  | 'lastWeek'
  | 'nextWeek'
  | 'thisMonth'
  | 'lastMonth'
  | 'nextMonth';

/** A conditional-format value object (threshold of a scale, bar or icon set). */
export interface Cfvo {
  type: 'min' | 'max' | 'num' | 'percent' | 'percentile' | 'formula' | 'autoMin' | 'autoMax';
  /** Number or formula text, depending on `type`. */
  value?: string;
  /** Greater-than-or-equal (icon sets); default true. */
  gte?: boolean;
}

export type IconSetName =
  | '3Arrows'
  | '3ArrowsGray'
  | '3Flags'
  | '3TrafficLights1'
  | '3TrafficLights2'
  | '3Signs'
  | '3Symbols'
  | '3Symbols2'
  | '3Stars'
  | '3Triangles'
  | '4Arrows'
  | '4ArrowsGray'
  | '4RedToBlack'
  | '4Rating'
  | '4TrafficLights'
  | '5Arrows'
  | '5ArrowsGray'
  | '5Rating'
  | '5Quarters'
  | '5Boxes';

interface CfBase {
  /** Workbook-unique rule id (for the UI and undo). */
  id: number;
  /** Lower numbers are evaluated first. */
  priority: number;
  stopIfTrue?: boolean;
}

/** Rules that apply a differential format (`dxf` index into the style table). */
export type CfRule = CfBase &
  (
    | { type: 'cellIs'; operator: CfOperator; formulas: string[]; dxf: number }
    | { type: 'expression'; formula: string; dxf: number }
    | {
        type: 'containsText' | 'notContainsText' | 'beginsWith' | 'endsWith';
        text: string;
        dxf: number;
      }
    | {
        type: 'containsBlanks' | 'notContainsBlanks' | 'containsErrors' | 'notContainsErrors';
        dxf: number;
      }
    | { type: 'timePeriod'; period: TimePeriod; dxf: number }
    | { type: 'duplicateValues' | 'uniqueValues'; dxf: number }
    | { type: 'top10'; rank: number; percent?: boolean; bottom?: boolean; dxf: number }
    | {
        type: 'aboveAverage';
        aboveAverage: boolean;
        equalAverage?: boolean;
        /** Number of standard deviations (1–3) for "above/below N std dev". */
        stdDev?: number;
        dxf: number;
      }
    | { type: 'colorScale'; cfvos: Cfvo[]; colors: Color[] }
    | {
        type: 'dataBar';
        min: Cfvo;
        max: Cfvo;
        color: Color;
        showValue?: boolean;
        minLength?: number;
        maxLength?: number;
        gradient?: boolean;
        borderColor?: Color;
        negativeColor?: Color;
        axisColor?: Color;
        axisPosition?: 'automatic' | 'middle' | 'none';
        direction?: 'context' | 'leftToRight' | 'rightToLeft';
      }
    | {
        type: 'iconSet';
        iconSet: IconSetName;
        cfvos: Cfvo[];
        reverse?: boolean;
        showValue?: boolean;
        customIcons?: { iconSet: IconSetName; iconId: number }[];
      }
  );

export type CfRuleType = CfRule['type'];

/** One `conditionalFormatting` block: rules applied to a list of ranges. */
export interface ConditionalFormat {
  ranges: CellRange[];
  rules: CfRule[];
}

// ── data validation ───────────────────────────────────────────────────────

export type ValidationType = 'any' | 'whole' | 'decimal' | 'list' | 'date' | 'time' | 'textLength' | 'custom';

export interface DataValidation {
  id: number;
  ranges: CellRange[];
  type: ValidationType;
  operator?: CfOperator;
  /** Criterion formula/value without `=` (a list source such as `"Yes,No"` or `$A$1:$A$5`). */
  formula1?: string;
  formula2?: string;
  allowBlank?: boolean;
  /** Show the in-cell dropdown for list validation (default true). */
  inCellDropdown?: boolean;
  showInputMessage?: boolean;
  promptTitle?: string;
  prompt?: string;
  showErrorMessage?: boolean;
  errorStyle?: 'stop' | 'warning' | 'information';
  errorTitle?: string;
  error?: string;
}

// ── hyperlinks and comments ───────────────────────────────────────────────

export interface Hyperlink {
  ref: CellRange;
  /** External target (URL, mailto:, file path). */
  target?: string;
  /** Location inside the workbook, e.g. `Sheet2!A1` or a defined name. */
  location?: string;
  display?: string;
  tooltip?: string;
}

/** A note (legacy comment): the yellow sticky note. */
export interface Note {
  row: number;
  col: number;
  author: string;
  text: string;
  visible?: boolean;
  /** Box size in px when shown. */
  width?: number;
  height?: number;
}

/** A threaded (modern) comment; replies point at the root with `parentId`. */
export interface ThreadedComment {
  id: string;
  row: number;
  col: number;
  author: string;
  authorId?: string;
  /** ISO 8601 timestamp. */
  date: string;
  text: string;
  parentId?: string;
  done?: boolean;
  mentions?: { personId: string; start: number; length: number }[];
}

// ── views ─────────────────────────────────────────────────────────────────

export interface Selection {
  ranges: CellRange[];
  active: CellRef;
}

export interface SheetView {
  /** Frozen rows/columns (Freeze Panes): number of rows above and columns left of the split. */
  freeze?: { rows: number; cols: number };
  /** Unfrozen split (View › Split): pane sizes in px at 100% zoom. */
  split?: { x: number; y: number };
  topLeft: CellRef;
  /** Percent, 10–400. */
  zoom: number;
  view: 'normal' | 'pageBreakPreview' | 'pageLayout';
  selection: Selection;
  showGridlines: boolean;
  showHeadings: boolean;
  showZeros: boolean;
  showFormulas: boolean;
  showRuler: boolean;
  rightToLeft: boolean;
  gridlineColor?: Color;
}

export function defaultSheetView(): SheetView {
  return {
    topLeft: { row: 0, col: 0 },
    zoom: 100,
    view: 'normal',
    selection: { ranges: [{ r0: 0, c0: 0, r1: 0, c1: 0 }], active: { row: 0, col: 0 } },
    showGridlines: true,
    showHeadings: true,
    showZeros: true,
    showFormulas: false,
    showRuler: true,
    rightToLeft: false,
  };
}

// ── print settings ────────────────────────────────────────────────────────

export interface HeaderFooter {
  oddHeader?: string;
  oddFooter?: string;
  evenHeader?: string;
  evenFooter?: string;
  firstHeader?: string;
  firstFooter?: string;
  differentOddEven?: boolean;
  differentFirst?: boolean;
  scaleWithDoc?: boolean;
  alignWithMargins?: boolean;
}

export interface PrintSettings {
  orientation: 'portrait' | 'landscape';
  /** SpreadsheetML paper size code: 1 = Letter, 9 = A4, 5 = Legal… */
  paperSize: number;
  /** Percent, 10–400 (used when not fitting to pages). */
  scale: number;
  fitToPage: boolean;
  /** Pages wide/tall when fitting (0 = automatic). */
  fitToWidth: number;
  fitToHeight: number;
  /** Margins in inches. */
  margins: { left: number; right: number; top: number; bottom: number; header: number; footer: number };
  horizontalCentered: boolean;
  verticalCentered: boolean;
  printGridlines: boolean;
  printHeadings: boolean;
  pageOrder: 'downThenOver' | 'overThenDown';
  blackAndWhite: boolean;
  draft: boolean;
  firstPageNumber?: number;
  cellComments: 'none' | 'atEnd' | 'asDisplayed';
  errors: 'displayed' | 'blank' | 'dash' | 'NA';
  headerFooter: HeaderFooter;
  /** Manual page breaks: a row break before row index n / a column break before column n. */
  rowBreaks: number[];
  colBreaks: number[];
}

export function defaultPrintSettings(): PrintSettings {
  return {
    orientation: 'portrait',
    paperSize: 1,
    scale: 100,
    fitToPage: false,
    fitToWidth: 1,
    fitToHeight: 1,
    // Excel's "Normal" margins.
    margins: { left: 0.7, right: 0.7, top: 0.75, bottom: 0.75, header: 0.3, footer: 0.3 },
    horizontalCentered: false,
    verticalCentered: false,
    printGridlines: false,
    printHeadings: false,
    pageOrder: 'downThenOver',
    blackAndWhite: false,
    draft: false,
    cellComments: 'none',
    errors: 'displayed',
    headerFooter: {},
    rowBreaks: [],
    colBreaks: [],
  };
}

// ── drawings ──────────────────────────────────────────────────────────────

/** A cell-relative anchor point; offsets in EMU (914,400 per inch, 9,525 per px). */
export interface AnchorPoint {
  row: number;
  col: number;
  rowOff: number;
  colOff: number;
}

export type Anchor =
  | { type: 'twoCell'; from: AnchorPoint; to: AnchorPoint; editAs?: 'twoCell' | 'oneCell' | 'absolute' }
  | { type: 'oneCell'; from: AnchorPoint; ext: { cx: number; cy: number } }
  | { type: 'absolute'; pos: { x: number; y: number }; ext: { cx: number; cy: number } };

/**
 * A chart, picture or shape placed on a sheet. The object itself lives elsewhere (the chart model of
 * `@officina/charts`, an image part…) and is referenced by `ref`.
 */
export interface DrawingAnchor {
  id: string;
  kind: 'chart' | 'picture' | 'shape' | 'group' | 'connector' | 'other';
  ref: string;
  anchor: Anchor;
  name?: string;
  altText?: string;
  hidden?: boolean;
  locked?: boolean;
}

// ── workbook properties ───────────────────────────────────────────────────

export type CalcMode = 'auto' | 'autoNoTable' | 'manual';

export interface CalcProps {
  mode: CalcMode;
  /** Recalculate before saving (manual mode). */
  calcOnSave: boolean;
  fullCalcOnLoad: boolean;
  iterate: boolean;
  iterateCount: number;
  iterateDelta: number;
  /** "Set precision as displayed". */
  fullPrecision: boolean;
}

export interface WorkbookProps {
  /** Use the 1904 date system (Mac Excel legacy). */
  date1904: boolean;
  calc: CalcProps;
  /** Active sheet index (workbook order). */
  activeSheet: number;
  /** Code name and other book-level flags preserved on round trip. */
  codeName?: string;
  filterPrivacy?: boolean;
  /** Visible horizontal position of the sheet tab strip (0–1000‰ of the window). */
  tabRatio?: number;
}

export function defaultCalcProps(): CalcProps {
  return {
    mode: 'auto',
    calcOnSave: true,
    fullCalcOnLoad: false,
    iterate: false,
    iterateCount: 100,
    iterateDelta: 0.001,
    fullPrecision: true,
  };
}

export type SheetVisibility = 'visible' | 'hidden' | 'veryHidden';
