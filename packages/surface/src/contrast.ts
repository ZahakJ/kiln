/**
 * Word's dark-page colour adaptation (WP-64). Automatic text colours are theme roles and follow the
 * theme already; text in an explicit document colour (Word's dark blue headings, dark greys) would
 * be unreadable on the dark page. Such a colour keeps its hue and has its perceived lightness
 * inverted (OKLab L → 1 − L), raised further until it reads at 4.5:1 against the page. Colours that
 * already read, and every colour on a light page, are painted as the document says.
 */

const cache = new Map<string, number>();

/** sRGB channel (0–255) to linear light. */
function lin(c: number): number {
  const v = c / 255;
  return v <= 0.04045 ? v / 12.92 : ((v + 0.055) / 1.055) ** 2.4;
}

/** Linear light to an sRGB channel (0–255). */
function enc(v: number): number {
  const c = v <= 0.0031308 ? v * 12.92 : 1.055 * v ** (1 / 2.4) - 0.055;
  return Math.round(Math.min(1, Math.max(0, c)) * 255);
}

function rgb(rgba: number): [number, number, number] {
  const v = rgba >>> 0;
  return [(v >>> 24) & 0xff, (v >>> 16) & 0xff, (v >>> 8) & 0xff];
}

/** WCAG relative luminance of `0xRRGGBBAA`. */
export function luminance(rgba: number): number {
  const [r, g, b] = rgb(rgba);
  return 0.2126 * lin(r) + 0.7152 * lin(g) + 0.0722 * lin(b);
}

/** WCAG contrast ratio of two colours. */
export function contrast(a: number, b: number): number {
  const la = luminance(a);
  const lb = luminance(b);
  return (Math.max(la, lb) + 0.05) / (Math.min(la, lb) + 0.05);
}

/** OKLab of linear sRGB. */
function toOklab(r: number, g: number, b: number): [number, number, number] {
  const l = Math.cbrt(0.4122214708 * r + 0.5363325363 * g + 0.0514459929 * b);
  const m = Math.cbrt(0.2119034982 * r + 0.6806995451 * g + 0.1073969566 * b);
  const s = Math.cbrt(0.0883024619 * r + 0.2817188376 * g + 0.6299787005 * b);
  return [
    0.2104542553 * l + 0.793617785 * m - 0.0040720468 * s,
    1.9779984951 * l - 2.428592205 * m + 0.4505937099 * s,
    0.0259040371 * l + 0.7827717662 * m - 0.808675766 * s,
  ];
}

/** Linear sRGB of OKLab. */
function fromOklab(L: number, a: number, b: number): [number, number, number] {
  const l = (L + 0.3963377774 * a + 0.2158037573 * b) ** 3;
  const m = (L - 0.1055613458 * a - 0.0638541728 * b) ** 3;
  const s = (L - 0.0894841775 * a - 1.291485548 * b) ** 3;
  return [
    4.0767416621 * l - 3.3077115913 * m + 0.2309699292 * s,
    -1.2684380046 * l + 2.6097574011 * m - 0.3413193965 * s,
    -0.0041960863 * l - 0.7034186147 * m + 1.707614701 * s,
  ];
}

/** The colour to paint text of colour `rgba` with on a page of colour `page`. */
export function textOnPage(rgba: number, page: number): number {
  if (luminance(page) > 0.18 || contrast(rgba, page) >= 4.5) return rgba >>> 0;
  const key = `${rgba >>> 0}:${page >>> 0}`;
  const hit = cache.get(key);
  if (hit !== undefined) return hit;
  const [r, g, b] = rgb(rgba);
  const [L, A, B] = toOklab(lin(r), lin(g), lin(b));
  const alpha = rgba & 0xff;
  let out = rgba >>> 0;
  for (let l = Math.max(1 - L, L); l <= 1.0001; l += 0.03) {
    const [lr, lg, lb] = fromOklab(Math.min(1, l), A, B);
    out = ((enc(lr) << 24) | (enc(lg) << 16) | (enc(lb) << 8) | alpha) >>> 0;
    if (contrast(out, page) >= 4.5) break;
  }
  if (cache.size > 512) cache.clear();
  cache.set(key, out);
  return out;
}
