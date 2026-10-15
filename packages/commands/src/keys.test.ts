import { describe, expect, it } from 'vitest';
import {
  chordId,
  chordToShortcut,
  eventToChord,
  formatChord,
  formatShortcut,
  isBrowserReservedChord,
  parseShortcut,
  tryParseShortcut,
} from './keys.ts';

describe('parseShortcut', () => {
  it('maps Mod to Ctrl on Windows/Linux and to ⌘ on macOS', () => {
    expect(chordId(parseShortcut('Mod+Shift+L', 'other'))).toBe('ctrl+shift+l');
    expect(chordId(parseShortcut('Mod+Shift+L', 'mac'))).toBe('shift+meta+l');
    // Ctrl is always the Control key.
    expect(chordId(parseShortcut('Ctrl+F1', 'mac'))).toBe('ctrl+f1');
  });

  it('normalises shifted symbols to Shift + base key, the way Office binds them', () => {
    expect(chordId(parseShortcut('Ctrl+Shift+>'))).toBe('ctrl+shift+.');
    expect(chordId(parseShortcut('Ctrl+Shift+%'))).toBe(chordId(parseShortcut('Ctrl+Shift+5')));
    expect(chordId(parseShortcut('Ctrl++'))).toBe('ctrl+shift+=');
    expect(chordId(parseShortcut('Ctrl+Shift+='))).toBe('ctrl+shift+=');
    expect(chordId(parseShortcut('Ctrl+Shift+_'))).toBe('ctrl+shift+-');
  });

  it('understands key names and aliases', () => {
    expect(parseShortcut('Ctrl+Page Down').key).toBe('pagedown');
    expect(parseShortcut('Ctrl+PgUp').key).toBe('pageup');
    expect(parseShortcut('Alt+Down').key).toBe('arrowdown');
    expect(parseShortcut('Esc').key).toBe('escape');
    expect(parseShortcut('Shift+Space').key).toBe('space');
    expect(parseShortcut('F12').key).toBe('f12');
    expect(parseShortcut('Mod+`').key).toBe('`');
    expect(parseShortcut('Alt')).toMatchObject({ key: 'alt', alt: false });
  });

  it('rejects malformed shortcuts', () => {
    expect(() => parseShortcut('Ctrl+Banana')).toThrow(/Unknown key/);
    expect(() => parseShortcut('B+Ctrl')).toThrow(/not a modifier/);
    expect(() => parseShortcut('Ctrl+Ctrl+B')).toThrow(/Duplicate/);
    expect(tryParseShortcut('Ctrl+')).toBeNull();
  });
});

describe('eventToChord', () => {
  it('uses the physical key for digits and punctuation, the printed letter for letters', () => {
    expect(chordId(eventToChord({ key: '>', code: 'Period', ctrlKey: true, shiftKey: true }))).toBe(
      'ctrl+shift+.',
    );
    expect(chordId(eventToChord({ key: '!', code: 'Digit1', ctrlKey: true, shiftKey: true }))).toBe(
      'ctrl+shift+1',
    );
    expect(chordId(eventToChord({ key: 'z', code: 'KeyY', ctrlKey: true }))).toBe('ctrl+z'); // German layout
    expect(chordId(eventToChord({ key: 'å', code: 'KeyA', altKey: true }))).toBe('alt+a'); // macOS Option
    expect(chordId(eventToChord({ key: ' ', code: 'Space', shiftKey: true }))).toBe('shift+space');
    expect(chordId(eventToChord({ key: '+', code: 'NumpadAdd', ctrlKey: true }))).toBe('ctrl+shift+=');
    expect(chordId(eventToChord({ key: 'ArrowUp', code: 'ArrowUp', altKey: true, shiftKey: true }))).toBe(
      'alt+shift+arrowup',
    );
  });

  it('reports a bare modifier press without its own flag, and handles synthetic events without code', () => {
    expect(eventToChord({ key: 'Alt', code: 'AltLeft', altKey: true })).toMatchObject({
      key: 'alt',
      alt: false,
    });
    expect(chordId(eventToChord({ key: '<', ctrlKey: true, shiftKey: true }))).toBe('ctrl+shift+,');
  });
});

describe('formatting', () => {
  it('formats for Windows/Linux and macOS', () => {
    expect(formatShortcut('Mod+Shift+P', 'other')).toBe('Ctrl+Shift+P');
    expect(formatShortcut('Mod+Shift+P', 'mac')).toBe('⇧⌘P');
    expect(formatShortcut('Mod+Alt+M', 'mac')).toBe('⌥⌘M');
    expect(formatShortcut('Mod+Shift+>', 'other')).toBe('Ctrl+>');
    expect(formatShortcut('Mod+PageDown', 'other')).toBe('Ctrl+Page Down');
    expect(formatShortcut('not a shortcut', 'other')).toBe('not a shortcut');
    expect(formatChord(parseShortcut('Shift+F5'), 'other')).toBe('Shift+F5');
  });

  it('round-trips chords to platform-neutral text', () => {
    for (const p of ['other', 'mac'] as const) {
      for (const s of ['Mod+Shift+L', 'Alt+ArrowDown', 'Mod+PageUp', 'Shift+F11', 'Mod+Alt+1']) {
        const chord = parseShortcut(s, p);
        expect(chordId(parseShortcut(chordToShortcut(chord, p), p))).toBe(chordId(chord));
      }
    }
  });

  it('knows which chords a browser tab cannot intercept', () => {
    expect(isBrowserReservedChord(parseShortcut('Mod+W', 'other'), 'other')).toBe(true);
    expect(isBrowserReservedChord(parseShortcut('Mod+B', 'other'), 'other')).toBe(false);
  });
});
