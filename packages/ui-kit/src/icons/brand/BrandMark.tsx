/**
 * The brand kit's own glyphs (PLAN.md §1.2, BRAND.md §2 and §5): the single-colour app marks (the
 * pilcrow, the table with its fill handle, the pulled pie slice, the folded page, the kiln), the spark
 * and the flicked flame, plus the glazed app tiles at the kit's four levels of detail.
 *
 * Marks are `currentColor` (except the flame, which always burns in fire colours) and follow the
 * `Icon` contract: decorative by default, `label` for a meaningful standalone mark.
 */
import {
  type ComponentPropsWithRef,
  type CSSProperties,
  createElement,
  type ReactNode,
  type SVGProps,
} from 'react';
import { BRAND_MARK_SVG } from './marks.generated.ts';
import type { SvgNode } from './svg-node.ts';

export type BrandMarkName = keyof typeof BRAND_MARK_SVG;
/** Apps with a mark and a tile. `home` is the suite kiln. */
export type BrandApp = 'home' | 'write' | 'grid' | 'deck' | 'notes';

const markOf = (app: BrandApp): BrandMarkName => (app === 'home' ? 'suite' : app);

/** SVG attribute names in markup → React prop names. */
const reactAttr = (name: string) => {
  if (name === 'class') return 'className';
  if (name.startsWith('xlink:')) return `xlink${name[6]?.toUpperCase()}${name.slice(7)}`;
  return name.replace(/-([a-z])/g, (_, c: string) => c.toUpperCase());
};

function render(nodes: readonly SvgNode[], prefix: string): ReactNode[] {
  return nodes.map(([tag, attrs, children], i) => {
    const props: Record<string, string> = { key: `${prefix}${i}` };
    for (const [k, v] of Object.entries(attrs)) props[reactAttr(k)] = v;
    return createElement(tag, props, ...render(children, `${prefix}${i}.`));
  });
}

export interface BrandMarkProps extends Omit<SVGProps<SVGSVGElement>, 'ref' | 'children'> {
  name: BrandMarkName;
  /** px (square). Default 16. */
  size?: number;
  /** Accessible name; omit for decorative marks next to a visible label. */
  label?: string;
}

/** One of the kit's marks as inline SVG. */
export function BrandMark({ name, size = 16, label, className, style, ...rest }: BrandMarkProps) {
  const mark = BRAND_MARK_SVG[name];
  return createElement(
    'svg',
    {
      xmlns: 'http://www.w3.org/2000/svg',
      viewBox: mark.viewBox,
      width: size,
      height: size,
      className: className ? `kiln-brand-mark ${className}` : 'kiln-brand-mark',
      'data-mark': name,
      style: { flex: 'none', ...style } as CSSProperties,
      'aria-hidden': label ? undefined : true,
      'aria-label': label,
      role: label ? 'img' : undefined,
      focusable: 'false',
      ...rest,
    },
    ...render(mark.nodes, `${name}.`),
  );
}

/** The single-colour glyph of an app (Write ¶, Grid table, Deck pie, Notes page, Home kiln). */
export function AppGlyph({ app, ...rest }: Omit<BrandMarkProps, 'name'> & { app: BrandApp }) {
  return <BrandMark name={markOf(app)} {...rest} />;
}

/** The four-point spark: the File button, the caret tip, "Take the tour". Use one at a time. */
export function Spark(props: Omit<BrandMarkProps, 'name'>) {
  return <BrandMark name="spark" {...props} />;
}

/** The flicked flame: only for saving ("Saved to Studio", "Autosave on"). Always in fire colours. */
export function Flame({ size = 12, ...rest }: Omit<BrandMarkProps, 'name'>) {
  // The flame artwork is taller than wide (260 × 390): keep its proportions.
  return <BrandMark name="flame" size={size} width={(size * 2) / 3} height={size} {...rest} />;
}

/** Level of detail of a tile for a rendered size (BRAND.md §2: "use the version drawn for the size"). */
export function tileLevel(size: number): '' | '-48' | '-32' | '-16' {
  if (size <= 16) return '-16';
  if (size <= 40) return '-32';
  if (size <= 96) return '-48';
  return '';
}

/** URL of a tile SVG (bundled by Vite from `./tiles/`). */
export function tileUrl(app: BrandApp, size: number): string {
  return new URL(`./tiles/${markOf(app)}${tileLevel(size)}.svg`, import.meta.url).href;
}

export interface AppTileProps extends Omit<ComponentPropsWithRef<'img'>, 'src' | 'width' | 'height'> {
  app: BrandApp;
  /** px (square). The tile drawn for this size is chosen automatically. Default 20. */
  size?: number;
  /** Accessible name; default decorative (empty alt) because a visible app name usually follows. */
  label?: string;
}

/** The glazed app tile (kiln arch) at the right level of detail for its size. */
export function AppTile({ app, size = 20, label, className, ...rest }: AppTileProps) {
  return (
    <img
      src={tileUrl(app, size)}
      width={size}
      height={size}
      alt={label ?? ''}
      draggable={false}
      className={className ? `kiln-app-tile ${className}` : 'kiln-app-tile'}
      data-app-tile={app}
      {...rest}
    />
  );
}
