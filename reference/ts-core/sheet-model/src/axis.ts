/**
 * Row and column properties (sizes, hidden, outline levels, formats) and the pixel geometry derived
 * from them.
 *
 * Units follow SpreadsheetML so files round-trip exactly:
 * - row heights are in points (Excel's default is 15 pt = 20 px at 96 dpi);
 * - column widths are in "character" units of the workbook's default font *including* the 5 px cell
 *   padding (`<col width>`), i.e. the default 8.43-character column is stored as 9.140625 and is
 *   64 px wide with Calibri 11 (maximum digit width 7 px). See ECMA-376 Part 1 §18.3.1.13.
 */
import { MAX_COLS, MAX_ROWS } from './address.ts';
import type { Font } from './styles.ts';

export interface RowProps {
  /** Height in points (custom or auto-fitted); undefined = sheet default. */
  height?: number;
  customHeight?: boolean;
  hidden?: boolean;
  /** Hidden by an AutoFilter (as opposed to manually); SUBTOTAL 1–11 still skips only these. */
  filtered?: boolean;
  outlineLevel?: number;
  collapsed?: boolean;
  /** Row format (`cellXfs` index) applied to empty cells in the row. */
  style?: number;
  thickTop?: boolean;
  thickBot?: boolean;
}

export interface ColProps {
  /** Width in SpreadsheetML character units (padding included); undefined = sheet default. */
  width?: number;
  customWidth?: boolean;
  bestFit?: boolean;
  hidden?: boolean;
  outlineLevel?: number;
  collapsed?: boolean;
  /** Column format (`cellXfs` index) applied to empty cells in the column. */
  style?: number;
}

// ── font metrics ──────────────────────────────────────────────────────────

/** Supplies the maximum digit width (px at 96 dpi) of a font; the grid can inject canvas measurements. */
export interface FontMetrics {
  maxDigitWidth(font: Pick<Font, 'name' | 'size'>): number;
}

/** Advance width of the widest digit as a fraction of the em, for common spreadsheet fonts. */
const DIGIT_ADVANCE: Record<string, number> = {
  calibri: 0.5068,
  carlito: 0.5068,
  'calibri light': 0.5068,
  aptos: 0.553,
  'aptos narrow': 0.48,
  arial: 0.556,
  'liberation sans': 0.556,
  helvetica: 0.556,
  'arial narrow': 0.456,
  'times new roman': 0.5,
  'liberation serif': 0.5,
  tinos: 0.5,
  cambria: 0.5537,
  caladea: 0.5537,
  georgia: 0.6157,
  verdana: 0.6362,
  tahoma: 0.5459,
  'segoe ui': 0.5537,
  consolas: 0.5498,
  'courier new': 0.6001,
  'liberation mono': 0.6001,
  inter: 0.62,
  'jetbrains mono': 0.6,
};

/** Built-in metrics: digit-advance table × pixel size, rounded like Excel's GDI measurement. */
export const DEFAULT_FONT_METRICS: FontMetrics = {
  maxDigitWidth(font) {
    const ratio = DIGIT_ADVANCE[font.name.toLowerCase()] ?? 0.55;
    return Math.max(1, Math.round(ptToPx(font.size) * ratio));
  },
};

// ── unit conversions ──────────────────────────────────────────────────────

export const DEFAULT_ROW_HEIGHT_PT = 15;
/** Excel's default column width in characters. */
export const DEFAULT_COL_CHARS = 8.43;
export const BASE_COL_WIDTH = 8;

export function ptToPx(pt: number): number {
  return (pt * 96) / 72;
}

export function pxToPt(px: number): number {
  return (px * 72) / 96;
}

/** Characters (as typed in Column Width…) → stored width, for a maximum digit width in px. */
export function charsToColWidth(chars: number, mdw: number): number {
  return Math.trunc(((chars * mdw + 5) / mdw) * 256) / 256;
}

/** Stored width → characters shown in the Column Width dialog. */
export function colWidthToChars(width: number, mdw: number): number {
  return Math.trunc(((width * mdw - 5) / mdw) * 100 + 0.5) / 100;
}

/** Stored width → pixels. */
export function colWidthToPx(width: number, mdw: number): number {
  return Math.trunc(((256 * width + Math.trunc(128 / mdw)) / 256) * mdw);
}

/** Pixels → stored width. */
export function pxToColWidth(px: number, mdw: number): number {
  return Math.trunc((px / mdw) * 256) / 256;
}

/**
 * Default column width (stored units) for a base width in characters: base × digit width plus 5 px
 * padding, rounded up to a multiple of 8 px (ECMA-376 §18.3.1.81) — 64 px / 9.140625 for Calibri 11.
 */
export function defaultColWidthFor(baseChars: number, mdw: number): number {
  return pxToColWidth(Math.ceil((baseChars * mdw + 5) / 8) * 8, mdw);
}

/** Row height in points → pixels (Excel rounds to whole pixels). */
export function rowHeightToPx(pt: number): number {
  return Math.round(ptToPx(pt));
}

// ── axis ──────────────────────────────────────────────────────────────────

/**
 * Sparse per-index properties along one axis plus O(log n) pixel geometry (offset of an index, index
 * at an offset) for the virtualised renderer. Only indices with non-default sizes are tracked in the
 * prefix sums, so a million default rows cost nothing.
 */
export class Axis<P extends RowProps | ColProps> {
  private readonly props = new Map<number, P>();
  private geo: { idx: Int32Array; delta: Float64Array; prefix: Float64Array } | null = null;

  constructor(
    readonly length: number,
    private readonly sizeOf: (p: P) => number | undefined,
    public defaultSize: number,
  ) {}

  get(i: number): Readonly<P> | undefined {
    return this.props.get(i);
  }

  /** Replaces the properties of an index (undefined or `{}` clears them). */
  set(i: number, p: P | undefined): void {
    if (!p || Object.keys(p).every((k) => (p as Record<string, unknown>)[k] === undefined))
      this.props.delete(i);
    else {
      const clean = {} as Record<string, unknown>;
      for (const [k, v] of Object.entries(p)) if (v !== undefined) clean[k] = v;
      this.props.set(i, clean as P);
    }
    this.geo = null;
  }

  patch(i: number, p: Partial<P>): void {
    this.set(i, { ...(this.props.get(i) ?? ({} as P)), ...p });
  }

  /** Indices with properties, ascending. */
  indices(): number[] {
    return [...this.props.keys()].sort((a, b) => a - b);
  }

  entries(): [number, P][] {
    return this.indices().map((i) => [i, this.props.get(i) as P]);
  }

  get count(): number {
    return this.props.size;
  }

  isHidden(i: number): boolean {
    return this.props.get(i)?.hidden === true;
  }

  outlineLevel(i: number): number {
    return this.props.get(i)?.outlineLevel ?? 0;
  }

  /** Size of an index in px (0 when hidden). */
  sizePx(i: number): number {
    const p = this.props.get(i);
    if (!p) return this.defaultSize;
    if (p.hidden) return 0;
    return this.sizeOf(p) ?? this.defaultSize;
  }

  private geometry() {
    if (this.geo) return this.geo;
    const idx: number[] = [];
    const delta: number[] = [];
    for (const i of this.indices()) {
      const d = this.sizePx(i) - this.defaultSize;
      if (d !== 0) {
        idx.push(i);
        delta.push(d);
      }
    }
    const prefix = new Float64Array(idx.length + 1);
    for (let k = 0; k < delta.length; k++) prefix[k + 1] = (prefix[k] as number) + (delta[k] as number);
    this.geo = { idx: Int32Array.from(idx), delta: Float64Array.from(delta), prefix };
    return this.geo;
  }

  /** Pixel offset of the start of index `i` (sum of the sizes of 0..i-1). */
  offsetPx(i: number): number {
    const g = this.geometry();
    let lo = 0;
    let hi = g.idx.length;
    while (lo < hi) {
      const mid = (lo + hi) >>> 1;
      if ((g.idx[mid] as number) < i) lo = mid + 1;
      else hi = mid;
    }
    return i * this.defaultSize + (g.prefix[lo] as number);
  }

  /** Total size of all indices. */
  totalPx(): number {
    return this.offsetPx(this.length);
  }

  /** The index whose span contains pixel offset `px` (clamped to the axis; skips hidden indices). */
  indexAtPx(px: number): number {
    if (px <= 0) return this.firstVisible(0);
    let lo = 0;
    let hi = this.length - 1;
    while (lo < hi) {
      const mid = (lo + hi + 1) >>> 1;
      if (this.offsetPx(mid) <= px) lo = mid;
      else hi = mid - 1;
    }
    // A zero-size (hidden) index shares its offset with the next visible one.
    return this.firstVisible(lo);
  }

  private firstVisible(i: number): number {
    let j = i;
    while (j < this.length - 1 && this.sizePx(j) === 0) j++;
    return j;
  }

  /** Shifts for an insert (count > 0) or delete (count < 0) at `at`. */
  shift(at: number, count: number): void {
    if (count === 0) return;
    const moved: [number, P][] = [];
    for (const [i, p] of this.props) {
      if (i < at) continue;
      this.props.delete(i);
      if (count < 0 && i < at - count) continue;
      const n = i + count;
      if (n < this.length) moved.push([n, p]);
    }
    for (const [i, p] of moved) this.props.set(i, p);
    this.geo = null;
  }

  /** Plain copy of the sparse properties. */
  toJSON(): [number, P][] {
    return this.entries().map(([i, p]) => [i, { ...p }]);
  }

  load(entries: readonly (readonly [number, P])[]): void {
    this.props.clear();
    for (const [i, p] of entries) this.props.set(i, { ...p });
    this.geo = null;
  }

  /** Changes the default size (e.g. after the default font changes). */
  setDefaultSize(px: number): void {
    this.defaultSize = px;
    this.geo = null;
  }
}

export function createRowAxis(defaultHeightPt = DEFAULT_ROW_HEIGHT_PT): Axis<RowProps> {
  return new Axis<RowProps>(
    MAX_ROWS,
    (p) => (p.height === undefined ? undefined : rowHeightToPx(p.height)),
    rowHeightToPx(defaultHeightPt),
  );
}

export function createColAxis(defaultWidth: number, mdw: number): Axis<ColProps> {
  return new Axis<ColProps>(
    MAX_COLS,
    (p) => (p.width === undefined ? undefined : colWidthToPx(p.width, mdw)),
    colWidthToPx(defaultWidth, mdw),
  );
}
