import { act, render, screen } from '@testing-library/react';
import { Bold } from 'lucide-react';
import { describe, expect, it } from 'vitest';
import { FormatPainter, Icon, ThemeProvider, useTheme } from '../src/index.ts';

function Probe() {
  const t = useTheme();
  return (
    <button type="button" onClick={() => t.setPreference('dark')}>
      {t.preference}:{t.resolved}:{t.app}
    </button>
  );
}

describe('ThemeProvider / useTheme', () => {
  it('applies data-app, switches theme and persists the choice', () => {
    render(
      <ThemeProvider app="grid">
        <Probe />
      </ThemeProvider>,
    );
    expect(document.documentElement.dataset.app).toBe('grid');
    const btn = screen.getByRole('button');
    expect(btn.textContent).toBe('system:light:grid');
    act(() => btn.click());
    expect(btn.textContent).toBe('dark:dark:grid');
    expect(document.documentElement.dataset.theme).toBe('dark');
    expect(localStorage.getItem('officina.theme')).toBe('dark');
  });

  it('restores the persisted theme on mount', () => {
    localStorage.setItem('officina.theme', 'hc');
    render(
      <ThemeProvider>
        <Probe />
      </ThemeProvider>,
    );
    expect(screen.getByRole('button').textContent).toBe('hc:hc:hub');
    expect(document.documentElement.dataset.theme).toBe('hc');
  });
});

describe('Icon', () => {
  it('renders glyphs by name and Lucide icons, decorative unless labelled', () => {
    const { container } = render(
      <>
        <Icon icon="FormatPainter" size={20} />
        <Icon icon={Bold} label="Bold" />
      </>,
    );
    const glyph = container.querySelector('[data-glyph="format-painter"]');
    expect(glyph?.getAttribute('aria-hidden')).toBe('true');
    expect(glyph?.getAttribute('viewBox')).toBe('0 0 20 20');
    expect(glyph?.getAttribute('width')).toBe('20');
    expect(screen.getByRole('img', { name: 'Bold' })).toBeTruthy();
  });

  it('glyph components accept a title and throw for unknown names', () => {
    render(<FormatPainter title="Format Painter" />);
    expect(screen.getByRole('img', { name: 'Format Painter' })).toBeTruthy();
    // @ts-expect-error unknown glyph names are a type error and a runtime error
    expect(() => render(<Icon icon="NoSuchGlyph" />)).toThrow(/unknown Officina glyph/);
  });
});
