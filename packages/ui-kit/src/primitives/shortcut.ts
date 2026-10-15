/**
 * Keyboard shortcut formatting. Shortcuts are written platform-neutrally, e.g. `Mod+Shift+S`, where
 * `Mod` is Ctrl on Windows/Linux/Android and ⌘ on macOS (PLAN.md §1.7).
 */
export type Platform = 'mac' | 'other';

export function detectPlatform(): Platform {
  if (typeof navigator === 'undefined') return 'other';
  const nav = navigator as Navigator & { userAgentData?: { platform?: string } };
  const p = nav.userAgentData?.platform ?? nav.platform ?? '';
  return /mac|iphone|ipad|ipod/i.test(p) ? 'mac' : 'other';
}

const MAC: Record<string, string> = {
  mod: '⌘',
  cmd: '⌘',
  meta: '⌘',
  ctrl: '⌃',
  control: '⌃',
  alt: '⌥',
  option: '⌥',
  shift: '⇧',
  enter: '↩',
  return: '↩',
  backspace: '⌫',
  delete: '⌦',
  escape: 'Esc',
  esc: 'Esc',
  tab: '⇥',
};

const OTHER: Record<string, string> = {
  mod: 'Ctrl',
  cmd: 'Ctrl',
  meta: 'Win',
  ctrl: 'Ctrl',
  control: 'Ctrl',
  alt: 'Alt',
  option: 'Alt',
  shift: 'Shift',
  enter: 'Enter',
  return: 'Enter',
  backspace: 'Backspace',
  delete: 'Del',
  escape: 'Esc',
  esc: 'Esc',
  tab: 'Tab',
};

const COMMON: Record<string, string> = {
  arrowup: '↑',
  arrowdown: '↓',
  arrowleft: '←',
  arrowright: '→',
  space: 'Space',
  pageup: 'PgUp',
  pagedown: 'PgDn',
  plus: '+',
};

/** Splits `Mod+Shift+S` (or `Ctrl+=`, `Ctrl++`) into display keys for the platform. */
export function formatShortcut(shortcut: string, platform: Platform = detectPlatform()): string[] {
  const raw = shortcut.trim();
  // Split on "+" but keep a trailing literal "+" key (e.g. "Ctrl++").
  const parts = raw.endsWith('++') ? [...raw.slice(0, -2).split('+'), '+'] : raw.split('+');
  const table = platform === 'mac' ? MAC : OTHER;
  return parts
    .filter((p) => p.length > 0)
    .map((p) => {
      const k = p.toLowerCase();
      return table[k] ?? COMMON[k] ?? (p.length === 1 ? p.toUpperCase() : p);
    });
}

/** Display string: `Ctrl+Shift+S` or `⌘⇧S` on macOS. */
export function shortcutLabel(shortcut: string, platform: Platform = detectPlatform()): string {
  const keys = formatShortcut(shortcut, platform);
  return platform === 'mac' ? keys.join('') : keys.join('+');
}

/** Value for `aria-keyshortcuts` (WAI-ARIA key names). */
export function ariaKeyShortcuts(shortcut: string, platform: Platform = detectPlatform()): string {
  const raw = shortcut.trim();
  const parts = raw.endsWith('++') ? [...raw.slice(0, -2).split('+'), 'Plus'] : raw.split('+');
  return parts
    .filter((p) => p.length > 0)
    .map((p) => {
      const k = p.toLowerCase();
      if (k === 'mod' || k === 'cmd') return platform === 'mac' ? 'Meta' : 'Control';
      if (k === 'ctrl') return 'Control';
      if (k === 'esc') return 'Escape';
      if (k === 'del') return 'Delete';
      return p.length === 1 ? p.toUpperCase() : p;
    })
    .join('+');
}
