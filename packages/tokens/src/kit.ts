/**
 * The shape of the brand kit's `tokens.json` (`brand/tokens.json`, schema `kiln-tokens/1`) and a strict
 * parser for it. The generator (`scripts/generate.ts`) parses the kit with `parseKit` so a malformed or
 * changed kit fails loudly instead of producing broken CSS; the parsed kit is emitted as the typed
 * object in `src/generated/kit.ts`.
 */

/** Surface and text colours of one theme. Every value is `#RRGGBB`. */
export interface KitThemeColors {
  bg: string;
  panel: string;
  raised: string;
  sunken: string;
  page: string;
  canvas: string;
  fg: string;
  muted: string;
  faint: string;
  line: string;
  lineStrong: string;
  ember: string;
  emberText: string;
  onEmber: string;
  selection: string;
  focus: string;
  danger: string;
  success: string;
  warning: string;
}

/** One app's glaze. `fill`/`icon` are for shapes only; the `text*` values are tuned for AA text. */
export interface KitProduct {
  name: string;
  replaces: string;
  glaze: string;
  fill: string;
  solid: string;
  onSolid: string;
  textLight: string;
  textDark: string;
  tintLight: string;
  tintDark: string;
  icon: readonly string[];
}

export const KIT_PRODUCTS = ['write', 'grid', 'deck', 'notes', 'home'] as const;
export type KitProductId = (typeof KIT_PRODUCTS)[number];

export const KIT_NEUTRALS = ['bone', 'bisque', 'terracotta', 'clay', 'tenmoku', 'soot', 'char'] as const;
export const KIT_FIRE = ['core', 'flame', 'ember', 'tongue', 'glow'] as const;
export const KIT_THEME_KEYS = [
  'bg',
  'panel',
  'raised',
  'sunken',
  'page',
  'canvas',
  'fg',
  'muted',
  'faint',
  'line',
  'lineStrong',
  'ember',
  'emberText',
  'onEmber',
  'selection',
  'focus',
  'danger',
  'success',
  'warning',
] as const satisfies readonly (keyof KitThemeColors)[];
export const KIT_PRODUCT_COLOR_KEYS = [
  'fill',
  'solid',
  'onSolid',
  'textLight',
  'textDark',
  'tintLight',
  'tintDark',
] as const satisfies readonly (keyof KitProduct)[];
export const KIT_TEXT_SIZES = ['xs', 'sm', 'base', 'md', 'lg', 'xl', '2xl', '3xl', 'hero'] as const;
export const KIT_RADII = ['xs', 'sm', 'md', 'lg', 'xl', 'pill'] as const;
export const KIT_SPACE = ['1', '2', '3', '4', '5', '6', '8', '10', '12'] as const;

export interface KitTokens {
  $schema: string;
  name: string;
  rule: string;
  color: {
    light: KitThemeColors;
    dark: KitThemeColors;
    product: Record<KitProductId, KitProduct>;
    neutral: Record<(typeof KIT_NEUTRALS)[number], string>;
    fire: Record<(typeof KIT_FIRE)[number], string>;
  };
  font: {
    display: { family: string; use: string; settings: Record<string, number> };
    ui: { family: string; use: string; weights: readonly number[] };
    docBody: { family: string; use: string; fallbackForOffice: string };
    stack: { display: string; ui: string };
    size: Record<(typeof KIT_TEXT_SIZES)[number], number>;
  };
  radius: Record<(typeof KIT_RADII)[number], number> & { archRatio: number };
  space: Record<(typeof KIT_SPACE)[number], number>;
  shadow: {
    light: { '1': string; '2': string; page: string };
    dark: { '1': string; '2': string; page: string };
  };
  motion: { fast: string; base: string; slow: string; ease: string; flicker: string };
  contrast: Record<string, number>;
  contrastNote: string;
}

const HEX = /^#[0-9A-F]{6}$/i;
const DURATION = /^\d+ms$/;

class KitError extends Error {
  constructor(path: string, problem: string) {
    super(`brand/tokens.json: ${path}: ${problem}`);
    this.name = 'KitError';
  }
}

type Json = Record<string, unknown>;

function obj(v: unknown, path: string): Json {
  if (typeof v !== 'object' || v === null || Array.isArray(v)) throw new KitError(path, 'expected an object');
  return v as Json;
}
function str(v: unknown, path: string): string {
  if (typeof v !== 'string' || v.length === 0) throw new KitError(path, 'expected a non-empty string');
  return v;
}
function hex(v: unknown, path: string): string {
  const s = str(v, path);
  if (!HEX.test(s)) throw new KitError(path, `expected #RRGGBB, got ${JSON.stringify(s)}`);
  return s;
}
function num(v: unknown, path: string): number {
  if (typeof v !== 'number' || !Number.isFinite(v) || v < 0)
    throw new KitError(path, 'expected a number ≥ 0');
  return v;
}
function shadow(v: unknown, path: string): string {
  const s = str(v, path);
  // A box-shadow list: no declarations smuggled into the generated CSS.
  if (/[;{}<>]/.test(s)) throw new KitError(path, 'not a box-shadow value');
  return s;
}
function fontStack(v: unknown, path: string): string {
  const s = str(v, path);
  if (/[;{}<>"]/.test(s)) throw new KitError(path, 'not a font-family list');
  return s;
}

/**
 * Validates a parsed `tokens.json` and returns it typed. Throws with the JSON path of the first problem.
 * Unknown extra keys are kept (the kit may grow); missing or malformed required keys are errors.
 */
export function parseKit(input: unknown): KitTokens {
  const root = obj(input, '$');
  if (root.$schema !== 'kiln-tokens/1') throw new KitError('$schema', 'expected "kiln-tokens/1"');
  const color = obj(root.color, 'color');
  for (const theme of ['light', 'dark'] as const) {
    const t = obj(color[theme], `color.${theme}`);
    for (const k of KIT_THEME_KEYS) hex(t[k], `color.${theme}.${k}`);
  }
  const product = obj(color.product, 'color.product');
  for (const p of KIT_PRODUCTS) {
    const entry = obj(product[p], `color.product.${p}`);
    str(entry.name, `color.product.${p}.name`);
    for (const k of KIT_PRODUCT_COLOR_KEYS) hex(entry[k], `color.product.${p}.${k}`);
    if (!Array.isArray(entry.icon)) throw new KitError(`color.product.${p}.icon`, 'expected an array');
    for (const [i, c] of entry.icon.entries()) hex(c, `color.product.${p}.icon[${i}]`);
  }
  const neutral = obj(color.neutral, 'color.neutral');
  for (const k of KIT_NEUTRALS) hex(neutral[k], `color.neutral.${k}`);
  const fire = obj(color.fire, 'color.fire');
  for (const k of KIT_FIRE) hex(fire[k], `color.fire.${k}`);

  const font = obj(root.font, 'font');
  const stack = obj(font.stack, 'font.stack');
  fontStack(stack.display, 'font.stack.display');
  fontStack(stack.ui, 'font.stack.ui');
  str(obj(font.display, 'font.display').family, 'font.display.family');
  str(obj(font.ui, 'font.ui').family, 'font.ui.family');
  const size = obj(font.size, 'font.size');
  for (const k of KIT_TEXT_SIZES) num(size[k], `font.size.${k}`);

  const radius = obj(root.radius, 'radius');
  for (const k of KIT_RADII) num(radius[k], `radius.${k}`);
  const space = obj(root.space, 'space');
  for (const k of KIT_SPACE) num(space[k], `space.${k}`);

  const shadows = obj(root.shadow, 'shadow');
  for (const theme of ['light', 'dark'] as const) {
    const s = obj(shadows[theme], `shadow.${theme}`);
    for (const k of ['1', '2', 'page']) shadow(s[k], `shadow.${theme}.${k}`);
  }
  const motion = obj(root.motion, 'motion');
  for (const k of ['fast', 'base', 'slow'] as const) {
    if (!DURATION.test(str(motion[k], `motion.${k}`))) throw new KitError(`motion.${k}`, 'expected <n>ms');
  }
  if (!/^cubic-bezier\([\d.,\s]+\)$/.test(str(motion.ease, 'motion.ease'))) {
    throw new KitError('motion.ease', 'expected a cubic-bezier()');
  }
  obj(root.contrast, 'contrast');
  return root as unknown as KitTokens;
}

/**
 * Source of `src/generated/kit.ts`: the parsed kit as a typed object. The generator formats it with
 * Biome after writing, so the committed file follows the house style.
 */
export function renderKitModule(k: KitTokens): string {
  return `/*\n * The brand kit's tokens.json as a typed object.\n * GENERATED from brand/tokens.json by \`pnpm --filter @kiln/tokens generate\`. Do not edit by hand.\n */\nimport type { KitTokens } from '../kit.ts';\n\nexport const kit: KitTokens = ${JSON.stringify(k, null, 2)};\n`;
}
