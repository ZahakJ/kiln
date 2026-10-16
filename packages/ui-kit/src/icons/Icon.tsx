import type { LucideIcon } from 'lucide-react';
import type { ComponentType, CSSProperties, SVGProps } from 'react';
import { BrandMark, type BrandMarkName } from './brand/BrandMark.tsx';
import { BRAND_MARK_SVG } from './brand/marks.generated.ts';
import { type GlyphName, glyphs, isGlyphName } from './glyphs/index.ts';

/** A mark from the brand kit's glyph set, e.g. `brand:spark`, `brand:write`. */
export type BrandIconName = `brand:${BrandMarkName}`;

/**
 * Anything `Icon` can render, in order of preference (PLAN.md §1.2): a brand-kit mark (`brand:*`), a
 * Kiln glyph name (Office-specific commands the kit does not draw), a Lucide icon (generic UI), or any
 * SVG component.
 */
export type IconSource =
  | BrandIconName
  | GlyphName
  | LucideIcon
  | ComponentType<SVGProps<SVGSVGElement> & { size?: number | string }>;

export interface IconProps extends Omit<SVGProps<SVGSVGElement>, 'ref' | 'strokeWidth'> {
  /** Stroke width in the icon's own units (default matches 1.5 px at 20 px). */
  strokeWidth?: number;
  icon: IconSource;
  /** px; 20 in the ribbon, 16 in menus (PLAN.md §1.2). Default 16. */
  size?: number;
  /** Accessible name. Omit for decorative icons (the usual case next to a visible label). */
  label?: string;
}

/** The kit draws its Lucide icons with a 1.75 stroke on the 24 grid (mockups/src/kiln-ui.css). */
export const LUCIDE_STROKE = 1.75;

const isBrandIcon = (icon: string): icon is BrandIconName =>
  icon.startsWith('brand:') && Object.hasOwn(BRAND_MARK_SVG, icon.slice(6));

/**
 * Renders a brand mark, a Kiln glyph or a Lucide icon with consistent size and `currentColor`.
 * Decorative by default (`aria-hidden`); pass `label` for a standalone meaningful icon.
 */
export function Icon({ icon, size = 16, label, className, style, strokeWidth, ...rest }: IconProps) {
  const common = {
    className: className ? `kiln-icon ${className}` : 'kiln-icon',
    style: { flex: 'none', ...style } as CSSProperties,
    width: size,
    height: size,
    'aria-hidden': label ? undefined : true,
    'aria-label': label,
    role: label ? 'img' : undefined,
    focusable: 'false' as const,
    ...rest,
  };
  if (typeof icon === 'string' && icon.startsWith('brand:')) {
    if (!isBrandIcon(icon)) throw new Error(`Icon: unknown brand mark "${icon}"`);
    return (
      <BrandMark
        name={icon.slice(6) as BrandMarkName}
        size={size}
        label={label}
        className={common.className}
        style={common.style}
      />
    );
  }
  if (typeof icon === 'string') {
    if (!isGlyphName(icon)) throw new Error(`Icon: unknown Kiln glyph "${icon}"`);
    const Glyph = glyphs[icon];
    return <Glyph size={size} strokeWidth={strokeWidth} {...common} />;
  }
  const Component = icon as ComponentType<
    SVGProps<SVGSVGElement> & { size?: number; absoluteStrokeWidth?: boolean }
  >;
  return <Component size={size} strokeWidth={strokeWidth ?? LUCIDE_STROKE} {...common} />;
}
