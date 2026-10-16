/**
 * `xl/styles.xml` ↔ {@link StyleTable}: number formats (custom and the built-in ids 0–49), fonts,
 * pattern and gradient fills, borders, cell formats with alignment and protection, named cell styles
 * (`cellStyleXfs` + `cellStyles`) and differential formats (`dxfs`).
 *
 * The style table keeps the file's indices: `cellXfs` entries come first (cells refer to them by
 * index), followed by the `cellStyleXfs` entries that back the named styles.
 */
import { attrsFrom, child, children, el, type XmlElement } from '@officina/ooxml';
import {
  type Alignment,
  type Border,
  type BorderSide,
  type BorderStyle,
  BUILTIN_NUMFMTS,
  builtinNumFmt,
  builtinNumFmtId,
  type Dxf,
  DEFAULT_FONT,
  type Fill,
  type Font,
  type NamedCellStyle,
  type PatternType,
  type Protection,
  StyleTable,
  type StyleTableJSON,
  type UnderlineStyle,
  type Xf,
} from '@officina/sheet-model';
import type { StylesPassthrough } from '../types.ts';
import { formatNumber, parseBool, parseInt10, parseNum } from '../util/text.ts';
import { colorAttrs, readColor, writeColor } from './color.ts';

// ── reading ──────────────────────────────────────────────────────────────

function flag(e: XmlElement | undefined): boolean | undefined {
  if (!e) return undefined;
  const v = e.attrs.val;
  return v === undefined ? true : v === '1' || v === 'true';
}

export function readFont(e: XmlElement): Partial<Font> {
  const f: Partial<Font> = {};
  for (const c of e.children) {
    if (c.type !== 'element') continue;
    const v = c.attrs.val;
    switch (c.name) {
      case 'b':
        if (flag(c)) f.bold = true;
        break;
      case 'i':
        if (flag(c)) f.italic = true;
        break;
      case 'strike':
        if (flag(c)) f.strike = true;
        break;
      case 'condense':
        if (flag(c)) f.condense = true;
        break;
      case 'extend':
        if (flag(c)) f.extend = true;
        break;
      case 'outline':
        if (flag(c)) f.outline = true;
        break;
      case 'shadow':
        if (flag(c)) f.shadow = true;
        break;
      case 'u':
        f.underline = (v ?? 'single') as UnderlineStyle;
        break;
      case 'vertAlign':
        if (v === 'superscript' || v === 'subscript') f.vertAlign = v;
        break;
      case 'sz': {
        const n = parseNum(v);
        if (n !== undefined) f.size = n;
        break;
      }
      case 'color': {
        const col = readColor(c);
        if (col) f.color = col;
        break;
      }
      case 'name':
      case 'rFont':
        if (v !== undefined) f.name = v;
        break;
      case 'family': {
        const n = parseInt10(v);
        if (n !== undefined) f.family = n;
        break;
      }
      case 'charset': {
        const n = parseInt10(v);
        if (n !== undefined) f.charset = n;
        break;
      }
      case 'scheme':
        if (v === 'minor' || v === 'major') f.scheme = v;
        break;
    }
  }
  return f;
}

const PATTERNS = new Set<string>([
  'none',
  'solid',
  'mediumGray',
  'darkGray',
  'lightGray',
  'darkHorizontal',
  'darkVertical',
  'darkDown',
  'darkUp',
  'darkGrid',
  'darkTrellis',
  'lightHorizontal',
  'lightVertical',
  'lightDown',
  'lightUp',
  'lightGrid',
  'lightTrellis',
  'gray125',
  'gray0625',
]);

/** Reads a `fill`; `noPattern` reports a pattern fill without `patternType` (dxf implicit solid). */
export function readFill(e: XmlElement | undefined, dxf: boolean): { fill: Fill; noPattern: boolean } {
  const gf = child(e, 'gradientFill');
  if (gf) {
    const fill: Fill = {
      type: 'gradient',
      gradient: gf.attrs.type === 'path' ? 'path' : 'linear',
      stops: children(gf, 'stop').map((s) => ({
        position: parseNum(s.attrs.position) ?? 0,
        color: readColor(child(s, 'color')) ?? { rgb: 'FF000000' },
      })),
    };
    const deg = parseNum(gf.attrs.degree);
    if (deg !== undefined) fill.degree = deg;
    for (const k of ['left', 'right', 'top', 'bottom'] as const) {
      const n = parseNum(gf.attrs[k]);
      if (n !== undefined) fill[k] = n;
    }
    return { fill, noPattern: false };
  }
  const pf = child(e, 'patternFill');
  const pt = pf?.attrs.patternType;
  const noPattern = pf !== undefined && pt === undefined;
  const pattern = (pt && PATTERNS.has(pt) ? pt : noPattern && dxf ? 'solid' : 'none') as PatternType;
  const fill: Fill = { type: 'pattern', pattern };
  const fg = readColor(child(pf, 'fgColor'));
  const bg = readColor(child(pf, 'bgColor'));
  if (fg) fill.fg = fg;
  if (bg) fill.bg = bg;
  return { fill, noPattern: noPattern && dxf };
}

const BORDER_SIDES: [keyof Border, string[]][] = [
  ['left', ['left', 'start']],
  ['right', ['right', 'end']],
  ['top', ['top']],
  ['bottom', ['bottom']],
  ['diagonal', ['diagonal']],
];

export function readBorder(e: XmlElement): Border {
  const b: Border = {};
  for (const [key, names] of BORDER_SIDES) {
    let side: XmlElement | undefined;
    for (const n of names) side ??= child(e, n);
    const style = side?.attrs.style;
    if (!side || !style || style === 'none') continue;
    const s: BorderSide = { style: style as BorderStyle };
    const c = readColor(child(side, 'color'));
    if (c) s.color = c;
    (b as Record<string, BorderSide>)[key] = s;
  }
  if (parseBool(e.attrs.diagonalUp)) b.diagonalUp = true;
  if (parseBool(e.attrs.diagonalDown)) b.diagonalDown = true;
  return b;
}

export function readAlignment(e: XmlElement | undefined): Alignment {
  const a: Alignment = {};
  if (!e) return a;
  const x = e.attrs;
  if (x.horizontal) a.horizontal = x.horizontal as Alignment['horizontal'];
  if (x.vertical) a.vertical = x.vertical as Alignment['vertical'];
  if (parseBool(x.wrapText)) a.wrapText = true;
  if (parseBool(x.shrinkToFit)) a.shrinkToFit = true;
  const indent = parseInt10(x.indent);
  if (indent) a.indent = indent;
  const rot = parseInt10(x.textRotation);
  if (rot) a.textRotation = rot;
  const ro = parseInt10(x.readingOrder);
  if (ro === 1 || ro === 2) a.readingOrder = ro;
  if (parseBool(x.justifyLastLine)) a.justifyLastLine = true;
  return a;
}

export function readProtection(e: XmlElement | undefined): Protection {
  const p: Protection = {};
  if (!e) return p;
  const locked = parseBool(e.attrs.locked);
  const hidden = parseBool(e.attrs.hidden);
  if (locked === false) p.locked = false;
  if (hidden) p.hidden = true;
  return p;
}

function readDxf(e: XmlElement, index: number, pass: StylesPassthrough, numFmts: Map<number, string>): Dxf {
  const d: Dxf = {};
  const font = child(e, 'font');
  if (font) d.font = readFont(font);
  const nf = child(e, 'numFmt');
  if (nf) {
    const id = parseInt10(nf.attrs.numFmtId) ?? 0;
    const code = nf.attrs.formatCode ?? numFmts.get(id) ?? builtinNumFmt(id) ?? 'General';
    d.numFmt = code;
    if (!pass.numFmtIds.has(code) && nf.attrs.formatCode !== undefined) pass.numFmtIds.set(code, id);
  }
  const fill = child(e, 'fill');
  if (fill) {
    const r = readFill(fill, true);
    d.fill = r.fill;
    if (r.noPattern) pass.dxfFillNoPattern.add(index);
  }
  const al = child(e, 'alignment');
  if (al) d.alignment = readAlignment(al);
  const border = child(e, 'border');
  if (border) d.border = readBorder(border);
  const prot = child(e, 'protection');
  if (prot) d.protection = readProtection(prot);
  return d;
}

export interface ReadStylesResult {
  table: StyleTable;
  pass: StylesPassthrough;
  warnings: string[];
}

export function emptyStylesPassthrough(): StylesPassthrough {
  return {
    cellXfsCount: 1,
    styleXfs: [0],
    numFmtIds: new Map(),
    xfNumFmtIds: [],
    dxfFillNoPattern: new Set(),
    extras: new Map(),
    xfExtras: new Map(),
  };
}

export function readStyles(root: XmlElement | undefined): ReadStylesResult {
  const warnings: string[] = [];
  const pass = emptyStylesPassthrough();
  if (!root) return { table: new StyleTable(DEFAULT_FONT), pass, warnings };

  const numFmts = new Map<number, string>();
  for (const nf of children(child(root, 'numFmts'), 'numFmt')) {
    const id = parseInt10(nf.attrs.numFmtId);
    const code = nf.attrs.formatCode;
    if (id === undefined || code === undefined) continue;
    numFmts.set(id, code);
    if (!pass.numFmtIds.has(code)) pass.numFmtIds.set(code, id);
  }
  const codeOf = (id: number): string => numFmts.get(id) ?? builtinNumFmt(id) ?? 'General';

  const fonts = children(child(root, 'fonts'), 'font').map((f): Font => {
    const r = readFont(f);
    return { ...r, name: r.name ?? DEFAULT_FONT.name, size: r.size ?? DEFAULT_FONT.size };
  });
  const fills = children(child(root, 'fills'), 'fill').map((f) => readFill(f, false).fill);
  const borders = children(child(root, 'borders'), 'border').map(readBorder);
  if (fonts.length === 0) fonts.push({ ...DEFAULT_FONT });
  if (fills.length === 0) fills.push({ type: 'pattern', pattern: 'none' });
  if (borders.length === 0) borders.push({});

  const clampIdx = (v: string | undefined, n: number, what: string): number => {
    const i = parseInt10(v) ?? 0;
    if (i >= 0 && i < n) return i;
    warnings.push(`A cell format refers to missing ${what} ${i}; using the default`);
    return 0;
  };
  const readXf = (x: XmlElement, styleXfCount: number, isCellXf: boolean): Xf => {
    const id = parseInt10(x.attrs.numFmtId) ?? 0;
    const xf: Xf = {
      font: clampIdx(x.attrs.fontId, fonts.length, 'font'),
      fill: clampIdx(x.attrs.fillId, fills.length, 'fill'),
      border: clampIdx(x.attrs.borderId, borders.length, 'border'),
      numFmt: codeOf(id),
      alignment: readAlignment(child(x, 'alignment')),
      protection: readProtection(child(x, 'protection')),
      styleId: 0,
    };
    if (isCellXf) {
      const sid = parseInt10(x.attrs.xfId) ?? 0;
      xf.styleId = sid >= 0 && sid < Math.max(1, styleXfCount) ? sid : 0;
    }
    if (parseBool(x.attrs.quotePrefix)) xf.quotePrefix = true;
    return xf;
  };

  const styleXfEls = children(child(root, 'cellStyleXfs'), 'xf');
  const cellXfEls = children(child(root, 'cellXfs'), 'xf');
  const xfs: Xf[] = [];
  for (const [i, x] of cellXfEls.entries()) {
    xfs.push(readXf(x, styleXfEls.length, true));
    pass.xfNumFmtIds[i] = parseInt10(x.attrs.numFmtId) ?? 0;
    if (parseBool(x.attrs.pivotButton)) pass.xfExtras.set(i, { pivotButton: '1' });
  }
  if (xfs.length === 0) {
    xfs.push({ font: 0, fill: 0, border: 0, numFmt: 'General', alignment: {}, protection: {}, styleId: 0 });
    pass.xfNumFmtIds[0] = 0;
  }
  pass.cellXfsCount = xfs.length;
  pass.styleXfs = [];
  for (const x of styleXfEls) {
    pass.styleXfs.push(xfs.length);
    xfs.push(readXf(x, 0, false));
  }

  const namedStyles: NamedCellStyle[] = [];
  for (const cs of children(child(root, 'cellStyles'), 'cellStyle')) {
    const xfId = parseInt10(cs.attrs.xfId) ?? 0;
    const holder = pass.styleXfs[xfId];
    if (holder === undefined) {
      warnings.push(`Cell style "${cs.attrs.name ?? ''}" refers to a missing format`);
      continue;
    }
    const s: NamedCellStyle = { name: cs.attrs.name ?? `Style ${namedStyles.length + 1}`, xf: holder };
    const builtin = parseInt10(cs.attrs.builtinId);
    if (builtin !== undefined) s.builtinId = builtin;
    if (parseBool(cs.attrs.hidden)) s.hidden = true;
    if (parseBool(cs.attrs.customBuiltin)) s.customBuiltin = true;
    namedStyles.push(s);
  }
  if (namedStyles.length === 0 && pass.styleXfs.length > 0)
    namedStyles.push({ name: 'Normal', builtinId: 0, xf: pass.styleXfs[0] as number });

  const dxfs = children(child(root, 'dxfs'), 'dxf').map((d, i) => readDxf(d, i, pass, numFmts));

  for (const name of ['colors', 'tableStyles', 'extLst']) {
    const e = child(root, name);
    if (e) pass.extras.set(name, e);
  }

  const parts: StyleTableJSON = { fonts, fills, borders, xfs, dxfs, namedStyles };
  return { table: StyleTable.fromParts(parts), pass, warnings };
}

// ── writing ──────────────────────────────────────────────────────────────

export function writeFont(f: Partial<Font>, tag = 'font', nameTag = 'name'): XmlElement {
  const kids: (XmlElement | null)[] = [];
  const on = (n: string, v: boolean | undefined) => (v ? el(n) : null);
  kids.push(on('b', f.bold), on('i', f.italic), on('strike', f.strike), on('condense', f.condense));
  kids.push(on('extend', f.extend), on('outline', f.outline), on('shadow', f.shadow));
  if (f.underline && f.underline !== 'none')
    kids.push(el('u', f.underline === 'single' ? {} : { val: f.underline }));
  if (f.vertAlign) kids.push(el('vertAlign', { val: f.vertAlign }));
  if (f.size !== undefined) kids.push(el('sz', { val: formatNumber(f.size) }));
  kids.push(writeColor('color', f.color));
  if (f.name !== undefined) kids.push(el(nameTag, { val: f.name }));
  if (f.family !== undefined) kids.push(el('family', { val: f.family }));
  if (f.charset !== undefined) kids.push(el('charset', { val: f.charset }));
  if (f.scheme) kids.push(el('scheme', { val: f.scheme }));
  return el(tag, {}, kids);
}

export function writeFill(f: Fill, noPattern = false): XmlElement {
  if (f.type === 'gradient') {
    return el(
      'fill',
      {},
      el(
        'gradientFill',
        {
          type: f.gradient === 'path' ? 'path' : undefined,
          degree: f.degree !== undefined ? formatNumber(f.degree) : undefined,
          left: f.left !== undefined ? formatNumber(f.left) : undefined,
          right: f.right !== undefined ? formatNumber(f.right) : undefined,
          top: f.top !== undefined ? formatNumber(f.top) : undefined,
          bottom: f.bottom !== undefined ? formatNumber(f.bottom) : undefined,
        },
        f.stops.map((s) => el('stop', { position: formatNumber(s.position) }, el('color', colorAttrs(s.color)))),
      ),
    );
  }
  return el(
    'fill',
    {},
    el(
      'patternFill',
      { patternType: noPattern ? undefined : f.pattern },
      writeColor('fgColor', f.fg),
      writeColor('bgColor', f.bg),
    ),
  );
}

export function writeBorder(b: Border, dxf = false): XmlElement {
  const side = (name: keyof Border) => {
    const s = b[name] as BorderSide | undefined;
    if (!s) return dxf ? null : el(name);
    return el(name, { style: s.style }, writeColor('color', s.color));
  };
  return el(
    'border',
    { diagonalUp: b.diagonalUp ? '1' : undefined, diagonalDown: b.diagonalDown ? '1' : undefined },
    side('left'),
    side('right'),
    side('top'),
    side('bottom'),
    side('diagonal'),
  );
}

export function writeAlignment(a: Alignment | undefined): XmlElement | null {
  if (!a || Object.keys(a).length === 0) return null;
  return el('alignment', {
    horizontal: a.horizontal,
    vertical: a.vertical,
    textRotation: a.textRotation,
    wrapText: a.wrapText ? '1' : undefined,
    indent: a.indent,
    justifyLastLine: a.justifyLastLine ? '1' : undefined,
    shrinkToFit: a.shrinkToFit ? '1' : undefined,
    readingOrder: a.readingOrder,
  });
}

export function writeProtection(p: Protection | undefined): XmlElement | null {
  if (!p || (p.locked !== false && !p.hidden)) return null;
  return el('protection', { locked: p.locked === false ? '0' : undefined, hidden: p.hidden ? '1' : undefined });
}

/** Number format id allocation shared by cell formats and differential formats. */
export class NumFmtRegistry {
  private readonly ids = new Map<string, number>();
  private readonly used = new Set<number>();
  readonly custom: [number, string][] = [];
  private next = 164;

  constructor(private readonly source: StylesPassthrough | undefined) {}

  idFor(code: string, originalId?: number): number {
    const hit = this.ids.get(code);
    if (hit !== undefined) return hit;
    let id: number | undefined;
    if (code === 'General' && originalId !== undefined && originalId !== 0 && BUILTIN_NUMFMTS[originalId] === undefined)
      return originalId; // a locale built-in id without a known code: keep it as written
    const src = this.source?.numFmtIds.get(code);
    if (src !== undefined && !this.used.has(src)) id = src;
    else {
      const b = builtinNumFmtId(code);
      if (b !== undefined) id = b;
    }
    if (id === undefined) {
      while (this.used.has(this.next) || this.isSourceId(this.next)) this.next++;
      id = this.next++;
    }
    this.ids.set(code, id);
    this.used.add(id);
    if (BUILTIN_NUMFMTS[id] !== code || id >= 164) {
      if (!this.custom.some(([i]) => i === id)) this.custom.push([id, code]);
    }
    return id;
  }

  private isSourceId(id: number): boolean {
    if (!this.source) return false;
    for (const v of this.source.numFmtIds.values()) if (v === id) return true;
    return false;
  }
}

export interface WriteStylesResult {
  xml: XmlElement;
  /** Old style-table index → `cellXfs` index in the output. */
  xfMap: Int32Array;
}

/**
 * Serialises the style table. `used` lists the table indices cells, rows and columns refer to; every
 * other source `cellXfs` entry is kept too (so indices stay stable), while table entries that only
 * back named styles go to `cellStyleXfs`.
 */
export function writeStyles(table: StyleTable, pass: StylesPassthrough | undefined, used: Set<number>): WriteStylesResult {
  const json = table.toJSON();
  const numFmts = new NumFmtRegistry(pass);
  const n = json.xfs.length;
  const holderSet = new Set<number>(pass ? pass.styleXfs : []);
  for (const s of json.namedStyles) if (!pass || s.xf >= pass.cellXfsCount) holderSet.add(s.xf);
  // cellXfs: source cell formats, then every newer entry that is not a style holder (or is used).
  const xfMap = new Int32Array(n).fill(-1);
  const cellOrder: number[] = [];
  const srcCount = pass ? pass.cellXfsCount : 1;
  for (let i = 0; i < n; i++) {
    const isCell = i < srcCount || !holderSet.has(i) || used.has(i);
    if (i === 0 || isCell) {
      xfMap[i] = cellOrder.length;
      cellOrder.push(i);
    }
  }
  // cellStyleXfs: the source order, then holders of named styles added later.
  const styleOrder: number[] = [];
  const styleIndex = new Map<number, number>();
  const addHolder = (i: number) => {
    if (styleIndex.has(i)) return;
    styleIndex.set(i, styleOrder.length);
    styleOrder.push(i);
  };
  if (pass) for (const i of pass.styleXfs) addHolder(i);
  for (const s of json.namedStyles) addHolder(s.xf);
  if (styleOrder.length === 0) addHolder(0);

  const xfEl = (i: number, cell: boolean): XmlElement => {
    const x = json.xfs[i] as Xf;
    const origId = cell && pass && i < pass.cellXfsCount ? pass.xfNumFmtIds[i] : undefined;
    const numFmtId = numFmts.idFor(x.numFmt, origId);
    const al = writeAlignment(x.alignment);
    const pr = writeProtection(x.protection);
    const extras = cell ? pass?.xfExtras.get(i) : undefined;
    const attrs: Record<string, string | number | undefined> = {
      numFmtId,
      fontId: x.font,
      fillId: x.fill,
      borderId: x.border,
    };
    if (cell) attrs.xfId = Math.min(x.styleId, Math.max(0, styleOrder.length - 1));
    if (x.quotePrefix) attrs.quotePrefix = '1';
    if (extras?.pivotButton) attrs.pivotButton = '1';
    if (numFmtId !== 0) attrs.applyNumberFormat = '1';
    if (x.font !== 0) attrs.applyFont = '1';
    if (x.fill !== 0) attrs.applyFill = '1';
    if (x.border !== 0) attrs.applyBorder = '1';
    if (al) attrs.applyAlignment = '1';
    if (pr) attrs.applyProtection = '1';
    return el('xf', attrs, al, pr);
  };

  const cellXfs = cellOrder.map((i) => xfEl(i, true));
  const styleXfs = styleOrder.map((i) => xfEl(i, false));
  const cellStyles = json.namedStyles.map((s) =>
    el('cellStyle', {
      name: s.name,
      xfId: styleIndex.get(s.xf) ?? 0,
      builtinId: s.builtinId,
      hidden: s.hidden ? '1' : undefined,
      customBuiltin: s.customBuiltin ? '1' : undefined,
    }),
  );
  const dxfs = json.dxfs.map((d, i) => writeDxf(d, numFmts, pass?.dxfFillNoPattern.has(i) ?? false));

  const fonts = json.fonts.map((f) => writeFont(f));
  const fills = json.fills.map((f) => writeFill(f));
  const borders = json.borders.map((b) => writeBorder(b));
  const numFmtEls = [...numFmts.custom]
    .sort((a, b) => a[0] - b[0])
    .map(([id, code]) => el('numFmt', { numFmtId: id, formatCode: code }));

  const xml = el(
    'styleSheet',
    attrsFrom({ xmlns: 'http://schemas.openxmlformats.org/spreadsheetml/2006/main' }),
    numFmtEls.length ? el('numFmts', { count: numFmtEls.length }, numFmtEls) : null,
    el('fonts', { count: fonts.length }, fonts),
    el('fills', { count: fills.length }, fills),
    el('borders', { count: borders.length }, borders),
    el('cellStyleXfs', { count: styleXfs.length }, styleXfs),
    el('cellXfs', { count: cellXfs.length }, cellXfs),
    el('cellStyles', { count: cellStyles.length }, cellStyles),
    el('dxfs', { count: dxfs.length }, dxfs),
    pass?.extras.get('tableStyles') ??
      el('tableStyles', { count: 0, defaultTableStyle: 'TableStyleMedium2', defaultPivotStyle: 'PivotStyleLight16' }),
    pass?.extras.get('colors') ?? null,
    pass?.extras.get('extLst') ?? null,
  );
  return { xml, xfMap };
}

function writeDxf(d: Dxf, numFmts: NumFmtRegistry, noPattern: boolean): XmlElement {
  return el(
    'dxf',
    {},
    d.font ? writeFont(d.font) : null,
    d.numFmt !== undefined ? el('numFmt', { numFmtId: numFmts.idFor(d.numFmt), formatCode: d.numFmt }) : null,
    d.fill ? writeFill(d.fill, noPattern && d.fill.type === 'pattern' && d.fill.pattern === 'solid') : null,
    d.alignment ? writeAlignment(d.alignment) : null,
    d.border ? writeBorder(d.border, true) : null,
    d.protection ? el('protection', { locked: d.protection.locked === false ? '0' : undefined, hidden: d.protection.hidden ? '1' : undefined }) : null,
  );
}
