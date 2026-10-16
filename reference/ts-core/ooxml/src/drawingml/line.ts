/** DrawingML outlines, `a:ln` (ECMA-376 Part 1 §20.1.2.2.24). */
import type {
  ST_CompoundLine,
  ST_LineCap,
  ST_LineEndType,
  ST_LineEndWidth,
  ST_PenAlignment,
  ST_PresetLineDashVal,
} from '../st.ts';
import { emuToPx } from '../units.ts';
import { el, type XmlElement } from '../xml/node.ts';
import { attr, attrInt, child, children } from '../xml/query.ts';
import { type Fill, findFill, serializeFill } from './fill.ts';

export interface LineEnd {
  type: ST_LineEndType;
  w?: ST_LineEndWidth;
  len?: ST_LineEndWidth;
}

export interface Line {
  /** Width in EMU. */
  width?: number;
  cap?: ST_LineCap;
  compound?: ST_CompoundLine;
  align?: ST_PenAlignment;
  fill?: Fill;
  /** Preset dash or a custom dash as `[dash, space]` pairs in percent of the line width. */
  dash?: ST_PresetLineDashVal | { custom: [number, number][] };
  join?: { type: 'round' } | { type: 'bevel' } | { type: 'miter'; limit?: number };
  head?: LineEnd;
  tail?: LineEnd;
}

function parseEnd(node: XmlElement | undefined): LineEnd | undefined {
  if (!node) return undefined;
  const e: LineEnd = { type: (attr(node, 'type') as ST_LineEndType | undefined) ?? 'none' };
  const w = attr(node, 'w');
  const len = attr(node, 'len');
  if (w) e.w = w as ST_LineEndWidth;
  if (len) e.len = len as ST_LineEndWidth;
  return e;
}

export function parseLine(node: XmlElement | undefined): Line | undefined {
  if (!node) return undefined;
  const line: Line = {};
  const w = attrInt(node, 'w');
  if (w !== undefined) line.width = w;
  const cap = attr(node, 'cap');
  if (cap) line.cap = cap as ST_LineCap;
  const cmpd = attr(node, 'cmpd');
  if (cmpd) line.compound = cmpd as ST_CompoundLine;
  const algn = attr(node, 'algn');
  if (algn) line.align = algn as ST_PenAlignment;
  const fill = findFill(node);
  if (fill) line.fill = fill;
  const prst = child(node, 'a:prstDash');
  const cust = child(node, 'a:custDash');
  if (prst) line.dash = (attr(prst, 'val') as ST_PresetLineDashVal | undefined) ?? 'solid';
  else if (cust) {
    line.dash = {
      custom: children(cust, 'a:ds').map((ds) => [
        (attrInt(ds, 'd') ?? 0) / 1000,
        (attrInt(ds, 'sp') ?? 0) / 1000,
      ]),
    };
  }
  if (child(node, 'a:round')) line.join = { type: 'round' };
  else if (child(node, 'a:bevel')) line.join = { type: 'bevel' };
  else {
    const miter = child(node, 'a:miter');
    if (miter) {
      const lim = attrInt(miter, 'lim');
      line.join = lim === undefined ? { type: 'miter' } : { type: 'miter', limit: lim / 100000 };
    }
  }
  const head = parseEnd(child(node, 'a:headEnd'));
  const tail = parseEnd(child(node, 'a:tailEnd'));
  if (head) line.head = head;
  if (tail) line.tail = tail;
  return line;
}

export function serializeLine(line: Line, name = 'a:ln'): XmlElement {
  const end = (n: string, e: LineEnd | undefined) => e && el(n, { type: e.type, w: e.w, len: e.len });
  return el(
    name,
    { w: line.width, cap: line.cap, cmpd: line.compound, algn: line.align },
    line.fill && serializeFill(line.fill),
    line.dash !== undefined &&
      (typeof line.dash === 'string'
        ? el('a:prstDash', { val: line.dash })
        : el(
            'a:custDash',
            {},
            line.dash.custom.map(([d, sp]) =>
              el('a:ds', { d: Math.round(d * 1000), sp: Math.round(sp * 1000) }),
            ),
          )),
    line.join?.type === 'round' && el('a:round'),
    line.join?.type === 'bevel' && el('a:bevel'),
    line.join?.type === 'miter' &&
      el('a:miter', {
        lim: line.join.limit === undefined ? undefined : Math.round(line.join.limit * 100000),
      }),
    end('a:headEnd', line.head),
    end('a:tailEnd', line.tail),
  );
}

/** Dash pattern multipliers of the line width for each preset (PowerPoint's rendering). */
const DASH_PATTERNS: Record<ST_PresetLineDashVal, number[]> = {
  solid: [],
  dot: [1, 3],
  dash: [4, 3],
  lgDash: [8, 3],
  dashDot: [4, 3, 1, 3],
  lgDashDot: [8, 3, 1, 3],
  lgDashDotDot: [8, 3, 1, 3, 1, 3],
  sysDash: [3, 1],
  sysDot: [1, 1],
  sysDashDot: [3, 1, 1, 1],
  sysDashDotDot: [3, 1, 1, 1, 1, 1],
};

/** SVG `stroke-dasharray` (in px) for a line, or `undefined` for solid lines. */
export function dashArrayPx(line: Line): string | undefined {
  const wPx = Math.max(emuToPx(line.width ?? 9525), 1);
  let pattern: number[];
  if (line.dash === undefined) return undefined;
  if (typeof line.dash === 'string') pattern = DASH_PATTERNS[line.dash] ?? [];
  else pattern = line.dash.custom.flatMap(([d, sp]) => [d / 100, sp / 100]);
  if (pattern.length === 0) return undefined;
  return pattern.map((m) => Math.round(m * wPx * 100) / 100).join(' ');
}

/** SVG `stroke-linecap` / `stroke-linejoin` values. */
export function svgCapJoin(line: Line): {
  linecap: 'round' | 'square' | 'butt';
  linejoin: 'round' | 'bevel' | 'miter';
} {
  return {
    linecap: line.cap === 'rnd' ? 'round' : line.cap === 'sq' ? 'square' : 'butt',
    linejoin: line.join?.type ?? 'round',
  };
}
