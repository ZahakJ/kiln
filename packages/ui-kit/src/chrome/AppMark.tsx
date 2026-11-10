import type { AppId } from '@kiln/commands';
import { AppTile, type BrandApp } from '../icons/brand/BrandMark.tsx';

export const APP_NAMES: Readonly<Record<AppId, string>> = {
  write: 'Kiln Write',
  grid: 'Kiln Grid',
  deck: 'Kiln Deck',
  notes: 'Kiln Notes',
  hub: 'Kiln Home',
};

/** The short names the title bar shows next to the tile (mockups: "Write", "Grid"…). */
export const APP_SHORT_NAMES: Readonly<Record<AppId, string>> = {
  write: 'Write',
  grid: 'Grid',
  deck: 'Deck',
  notes: 'Notes',
  hub: 'Kiln',
};

/** The brand kit's name for an app's tile (Home is the suite tile). */
export function brandAppOf(app: AppId): BrandApp {
  return app === 'hub' ? 'home' : app;
}

/**
 * The app's glazed tile from the brand kit (`<AppTile>` picks the drawing made for the size).
 * Decorative unless `label` is given.
 */
export function AppMark({ app = 'hub', size = 24, label }: { app?: AppId; size?: number; label?: string }) {
  return <AppTile app={brandAppOf(app)} size={size} label={label} className="kiln-appmark" />;
}
