/**
 * Property-level merging used by style resolution. Word inherits each attribute independently
 * (e.g. `w:ind/@w:left` from one level and `@w:firstLine` from another), so nested objects merge
 * per field, with the WordprocessingML exclusivity rules applied:
 *
 * - `firstLine` and `hanging` are mutually exclusive — setting one clears the other;
 * - an explicit font name replaces a theme font reference for the same slot and vice versa;
 * - tab stops accumulate, and a `clear` tab at a position removes an inherited stop there;
 * - borders merge per edge; colours, shading and underline replace as a whole.
 */
import type {
  Border,
  CellBorders,
  Indent,
  ParagraphBorders,
  ParagraphProps,
  RunFonts,
  RunProps,
  TableBorders,
  TableCellProps,
  TableProps,
  TabStop,
} from '../types.ts';

type Mutable<T> = { -readonly [K in keyof T]: T[K] };

export function mergeIndent(base: Indent | undefined, over: Indent | undefined): Indent | undefined {
  if (!over) return base;
  const out: Mutable<Indent> = { ...base };
  if (over.left !== undefined) out.left = over.left;
  if (over.right !== undefined) out.right = over.right;
  if (over.firstLine !== undefined) {
    out.firstLine = over.firstLine;
    delete out.hanging;
  }
  if (over.hanging !== undefined) {
    out.hanging = over.hanging;
    delete out.firstLine;
  }
  return out;
}

/** Applies tab stops: same-position stops are replaced; `clear` removes; result sorted by position. */
export function mergeTabs(base: TabStop[] | undefined, over: TabStop[] | undefined): TabStop[] | undefined {
  if (!over) return base;
  const map = new Map<number, TabStop>();
  for (const t of base ?? []) map.set(t.pos, t);
  for (const t of over) {
    if (t.align === 'clear') map.delete(t.pos);
    else map.set(t.pos, t);
  }
  return [...map.values()].sort((a, b) => a.pos - b.pos);
}

function mergeEdges<T extends object>(base: T | undefined, over: T | undefined): T | undefined {
  if (!over) return base;
  const out = { ...base } as Record<string, Border | undefined>;
  for (const [k, v] of Object.entries(over)) if (v !== undefined) out[k] = v as Border;
  return out as T;
}

export const mergeParagraphBorders = (a?: ParagraphBorders, b?: ParagraphBorders) => mergeEdges(a, b);
export const mergeTableBorders = (a?: TableBorders, b?: TableBorders) => mergeEdges(a, b);
export const mergeCellBorders = (a?: CellBorders, b?: CellBorders) => mergeEdges(a, b);

/** Merges paragraph properties; `over` wins per attribute. `styleId` is never inherited. */
export function mergeParagraphProps(base: ParagraphProps, over: ParagraphProps | undefined): ParagraphProps {
  if (!over) return base;
  const out: ParagraphProps = { ...base };
  for (const [key, value] of Object.entries(over) as [keyof ParagraphProps, unknown][]) {
    if (value === undefined || value === null) continue;
    switch (key) {
      case 'styleId':
        break;
      case 'indent':
        out.indent = mergeIndent(out.indent, value as Indent);
        break;
      case 'spacing':
        out.spacing = { ...out.spacing, ...stripUndefined(value as object) };
        break;
      case 'numbering':
        out.numbering = {
          ...out.numbering,
          ...stripUndefined(value as object),
        } as ParagraphProps['numbering'];
        break;
      case 'tabs':
        out.tabs = mergeTabs(out.tabs, value as TabStop[]);
        break;
      case 'borders':
        out.borders = mergeParagraphBorders(out.borders, value as ParagraphBorders);
        break;
      default:
        (out as Record<string, unknown>)[key] = value;
    }
  }
  return out;
}

export function mergeFonts(base: RunFonts | undefined, over: RunFonts | undefined): RunFonts | undefined {
  if (!over) return base;
  const out: RunFonts = { ...base };
  for (const slot of ['ascii', 'hAnsi', 'eastAsia', 'cs'] as const) {
    const theme = `${slot}Theme` as const;
    if (over[theme] !== undefined) {
      out[theme] = over[theme];
      delete out[slot];
      if (over[slot] !== undefined) out[slot] = over[slot]; // cached name kept alongside the theme ref
    } else if (over[slot] !== undefined) {
      out[slot] = over[slot];
      delete out[theme];
    }
  }
  return out;
}

/**
 * Merges run properties with plain override semantics (`over` wins). Toggle-property XOR between
 * style types is handled by the resolver, not here. `styleId` is never inherited.
 */
export function mergeRunProps(base: RunProps, over: RunProps | undefined): RunProps {
  if (!over) return base;
  const out: RunProps = { ...base };
  for (const [key, value] of Object.entries(over) as [keyof RunProps, unknown][]) {
    if (value === undefined || value === null) continue;
    if (key === 'styleId') continue;
    if (key === 'fonts') out.fonts = mergeFonts(out.fonts, value as RunFonts);
    else if (key === 'lang') out.lang = { ...out.lang, ...(value as object) };
    else (out as Record<string, unknown>)[key] = value;
  }
  return out;
}

export function mergeTableProps(base: TableProps, over: TableProps | undefined): TableProps {
  if (!over) return base;
  const out: TableProps = { ...base };
  for (const [key, value] of Object.entries(over) as [keyof TableProps, unknown][]) {
    if (value === undefined || value === null || key === 'styleId') continue;
    if (key === 'borders') out.borders = mergeTableBorders(out.borders, value as TableBorders);
    else if (key === 'cellMargins')
      out.cellMargins = { ...out.cellMargins, ...stripUndefined(value as object) };
    else (out as Record<string, unknown>)[key] = value;
  }
  return out;
}

export function mergeCellProps(base: TableCellProps, over: TableCellProps | undefined): TableCellProps {
  if (!over) return base;
  const out: TableCellProps = { ...base };
  for (const [key, value] of Object.entries(over) as [keyof TableCellProps, unknown][]) {
    if (value === undefined || value === null) continue;
    if (key === 'borders') out.borders = mergeCellBorders(out.borders, value as CellBorders);
    else if (key === 'margins') out.margins = { ...out.margins, ...stripUndefined(value as object) };
    else (out as Record<string, unknown>)[key] = value;
  }
  return out;
}

export function stripUndefined<T extends object>(o: T): Partial<T> {
  const out: Partial<T> = {};
  for (const [k, v] of Object.entries(o))
    if (v !== undefined && v !== null) (out as Record<string, unknown>)[k] = v;
  return out;
}
