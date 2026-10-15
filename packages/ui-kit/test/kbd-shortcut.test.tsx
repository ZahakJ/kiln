import { render } from '@testing-library/react';
import { describe, expect, it } from 'vitest';
import { ariaKeyShortcuts, formatShortcut, Kbd, shortcutLabel } from '../src/index.ts';

describe('shortcut formatting', () => {
  it('maps Mod to Ctrl or ⌘ and keeps a literal plus key', () => {
    expect(formatShortcut('Mod+Shift+S', 'other')).toEqual(['Ctrl', 'Shift', 'S']);
    expect(formatShortcut('Mod+Shift+S', 'mac')).toEqual(['⌘', '⇧', 'S']);
    expect(shortcutLabel('Mod+Shift+S', 'mac')).toBe('⌘⇧S');
    expect(formatShortcut('Ctrl++', 'other')).toEqual(['Ctrl', '+']);
    expect(shortcutLabel('Alt+ArrowDown', 'other')).toBe('Alt+↓');
  });

  it('builds aria-keyshortcuts values', () => {
    expect(ariaKeyShortcuts('Mod+B', 'other')).toBe('Control+B');
    expect(ariaKeyShortcuts('Mod+B', 'mac')).toBe('Meta+B');
    expect(ariaKeyShortcuts('Ctrl++', 'other')).toBe('Control+Plus');
  });

  it('Kbd renders one keycap per key with an accessible label', () => {
    const { container } = render(<Kbd shortcut="Mod+Shift+P" platform="other" />);
    expect(container.querySelectorAll('.of-kbd__key')).toHaveLength(3);
    expect(container.querySelector('.of-visually-hidden')?.textContent).toBe('Ctrl+Shift+P');
    const inline = render(<Kbd shortcut="Mod+C" platform="mac" variant="inline" />);
    expect(inline.container.textContent).toBe('⌘C');
  });
});
