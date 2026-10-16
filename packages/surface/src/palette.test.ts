import { readFileSync } from 'node:fs';
import { fileURLToPath } from 'node:url';
import type { Palette } from '@kiln/protocol';
import { describe, expect, it } from 'vitest';
import { channels, cssColor, DARK, LIGHT, paletteFor, resolvePaint } from './palette';

const FIXTURE = fileURLToPath(new URL('../../../fixtures/golden/scene/palette.json', import.meta.url));

describe('palettes', () => {
  it('equal the Rust palettes (fixtures/golden/scene/palette.json)', () => {
    const [light, dark] = JSON.parse(readFileSync(FIXTURE, 'utf8')) as [Palette, Palette];
    expect(LIGHT).toEqual(light);
    expect(DARK).toEqual(dark);
  });

  it('resolves roles per theme and literal colours as they are', () => {
    expect(resolvePaint(LIGHT, { Role: 'Page' })).toBe(0xffffffff);
    expect(resolvePaint(DARK, { Role: 'Page' })).toBe(0x221a15ff);
    expect(resolvePaint(DARK, { Rgba: 0x12345678 })).toBe(0x12345678);
    expect(paletteFor('dark')).toBe(DARK);
    expect(channels(0xff6a2638)).toEqual([255, 106, 38, 56]);
    expect(cssColor(0x3159c6ff)).toBe('rgb(49 89 198)');
    expect(cssColor(0x00000080)).toBe(`rgb(0 0 0 / ${128 / 255})`);
  });
});
