/**
 * DrawingML effects, `a:effectLst` (ECMA-376 Part 1 §20.1.8.26). Shadows, glow, reflection, soft edges
 * and blur are modelled; outer shadows can be rendered as CSS. Anything else is preserved verbatim.
 */
import type { ST_RectAlignment } from '../st.ts';
import { emuToPx } from '../units.ts';
import { cloneNode, el, type XmlElement } from '../xml/node.ts';
import { attr, attrBool, attrInt, children } from '../xml/query.ts';
import {
  type ColorContext,
  type DrawingColor,
  parseColorChild,
  resolveColor,
  rgbaToCss,
  serializeColor,
} from './color.ts';

export interface OuterShadow {
  /** Blur radius, EMU. */
  blurRad: number;
  /** Distance, EMU. */
  dist: number;
  /** Direction, degrees clockwise from the x axis. */
  dir: number;
  sx: number;
  sy: number;
  kx: number;
  ky: number;
  algn: ST_RectAlignment;
  rotWithShape: boolean;
  color: DrawingColor;
}

export interface InnerShadow {
  blurRad: number;
  dist: number;
  dir: number;
  color: DrawingColor;
}

export interface Glow {
  /** Radius, EMU. */
  rad: number;
  color: DrawingColor;
}

export interface Reflection {
  blurRad: number;
  stA: number;
  stPos: number;
  endA: number;
  endPos: number;
  dist: number;
  dir: number;
  fadeDir: number;
  sx: number;
  sy: number;
  kx: number;
  ky: number;
  algn: ST_RectAlignment;
  rotWithShape: boolean;
}

export interface EffectList {
  outerShadow?: OuterShadow;
  innerShadow?: InnerShadow;
  glow?: Glow;
  reflection?: Reflection;
  /** Soft edge radius, EMU. */
  softEdge?: number;
  blur?: { rad: number; grow: boolean };
  /** Effects we do not model (`a:prstShdw`, `a:fillOverlay`, …), preserved in order. */
  other?: XmlElement[];
}

const pct = (n: XmlElement, k: string, d: number) => (attrInt(n, k) ?? d) / 100000;
const ang = (n: XmlElement, k: string) => (attrInt(n, k) ?? 0) / 60000;
const BLACK: DrawingColor = { kind: 'srgb', hex: '000000', transforms: [] };

export function parseEffectList(node: XmlElement | undefined): EffectList | undefined {
  if (!node) return undefined;
  const out: EffectList = {};
  for (const c of children(node)) {
    switch (c.name) {
      case 'a:outerShdw':
        out.outerShadow = {
          blurRad: attrInt(c, 'blurRad') ?? 0,
          dist: attrInt(c, 'dist') ?? 0,
          dir: ang(c, 'dir'),
          sx: pct(c, 'sx', 100000),
          sy: pct(c, 'sy', 100000),
          kx: ang(c, 'kx'),
          ky: ang(c, 'ky'),
          algn: (attr(c, 'algn') as ST_RectAlignment | undefined) ?? 'b',
          rotWithShape: attrBool(c, 'rotWithShape', true) ?? true,
          color: parseColorChild(c) ?? BLACK,
        };
        break;
      case 'a:innerShdw':
        out.innerShadow = {
          blurRad: attrInt(c, 'blurRad') ?? 0,
          dist: attrInt(c, 'dist') ?? 0,
          dir: ang(c, 'dir'),
          color: parseColorChild(c) ?? BLACK,
        };
        break;
      case 'a:glow':
        out.glow = { rad: attrInt(c, 'rad') ?? 0, color: parseColorChild(c) ?? BLACK };
        break;
      case 'a:reflection':
        out.reflection = {
          blurRad: attrInt(c, 'blurRad') ?? 0,
          stA: pct(c, 'stA', 100000),
          stPos: pct(c, 'stPos', 0),
          endA: pct(c, 'endA', 0),
          endPos: pct(c, 'endPos', 100000),
          dist: attrInt(c, 'dist') ?? 0,
          dir: ang(c, 'dir'),
          fadeDir: (attrInt(c, 'fadeDir') ?? 5400000) / 60000,
          sx: pct(c, 'sx', 100000),
          sy: pct(c, 'sy', 100000),
          kx: ang(c, 'kx'),
          ky: ang(c, 'ky'),
          algn: (attr(c, 'algn') as ST_RectAlignment | undefined) ?? 'b',
          rotWithShape: attrBool(c, 'rotWithShape', true) ?? true,
        };
        break;
      case 'a:softEdge':
        out.softEdge = attrInt(c, 'rad') ?? 0;
        break;
      case 'a:blur':
        out.blur = { rad: attrInt(c, 'rad') ?? 0, grow: attrBool(c, 'grow', true) ?? true };
        break;
      default:
        out.other = [...(out.other ?? []), cloneNode(c)];
    }
  }
  return out;
}

const nz = (v: number, d: number) => (v === d ? undefined : v);
const toPct = (v: number, d: number) => nz(Math.round(v * 100000), Math.round(d * 100000));
const toAng = (v: number) => nz(Math.round(v * 60000), 0);

/** Serialises in schema order (blur, fillOverlay, glow, innerShdw, outerShdw, prstShdw, reflection, softEdge). */
export function serializeEffectList(fx: EffectList): XmlElement {
  const other = fx.other ?? [];
  const o = (name: string) => other.filter((x) => x.name === name).map((x) => cloneNode(x));
  const known = new Set(['a:fillOverlay', 'a:prstShdw']);
  const s = fx.outerShadow;
  const r = fx.reflection;
  return el(
    'a:effectLst',
    {},
    fx.blur && el('a:blur', { rad: nz(fx.blur.rad, 0), grow: fx.blur.grow ? undefined : '0' }),
    o('a:fillOverlay'),
    fx.glow && el('a:glow', { rad: nz(fx.glow.rad, 0) }, serializeColor(fx.glow.color)),
    fx.innerShadow &&
      el(
        'a:innerShdw',
        {
          blurRad: nz(fx.innerShadow.blurRad, 0),
          dist: nz(fx.innerShadow.dist, 0),
          dir: toAng(fx.innerShadow.dir),
        },
        serializeColor(fx.innerShadow.color),
      ),
    s &&
      el(
        'a:outerShdw',
        {
          blurRad: nz(s.blurRad, 0),
          dist: nz(s.dist, 0),
          dir: toAng(s.dir),
          sx: toPct(s.sx, 1),
          sy: toPct(s.sy, 1),
          kx: toAng(s.kx),
          ky: toAng(s.ky),
          algn: s.algn === 'b' ? undefined : s.algn,
          rotWithShape: s.rotWithShape ? undefined : '0',
        },
        serializeColor(s.color),
      ),
    o('a:prstShdw'),
    r &&
      el('a:reflection', {
        blurRad: nz(r.blurRad, 0),
        stA: toPct(r.stA, 1),
        stPos: toPct(r.stPos, 0),
        endA: toPct(r.endA, 0),
        endPos: toPct(r.endPos, 1),
        dist: nz(r.dist, 0),
        dir: toAng(r.dir),
        fadeDir: nz(Math.round(r.fadeDir * 60000), 5400000),
        sx: toPct(r.sx, 1),
        sy: toPct(r.sy, 1),
        kx: toAng(r.kx),
        ky: toAng(r.ky),
        algn: r.algn === 'b' ? undefined : r.algn,
        rotWithShape: r.rotWithShape ? undefined : '0',
      }),
    fx.softEdge !== undefined && el('a:softEdge', { rad: fx.softEdge }),
    other.filter((x) => !known.has(x.name)).map((x) => cloneNode(x)),
  );
}

/**
 * The CSS for an outer shadow: `{ x, y, blur, color }` in px plus a ready `filter: drop-shadow(...)`
 * value (drop-shadow follows the shape's outline, as PowerPoint's shadow does).
 */
export function outerShadowToCss(
  shadow: OuterShadow,
  ctx: ColorContext = {},
): { x: number; y: number; blur: number; color: string; filter: string } {
  const rad = (shadow.dir * Math.PI) / 180;
  const d = emuToPx(shadow.dist);
  const x = Math.round(Math.cos(rad) * d * 100) / 100;
  const y = Math.round(Math.sin(rad) * d * 100) / 100;
  // CSS blur radius is a standard deviation ×2; DrawingML's blurRad is the full blur extent.
  const blur = Math.round(emuToPx(shadow.blurRad) * 100) / 100;
  const color = rgbaToCss(resolveColor(shadow.color, ctx));
  return {
    x,
    y,
    blur,
    color,
    filter: `drop-shadow(${x}px ${y}px ${Math.round((blur / 2) * 100) / 100}px ${color})`,
  };
}
