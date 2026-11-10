/**
 * Keyboard shortcut formatting. Shortcuts are written platform-neutrally, e.g. `Mod+Shift+S`, where
 * `Mod` is Ctrl on Windows/Linux/Android and ⌘ on macOS.
 */
import { tOr } from '@kiln/i18n';
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

/** Key names as the keyboard of the interface language labels them (German Strg, Entf, Pos1). */
function otherKeys(): Record<string, string> {
  const ctrl = tOr('key.ctrl', 'Ctrl');
  const alt = tOr('key.alt', 'Alt');
  const enter = tOr('key.enter', 'Enter');
  const esc = tOr('key.esc', 'Esc');
  return {
    mod: ctrl,
    cmd: ctrl,
    meta: tOr('key.win', 'Win'),
    ctrl,
    control: ctrl,
    alt,
    option: alt,
    shift: tOr('key.shift', 'Shift'),
    enter,
    return: enter,
    backspace: tOr('key.backspace', 'Backspace'),
    delete: tOr('key.delete', 'Del'),
    del: tOr('key.delete', 'Del'),
    escape: esc,
    esc,
    tab: tOr('key.tab', 'Tab'),
  };
}

function commonKeys(): Record<string, string> {
  return {
    arrowup: '↑',
    arrowdown: '↓',
    arrowleft: '←',
    arrowright: '→',
    space: tOr('key.space', 'Space'),
    pageup: tOr('key.pageUp', 'PgUp'),
    pagedown: tOr('key.pageDown', 'PgDn'),
    home: tOr('key.home', 'Home'),
    end: tOr('key.end', 'End'),
    insert: tOr('key.insert', 'Ins'),
    plus: '+',
  };
}

/** Splits `Mod+Shift+S` (or `Ctrl+=`, `Ctrl++`) into display keys for the platform. */
export function formatShortcut(shortcut: string, platform: Platform = detectPlatform()): string[] {
  const raw = shortcut.trim();
  // Split on "+" but keep a trailing literal "+" key (e.g. "Ctrl++").
  const parts = raw.endsWith('++') ? [...raw.slice(0, -2).split('+'), '+'] : raw.split('+');
  const table = platform === 'mac' ? MAC : otherKeys();
  const common = commonKeys();
  return parts
    .filter((p) => p.length > 0)
    .map((p) => {
      const k = p.toLowerCase();
      return table[k] ?? common[k] ?? (p.length === 1 ? p.toUpperCase() : p);
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
