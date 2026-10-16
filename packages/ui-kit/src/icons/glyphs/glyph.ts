/**
 * Kiln Glyphs: the factory and drawing helpers. Every glyph is a 20×20 `viewBox`, 1.5 px stroke,
 * round caps/joins and `currentColor` — consistent with Lucide at 20 px (PLAN.md §1.2).
 *
 * Parts flagged `accent` take the active app accent in the "colourful" icon style
 * (`data-icon-style="colorful"` on any ancestor) and stay `currentColor` otherwise.
 */
import { createElement, type ReactElement, type SVGProps } from 'react';

export interface GlyphPart {
  d: string;
  /** Filled instead of stroked. */
  fill?: boolean;
  /** Coloured in the colourful icon style. */
  accent?: boolean;
  /** Reduced emphasis (secondary detail). */
  muted?: boolean;
}

export type GlyphPartInput = string | GlyphPart;

export interface GlyphProps extends Omit<SVGProps<SVGSVGElement>, 'children'> {
  /** Rendered size in px (width and height). Default 20. */
  size?: number | string;
  /** Accessible name; when omitted the glyph is decorative (`aria-hidden`). */
  title?: string;
  /** Stroke width in viewBox units. Default 1.5. */
  strokeWidth?: number;
}

export interface GlyphComponent {
  (props: GlyphProps): ReactElement;
  displayName: string;
  /** Stable kebab-case id, e.g. `format-painter`. */
  glyphId: string;
  /** Human-readable name used in docs and the gallery. */
  glyphLabel: string;
  parts: readonly GlyphPart[];
}

const kebab = (name: string) => name.replace(/([a-z0-9])([A-Z])/g, '$1-$2').toLowerCase();

export function glyph(name: string, label: string, input: readonly GlyphPartInput[]): GlyphComponent {
  const parts = input.map((p) => (typeof p === 'string' ? { d: p } : p));
  const glyphId = kebab(name);
  const Component = ({ size = 20, title, strokeWidth = 1.5, className, ...rest }: GlyphProps) =>
    createElement(
      'svg',
      {
        xmlns: 'http://www.w3.org/2000/svg',
        viewBox: '0 0 20 20',
        width: size,
        height: size,
        fill: 'none',
        stroke: 'currentColor',
        strokeWidth,
        strokeLinecap: 'round',
        strokeLinejoin: 'round',
        className: className ? `kiln-glyph ${className}` : 'kiln-glyph',
        'data-glyph': glyphId,
        role: title ? 'img' : undefined,
        'aria-hidden': title ? undefined : true,
        'aria-label': title,
        focusable: 'false',
        ...rest,
      },
      ...parts.map((p, i) =>
        createElement('path', {
          key: i,
          d: p.d,
          fill: p.fill ? 'currentColor' : undefined,
          stroke: p.fill ? 'none' : undefined,
          'data-accent': p.accent ? '' : undefined,
          opacity: p.muted ? 0.55 : undefined,
        }),
      ),
    );
  return Object.assign(Component, { displayName: `Glyph(${name})`, glyphId, glyphLabel: label, parts });
}

/** Serialises a glyph to a standalone SVG string (sprites, docs, tests). */
export function glyphToSvg(g: GlyphComponent, size = 20): string {
  const paths = g.parts
    .map((p) =>
      p.fill
        ? `<path d="${p.d}" fill="currentColor" stroke="none"${p.muted ? ' opacity="0.55"' : ''}/>`
        : `<path d="${p.d}"${p.muted ? ' opacity="0.55"' : ''}/>`,
    )
    .join('');
  return `<svg xmlns="http://www.w3.org/2000/svg" viewBox="0 0 20 20" width="${size}" height="${size}" fill="none" stroke="currentColor" stroke-width="1.5" stroke-linecap="round" stroke-linejoin="round">${paths}</svg>`;
}

// ---------------------------------------------------------------------------------------------
// Drawing helpers (all coordinates in the 20×20 viewBox).
// ---------------------------------------------------------------------------------------------

const n = (v: number) => Number(v.toFixed(3)).toString();

/** Rectangle, optionally rounded. */
export function rect(x: number, y: number, w: number, h: number, r = 0): string {
  if (!r) return `M${n(x)} ${n(y)}h${n(w)}v${n(h)}h${n(-w)}z`;
  return `M${n(x + r)} ${n(y)}h${n(w - 2 * r)}a${n(r)} ${n(r)} 0 0 1 ${n(r)} ${n(r)}v${n(h - 2 * r)}a${n(r)} ${n(r)} 0 0 1 ${n(-r)} ${n(r)}h${n(-(w - 2 * r))}a${n(r)} ${n(r)} 0 0 1 ${n(-r)} ${n(-r)}v${n(-(h - 2 * r))}a${n(r)} ${n(r)} 0 0 1 ${n(r)} ${n(-r)}z`;
}

/** Circle. */
export function circle(cx: number, cy: number, r: number): string {
  return `M${n(cx - r)} ${n(cy)}a${n(r)} ${n(r)} 0 1 0 ${n(2 * r)} 0a${n(r)} ${n(r)} 0 1 0 ${n(-2 * r)} 0z`;
}

/** Straight segments through the given points (pairs). */
export function line(...pts: number[]): string {
  let d = '';
  for (let i = 0; i < pts.length; i += 2)
    d += `${i === 0 ? 'M' : 'L'}${n(pts[i] ?? 0)} ${n(pts[i + 1] ?? 0)}`;
  return d;
}

/** Several horizontal lines starting at x with the given widths, `gap` apart from y. */
export function hlines(x: number, y: number, gap: number, widths: readonly number[]): string {
  return widths.map((w, i) => `M${n(x)} ${n(y + i * gap)}h${n(w)}`).join('');
}

/** Portrait page with a folded corner. */
export function page(x = 4.75, y = 2.75, w = 10.5, h = 14.5, fold = 3): string {
  return `M${n(x)} ${n(y)}h${n(w - fold)}l${n(fold)} ${n(fold)}v${n(h - fold)}h${n(-w)}zM${n(x + w - fold)} ${n(y)}v${n(fold)}h${n(fold)}`;
}

/** Small "+" badge centred at (cx, cy). */
export function plus(cx: number, cy: number, s = 2.5): string {
  return `M${n(cx - s)} ${n(cy)}h${n(2 * s)}M${n(cx)} ${n(cy - s)}v${n(2 * s)}`;
}

/** Small "×" centred at (cx, cy). */
export function cross(cx: number, cy: number, s = 2): string {
  return `M${n(cx - s)} ${n(cy - s)}l${n(2 * s)} ${n(2 * s)}M${n(cx + s)} ${n(cy - s)}l${n(-2 * s)} ${n(2 * s)}`;
}

/** Arrow from (x1,y1) to (x2,y2) with a head of size `h`. */
export function arrow(x1: number, y1: number, x2: number, y2: number, h = 2.5): string {
  const a = Math.atan2(y2 - y1, x2 - x1);
  const p = (ang: number) => [x2 - h * Math.cos(a + ang), y2 - h * Math.sin(a + ang)] as const;
  const [ax, ay] = p(Math.PI / 4);
  const [bx, by] = p(-Math.PI / 4);
  return `M${n(x1)} ${n(y1)}L${n(x2)} ${n(y2)}M${n(ax)} ${n(ay)}L${n(x2)} ${n(y2)}L${n(bx)} ${n(by)}`;
}

/** Chevron pointing in `dir` centred at (cx, cy). */
export function chevron(cx: number, cy: number, dir: 'up' | 'down' | 'left' | 'right', s = 2): string {
  switch (dir) {
    case 'up':
      return line(cx - s, cy + s / 2, cx, cy - s / 2, cx + s, cy + s / 2);
    case 'down':
      return line(cx - s, cy - s / 2, cx, cy + s / 2, cx + s, cy - s / 2);
    case 'left':
      return line(cx + s / 2, cy - s, cx - s / 2, cy, cx + s / 2, cy + s);
    default:
      return line(cx - s / 2, cy - s, cx + s / 2, cy, cx - s / 2, cy + s);
  }
}

/** Table grid with `cols`×`rows` cells. */
export function grid(x: number, y: number, w: number, h: number, cols: number, rows: number, r = 1): string {
  let d = rect(x, y, w, h, r);
  for (let c = 1; c < cols; c++) d += `M${n(x + (w * c) / cols)} ${n(y)}v${n(h)}`;
  for (let k = 1; k < rows; k++) d += `M${n(x)} ${n(y + (h * k) / rows)}h${n(w)}`;
  return d;
}

/** Filled dot. */
export function dot(cx: number, cy: number, r = 1): GlyphPart {
  return { d: circle(cx, cy, r), fill: true };
}
