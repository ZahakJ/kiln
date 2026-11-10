import { glyphs, isGlyphName } from '../icons/glyphs/index.ts';
import { Icon, type IconSource } from '../icons/Icon.tsx';
import { LUCIDE_COMMAND_ICONS } from './lucideIcons.ts';

const extra = new Map<string, IconSource>();

/**
 * Resolves an icon name from command metadata or a ribbon spec: an Kiln glyph (`FormatPainter`),
 * a curated Lucide icon (`Bold`), or one added with `registerCommandIcons`.
 */
export function resolveCommandIcon(name: string | undefined): IconSource | undefined {
  if (!name) return undefined;
  if (isGlyphName(name)) return name;
  return LUCIDE_COMMAND_ICONS[name] ?? extra.get(name);
}

export function hasCommandIcon(name: string): boolean {
  return resolveCommandIcon(name) !== undefined;
}

/** Lets an app or plug-in add icons by name (e.g. a Lucide icon not in the curated set). */
export function registerCommandIcons(icons: Readonly<Record<string, IconSource>>): void {
  for (const [name, icon] of Object.entries(icons)) extra.set(name, icon);
}

/** Every resolvable icon name (glyphs and curated Lucide icons). */
export function commandIconNames(): string[] {
  return [...Object.keys(glyphs), ...Object.keys(LUCIDE_COMMAND_ICONS), ...extra.keys()];
}

export interface CommandIconProps {
  name: string | undefined;
  size?: number;
  className?: string;
}

/** Renders a named icon, or nothing when the name is unknown (validated by `validateRibbonSpec`). */
export function CommandIcon({ name, size = 16, className }: CommandIconProps) {
  const icon = resolveCommandIcon(name);
  return icon ? <Icon icon={icon} size={size} className={className} /> : null;
}
