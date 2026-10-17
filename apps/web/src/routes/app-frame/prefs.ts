/**
 * UI preferences (theme, ribbon layout, collapsed ribbon, dismissed banners) go through the host's
 * preference store (`HostOp::StorePref` / `LoadPref`), the same store the core uses, so the desktop
 * shell keeps them with its other settings and the web build in `localStorage`.
 */
import { type HostHandler, WebHost } from '@kiln/protocol';
import type { ThemePreference } from '@kiln/tokens';

export interface UiPrefs {
  theme: ThemePreference;
  ribbonLayout: 'auto' | 'classic' | 'simplified';
  ribbonCollapsed: boolean;
  switcherDismissed: boolean;
}

export const DEFAULT_PREFS: UiPrefs = {
  theme: 'system',
  ribbonLayout: 'auto',
  ribbonCollapsed: false,
  switcherDismissed: false,
};

const KEYS: Record<keyof UiPrefs, string> = {
  theme: 'ui.theme',
  ribbonLayout: 'ui.ribbon.layout',
  ribbonCollapsed: 'ui.ribbon.collapsed',
  switcherDismissed: 'ui.home.switcher_dismissed',
};

const THEMES: readonly ThemePreference[] = ['system', 'light', 'dark', 'hc'];
const LAYOUTS: readonly UiPrefs['ribbonLayout'][] = ['auto', 'classic', 'simplified'];

let host: HostHandler = new WebHost();

/** Swaps the preference host (tests). */
export function setPrefsHost(next: HostHandler): void {
  host = next;
}

async function load(key: string): Promise<string | null> {
  const reply = await host.handle({ LoadPref: { key } });
  return typeof reply === 'object' && 'Text' in reply ? reply.Text : null;
}

/** Reads every preference (unknown or damaged values fall back to the defaults). */
export async function loadPrefs(): Promise<UiPrefs> {
  const [theme, layout, collapsed, dismissed] = await Promise.all([
    load(KEYS.theme),
    load(KEYS.ribbonLayout),
    load(KEYS.ribbonCollapsed),
    load(KEYS.switcherDismissed),
  ]);
  return {
    theme: THEMES.includes(theme as ThemePreference) ? (theme as ThemePreference) : DEFAULT_PREFS.theme,
    ribbonLayout: LAYOUTS.includes(layout as UiPrefs['ribbonLayout'])
      ? (layout as UiPrefs['ribbonLayout'])
      : DEFAULT_PREFS.ribbonLayout,
    ribbonCollapsed: collapsed === '1',
    switcherDismissed: dismissed === '1',
  };
}

/** Stores one preference; failures (private mode) only lose the choice for later sessions. */
export async function storePref<K extends keyof UiPrefs>(key: K, value: UiPrefs[K]): Promise<void> {
  const text = typeof value === 'boolean' ? (value ? '1' : '0') : String(value);
  const reply = await host.handle({ StorePref: { key: KEYS[key], value: text } });
  if (typeof reply === 'object' && 'Failed' in reply) {
    console.warn(`Kiln: could not store the ${key} preference: ${reply.Failed.message}`);
  }
}
