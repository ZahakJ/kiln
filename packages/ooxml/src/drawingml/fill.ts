/** DrawingML fills (ECMA-376 Part 1 §20.1.8): none, solid, gradient, pattern, picture and group. */
import type { ST_PathShadeType, ST_PresetPatternVal, ST_RectAlignment, ST_TileFlipMode } from '../st.ts';
import { cloneNode, el, type XmlElement } from '../xml/node.ts';
import { attr, attrBool, attrInt, child, children } from '../xml/query.ts';
import {
  type ColorContext,
  type DrawingColor,
  parseColorChild,
  type Rgba,
  resolveColor,
  rgbaToCss,
  serializeColor,
} from './color.ts';

/** Fractions (1 = 100 %) of an `a:fillToRect`/`a:srcRect`/`a:fillRect`/`a:tileRect` (inset from each edge). */
export interface RelativeRect {
  l: number;
  t: number;
  r: number;
  b: number;
}

export interface GradientStop {
  /** 0–1 */
  pos: number;
  color: DrawingColor;
}

export interface BlipFill {
  type: 'blip';
  /** Relationship id of the embedded image (`r:embed`). */
  embed?: string;
  /** Relationship id of a linked image (`r:link`). */
  link?: string;
  /** Crop as fractions inset from each edge (`a:srcRect`); negative values extend. */
  srcRect?: RelativeRect;
  mode: 'stretch' | 'tile' | 'none';
  /** `a:stretch/a:fillRect`. */
  fillRect?: RelativeRect;
  tile?: { tx: number; ty: number; sx: number; sy: number; flip: ST_TileFlipMode; algn: ST_RectAlignment };
  dpi?: number;
  rotWithShape?: boolean;
  /** `a:blip/a:alphaModFix/@amt` as a fraction. */
  alpha?: number;
  /** `a:blip/@cstate` (`print`, `screen`, `email`, `hqprint`). */
  compressionState?: string;
  /** Other `a:blip` children (effects, `a:extLst` with SVG/HDP images), preserved as written. */
  blipExtras?: XmlElement[];
}

export type Fill =
  | { type: 'none' }
  | { type: 'solid'; color: DrawingColor }
  | {
      type: 'gradient';
      stops: GradientStop[];
      /** Linear gradient: angle in degrees clockwise from left-to-right. */
      linear?: { angle: number; scaled: boolean };
      /** Path gradient; `fillToRect` is the focus rectangle. */
      path?: { shade: ST_PathShadeType; fillToRect?: RelativeRect };
      tileRect?: RelativeRect;
      flip?: ST_TileFlipMode;
      rotWithShape?: boolean;
    }
  | { type: 'pattern'; preset: ST_PresetPatternVal; fg?: DrawingColor; bg?: DrawingColor }
  | BlipFill
  | { type: 'group' };

export const FILL_ELEMENTS = [
  'a:noFill',
  'a:solidFill',
  'a:gradFill',
  'a:pattFill',
  'a:blipFill',
  'a:grpFill',
] as const;

function parseRelRect(node: XmlElement | undefined): RelativeRect | undefined {
  if (!node) return undefined;
  const f = (k: string) => (attrInt(node, k) ?? 0) / 100000;
  return { l: f('l'), t: f('t'), r: f('r'), b: f('b') };
}

function relRectAttrs(r: RelativeRect | undefined): Record<string, number | undefined> {
  if (!r) return {};
  const v = (x: number) => (x === 0 ? undefined : Math.round(x * 100000));
  return { l: v(r.l), t: v(r.t), r: v(r.r), b: v(r.b) };
}

/** Parses a fill element. `p:blipFill`/`xdr:blipFill`/`pic:blipFill` are accepted as picture fills. */
export function parseFill(node: XmlElement | undefined): Fill | undefined {
  if (!node) return undefined;
  const local = node.name.slice(node.name.indexOf(':') + 1);
  switch (local) {
    case 'noFill':
      return { type: 'none' };
    case 'grpFill':
      return { type: 'group' };
    case 'solidFill': {
      const color = parseColorChild(node);
      return color ? { type: 'solid', color } : { type: 'none' };
    }
    case 'gradFill': {
      const stops: GradientStop[] = children(child(node, 'a:gsLst'), 'a:gs')
        .map((gs) => ({ pos: (attrInt(gs, 'pos') ?? 0) / 100000, color: parseColorChild(gs) }))
        .filter((s): s is GradientStop => s.color !== undefined);
      const g: Extract<Fill, { type: 'gradient' }> = { type: 'gradient', stops };
      const lin = child(node, 'a:lin');
      if (lin)
        g.linear = {
          angle: (attrInt(lin, 'ang') ?? 0) / 60000,
          scaled: attrBool(lin, 'scaled', false) ?? false,
        };
      const path = child(node, 'a:path');
      if (path) {
        g.path = { shade: (attr(path, 'path') as ST_PathShadeType | undefined) ?? 'circle' };
        const ftr = parseRelRect(child(path, 'a:fillToRect'));
        if (ftr) g.path.fillToRect = ftr;
      }
      const tr = parseRelRect(child(node, 'a:tileRect'));
      if (tr) g.tileRect = tr;
      const flip = attr(node, 'flip');
      if (flip) g.flip = flip as ST_TileFlipMode;
      const rws = attrBool(node, 'rotWithShape');
      if (rws !== undefined) g.rotWithShape = rws;
      return g;
    }
    case 'pattFill': {
      const p: Extract<Fill, { type: 'pattern' }> = {
        type: 'pattern',
        preset: (attr(node, 'prst') as ST_PresetPatternVal | undefined) ?? 'pct5',
      };
      const fg = parseColorChild(child(node, 'a:fgClr'));
      const bg = parseColorChild(child(node, 'a:bgClr'));
      if (fg) p.fg = fg;
      if (bg) p.bg = bg;
      return p;
    }
    case 'blipFill': {
      const blip = child(node, 'a:blip');
      const f: BlipFill = { type: 'blip', mode: 'none' };
      const embed = attr(blip, 'r:embed');
      const link = attr(blip, 'r:link');
      if (embed) f.embed = embed;
      if (link) f.link = link;
      const cstate = attr(blip, 'cstate');
      if (cstate) f.compressionState = cstate;
      const extras: XmlElement[] = [];
      for (const c of children(blip)) {
        if (c.name === 'a:alphaModFix') f.alpha = (attrInt(c, 'amt') ?? 100000) / 100000;
        else extras.push(cloneNode(c));
      }
      if (extras.length) f.blipExtras = extras;
      const src = parseRelRect(child(node, 'a:srcRect'));
      if (src && (src.l || src.t || src.r || src.b)) f.srcRect = src;
      const stretch = child(node, 'a:stretch');
      const tile = child(node, 'a:tile');
      if (stretch) {
        f.mode = 'stretch';
        const fr = parseRelRect(child(stretch, 'a:fillRect'));
        if (fr && (fr.l || fr.t || fr.r || fr.b)) f.fillRect = fr;
      } else if (tile) {
        f.mode = 'tile';
        f.tile = {
          tx: attrInt(tile, 'tx') ?? 0,
          ty: attrInt(tile, 'ty') ?? 0,
          sx: (attrInt(tile, 'sx') ?? 100000) / 100000,
          sy: (attrInt(tile, 'sy') ?? 100000) / 100000,
          flip: (attr(tile, 'flip') as ST_TileFlipMode | undefined) ?? 'none',
          algn: (attr(tile, 'algn') as ST_RectAlignment | undefined) ?? 'tl',
        };
      }
      const dpi = attrInt(node, 'dpi');
      if (dpi !== undefined) f.dpi = dpi;
      const rws = attrBool(node, 'rotWithShape');
      if (rws !== undefined) f.rotWithShape = rws;
      return f;
    }
    default:
      return undefined;
  }
}

/** The first fill child of a properties element (`p:spPr`, `a:rPr`, `a:ln`, `a:tcPr`, …). */
export function findFill(container: XmlElement | undefined): Fill | undefined {
  for (const c of children(container)) {
    if ((FILL_ELEMENTS as readonly string[]).includes(c.name)) return parseFill(c);
  }
  return undefined;
}

export interface SerializeFillOptions {
  /** Element name for picture fills (`p:blipFill` in a `p:pic`, `pic:blipFill` in WordprocessingML). */
  blipFillName?: string;
}

export function serializeFill(fill: Fill, options: SerializeFillOptions = {}): XmlElement {
  switch (fill.type) {
    case 'none':
      return el('a:noFill');
    case 'group':
      return el('a:grpFill');
    case 'solid':
      return el('a:solidFill', {}, serializeColor(fill.color));
    case 'gradient':
      return el(
        'a:gradFill',
        { flip: fill.flip, rotWithShape: fill.rotWithShape },
        el(
          'a:gsLst',
          {},
          fill.stops.map((s) => el('a:gs', { pos: Math.round(s.pos * 100000) }, serializeColor(s.color))),
        ),
        fill.linear &&
          el('a:lin', { ang: Math.round(fill.linear.angle * 60000), scaled: fill.linear.scaled }),
        fill.path &&
          el(
            'a:path',
            { path: fill.path.shade },
            fill.path.fillToRect && el('a:fillToRect', relRectAttrs(fill.path.fillToRect)),
          ),
        fill.tileRect && el('a:tileRect', relRectAttrs(fill.tileRect)),
      );
    case 'pattern':
      return el(
        'a:pattFill',
        { prst: fill.preset },
        fill.fg && el('a:fgClr', {}, serializeColor(fill.fg)),
        fill.bg && el('a:bgClr', {}, serializeColor(fill.bg)),
      );
    case 'blip':
      return el(
        options.blipFillName ?? 'a:blipFill',
        { dpi: fill.dpi, rotWithShape: fill.rotWithShape },
        el(
          'a:blip',
          { 'r:embed': fill.embed, 'r:link': fill.link, cstate: fill.compressionState },
          fill.alpha !== undefined && el('a:alphaModFix', { amt: Math.round(fill.alpha * 100000) }),
          (fill.blipExtras ?? []).map((x) => cloneNode(x)),
        ),
        fill.srcRect && el('a:srcRect', relRectAttrs(fill.srcRect)),
        fill.mode === 'stretch' && el('a:stretch', {}, el('a:fillRect', relRectAttrs(fill.fillRect))),
        fill.mode === 'tile' &&
          fill.tile &&
          el('a:tile', {
            tx: fill.tile.tx,
            ty: fill.tile.ty,
            sx: Math.round(fill.tile.sx * 100000),
            sy: Math.round(fill.tile.sy * 100000),
            flip: fill.tile.flip,
            algn: fill.tile.algn,
          }),
      );
  }
}

/**
 * A CSS `background` value for solid and gradient fills (pattern fills fall back to their foreground
 * colour; picture fills need the caller to supply the image URL and return `undefined`).
 */
export function fillToCss(fill: Fill, ctx: ColorContext = {}): string | undefined {
  switch (fill.type) {
    case 'none':
      return 'transparent';
    case 'solid':
      return rgbaToCss(resolveColor(fill.color, ctx));
    case 'pattern':
      return fill.fg ? rgbaToCss(resolveColor(fill.fg, ctx)) : undefined;
    case 'gradient': {
      const stops = [...fill.stops]
        .sort((a, b) => a.pos - b.pos)
        .map((s) => `${rgbaToCss(resolveColor(s.color, ctx))} ${round(s.pos * 100)}%`)
        .join(', ');
      if (fill.path) {
        const r = fill.path.fillToRect ?? { l: 0.5, t: 0.5, r: 0.5, b: 0.5 };
        const cx = round(((r.l + (1 - r.r)) / 2) * 100);
        const cy = round(((r.t + (1 - r.b)) / 2) * 100);
        const shape = fill.path.shade === 'circle' ? 'circle' : 'ellipse';
        return `radial-gradient(${shape} at ${cx}% ${cy}%, ${stops})`;
      }
      // DrawingML angles run clockwise from "left to right"; CSS's run clockwise from "bottom to top".
      const angle = round(((fill.linear?.angle ?? 0) + 90) % 360);
      return `linear-gradient(${angle}deg, ${stops})`;
    }
    default:
      return undefined;
  }
}

const round = (n: number) => Math.round(n * 100) / 100;

/** The solid colour of a fill, if it has one. */
export function solidFillColor(fill: Fill | undefined, ctx?: ColorContext): Rgba | undefined {
  return fill?.type === 'solid' ? resolveColor(fill.color, ctx) : undefined;
}
