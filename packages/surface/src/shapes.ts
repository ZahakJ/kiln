/**
 * Shapes both painters draw identically: the TypeScript twin of `kiln_scene::shapes`. Keep the
 * numbers in step with `crates/kiln-scene/src/shapes.rs` (the painter-parity e2e test compares
 * the canvas with kiln-raster pixel by pixel).
 */

import type { Bounds, HandleSide, PathData } from '@kiln/protocol';

/** Cubic Bézier constant for a quarter circle. */
export const KAPPA = 0.5522848;

/** Verb codes of {@link PathData}. */
export const VERB = { MOVE: 0, LINE: 1, QUAD: 2, CUBIC: 3, CLOSE: 4 } as const;

const ARITY = [2, 2, 4, 6, 0];

/** Builds paths in the wire form. */
export class PathBuilder {
  readonly verbs: number[] = [];
  readonly coords: number[] = [];
  moveTo(x: number, y: number): this {
    this.verbs.push(VERB.MOVE);
    this.coords.push(x, y);
    return this;
  }
  lineTo(x: number, y: number): this {
    this.verbs.push(VERB.LINE);
    this.coords.push(x, y);
    return this;
  }
  quadTo(cx: number, cy: number, x: number, y: number): this {
    this.verbs.push(VERB.QUAD);
    this.coords.push(cx, cy, x, y);
    return this;
  }
  cubicTo(a: number, b: number, c: number, d: number, x: number, y: number): this {
    this.verbs.push(VERB.CUBIC);
    this.coords.push(a, b, c, d, x, y);
    return this;
  }
  close(): this {
    this.verbs.push(VERB.CLOSE);
    return this;
  }
  build(): PathData {
    return { verbs: this.verbs, coords: this.coords };
  }
}

/** Whether verbs and coordinates agree and every coordinate is finite. */
export function isValidPath(p: PathData): boolean {
  let needed = 0;
  for (const v of p.verbs) {
    if (v > VERB.CLOSE) return false;
    needed += ARITY[v] ?? 0;
  }
  return needed === p.coords.length && p.coords.every(Number.isFinite);
}

/** Replays a path on anything with the canvas path methods (`Path2D`, a context). */
export function tracePath(
  p: PathData,
  target: Pick<Path2D, 'moveTo' | 'lineTo' | 'quadraticCurveTo' | 'bezierCurveTo' | 'closePath'>,
): boolean {
  if (!isValidPath(p)) return false;
  const c = p.coords;
  let i = 0;
  for (const v of p.verbs) {
    switch (v) {
      case VERB.MOVE:
        target.moveTo(c[i] as number, c[i + 1] as number);
        break;
      case VERB.LINE:
        target.lineTo(c[i] as number, c[i + 1] as number);
        break;
      case VERB.QUAD:
        target.quadraticCurveTo(c[i] as number, c[i + 1] as number, c[i + 2] as number, c[i + 3] as number);
        break;
      case VERB.CUBIC:
        target.bezierCurveTo(
          c[i] as number,
          c[i + 1] as number,
          c[i + 2] as number,
          c[i + 3] as number,
          c[i + 4] as number,
          c[i + 5] as number,
        );
        break;
      default:
        target.closePath();
    }
    i += ARITY[v] ?? 0;
  }
  return true;
}

/** A rectangle with corners of radius `r` (clamped to half the shorter side). */
export function roundedRect(b: Bounds, radius: number): PathData {
  const r = Math.max(0, Math.min(radius, Math.min(b.w, b.h) / 2));
  const x0 = b.x;
  const y0 = b.y;
  const x1 = b.x + b.w;
  const y1 = b.y + b.h;
  const p = new PathBuilder();
  if (r <= 0) return p.moveTo(x0, y0).lineTo(x1, y0).lineTo(x1, y1).lineTo(x0, y1).close().build();
  const k = r * (1 - KAPPA);
  return p
    .moveTo(x0 + r, y0)
    .lineTo(x1 - r, y0)
    .cubicTo(x1 - k, y0, x1, y0 + k, x1, y0 + r)
    .lineTo(x1, y1 - r)
    .cubicTo(x1, y1 - k, x1 - k, y1, x1 - r, y1)
    .lineTo(x0 + r, y1)
    .cubicTo(x0 + k, y1, x0, y1 - k, x0, y1 - r)
    .lineTo(x0, y0 + r)
    .cubicTo(x0, y0 + k, x0 + k, y0, x0 + r, y0)
    .close()
    .build();
}

const SPARK: ReadonlyArray<readonly [number, number]> = [
  [12, 1],
  [13.85, 7.38],
  [16.62, 10.15],
  [20.14, 12],
  [16.62, 13.85],
  [13.85, 16.62],
  [12, 23],
  [10.15, 16.62],
  [7.38, 13.85],
  [3.86, 12],
  [7.38, 10.15],
  [10.15, 7.38],
  [12, 1],
];

/** The Kiln spark (brand kit `spark.svg`) centred on `(cx, cy)`, `size` across. */
export function spark(cx: number, cy: number, size: number): PathData {
  const s = size / 24;
  const m = (p: readonly [number, number]): [number, number] => [(p[0] - 12) * s + cx, (p[1] - 12) * s + cy];
  const b = new PathBuilder();
  const [x, y] = m(SPARK[0] as readonly [number, number]);
  b.moveTo(x, y);
  for (let i = 1; i + 2 < SPARK.length + 1; i += 3) {
    const a = m(SPARK[i] as readonly [number, number]);
    const c = m(SPARK[i + 1] as readonly [number, number]);
    const e = m(SPARK[i + 2] as readonly [number, number]);
    b.cubicTo(a[0], a[1], c[0], c[1], e[0], e[1]);
  }
  return b.close().build();
}

/** The spark crowning a caret bar: 8 px, centred on the bar, 3 px above its top. */
export function caretSpark(caret: Bounds): PathData {
  return spark(caret.x + caret.w / 2, caret.y - 3, 8);
}

/** A wavy squiggle centre line from `x` to `x + width` around `y` (stroke it 1 px wide). */
export function squiggle(x: number, y: number, width: number): PathData {
  const p = new PathBuilder();
  if (!(width > 0 && Number.isFinite(x) && Number.isFinite(y) && Number.isFinite(width))) return p.build();
  const w = Math.min(width, 16384);
  p.moveTo(x, y);
  let at = 0;
  let up = true;
  while (at < w) {
    const step = Math.min(2, w - at);
    p.quadTo(x + at + step / 2, up ? y - 1.5 : y + 1.5, x + at + step, y);
    at += step;
    up = !up;
  }
  return p.build();
}

/** A teardrop touch handle with its sharp corner at `(x, y)`, `size` across. */
export function teardrop(x: number, y: number, size: number, side: HandleSide): PathData {
  const r = size / 2;
  const k = r * KAPPA;
  const segs = [
    [r + k, 0, 2 * r, r - k, 2 * r, r],
    [2 * r, r + k, r + k, 2 * r, r, 2 * r],
    [r - k, 2 * r, 0, r + k, 0, r],
  ];
  const [cos, sin] = side === 'Caret' ? [Math.SQRT1_2, Math.SQRT1_2] : [1, 0];
  const mirror = side === 'Start' ? -1 : 1;
  const t = (px: number, py: number): [number, number] => {
    const mx = px * mirror;
    return [x + mx * cos - py * sin, y + mx * sin + py * cos];
  };
  const b = new PathBuilder().moveTo(x, y);
  const [lx, ly] = t(r, 0);
  b.lineTo(lx, ly);
  for (const s of segs) {
    const [a, bb] = t(s[0] as number, s[1] as number);
    const [c, d] = t(s[2] as number, s[3] as number);
    const [e, f] = t(s[4] as number, s[5] as number);
    b.cubicTo(a, bb, c, d, e, f);
  }
  return b.close().build();
}

/** Error function (Abramowitz & Stegun 7.1.26), as in kiln-scene. */
export function erf(x: number): number {
  const sign = x < 0 ? -1 : 1;
  const ax = Math.abs(x);
  const t = 1 / (1 + 0.3275911 * ax);
  const y =
    1 -
    ((((1.061405429 * t - 1.453152027) * t + 1.421413741) * t - 0.284496736) * t + 0.254829592) *
      t *
      Math.exp(-ax * ax);
  return sign * y;
}

/** Coverage of a Gaussian-blurred interval `[a, b]` (σ = `sigma`) at `t`. */
export function blurredInterval(t: number, a: number, b: number, sigma: number): number {
  if (sigma <= 0) return t >= a && t < b ? 1 : 0;
  const s = sigma * Math.SQRT2;
  return 0.5 * (erf((t - a) / s) - erf((t - b) / s));
}

/** `[x, width]` of the dashes of an underline. */
export function underlineSegments(
  x: number,
  width: number,
  thickness: number,
  style: 'Solid' | 'Dotted' | 'Dashed',
): Array<[number, number]> {
  if (!(width > 0 && Number.isFinite(width) && Number.isFinite(x))) return [];
  const w = Math.min(width, 16384);
  if (style === 'Solid') return [[x, w]];
  const t = Math.max(thickness, 1);
  const [on, off] = style === 'Dotted' ? [t, t] : [t * 3, t * 2];
  const out: Array<[number, number]> = [];
  for (let at = 0; at < w; at += on + off) out.push([x + at, Math.min(on, w - at)]);
  return out;
}
