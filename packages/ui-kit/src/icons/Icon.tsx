import type { LucideIcon } from 'lucide-react';
import type { ComponentType, CSSProperties, SVGProps } from 'react';
import { type GlyphName, glyphs, isGlyphName } from './glyphs/index.ts';

/** Anything `Icon` can render: an Officina glyph name, a Lucide icon, or any SVG component. */
export type IconSource =
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

/**
 * Renders a Lucide icon or an Officina glyph with consistent size, 1.5 px stroke and `currentColor`.
 * Decorative by default (`aria-hidden`); pass `label` for a standalone meaningful icon.
 */
export function Icon({ icon, size = 16, label, className, style, strokeWidth, ...rest }: IconProps) {
  const common = {
    className: className ? `of-icon ${className}` : 'of-icon',
    style: { flex: 'none', ...style } as CSSProperties,
    width: size,
    height: size,
    'aria-hidden': label ? undefined : true,
    'aria-label': label,
    role: label ? 'img' : undefined,
    focusable: 'false' as const,
    ...rest,
  };
  if (typeof icon === 'string') {
    if (!isGlyphName(icon)) throw new Error(`Icon: unknown Officina glyph "${icon}"`);
    const Glyph = glyphs[icon];
    return <Glyph size={size} strokeWidth={strokeWidth} {...common} />;
  }
  const Component = icon as ComponentType<
    SVGProps<SVGSVGElement> & { size?: number; absoluteStrokeWidth?: boolean }
  >;
  // Lucide draws on a 24 grid; 1.5 px at 20 px ≈ 1.8 units, matching the glyph stroke.
  return <Component size={size} strokeWidth={strokeWidth ?? (1.5 * 24) / 20} {...common} />;
}
