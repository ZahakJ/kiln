/**
 * Platform-aware key chords. Shortcuts are written platform-neutrally (`Mod+Shift+L`): `Mod` is
 * Ctrl on Windows/Linux/ChromeOS/Android and ⌘ on macOS (PLAN.md §1.7: "Mac uses ⌘ for Ctrl").
 * `Ctrl` always means the Control key. Chords are single key presses (no Emacs-style sequences).
 *
 * Symbols that need Shift on a US layout are normalised to Shift + the base key, so `Ctrl+Shift+>`
 * and `Ctrl+Shift+.` are the same chord, and `Ctrl+Shift+%` is `Ctrl+Shift+5` (which is exactly how
 * Office binds them).
 */
export type Platform = 'mac' | 'other';

export function detectPlatform(): Platform {
  const nav = (globalThis as { navigator?: Navigator & { userAgentData?: { platform?: string } } }).navigator;
  if (!nav) {
    const proc = (globalThis as { process?: { platform?: string } }).process;
    return proc?.platform === 'darwin' ? 'mac' : 'other';
  }
  const p = nav.userAgentData?.platform ?? nav.platform ?? '';
  return /mac|iphone|ipad|ipod/i.test(p) ? 'mac' : 'other';
}

export interface KeyChord {
  /** Canonical key: `a`, `5`, `.`, `f5`, `arrowup`, `pagedown`, `space`, or a bare modifier `alt`. */
  key: string;
  ctrl: boolean;
  alt: boolean;
  shift: boolean;
  meta: boolean;
}

export class ShortcutParseError extends Error {
  override name = 'ShortcutParseError';
}

/** US-layout shifted symbols → base key. */
export const SHIFTED_SYMBOLS: Readonly<Record<string, string>> = {
  '>': '.',
  '<': ',',
  '+': '=',
  _: '-',
  '&': '7',
  '%': '5',
  '!': '1',
  '@': '2',
  '#': '3',
  $: '4',
  '^': '6',
  '*': '8',
  '(': '9',
  ')': '0',
  ':': ';',
  '"': "'",
  '?': '/',
  '{': '[',
  '}': ']',
  '|': '\\',
  '~': '`',
};

const BASE_SYMBOLS = new Set(['.', ',', '=', '-', '[', ']', ';', "'", '`', '/', '\\']);

const NAMED_KEYS: Readonly<Record<string, string>> = {
  esc: 'escape',
  escape: 'escape',
  enter: 'enter',
  return: 'enter',
  tab: 'tab',
  space: 'space',
  spacebar: 'space',
  backspace: 'backspace',
  del: 'delete',
  delete: 'delete',
  ins: 'insert',
  insert: 'insert',
  home: 'home',
  end: 'end',
  pgup: 'pageup',
  pageup: 'pageup',
  'page up': 'pageup',
  pgdn: 'pagedown',
  pagedown: 'pagedown',
  'page down': 'pagedown',
  up: 'arrowup',
  down: 'arrowdown',
  left: 'arrowleft',
  right: 'arrowright',
  arrowup: 'arrowup',
  arrowdown: 'arrowdown',
  arrowleft: 'arrowleft',
  arrowright: 'arrowright',
  contextmenu: 'contextmenu',
  menu: 'contextmenu',
  apps: 'contextmenu',
  minus: '-',
  equal: '=',
  period: '.',
  comma: ',',
  slash: '/',
  backquote: '`',
  semicolon: ';',
};

/** Modifier tokens. `mod` is resolved per platform. */
const MODIFIERS: Readonly<Record<string, 'ctrl' | 'alt' | 'shift' | 'meta' | 'mod'>> = {
  mod: 'mod',
  ctrl: 'ctrl',
  control: 'ctrl',
  '⌃': 'ctrl',
  cmd: 'meta',
  command: 'meta',
  meta: 'meta',
  win: 'meta',
  super: 'meta',
  '⌘': 'meta',
  alt: 'alt',
  option: 'alt',
  opt: 'alt',
  '⌥': 'alt',
  shift: 'shift',
  '⇧': 'shift',
};

const BARE_MODIFIER_KEYS: Readonly<Record<string, string>> = {
  ctrl: 'ctrl',
  alt: 'alt',
  shift: 'shift',
  meta: 'meta',
};

function canonicalKey(token: string): { key: string; shift: boolean } | null {
  const raw = token.trim();
  if (raw.length === 1) {
    const ch = raw.toLowerCase();
    if (/^[a-z0-9]$/.test(ch)) return { key: ch, shift: false };
    if (BASE_SYMBOLS.has(ch)) return { key: ch, shift: false };
    const base = SHIFTED_SYMBOLS[ch];
    if (base) return { key: base, shift: true };
    return null;
  }
  const lower = raw.toLowerCase();
  if (lower === 'plus') return { key: '=', shift: true };
  if (/^f([1-9]|1[0-9]|2[0-4])$/.test(lower)) return { key: lower, shift: false };
  const named = NAMED_KEYS[lower];
  return named ? { key: named, shift: false } : null;
}

/** Splits on `+`, keeping a literal `+` key (`Ctrl++`, `Ctrl+Shift++`). */
function tokens(text: string): string[] {
  const t = text.trim();
  if (t === '+') return ['+'];
  if (t.endsWith('++')) return [...t.slice(0, -2).split('+'), '+'];
  return t.split('+');
}

/** Parses `Mod+Shift+L`, `Ctrl+Page Down`, `F12`, `Alt` (bare modifier) into a chord. */
export function parseShortcut(text: string, platform: Platform = 'other'): KeyChord {
  const parts = tokens(text);
  const chord: KeyChord = { key: '', ctrl: false, alt: false, shift: false, meta: false };
  const lastIndex = parts.length - 1;
  for (let i = 0; i <= lastIndex; i++) {
    const part = parts[i]?.trim() ?? '';
    if (!part) throw new ShortcutParseError(`Empty key in shortcut "${text}"`);
    const mod = MODIFIERS[part.toLowerCase()];
    if (i < lastIndex) {
      if (!mod) throw new ShortcutParseError(`"${part}" is not a modifier in shortcut "${text}"`);
      const flag = mod === 'mod' ? (platform === 'mac' ? 'meta' : 'ctrl') : mod;
      if (chord[flag]) throw new ShortcutParseError(`Duplicate modifier in shortcut "${text}"`);
      chord[flag] = true;
      continue;
    }
    // Last token: the key, or a bare modifier (Alt for Key Tips).
    if (mod && parts.length === 1) {
      const flag = mod === 'mod' ? (platform === 'mac' ? 'meta' : 'ctrl') : mod;
      chord.key = BARE_MODIFIER_KEYS[flag] ?? flag;
      continue;
    }
    const k = canonicalKey(part);
    if (!k) throw new ShortcutParseError(`Unknown key "${part}" in shortcut "${text}"`);
    chord.key = k.key;
    if (k.shift) chord.shift = true;
  }
  return chord;
}

export function tryParseShortcut(text: string, platform: Platform = 'other'): KeyChord | null {
  try {
    return parseShortcut(text, platform);
  } catch {
    return null;
  }
}

/** Stable string id of a chord, usable as a map key: `ctrl+shift+l`. */
export function chordId(c: KeyChord): string {
  return [c.ctrl && 'ctrl', c.alt && 'alt', c.shift && 'shift', c.meta && 'meta', c.key]
    .filter(Boolean)
    .join('+');
}

export function isBareModifier(c: KeyChord): boolean {
  return c.key in BARE_MODIFIER_KEYS;
}

const CODE_KEYS: Readonly<Record<string, string>> = {
  Period: '.',
  Comma: ',',
  Equal: '=',
  Minus: '-',
  BracketLeft: '[',
  BracketRight: ']',
  Semicolon: ';',
  Quote: "'",
  Backquote: '`',
  Slash: '/',
  Backslash: '\\',
  IntlBackslash: '\\',
  NumpadSubtract: '-',
  NumpadDecimal: '.',
  NumpadDivide: '/',
};

const EVENT_MODIFIER_KEYS: Readonly<Record<string, string>> = {
  Control: 'ctrl',
  Alt: 'alt',
  AltGraph: 'alt',
  Shift: 'shift',
  Meta: 'meta',
  OS: 'meta',
};

/** The parts of a `KeyboardEvent` the keymap needs (so tests can pass plain objects). */
export interface KeyEventLike {
  key: string;
  code?: string;
  ctrlKey?: boolean;
  altKey?: boolean;
  shiftKey?: boolean;
  metaKey?: boolean;
}

/**
 * Converts a keyboard event into a chord. Letters use `event.key` when it is a plain letter (so
 * AZERTY/Dvorak users get the letter printed on the key); digits and punctuation use the physical
 * `code` (US positions), because `key` changes with Shift and layout.
 */
export function eventToChord(e: KeyEventLike): KeyChord {
  const chord: KeyChord = {
    key: '',
    ctrl: !!e.ctrlKey,
    alt: !!e.altKey,
    shift: !!e.shiftKey,
    meta: !!e.metaKey,
  };
  const bare = EVENT_MODIFIER_KEYS[e.key];
  if (bare) {
    chord.key = bare;
    chord[bare as 'ctrl' | 'alt' | 'shift' | 'meta'] = false;
    return chord;
  }
  const code = e.code ?? '';
  const k = e.key ?? '';
  if (k.length === 1 && /^[a-z]$/i.test(k)) chord.key = k.toLowerCase();
  else if (/^Key[A-Z]$/.test(code)) chord.key = code.slice(3).toLowerCase();
  else if (/^Digit[0-9]$/.test(code)) chord.key = code.slice(5);
  else if (/^Numpad[0-9]$/.test(code)) chord.key = code.slice(6);
  else if (code === 'NumpadAdd') {
    chord.key = '=';
    chord.shift = true;
  } else if (CODE_KEYS[code]) chord.key = CODE_KEYS[code];
  else if (k === ' ' || code === 'Space') chord.key = 'space';
  else if (k.length === 1) {
    // Synthetic events without `code`: interpret the character on a US layout.
    const parsed = canonicalKey(k);
    if (parsed) {
      chord.key = parsed.key;
      if (parsed.shift) chord.shift = true;
    } else chord.key = k.toLowerCase();
  } else {
    const parsed = canonicalKey(k);
    chord.key = parsed ? parsed.key : k.toLowerCase();
  }
  return chord;
}

const DISPLAY_KEYS: Readonly<Record<string, string>> = {
  escape: 'Esc',
  enter: 'Enter',
  tab: 'Tab',
  space: 'Space',
  backspace: 'Backspace',
  delete: 'Del',
  insert: 'Ins',
  home: 'Home',
  end: 'End',
  pageup: 'Page Up',
  pagedown: 'Page Down',
  arrowup: '↑',
  arrowdown: '↓',
  arrowleft: '←',
  arrowright: '→',
  contextmenu: 'Menu',
  ctrl: 'Ctrl',
  alt: 'Alt',
  shift: 'Shift',
  meta: 'Win',
};

const MAC_DISPLAY_KEYS: Readonly<Record<string, string>> = {
  ...DISPLAY_KEYS,
  enter: '↩',
  tab: '⇥',
  backspace: '⌫',
  delete: '⌦',
  ctrl: '⌃',
  alt: '⌥',
  shift: '⇧',
  meta: '⌘',
};

function displayKey(key: string, platform: Platform): string {
  const table = platform === 'mac' ? MAC_DISPLAY_KEYS : DISPLAY_KEYS;
  if (table[key]) return table[key];
  if (/^f\d+$/.test(key)) return key.toUpperCase();
  return key.length === 1 ? key.toUpperCase() : key;
}

/** Human label: `Ctrl+Shift+L`, or `⇧⌘L` on macOS (Apple's modifier order ⌃⌥⇧⌘). */
export function formatChord(c: KeyChord, platform: Platform = 'other'): string {
  // Show the symbol people know for shifted punctuation: Ctrl+Shift+. → Ctrl+>.
  let key = c.key;
  let shift = c.shift;
  if (shift && BASE_SYMBOLS.has(key) && key !== '=' && key !== '-') {
    const sym = Object.entries(SHIFTED_SYMBOLS).find(([, base]) => base === key)?.[0];
    if (sym) {
      key = sym;
      shift = false;
    }
  }
  if (platform === 'mac') {
    return `${c.ctrl ? '⌃' : ''}${c.alt ? '⌥' : ''}${shift ? '⇧' : ''}${c.meta ? '⌘' : ''}${displayKey(key, 'mac')}`;
  }
  const mods = [c.ctrl && 'Ctrl', c.alt && 'Alt', shift && 'Shift', c.meta && 'Win'].filter(Boolean);
  return [...mods, displayKey(key, 'other')].join('+');
}

/** Platform-neutral text for a chord (`Mod+Shift+L`), the format keymaps and overrides are stored in. */
export function chordToShortcut(c: KeyChord, platform: Platform = 'other'): string {
  const mods: string[] = [];
  if (platform === 'mac') {
    if (c.ctrl) mods.push('Ctrl');
    if (c.alt) mods.push('Alt');
    if (c.shift) mods.push('Shift');
    if (c.meta) mods.push('Mod');
  } else {
    if (c.ctrl) mods.push('Mod');
    if (c.alt) mods.push('Alt');
    if (c.shift) mods.push('Shift');
    if (c.meta) mods.push('Meta');
  }
  const named: Record<string, string> = {
    escape: 'Esc',
    pageup: 'PageUp',
    pagedown: 'PageDown',
    arrowup: 'ArrowUp',
    arrowdown: 'ArrowDown',
    arrowleft: 'ArrowLeft',
    arrowright: 'ArrowRight',
    contextmenu: 'ContextMenu',
  };
  const k =
    named[c.key] ??
    (/^f\d+$/.test(c.key) ? c.key.toUpperCase() : c.key.length === 1 ? c.key.toUpperCase() : cap(c.key));
  return [...mods, k].join('+');
}

function cap(s: string): string {
  return s.charAt(0).toUpperCase() + s.slice(1);
}

/** Formats a platform-neutral shortcut for display. Invalid text is returned unchanged. */
export function formatShortcut(text: string, platform: Platform = detectPlatform()): string {
  const c = tryParseShortcut(text, platform);
  return c ? formatChord(c, platform) : text;
}

/**
 * Chords that a browser tab cannot intercept (the browser or OS acts first). They work in the
 * desktop app, so they stay bound; Options ▸ Keyboard shows a hint when a user picks one.
 */
export function isBrowserReservedChord(c: KeyChord, platform: Platform = 'other'): boolean {
  const reserved =
    platform === 'mac'
      ? ['meta+q', 'meta+h', 'meta+m', 'meta+w', 'meta+n', 'meta+t', 'meta+tab', 'meta+space']
      : [
          'ctrl+alt+delete',
          'alt+tab',
          'alt+f4',
          'ctrl+w',
          'ctrl+n',
          'ctrl+t',
          'ctrl+shift+n',
          'ctrl+shift+t',
        ];
  return reserved.includes(chordId(c));
}
