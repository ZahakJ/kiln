/**
 * Cell formatting: fonts, fills, borders, alignment, protection and number formats, deduplicated in
 * a {@link StyleTable} the way SpreadsheetML's `styles.xml` does it (`fonts`, `fills`, `borders`,
 * `numFmts` and `cellXfs` combining them). Cells refer to a `cellXfs` index; index 0 is the
 * workbook default ("Normal"). Differential formats (`dxfs`, used by conditional formatting and table
 * styles) live in the same table.
 *
 * Every component is interned by a canonical hash, so equal formats always share one index and
 * applying the same formatting twice never grows the table.
 */
import { ModelError } from './address.ts';

// ── colours ───────────────────────────────────────────────────────────────

/** A SpreadsheetML colour: ARGB, theme slot (+ tint), legacy indexed palette or automatic. */
export type Color =
  | { rgb: string; tint?: number }
  | { theme: number; tint?: number }
  | { indexed: number; tint?: number }
  | { auto: true };

export function rgb(hex: string): Color {
  const h = hex.replace(/^#/, '').toUpperCase();
  if (!/^([0-9A-F]{6}|[0-9A-F]{8})$/.test(h)) throw new ModelError(`Invalid colour "${hex}"`);
  return { rgb: h.length === 6 ? `FF${h}` : h };
}

export function themeColor(theme: number, tint?: number): Color {
  return tint === undefined || tint === 0 ? { theme } : { theme, tint };
}

// ── components ────────────────────────────────────────────────────────────

export type UnderlineStyle = 'none' | 'single' | 'double' | 'singleAccounting' | 'doubleAccounting';

export interface Font {
  name: string;
  /** Size in points. */
  size: number;
  bold?: boolean;
  italic?: boolean;
  underline?: UnderlineStyle;
  strike?: boolean;
  color?: Color;
  vertAlign?: 'superscript' | 'subscript';
  /** Theme font scheme (`minor` = body font, `major` = headings). */
  scheme?: 'minor' | 'major';
  family?: number;
  charset?: number;
  outline?: boolean;
  shadow?: boolean;
  condense?: boolean;
  extend?: boolean;
}

export type PatternType =
  | 'none'
  | 'solid'
  | 'mediumGray'
  | 'darkGray'
  | 'lightGray'
  | 'darkHorizontal'
  | 'darkVertical'
  | 'darkDown'
  | 'darkUp'
  | 'darkGrid'
  | 'darkTrellis'
  | 'lightHorizontal'
  | 'lightVertical'
  | 'lightDown'
  | 'lightUp'
  | 'lightGrid'
  | 'lightTrellis'
  | 'gray125'
  | 'gray0625';

export interface GradientStop {
  position: number;
  color: Color;
}

export type Fill =
  | { type: 'pattern'; pattern: PatternType; fg?: Color; bg?: Color }
  | {
      type: 'gradient';
      gradient: 'linear' | 'path';
      degree?: number;
      left?: number;
      right?: number;
      top?: number;
      bottom?: number;
      stops: GradientStop[];
    };

export type BorderStyle =
  | 'none'
  | 'thin'
  | 'medium'
  | 'dashed'
  | 'dotted'
  | 'thick'
  | 'double'
  | 'hair'
  | 'mediumDashed'
  | 'dashDot'
  | 'mediumDashDot'
  | 'dashDotDot'
  | 'mediumDashDotDot'
  | 'slantDashDot';

export const BORDER_STYLES: readonly BorderStyle[] = [
  'none',
  'thin',
  'medium',
  'dashed',
  'dotted',
  'thick',
  'double',
  'hair',
  'mediumDashed',
  'dashDot',
  'mediumDashDot',
  'dashDotDot',
  'mediumDashDotDot',
  'slantDashDot',
];

export interface BorderSide {
  style: BorderStyle;
  color?: Color;
}

export interface Border {
  left?: BorderSide;
  right?: BorderSide;
  top?: BorderSide;
  bottom?: BorderSide;
  diagonal?: BorderSide;
  diagonalUp?: boolean;
  diagonalDown?: boolean;
}

export type HorizontalAlign =
  | 'general'
  | 'left'
  | 'center'
  | 'right'
  | 'fill'
  | 'justify'
  | 'centerContinuous'
  | 'distributed';

export type VerticalAlign = 'top' | 'center' | 'bottom' | 'justify' | 'distributed';

export interface Alignment {
  horizontal?: HorizontalAlign;
  vertical?: VerticalAlign;
  wrapText?: boolean;
  shrinkToFit?: boolean;
  /** Indent level (each level is about three characters wide). */
  indent?: number;
  /** 0–90 counter-clockwise, 91–180 clockwise (90 + degrees), 255 = stacked vertical text. */
  textRotation?: number;
  /** 0 = context, 1 = left-to-right, 2 = right-to-left. */
  readingOrder?: 0 | 1 | 2;
  justifyLastLine?: boolean;
}

export interface Protection {
  locked?: boolean;
  hidden?: boolean;
}

/** A fully specified cell format (one `cellXfs` entry, resolved). */
export interface CellStyle {
  numFmt: string;
  font: Font;
  fill: Fill;
  border: Border;
  alignment: Alignment;
  protection: Protection;
  /** Index into the named cell styles (`cellStyleXfs`); 0 = Normal. */
  styleId: number;
  /** Quote prefix (`'` typed in front of a value to keep it as text). */
  quotePrefix?: boolean;
}

/** A partial change applied to a cell format (Home ribbon commands, Format Cells dialog). */
export interface StylePatch {
  numFmt?: string;
  font?: Partial<Font>;
  fill?: Fill | null;
  border?: { [K in keyof Border]?: Border[K] | null };
  alignment?: { [K in keyof Alignment]?: Alignment[K] | null };
  protection?: Partial<Protection>;
  styleId?: number;
  quotePrefix?: boolean;
}

/** A differential format (conditional formatting, table styles): only what it sets. */
export interface Dxf {
  numFmt?: string;
  font?: Partial<Font>;
  fill?: Fill;
  border?: Border;
  alignment?: Alignment;
  protection?: Protection;
}

/** A named cell style (Home › Cell Styles): "Normal", "Good", "Heading 1"… */
export interface NamedCellStyle {
  name: string;
  /** Excel's built-in style id (0 = Normal) when this is a built-in style. */
  builtinId?: number;
  /** The `cellXfs`-style index holding the style's format. */
  xf: number;
  hidden?: boolean;
  customBuiltin?: boolean;
}

// ── built-in number formats ───────────────────────────────────────────────

/** SpreadsheetML's implicit number formats (ECMA-376 Part 1 §18.8.30), en-US. */
export const BUILTIN_NUMFMTS: Readonly<Record<number, string>> = {
  0: 'General',
  1: '0',
  2: '0.00',
  3: '#,##0',
  4: '#,##0.00',
  5: '"$"#,##0_);\\("$"#,##0\\)',
  6: '"$"#,##0_);[Red]\\("$"#,##0\\)',
  7: '"$"#,##0.00_);\\("$"#,##0.00\\)',
  8: '"$"#,##0.00_);[Red]\\("$"#,##0.00\\)',
  9: '0%',
  10: '0.00%',
  11: '0.00E+00',
  12: '# ?/?',
  13: '# ??/??',
  14: 'm/d/yyyy',
  15: 'd-mmm-yy',
  16: 'd-mmm',
  17: 'mmm-yy',
  18: 'h:mm AM/PM',
  19: 'h:mm:ss AM/PM',
  20: 'h:mm',
  21: 'h:mm:ss',
  22: 'm/d/yyyy h:mm',
  37: '#,##0_);(#,##0)',
  38: '#,##0_);[Red](#,##0)',
  39: '#,##0.00_);(#,##0.00)',
  40: '#,##0.00_);[Red](#,##0.00)',
  41: '_(* #,##0_);_(* \\(#,##0\\);_(* "-"_);_(@_)',
  42: '_("$"* #,##0_);_("$"* \\(#,##0\\);_("$"* "-"_);_(@_)',
  43: '_(* #,##0.00_);_(* \\(#,##0.00\\);_(* "-"??_);_(@_)',
  44: '_("$"* #,##0.00_);_("$"* \\(#,##0.00\\);_("$"* "-"??_);_(@_)',
  45: 'mm:ss',
  46: '[h]:mm:ss',
  47: 'mm:ss.0',
  48: '##0.0E+0',
  49: '@',
};

const BUILTIN_BY_CODE = new Map(Object.entries(BUILTIN_NUMFMTS).map(([id, code]) => [code, Number(id)]));

/** The format code for a numFmtId (built-in ids only; custom ids come from the file). */
export function builtinNumFmt(id: number): string | undefined {
  return BUILTIN_NUMFMTS[id];
}

/** The built-in numFmtId for a format code, if it is one. */
export function builtinNumFmtId(code: string): number | undefined {
  return BUILTIN_BY_CODE.get(code);
}

// ── defaults ──────────────────────────────────────────────────────────────

/** Excel 2016–2021 default body font (Calibri 11, theme minor font, text colour theme 1). */
export const DEFAULT_FONT: Font = Object.freeze({
  name: 'Calibri',
  size: 11,
  color: { theme: 1 },
  family: 2,
  scheme: 'minor',
}) as Font;

export const NO_FILL: Fill = Object.freeze({ type: 'pattern', pattern: 'none' }) as Fill;
/** The mandatory second fill of every styles.xml. */
export const GRAY125_FILL: Fill = Object.freeze({ type: 'pattern', pattern: 'gray125' }) as Fill;

export function defaultCellStyle(font: Font = DEFAULT_FONT): CellStyle {
  return {
    numFmt: 'General',
    font: { ...font },
    fill: { ...NO_FILL },
    border: {},
    alignment: {},
    protection: {},
    styleId: 0,
  };
}

// ── canonical hashing ─────────────────────────────────────────────────────

/** Canonical JSON with sorted keys and `undefined`/default members dropped. */
export function canonicalKey(v: unknown): string {
  if (v === null || typeof v !== 'object') return JSON.stringify(v) ?? 'null';
  if (Array.isArray(v)) return `[${v.map(canonicalKey).join(',')}]`;
  const o = v as Record<string, unknown>;
  const keys = Object.keys(o)
    .filter((k) => o[k] !== undefined)
    .sort();
  return `{${keys.map((k) => `${JSON.stringify(k)}:${canonicalKey(o[k])}`).join(',')}}`;
}

function clean<T extends object>(o: T): T {
  const out = {} as Record<string, unknown>;
  for (const [k, v] of Object.entries(o)) {
    if (v === undefined || v === null) continue;
    out[k] = v;
  }
  return out as T;
}

/** Normalises a font: drops falsy flags so `{bold:false}` and `{}` hash the same. */
function normFont(f: Font): Font {
  const o = clean({ ...f });
  for (const k of ['bold', 'italic', 'strike', 'outline', 'shadow', 'condense', 'extend'] as const)
    if (!o[k]) delete o[k];
  if (o.underline === 'none') delete o.underline;
  return o;
}

function normFill(f: Fill): Fill {
  if (f.type === 'pattern' && f.pattern === 'none') return { type: 'pattern', pattern: 'none' };
  return clean({ ...f });
}

function normBorder(b: Border): Border {
  const o: Border = {};
  for (const side of ['left', 'right', 'top', 'bottom', 'diagonal'] as const) {
    const s = b[side];
    if (s && s.style !== 'none') o[side] = clean({ ...s });
  }
  if (b.diagonalUp) o.diagonalUp = true;
  if (b.diagonalDown) o.diagonalDown = true;
  return o;
}

function normAlignment(a: Alignment): Alignment {
  const o = clean({ ...a });
  if (o.horizontal === 'general') delete o.horizontal;
  if (o.vertical === 'bottom') delete o.vertical;
  if (!o.wrapText) delete o.wrapText;
  if (!o.shrinkToFit) delete o.shrinkToFit;
  if (!o.indent) delete o.indent;
  if (!o.textRotation) delete o.textRotation;
  if (!o.readingOrder) delete o.readingOrder;
  if (!o.justifyLastLine) delete o.justifyLastLine;
  return o;
}

function normProtection(p: Protection): Protection {
  const o: Protection = {};
  if (p.locked === false) o.locked = false;
  if (p.hidden === true) o.hidden = true;
  return o;
}

/** An interning table: equal items share one index. */
class Interner<T> {
  readonly items: T[] = [];
  private readonly keys = new Map<string, number>();

  intern(item: T): number {
    const key = canonicalKey(item);
    const hit = this.keys.get(key);
    if (hit !== undefined) return hit;
    const idx = this.items.length;
    this.items.push(Object.freeze(item));
    this.keys.set(key, idx);
    return idx;
  }

  /** Appends without deduplication (files may contain duplicates we must keep for index stability). */
  push(item: T): number {
    const idx = this.items.length;
    this.items.push(Object.freeze(item));
    const key = canonicalKey(item);
    if (!this.keys.has(key)) this.keys.set(key, idx);
    return idx;
  }

  get(i: number): T {
    const v = this.items[i];
    if (v === undefined) throw new ModelError(`Style component index ${i} is out of range`);
    return v;
  }
}

/** One `cellXfs` entry: component indices plus inline alignment/protection. */
export interface Xf {
  font: number;
  fill: number;
  border: number;
  numFmt: string;
  alignment: Alignment;
  protection: Protection;
  styleId: number;
  quotePrefix?: boolean;
}

export interface StyleTableJSON {
  fonts: Font[];
  fills: Fill[];
  borders: Border[];
  xfs: Xf[];
  dxfs: Dxf[];
  namedStyles: NamedCellStyle[];
}

/** The workbook's style table. */
export class StyleTable {
  private readonly fonts = new Interner<Font>();
  private readonly fills = new Interner<Fill>();
  private readonly borders = new Interner<Border>();
  private readonly xfs = new Interner<Xf>();
  private readonly dxfTable = new Interner<Dxf>();
  private readonly resolved: (CellStyle | undefined)[] = [];
  namedStyles: NamedCellStyle[] = [];

  constructor(defaultFont: Font = DEFAULT_FONT) {
    this.fonts.intern(normFont({ ...defaultFont }));
    this.fills.intern(normFill(NO_FILL));
    this.fills.intern(normFill(GRAY125_FILL));
    this.borders.intern({});
    this.xfs.intern({
      font: 0,
      fill: 0,
      border: 0,
      numFmt: 'General',
      alignment: {},
      protection: {},
      styleId: 0,
    });
    this.namedStyles.push({ name: 'Normal', builtinId: 0, xf: 0 });
  }

  /** Number of `cellXfs` entries. */
  get count(): number {
    return this.xfs.items.length;
  }

  get dxfCount(): number {
    return this.dxfTable.items.length;
  }

  /** The default (index 0) font. */
  get defaultFont(): Font {
    return this.fonts.get(0);
  }

  /** The resolved format of a `cellXfs` index (cached, frozen). */
  get(index: number): CellStyle {
    const hit = this.resolved[index];
    if (hit) return hit;
    const xf = this.xfs.get(index);
    const style: CellStyle = {
      numFmt: xf.numFmt,
      font: this.fonts.get(xf.font),
      fill: this.fills.get(xf.fill),
      border: this.borders.get(xf.border),
      alignment: xf.alignment,
      protection: xf.protection,
      styleId: xf.styleId,
    };
    if (xf.quotePrefix) style.quotePrefix = true;
    Object.freeze(style);
    this.resolved[index] = style;
    return style;
  }

  getXf(index: number): Xf {
    return this.xfs.get(index);
  }

  has(index: number): boolean {
    return Number.isInteger(index) && index >= 0 && index < this.xfs.items.length;
  }

  /** Interns a full cell style and returns its `cellXfs` index. */
  intern(style: CellStyle): number {
    const xf: Xf = {
      font: this.fonts.intern(normFont(style.font)),
      fill: this.fills.intern(normFill(style.fill)),
      border: this.borders.intern(normBorder(style.border)),
      numFmt: style.numFmt || 'General',
      alignment: normAlignment(style.alignment),
      protection: normProtection(style.protection),
      styleId: style.styleId ?? 0,
    };
    if (style.quotePrefix) xf.quotePrefix = true;
    return this.xfs.intern(xf);
  }

  /** Applies a patch to the style at `index` and returns the index of the result. */
  patch(index: number, patch: StylePatch): number {
    const base = this.get(index);
    const next: CellStyle = {
      numFmt: patch.numFmt ?? base.numFmt,
      font: patch.font ? { ...base.font, ...patch.font } : base.font,
      fill: patch.fill === undefined ? base.fill : (patch.fill ?? NO_FILL),
      border: base.border,
      alignment: base.alignment,
      protection: patch.protection ? { ...base.protection, ...patch.protection } : base.protection,
      styleId: patch.styleId ?? base.styleId,
    };
    if (patch.border) {
      const b: Record<string, unknown> = { ...base.border };
      for (const [k, v] of Object.entries(patch.border)) {
        if (v === null) delete b[k];
        else if (v !== undefined) b[k] = v;
      }
      next.border = b as Border;
    }
    if (patch.alignment) {
      const a: Record<string, unknown> = { ...base.alignment };
      for (const [k, v] of Object.entries(patch.alignment)) {
        if (v === null) delete a[k];
        else if (v !== undefined) a[k] = v;
      }
      next.alignment = a as Alignment;
    }
    const quote = patch.quotePrefix ?? base.quotePrefix;
    if (quote) next.quotePrefix = true;
    return this.intern(next);
  }

  /** Interns a differential format. */
  internDxf(dxf: Dxf): number {
    return this.dxfTable.intern(clean({ ...dxf }));
  }

  getDxf(index: number): Dxf {
    return this.dxfTable.get(index);
  }

  /** Adds or replaces a named cell style. */
  setNamedStyle(style: NamedCellStyle): void {
    const i = this.namedStyles.findIndex((s) => s.name.toUpperCase() === style.name.toUpperCase());
    if (i >= 0) this.namedStyles[i] = style;
    else this.namedStyles.push(style);
  }

  // ── raw access for file readers/writers ────────────────────────────────

  /** Appends raw components exactly as read from a file (keeps indices stable). */
  static fromParts(parts: StyleTableJSON): StyleTable {
    const t = Object.create(StyleTable.prototype) as StyleTable;
    // biome-ignore lint/suspicious/noExplicitAny: initialising readonly fields on a bare instance
    const w = t as any;
    w.fonts = new Interner<Font>();
    w.fills = new Interner<Fill>();
    w.borders = new Interner<Border>();
    w.xfs = new Interner<Xf>();
    w.dxfTable = new Interner<Dxf>();
    w.resolved = [];
    if (
      parts.fonts.length === 0 ||
      parts.fills.length === 0 ||
      parts.borders.length === 0 ||
      parts.xfs.length === 0
    )
      throw new ModelError('A style table needs at least one font, fill, border and cell format');
    for (const f of parts.fonts) w.fonts.push(normFont(f));
    for (const f of parts.fills) w.fills.push(normFill(f));
    for (const b of parts.borders) w.borders.push(normBorder(b));
    for (const x of parts.xfs) {
      if (x.font >= parts.fonts.length || x.fill >= parts.fills.length || x.border >= parts.borders.length)
        throw new ModelError('A cell format refers to a missing font, fill or border');
      w.xfs.push({ ...x, alignment: normAlignment(x.alignment), protection: normProtection(x.protection) });
    }
    for (const d of parts.dxfs) w.dxfTable.push(clean({ ...d }));
    t.namedStyles = parts.namedStyles.map((s) => ({ ...s }));
    return t;
  }

  toJSON(): StyleTableJSON {
    return {
      fonts: [...this.fonts.items],
      fills: [...this.fills.items],
      borders: [...this.borders.items],
      xfs: [...this.xfs.items],
      dxfs: [...this.dxfTable.items],
      namedStyles: this.namedStyles.map((s) => ({ ...s })),
    };
  }
}
