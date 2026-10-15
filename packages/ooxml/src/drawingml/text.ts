/**
 * DrawingML text properties (ECMA-376 Part 1 §21.1.2): `a:bodyPr`, `a:lstStyle`, `a:pPr`/`a:lvlNpPr`
 * and `a:rPr`/`a:defRPr`/`a:endParaRPr`. Unmodelled children are preserved in `extras`.
 */
import type {
  ST_TextAlignType,
  ST_TextAnchoringType,
  ST_TextAutonumberScheme,
  ST_TextCapsType,
  ST_TextFontAlignType,
  ST_TextHorzOverflowType,
  ST_TextStrikeType,
  ST_TextUnderlineType,
  ST_TextVerticalType,
  ST_TextVertOverflowType,
  ST_TextWrappingType,
} from '../st.ts';
import { cloneNode, el, type XmlElement } from '../xml/node.ts';
import { attr, attrBool, attrInt, child, children } from '../xml/query.ts';
import { type DrawingColor, parseColorChild, serializeColor } from './color.ts';
import { type EffectList, parseEffectList, serializeEffectList } from './effects.ts';
import { FILL_ELEMENTS, type Fill, parseFill, serializeFill } from './fill.ts';
import { type Line, parseLine, serializeLine } from './line.ts';

// ------------------------------------------------------------------ bodyPr

export type Autofit =
  | { type: 'none' }
  | { type: 'shape' }
  | { type: 'normal'; fontScale: number; lnSpcReduction: number };

export interface BodyProperties {
  rot?: number;
  spcFirstLastPara?: boolean;
  vertOverflow?: ST_TextVertOverflowType;
  horzOverflow?: ST_TextHorzOverflowType;
  vert?: ST_TextVerticalType;
  wrap?: ST_TextWrappingType;
  /** Insets in EMU (defaults 91440 left/right, 45720 top/bottom when absent). */
  lIns?: number;
  tIns?: number;
  rIns?: number;
  bIns?: number;
  numCol?: number;
  spcCol?: number;
  rtlCol?: boolean;
  fromWordArt?: boolean;
  anchor?: ST_TextAnchoringType;
  anchorCtr?: boolean;
  forceAA?: boolean;
  upright?: boolean;
  compatLnSpc?: boolean;
  autofit?: Autofit;
  /** `a:prstTxWarp`, `a:scene3d`, `a:sp3d`, `a:flatTx`, `a:extLst`, preserved. */
  extras: XmlElement[];
}

export const DEFAULT_INSETS = { lIns: 91440, tIns: 45720, rIns: 91440, bIns: 45720 } as const;

const intKeys = ['rot', 'lIns', 'tIns', 'rIns', 'bIns', 'numCol', 'spcCol'] as const;
const boolKeys = [
  'spcFirstLastPara',
  'rtlCol',
  'fromWordArt',
  'anchorCtr',
  'forceAA',
  'upright',
  'compatLnSpc',
] as const;
const strKeys = ['vertOverflow', 'horzOverflow', 'vert', 'wrap', 'anchor'] as const;

export function parseBodyProperties(node: XmlElement | undefined): BodyProperties {
  const bp: BodyProperties = { extras: [] };
  if (!node) return bp;
  const rec = bp as unknown as Record<string, unknown>;
  for (const k of intKeys) {
    const v = attrInt(node, k);
    if (v !== undefined) rec[k] = v;
  }
  for (const k of boolKeys) {
    const v = attrBool(node, k);
    if (v !== undefined) rec[k] = v;
  }
  for (const k of strKeys) {
    const v = attr(node, k);
    if (v !== undefined) rec[k] = v;
  }
  for (const c of children(node)) {
    if (c.name === 'a:noAutofit') bp.autofit = { type: 'none' };
    else if (c.name === 'a:spAutoFit') bp.autofit = { type: 'shape' };
    else if (c.name === 'a:normAutofit') {
      bp.autofit = {
        type: 'normal',
        fontScale: (attrInt(c, 'fontScale') ?? 100000) / 100000,
        lnSpcReduction: (attrInt(c, 'lnSpcReduction') ?? 0) / 100000,
      };
    } else bp.extras.push(cloneNode(c));
  }
  return bp;
}

export function serializeBodyProperties(bp: BodyProperties): XmlElement {
  const a: Record<string, string | number | undefined> = {};
  const rec = bp as unknown as Record<string, unknown>;
  // Office's attribute order.
  for (const k of [
    'rot',
    'spcFirstLastPara',
    'vertOverflow',
    'horzOverflow',
    'vert',
    'wrap',
    'lIns',
    'tIns',
    'rIns',
    'bIns',
    'numCol',
    'spcCol',
    'rtlCol',
    'fromWordArt',
    'anchor',
    'anchorCtr',
    'forceAA',
    'upright',
    'compatLnSpc',
  ]) {
    const v = rec[k];
    if (v === undefined) continue;
    a[k] = typeof v === 'boolean' ? (v ? '1' : '0') : (v as string | number);
  }
  const warp = bp.extras.filter((x) => x.name === 'a:prstTxWarp');
  const rest = bp.extras.filter((x) => x.name !== 'a:prstTxWarp');
  const af = bp.autofit;
  return el(
    'a:bodyPr',
    a,
    warp.map((x) => cloneNode(x)),
    af?.type === 'none' && el('a:noAutofit'),
    af?.type === 'shape' && el('a:spAutoFit'),
    af?.type === 'normal' &&
      el('a:normAutofit', {
        fontScale: af.fontScale === 1 ? undefined : Math.round(af.fontScale * 100000),
        lnSpcReduction: af.lnSpcReduction === 0 ? undefined : Math.round(af.lnSpcReduction * 100000),
      }),
    rest.map((x) => cloneNode(x)),
  );
}

// ------------------------------------------------------------------- rPr

export interface TextFont {
  typeface: string;
  panose?: string;
  pitchFamily?: string;
  charset?: string;
}

export interface RunProperties {
  kumimoji?: boolean;
  lang?: string;
  altLang?: string;
  /** Font size in hundredths of a point. */
  sz?: number;
  b?: boolean;
  i?: boolean;
  u?: ST_TextUnderlineType;
  strike?: ST_TextStrikeType;
  /** Minimum font size for kerning, hundredths of a point. */
  kern?: number;
  cap?: ST_TextCapsType;
  /** Character spacing, hundredths of a point. */
  spc?: number;
  normalizeH?: boolean;
  /** Superscript/subscript offset, thousandths of a percent (30000 = superscript). */
  baseline?: number;
  noProof?: boolean;
  dirty?: boolean;
  err?: boolean;
  smtClean?: boolean;
  smtId?: number;
  bmk?: string;
  line?: Line;
  fill?: Fill;
  effects?: EffectList;
  highlight?: DrawingColor;
  latin?: TextFont;
  ea?: TextFont;
  cs?: TextFont;
  sym?: TextFont;
  /** `a:hlinkClick` / `a:hlinkMouseOver`, preserved (relationship ids, actions). */
  hyperlinks: XmlElement[];
  rtl?: boolean;
  /** `a:uLnTx`, `a:uLn`, `a:uFillTx`, `a:uFill`, `a:extLst`, … preserved. */
  extras: XmlElement[];
}

const RPR_INT = ['sz', 'kern', 'spc', 'baseline', 'smtId'] as const;
const RPR_BOOL = ['kumimoji', 'b', 'i', 'normalizeH', 'noProof', 'dirty', 'err', 'smtClean'] as const;
const RPR_STR = ['lang', 'altLang', 'u', 'strike', 'cap', 'bmk'] as const;
const RPR_ATTR_ORDER = [
  'kumimoji',
  'lang',
  'altLang',
  'sz',
  'b',
  'i',
  'u',
  'strike',
  'kern',
  'cap',
  'spc',
  'normalizeH',
  'baseline',
  'noProof',
  'dirty',
  'err',
  'smtClean',
  'smtId',
  'bmk',
] as const;

function parseFont(node: XmlElement | undefined): TextFont | undefined {
  if (!node) return undefined;
  const f: TextFont = { typeface: attr(node, 'typeface') ?? '' };
  for (const k of ['panose', 'pitchFamily', 'charset'] as const) {
    const v = attr(node, k);
    if (v !== undefined) f[k] = v;
  }
  return f;
}

const fontEl = (name: string, f: TextFont | undefined) =>
  f && el(name, { typeface: f.typeface, panose: f.panose, pitchFamily: f.pitchFamily, charset: f.charset });

/** Parses `a:rPr`, `a:defRPr` or `a:endParaRPr`. */
export function parseRunProperties(node: XmlElement | undefined): RunProperties {
  const rp: RunProperties = { hyperlinks: [], extras: [] };
  if (!node) return rp;
  const rec = rp as unknown as Record<string, unknown>;
  for (const k of RPR_INT) {
    const v = attrInt(node, k);
    if (v !== undefined) rec[k] = v;
  }
  for (const k of RPR_BOOL) {
    const v = attrBool(node, k);
    if (v !== undefined) rec[k] = v;
  }
  for (const k of RPR_STR) {
    const v = attr(node, k);
    if (v !== undefined) rec[k] = v;
  }
  for (const c of children(node)) {
    if ((FILL_ELEMENTS as readonly string[]).includes(c.name)) {
      const f = parseFill(c);
      if (f) rp.fill = f;
      continue;
    }
    switch (c.name) {
      case 'a:ln':
        rp.line = parseLine(c) ?? {};
        break;
      case 'a:effectLst':
        rp.effects = parseEffectList(c) ?? {};
        break;
      case 'a:highlight': {
        const h = parseColorChild(c);
        if (h) rp.highlight = h;
        break;
      }
      case 'a:latin':
      case 'a:ea':
      case 'a:cs':
      case 'a:sym': {
        const f = parseFont(c);
        if (f) rec[c.name.slice(2)] = f;
        break;
      }
      case 'a:hlinkClick':
      case 'a:hlinkMouseOver':
        rp.hyperlinks.push(cloneNode(c));
        break;
      case 'a:rtl':
        rp.rtl = attrBool(c, 'val', true) ?? true;
        break;
      default:
        rp.extras.push(cloneNode(c));
    }
  }
  return rp;
}

export function serializeRunProperties(rp: RunProperties, name = 'a:rPr'): XmlElement {
  const a: Record<string, string | number | undefined> = {};
  const rec = rp as unknown as Record<string, unknown>;
  for (const k of RPR_ATTR_ORDER) {
    const v = rec[k];
    if (v === undefined) continue;
    a[k] = typeof v === 'boolean' ? (v ? '1' : '0') : (v as string | number);
  }
  const ext = rp.extras.filter((x) => x.name === 'a:extLst');
  const underline = rp.extras.filter((x) => x.name !== 'a:extLst');
  // Schema order: ln, fill, effect, highlight, uLnTx/uLn, uFillTx/uFill, latin, ea, cs, sym, hlinkClick,
  // hlinkMouseOver, rtl, extLst.
  return el(
    name,
    a,
    rp.line && serializeLine(rp.line),
    rp.fill && serializeFill(rp.fill),
    rp.effects && serializeEffectList(rp.effects),
    rp.highlight && el('a:highlight', {}, serializeColor(rp.highlight)),
    underline.map((x) => cloneNode(x)),
    fontEl('a:latin', rp.latin),
    fontEl('a:ea', rp.ea),
    fontEl('a:cs', rp.cs),
    fontEl('a:sym', rp.sym),
    rp.hyperlinks.map((x) => cloneNode(x)),
    rp.rtl !== undefined && el('a:rtl', { val: rp.rtl ? undefined : '0' }),
    ext.map((x) => cloneNode(x)),
  );
}

// ------------------------------------------------------------------- pPr

/** Line or paragraph spacing: a percentage of single spacing, or an absolute size in hundredths of a point. */
export type TextSpacing = { pct: number } | { pts: number };

export type Bullet =
  | { type: 'none' }
  | { type: 'char'; char: string }
  | { type: 'autoNum'; scheme: ST_TextAutonumberScheme; startAt?: number }
  | { type: 'blip'; embed?: string };

export interface ParagraphProperties {
  /** Left margin, EMU. */
  marL?: number;
  marR?: number;
  /** Outline level 0–8. */
  lvl?: number;
  /** First-line indent, EMU (negative = hanging). */
  indent?: number;
  algn?: ST_TextAlignType;
  defTabSz?: number;
  rtl?: boolean;
  eaLnBrk?: boolean;
  fontAlgn?: ST_TextFontAlignType;
  latinLnBrk?: boolean;
  hangingPunct?: boolean;
  lnSpc?: TextSpacing;
  spcBef?: TextSpacing;
  spcAft?: TextSpacing;
  /** Bullet colour: `'text'` follows the text (`a:buClrTx`). */
  buClr?: DrawingColor | 'text';
  /** Bullet size: `'text'`, a percentage (`{ pct }`) or points (`{ pts }` in hundredths). */
  buSz?: 'text' | TextSpacing;
  /** Bullet font: `'text'` or a font. */
  buFont?: 'text' | TextFont;
  bullet?: Bullet;
  /** Tab stops: position in EMU and alignment. */
  tabs?: { pos: number; algn: 'l' | 'ctr' | 'r' | 'dec' }[];
  defRPr?: RunProperties;
  extras: XmlElement[];
}

const PPR_INT = ['marL', 'marR', 'lvl', 'indent', 'defTabSz'] as const;
const PPR_BOOL = ['rtl', 'eaLnBrk', 'latinLnBrk', 'hangingPunct'] as const;
const PPR_STR = ['algn', 'fontAlgn'] as const;
const PPR_ATTR_ORDER = [
  'marL',
  'marR',
  'lvl',
  'indent',
  'algn',
  'defTabSz',
  'rtl',
  'eaLnBrk',
  'fontAlgn',
  'latinLnBrk',
  'hangingPunct',
] as const;

function parseSpacing(node: XmlElement | undefined): TextSpacing | undefined {
  if (!node) return undefined;
  const pct = child(node, 'a:spcPct');
  if (pct) return { pct: (attrInt(pct, 'val') ?? 100000) / 100000 };
  const pts = child(node, 'a:spcPts');
  if (pts) return { pts: attrInt(pts, 'val') ?? 0 };
  return undefined;
}

function spacingEl(name: string, s: TextSpacing | undefined): XmlElement | undefined {
  if (!s) return undefined;
  return el(
    name,
    {},
    'pct' in s ? el('a:spcPct', { val: Math.round(s.pct * 100000) }) : el('a:spcPts', { val: s.pts }),
  );
}

/** Parses `a:pPr` or a list-style level (`a:lvl1pPr` … `a:lvl9pPr`, `a:defPPr`). */
export function parseParagraphProperties(node: XmlElement | undefined): ParagraphProperties {
  const pp: ParagraphProperties = { extras: [] };
  if (!node) return pp;
  const rec = pp as unknown as Record<string, unknown>;
  for (const k of PPR_INT) {
    const v = attrInt(node, k);
    if (v !== undefined) rec[k] = v;
  }
  for (const k of PPR_BOOL) {
    const v = attrBool(node, k);
    if (v !== undefined) rec[k] = v;
  }
  for (const k of PPR_STR) {
    const v = attr(node, k);
    if (v !== undefined) rec[k] = v;
  }
  for (const c of children(node)) {
    switch (c.name) {
      case 'a:lnSpc':
      case 'a:spcBef':
      case 'a:spcAft': {
        const s = parseSpacing(c);
        if (s) rec[c.name.slice(2)] = s;
        break;
      }
      case 'a:buClrTx':
        pp.buClr = 'text';
        break;
      case 'a:buClr': {
        const col = parseColorChild(c);
        if (col) pp.buClr = col;
        break;
      }
      case 'a:buSzTx':
        pp.buSz = 'text';
        break;
      case 'a:buSzPct':
        pp.buSz = { pct: (attrInt(c, 'val') ?? 100000) / 100000 };
        break;
      case 'a:buSzPts':
        pp.buSz = { pts: attrInt(c, 'val') ?? 0 };
        break;
      case 'a:buFontTx':
        pp.buFont = 'text';
        break;
      case 'a:buFont': {
        const f = parseFont(c);
        if (f) pp.buFont = f;
        break;
      }
      case 'a:buNone':
        pp.bullet = { type: 'none' };
        break;
      case 'a:buChar':
        pp.bullet = { type: 'char', char: attr(c, 'char') ?? '•' };
        break;
      case 'a:buAutoNum': {
        const b: Bullet = {
          type: 'autoNum',
          scheme: (attr(c, 'type') as ST_TextAutonumberScheme | undefined) ?? 'arabicPeriod',
        };
        const st = attrInt(c, 'startAt');
        if (st !== undefined) b.startAt = st;
        pp.bullet = b;
        break;
      }
      case 'a:buBlip': {
        const embed = attr(child(c, 'a:blip'), 'r:embed');
        pp.bullet = embed ? { type: 'blip', embed } : { type: 'blip' };
        break;
      }
      case 'a:tabLst':
        pp.tabs = children(c, 'a:tab').map((t) => ({
          pos: attrInt(t, 'pos') ?? 0,
          algn: (attr(t, 'algn') as 'l' | 'ctr' | 'r' | 'dec' | undefined) ?? 'l',
        }));
        break;
      case 'a:defRPr':
        pp.defRPr = parseRunProperties(c);
        break;
      default:
        pp.extras.push(cloneNode(c));
    }
  }
  return pp;
}

export function serializeParagraphProperties(pp: ParagraphProperties, name = 'a:pPr'): XmlElement {
  const a: Record<string, string | number | undefined> = {};
  const rec = pp as unknown as Record<string, unknown>;
  for (const k of PPR_ATTR_ORDER) {
    const v = rec[k];
    if (v === undefined) continue;
    a[k] = typeof v === 'boolean' ? (v ? '1' : '0') : (v as string | number);
  }
  const b = pp.bullet;
  return el(
    name,
    a,
    spacingEl('a:lnSpc', pp.lnSpc),
    spacingEl('a:spcBef', pp.spcBef),
    spacingEl('a:spcAft', pp.spcAft),
    pp.buClr === 'text' ? el('a:buClrTx') : pp.buClr && el('a:buClr', {}, serializeColor(pp.buClr)),
    pp.buSz === 'text'
      ? el('a:buSzTx')
      : pp.buSz &&
          ('pct' in pp.buSz
            ? el('a:buSzPct', { val: Math.round(pp.buSz.pct * 100000) })
            : el('a:buSzPts', { val: pp.buSz.pts })),
    pp.buFont === 'text' ? el('a:buFontTx') : fontEl('a:buFont', pp.buFont),
    b?.type === 'none' && el('a:buNone'),
    b?.type === 'autoNum' && el('a:buAutoNum', { type: b.scheme, startAt: b.startAt }),
    b?.type === 'char' && el('a:buChar', { char: b.char }),
    b?.type === 'blip' && el('a:buBlip', {}, el('a:blip', { 'r:embed': b.embed })),
    pp.tabs &&
      el(
        'a:tabLst',
        {},
        pp.tabs.map((t) => el('a:tab', { pos: t.pos, algn: t.algn })),
      ),
    pp.defRPr && serializeRunProperties(pp.defRPr, 'a:defRPr'),
    pp.extras.map((x) => cloneNode(x)),
  );
}

// -------------------------------------------------------------- lstStyle

export interface ListStyle {
  /** `a:defPPr`. */
  def?: ParagraphProperties;
  /** `a:lvl1pPr` … `a:lvl9pPr` at indexes 0–8. */
  levels: (ParagraphProperties | undefined)[];
  extLst?: XmlElement;
}

export function parseListStyle(node: XmlElement | undefined): ListStyle {
  const ls: ListStyle = { levels: [] };
  if (!node) return ls;
  const def = child(node, 'a:defPPr');
  if (def) ls.def = parseParagraphProperties(def);
  for (let i = 1; i <= 9; i++) {
    const lvl = child(node, `a:lvl${i}pPr`);
    ls.levels[i - 1] = lvl ? parseParagraphProperties(lvl) : undefined;
  }
  const ext = child(node, 'a:extLst');
  if (ext) ls.extLst = cloneNode(ext);
  return ls;
}

export function serializeListStyle(ls: ListStyle, name = 'a:lstStyle'): XmlElement {
  return el(
    name,
    {},
    ls.def && serializeParagraphProperties(ls.def, 'a:defPPr'),
    ls.levels.map((l, i) => l && serializeParagraphProperties(l, `a:lvl${i + 1}pPr`)),
    ls.extLst && cloneNode(ls.extLst),
  );
}

/**
 * Merges run properties: values in `over` win, `base` fills the gaps (used for master → layout → slide
 * → paragraph → run inheritance). Preserved extras come from `over` when present.
 */
export function mergeRunProperties(
  base: RunProperties | undefined,
  over: RunProperties | undefined,
): RunProperties {
  if (!base) return over ?? { hyperlinks: [], extras: [] };
  if (!over) return base;
  const out: Record<string, unknown> = { ...base };
  for (const [k, v] of Object.entries(over)) {
    if (v === undefined) continue;
    if ((k === 'hyperlinks' || k === 'extras') && Array.isArray(v) && v.length === 0) continue;
    out[k] = v;
  }
  return out as unknown as RunProperties;
}

/** Merges paragraph properties the same way (`defRPr` merged recursively). */
export function mergeParagraphProperties(
  base: ParagraphProperties | undefined,
  over: ParagraphProperties | undefined,
): ParagraphProperties {
  if (!base) return over ?? { extras: [] };
  if (!over) return base;
  const out: Record<string, unknown> = { ...base };
  for (const [k, v] of Object.entries(over)) {
    if (v === undefined) continue;
    if (k === 'extras' && Array.isArray(v) && v.length === 0) continue;
    out[k] = v;
  }
  out.defRPr = mergeRunProperties(base.defRPr, over.defRPr);
  if (!base.defRPr && !over.defRPr) delete out.defRPr;
  return out as unknown as ParagraphProperties;
}
