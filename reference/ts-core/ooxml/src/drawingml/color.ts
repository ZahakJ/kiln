/**
 * DrawingML colours (ECMA-376 Part 1 §20.1.2.3): the six colour models, their transforms, and resolution
 * to RGBA against a theme and colour map.
 */
import type { ST_SchemeColorVal } from '../st.ts';
import { el, type XmlElement } from '../xml/node.ts';
import { attr, attrInt, children } from '../xml/query.ts';

/** A colour transform in document order: `{ name: 'lumMod', val: 75000 }`. */
export interface ColorTransform {
  name: string;
  /** Raw integer value (thousandths of a percent, or 60000ths of a degree for hue); absent for flags. */
  val?: number;
}

export type DrawingColor =
  | { kind: 'srgb'; hex: string; transforms: ColorTransform[] }
  | { kind: 'scheme'; scheme: ST_SchemeColorVal | string; transforms: ColorTransform[] }
  | { kind: 'sys'; sys: string; lastClr?: string; transforms: ColorTransform[] }
  | { kind: 'preset'; preset: string; transforms: ColorTransform[] }
  | { kind: 'hsl'; hue: number; sat: number; lum: number; transforms: ColorTransform[] }
  | { kind: 'scrgb'; r: number; g: number; b: number; transforms: ColorTransform[] };

/** Channels 0–255 (may be fractional before rounding), alpha 0–1. */
export interface Rgba {
  r: number;
  g: number;
  b: number;
  a: number;
}

export const COLOR_ELEMENTS = [
  'a:srgbClr',
  'a:schemeClr',
  'a:sysClr',
  'a:prstClr',
  'a:hslClr',
  'a:scrgbClr',
] as const;

const FLAG_TRANSFORMS = new Set(['comp', 'inv', 'gray', 'gamma', 'invGamma']);

// --------------------------------------------------------------- parsing

function parseTransforms(node: XmlElement): ColorTransform[] {
  return children(node).map((c) => {
    const name = c.name.startsWith('a:') ? c.name.slice(2) : c.name;
    const t: ColorTransform = { name };
    const v = attrInt(c, 'val');
    if (v !== undefined && !FLAG_TRANSFORMS.has(name)) t.val = v;
    return t;
  });
}

/** Parses a colour element (`a:srgbClr`, `a:schemeClr`, …). Returns `undefined` for other elements. */
export function parseColor(node: XmlElement | undefined): DrawingColor | undefined {
  if (!node) return undefined;
  const transforms = parseTransforms(node);
  switch (node.name) {
    case 'a:srgbClr':
      return { kind: 'srgb', hex: normalizeHex(attr(node, 'val') ?? '000000'), transforms };
    case 'a:schemeClr':
      return { kind: 'scheme', scheme: attr(node, 'val') ?? 'tx1', transforms };
    case 'a:sysClr': {
      const c: DrawingColor = { kind: 'sys', sys: attr(node, 'val') ?? 'windowText', transforms };
      const last = attr(node, 'lastClr');
      if (last) c.lastClr = normalizeHex(last);
      return c;
    }
    case 'a:prstClr':
      return { kind: 'preset', preset: attr(node, 'val') ?? 'black', transforms };
    case 'a:hslClr':
      return {
        kind: 'hsl',
        hue: attrInt(node, 'hue') ?? 0,
        sat: parsePct(attr(node, 'sat')),
        lum: parsePct(attr(node, 'lum')),
        transforms,
      };
    case 'a:scrgbClr':
      return {
        kind: 'scrgb',
        r: parsePct(attr(node, 'r')),
        g: parsePct(attr(node, 'g')),
        b: parsePct(attr(node, 'b')),
        transforms,
      };
    default:
      return undefined;
  }
}

/** The colour inside a container such as `a:solidFill`, `a:fgClr`, `a:outerShdw`, `a:dk1`. */
export function parseColorChild(container: XmlElement | undefined): DrawingColor | undefined {
  for (const c of children(container)) {
    const parsed = parseColor(c);
    if (parsed) return parsed;
  }
  return undefined;
}

function parsePct(v: string | undefined): number {
  if (v === undefined) return 0;
  if (v.endsWith('%')) return Math.round(Number(v.slice(0, -1)) * 1000);
  const n = Number(v);
  return Number.isFinite(n) ? n : 0;
}

function normalizeHex(v: string): string {
  const h = v.trim().replace(/^#/, '').toUpperCase();
  return /^[0-9A-F]{6}$/.test(h) ? h : /^[0-9A-F]{8}$/.test(h) ? h.slice(2) : '000000';
}

export function serializeColor(c: DrawingColor): XmlElement {
  const kids = c.transforms.map((t) => el(`a:${t.name}`, { val: t.val }));
  switch (c.kind) {
    case 'srgb':
      return el('a:srgbClr', { val: c.hex }, kids);
    case 'scheme':
      return el('a:schemeClr', { val: c.scheme }, kids);
    case 'sys':
      return el('a:sysClr', { val: c.sys, lastClr: c.lastClr }, kids);
    case 'preset':
      return el('a:prstClr', { val: c.preset }, kids);
    case 'hsl':
      return el('a:hslClr', { hue: c.hue, sat: c.sat, lum: c.lum }, kids);
    case 'scrgb':
      return el('a:scrgbClr', { r: c.r, g: c.g, b: c.b }, kids);
  }
}

/** Convenience constructors. */
export const srgb = (hex: string, ...transforms: ColorTransform[]): DrawingColor => ({
  kind: 'srgb',
  hex: normalizeHex(hex),
  transforms,
});
export const schemeColor = (scheme: ST_SchemeColorVal, ...transforms: ColorTransform[]): DrawingColor => ({
  kind: 'scheme',
  scheme,
  transforms,
});

// ------------------------------------------------------------ resolution

export interface ColorContext {
  /** Theme colour slots by name (`dk1`, `accent1`, …), already resolved. */
  themeColors?: Partial<Record<string, Rgba | string>>;
  /**
   * `p:clrMap` / `w:clrSchemeMapping`: maps `bg1`/`tx1`/`bg2`/`tx2` (and any slot) to theme slots.
   * Defaults to Office's `bg1→lt1, tx1→dk1, bg2→lt2, tx2→dk2`.
   */
  colorMap?: Partial<Record<string, string>>;
  /** The placeholder colour (`phClr`) substituted inside theme style matrices. */
  phClr?: Rgba;
}

export const DEFAULT_COLOR_MAP: Readonly<Record<string, string>> = {
  bg1: 'lt1',
  tx1: 'dk1',
  bg2: 'lt2',
  tx2: 'dk2',
};

/** Windows default system colours, used when `a:sysClr` has no `lastClr`. */
export const SYSTEM_COLORS: Readonly<Record<string, string>> = {
  scrollBar: 'C8C8C8',
  background: '000000',
  activeCaption: '99B4D1',
  inactiveCaption: 'BFCDDB',
  menu: 'F0F0F0',
  window: 'FFFFFF',
  windowFrame: '646464',
  menuText: '000000',
  windowText: '000000',
  captionText: '000000',
  activeBorder: 'B4B4B4',
  inactiveBorder: 'F4F7FC',
  appWorkspace: 'ABABAB',
  highlight: '0078D7',
  highlightText: 'FFFFFF',
  btnFace: 'F0F0F0',
  btnShadow: 'A0A0A0',
  grayText: '6D6D6D',
  btnText: '000000',
  inactiveCaptionText: '000000',
  btnHighlight: 'FFFFFF',
  '3dDkShadow': '696969',
  '3dLight': 'E3E3E3',
  infoText: '000000',
  infoBk: 'FFFFE1',
  hotLight: '0066CC',
  gradientActiveCaption: 'B9D1EA',
  gradientInactiveCaption: 'D7E4F2',
  menuHighlight: '3399FF',
  menuBar: 'F0F0F0',
};

/** ST_PresetColorVal (CSS/X11 names; DrawingML also accepts `dk`/`lt`/`med` abbreviations). */
const PRESET_COLORS: Record<string, string> = {
  aliceBlue: 'F0F8FF',
  antiqueWhite: 'FAEBD7',
  aqua: '00FFFF',
  aquamarine: '7FFFD4',
  azure: 'F0FFFF',
  beige: 'F5F5DC',
  bisque: 'FFE4C4',
  black: '000000',
  blanchedAlmond: 'FFEBCD',
  blue: '0000FF',
  blueViolet: '8A2BE2',
  brown: 'A52A2A',
  burlyWood: 'DEB887',
  cadetBlue: '5F9EA0',
  chartreuse: '7FFF00',
  chocolate: 'D2691E',
  coral: 'FF7F50',
  cornflowerBlue: '6495ED',
  cornsilk: 'FFF8DC',
  crimson: 'DC143C',
  cyan: '00FFFF',
  darkBlue: '00008B',
  darkCyan: '008B8B',
  darkGoldenrod: 'B8860B',
  darkGray: 'A9A9A9',
  darkGrey: 'A9A9A9',
  darkGreen: '006400',
  darkKhaki: 'BDB76B',
  darkMagenta: '8B008B',
  darkOliveGreen: '556B2F',
  darkOrange: 'FF8C00',
  darkOrchid: '9932CC',
  darkRed: '8B0000',
  darkSalmon: 'E9967A',
  darkSeaGreen: '8FBC8F',
  darkSlateBlue: '483D8B',
  darkSlateGray: '2F4F4F',
  darkSlateGrey: '2F4F4F',
  darkTurquoise: '00CED1',
  darkViolet: '9400D3',
  deepPink: 'FF1493',
  deepSkyBlue: '00BFFF',
  dimGray: '696969',
  dimGrey: '696969',
  dodgerBlue: '1E90FF',
  firebrick: 'B22222',
  floralWhite: 'FFFAF0',
  forestGreen: '228B22',
  fuchsia: 'FF00FF',
  gainsboro: 'DCDCDC',
  ghostWhite: 'F8F8FF',
  gold: 'FFD700',
  goldenrod: 'DAA520',
  gray: '808080',
  grey: '808080',
  green: '008000',
  greenYellow: 'ADFF2F',
  honeydew: 'F0FFF0',
  hotPink: 'FF69B4',
  indianRed: 'CD5C5C',
  indigo: '4B0082',
  ivory: 'FFFFF0',
  khaki: 'F0E68C',
  lavender: 'E6E6FA',
  lavenderBlush: 'FFF0F5',
  lawnGreen: '7CFC00',
  lemonChiffon: 'FFFACD',
  lightBlue: 'ADD8E6',
  lightCoral: 'F08080',
  lightCyan: 'E0FFFF',
  lightGoldenrodYellow: 'FAFAD2',
  lightGray: 'D3D3D3',
  lightGrey: 'D3D3D3',
  lightGreen: '90EE90',
  lightPink: 'FFB6C1',
  lightSalmon: 'FFA07A',
  lightSeaGreen: '20B2AA',
  lightSkyBlue: '87CEFA',
  lightSlateGray: '778899',
  lightSlateGrey: '778899',
  lightSteelBlue: 'B0C4DE',
  lightYellow: 'FFFFE0',
  lime: '00FF00',
  limeGreen: '32CD32',
  linen: 'FAF0E6',
  magenta: 'FF00FF',
  maroon: '800000',
  mediumAquamarine: '66CDAA',
  mediumBlue: '0000CD',
  mediumOrchid: 'BA55D3',
  mediumPurple: '9370DB',
  mediumSeaGreen: '3CB371',
  mediumSlateBlue: '7B68EE',
  mediumSpringGreen: '00FA9A',
  mediumTurquoise: '48D1CC',
  mediumVioletRed: 'C71585',
  midnightBlue: '191970',
  mintCream: 'F5FFFA',
  mistyRose: 'FFE4E1',
  moccasin: 'FFE4B5',
  navajoWhite: 'FFDEAD',
  navy: '000080',
  oldLace: 'FDF5E6',
  olive: '808000',
  oliveDrab: '6B8E23',
  orange: 'FFA500',
  orangeRed: 'FF4500',
  orchid: 'DA70D6',
  paleGoldenrod: 'EEE8AA',
  paleGreen: '98FB98',
  paleTurquoise: 'AFEEEE',
  paleVioletRed: 'DB7093',
  papayaWhip: 'FFEFD5',
  peachPuff: 'FFDAB9',
  peru: 'CD853F',
  pink: 'FFC0CB',
  plum: 'DDA0DD',
  powderBlue: 'B0E0E6',
  purple: '800080',
  red: 'FF0000',
  rosyBrown: 'BC8F8F',
  royalBlue: '4169E1',
  saddleBrown: '8B4513',
  salmon: 'FA8072',
  sandyBrown: 'F4A460',
  seaGreen: '2E8B57',
  seaShell: 'FFF5EE',
  sienna: 'A0522D',
  silver: 'C0C0C0',
  skyBlue: '87CEEB',
  slateBlue: '6A5ACD',
  slateGray: '708090',
  slateGrey: '708090',
  snow: 'FFFAFA',
  springGreen: '00FF7F',
  steelBlue: '4682B4',
  tan: 'D2B48C',
  teal: '008080',
  thistle: 'D8BFD8',
  tomato: 'FF6347',
  turquoise: '40E0D0',
  violet: 'EE82EE',
  wheat: 'F5DEB3',
  white: 'FFFFFF',
  whiteSmoke: 'F5F5F5',
  yellow: 'FFFF00',
  yellowGreen: '9ACD32',
};

/** Hex value of an ST_PresetColorVal name (`dkBlue`, `ltGray`, `medPurple` included), or `undefined`. */
export function presetColorHex(name: string): string | undefined {
  const direct = PRESET_COLORS[name];
  if (direct) return direct;
  const expanded = name.replace(/^dk/, 'dark').replace(/^lt/, 'light').replace(/^med/, 'medium');
  return PRESET_COLORS[expanded];
}

export function hexToRgba(hex: string, a = 1): Rgba {
  const h = normalizeHex(hex);
  return {
    r: Number.parseInt(h.slice(0, 2), 16),
    g: Number.parseInt(h.slice(2, 4), 16),
    b: Number.parseInt(h.slice(4, 6), 16),
    a,
  };
}

const clamp01 = (x: number) => (x < 0 ? 0 : x > 1 ? 1 : x);
const clamp255 = (x: number) => Math.round(x < 0 ? 0 : x > 255 ? 255 : x);

/** `RRGGBB` (upper case) of a resolved colour; alpha is ignored. */
export function rgbaToHex(c: Rgba): string {
  return [c.r, c.g, c.b]
    .map((v) => clamp255(v).toString(16).padStart(2, '0'))
    .join('')
    .toUpperCase();
}

/** CSS colour: `#rrggbb` when opaque, `rgba(r, g, b, a)` otherwise. */
export function rgbaToCss(c: Rgba): string {
  if (c.a >= 1) return `#${rgbaToHex(c).toLowerCase()}`;
  const a = Math.round(clamp01(c.a) * 1000) / 1000;
  return `rgba(${clamp255(c.r)}, ${clamp255(c.g)}, ${clamp255(c.b)}, ${a})`;
}

interface Hsl {
  h: number; // 0–360
  s: number; // 0–1
  l: number; // 0–1
}

export function rgbToHsl(r: number, g: number, b: number): Hsl {
  const R = r / 255;
  const G = g / 255;
  const B = b / 255;
  const max = Math.max(R, G, B);
  const min = Math.min(R, G, B);
  const l = (max + min) / 2;
  if (max === min) return { h: 0, s: 0, l };
  const d = max - min;
  const s = l > 0.5 ? d / (2 - max - min) : d / (max + min);
  let h: number;
  if (max === R) h = (G - B) / d + (G < B ? 6 : 0);
  else if (max === G) h = (B - R) / d + 2;
  else h = (R - G) / d + 4;
  return { h: h * 60, s, l };
}

export function hslToRgb(h: number, s: number, l: number): [number, number, number] {
  const hh = (((h % 360) + 360) % 360) / 360;
  if (s === 0) return [l * 255, l * 255, l * 255];
  const q = l < 0.5 ? l * (1 + s) : l + s - l * s;
  const p = 2 * l - q;
  const ch = (t: number) => {
    let x = t;
    if (x < 0) x += 1;
    if (x > 1) x -= 1;
    if (x < 1 / 6) return p + (q - p) * 6 * x;
    if (x < 1 / 2) return q;
    if (x < 2 / 3) return p + (q - p) * (2 / 3 - x) * 6;
    return p;
  };
  return [ch(hh + 1 / 3) * 255, ch(hh) * 255, ch(hh - 1 / 3) * 255];
}

const toLinear = (c: number) => {
  const x = c / 255;
  return x <= 0.04045 ? x / 12.92 : ((x + 0.055) / 1.055) ** 2.4;
};
const fromLinear = (x: number) => {
  const c = clamp01(x);
  return (c <= 0.0031308 ? c * 12.92 : 1.055 * c ** (1 / 2.4) - 0.055) * 255;
};

/** Applies DrawingML colour transforms in document order. */
export function applyColorTransforms(base: Rgba, transforms: readonly ColorTransform[]): Rgba {
  let { r, g, b, a } = base;
  const f = (t: ColorTransform) => (t.val ?? 0) / 100000;
  const hslOp = (op: (hsl: Hsl) => void) => {
    const hsl = rgbToHsl(r, g, b);
    op(hsl);
    [r, g, b] = hslToRgb(hsl.h, clamp01(hsl.s), clamp01(hsl.l));
  };
  const linOp = (op: (lin: [number, number, number]) => [number, number, number]) => {
    const [lr, lg, lb] = op([toLinear(r), toLinear(g), toLinear(b)]);
    r = fromLinear(lr);
    g = fromLinear(lg);
    b = fromLinear(lb);
  };
  for (const t of transforms) {
    switch (t.name) {
      case 'alpha':
        a = clamp01(f(t));
        break;
      case 'alphaMod':
        a = clamp01(a * f(t));
        break;
      case 'alphaOff':
        a = clamp01(a + f(t));
        break;
      case 'lum':
        hslOp((x) => {
          x.l = f(t);
        });
        break;
      case 'lumMod':
        hslOp((x) => {
          x.l *= f(t);
        });
        break;
      case 'lumOff':
        hslOp((x) => {
          x.l += f(t);
        });
        break;
      case 'sat':
        hslOp((x) => {
          x.s = f(t);
        });
        break;
      case 'satMod':
        hslOp((x) => {
          x.s *= f(t);
        });
        break;
      case 'satOff':
        hslOp((x) => {
          x.s += f(t);
        });
        break;
      case 'hue':
        hslOp((x) => {
          x.h = (t.val ?? 0) / 60000;
        });
        break;
      case 'hueMod':
        hslOp((x) => {
          x.h *= f(t);
        });
        break;
      case 'hueOff':
        hslOp((x) => {
          x.h += (t.val ?? 0) / 60000;
        });
        break;
      case 'comp':
        hslOp((x) => {
          x.h += 180;
        });
        break;
      case 'inv':
        r = 255 - r;
        g = 255 - g;
        b = 255 - b;
        break;
      case 'gray': {
        const y = r * 0.22 + g * 0.72 + b * 0.06;
        r = g = b = y;
        break;
      }
      case 'tint': {
        // Lighten toward white in linear RGB (matches LibreOffice and PowerPoint rendering).
        const k = f(t);
        linOp((c) => c.map((x) => 1 - (1 - x) * k) as [number, number, number]);
        break;
      }
      case 'shade': {
        const k = f(t);
        linOp((c) => c.map((x) => x * k) as [number, number, number]);
        break;
      }
      case 'red':
      case 'green':
      case 'blue':
      case 'redMod':
      case 'greenMod':
      case 'blueMod':
      case 'redOff':
      case 'greenOff':
      case 'blueOff': {
        const idx = t.name.startsWith('red') ? 0 : t.name.startsWith('green') ? 1 : 2;
        const kind = t.name.endsWith('Mod') ? 'mod' : t.name.endsWith('Off') ? 'off' : 'set';
        linOp((c) => {
          const v = c[idx] as number;
          c[idx] = kind === 'set' ? f(t) : kind === 'mod' ? v * f(t) : v + f(t);
          return c;
        });
        break;
      }
      case 'gamma':
        r = fromLinear(r / 255);
        g = fromLinear(g / 255);
        b = fromLinear(b / 255);
        break;
      case 'invGamma':
        r = toLinear(r) * 255;
        g = toLinear(g) * 255;
        b = toLinear(b) * 255;
        break;
      default:
        // Unknown transforms are ignored rather than failing the render.
        break;
    }
  }
  return { r: clamp255(r), g: clamp255(g), b: clamp255(b), a };
}

/** The base colour of a theme slot after the colour map, before transforms. */
function themeSlot(name: string, ctx: ColorContext, depth = 0): Rgba | undefined {
  if (name === 'phClr') return ctx.phClr;
  const mapped = ctx.colorMap?.[name] ?? DEFAULT_COLOR_MAP[name] ?? name;
  const v = ctx.themeColors?.[mapped];
  if (v === undefined) {
    // A colour map can chain (`bg1 → lt1`); stop after one hop to avoid cycles.
    return depth === 0 && mapped !== name ? themeSlot(mapped, ctx, 1) : undefined;
  }
  return typeof v === 'string' ? hexToRgba(v) : v;
}

/**
 * Resolves a colour to RGBA. Unknown scheme slots fall back to black, as Office does when a theme lacks
 * the slot.
 */
export function resolveColor(color: DrawingColor, ctx: ColorContext = {}): Rgba {
  let base: Rgba;
  switch (color.kind) {
    case 'srgb':
      base = hexToRgba(color.hex);
      break;
    case 'scheme':
      base = themeSlot(color.scheme, ctx) ?? hexToRgba('000000');
      break;
    case 'sys':
      base = hexToRgba(color.lastClr ?? SYSTEM_COLORS[color.sys] ?? '000000');
      break;
    case 'preset':
      base = hexToRgba(presetColorHex(color.preset) ?? '000000');
      break;
    case 'hsl': {
      const [r, g, b] = hslToRgb(color.hue / 60000, clamp01(color.sat / 100000), clamp01(color.lum / 100000));
      base = { r, g, b, a: 1 };
      break;
    }
    case 'scrgb':
      base = {
        r: fromLinear(color.r / 100000),
        g: fromLinear(color.g / 100000),
        b: fromLinear(color.b / 100000),
        a: 1,
      };
      break;
  }
  return applyColorTransforms(base, color.transforms);
}

export function resolveColorHex(color: DrawingColor, ctx?: ColorContext): string {
  return rgbaToHex(resolveColor(color, ctx));
}

/**
 * SpreadsheetML/WordprocessingML `tint` on a colour (−1…1), applied in HLS the way Excel does: negative
 * values darken, positive values lighten.
 */
export function applyExcelTint(hex: string, tint: number): string {
  const { r, g, b } = hexToRgba(hex);
  const hsl = rgbToHsl(r, g, b);
  hsl.l = tint < 0 ? hsl.l * (1 + tint) : hsl.l * (1 - tint) + tint;
  const [nr, ng, nb] = hslToRgb(hsl.h, hsl.s, clamp01(hsl.l));
  return rgbaToHex({ r: nr, g: ng, b: nb, a: 1 });
}
