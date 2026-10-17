import { describe, expect, it } from 'vitest';
import { contrast, textOnPage } from './contrast';
import { DARK, LIGHT } from './palette';

const hue = (rgba: number) => {
  const r = (rgba >>> 24) & 0xff;
  const g = (rgba >>> 16) & 0xff;
  const b = (rgba >>> 8) & 0xff;
  return Math.atan2(Math.sqrt(3) * (g - b), 2 * r - g - b);
};

describe('text on a dark page (Word dark mode)', () => {
  it("lightens Word's dark heading blue until it reads, keeping its hue", () => {
    const blue = 0x2f5496ff;
    expect(contrast(blue, DARK.page)).toBeLessThan(4.5);
    const out = textOnPage(blue, DARK.page);
    expect(contrast(out, DARK.page)).toBeGreaterThanOrEqual(4.5);
    expect(Math.abs(hue(out) - hue(blue))).toBeLessThan(0.25);
    expect(out & 0xff).toBe(0xff);
  });

  it('turns black and dark grey light; leaves readable colours and light pages alone', () => {
    expect(contrast(textOnPage(0x000000ff, DARK.page), DARK.page)).toBeGreaterThanOrEqual(4.5);
    expect(contrast(textOnPage(0x404040ff, DARK.page), DARK.page)).toBeGreaterThanOrEqual(4.5);
    expect(textOnPage(0xffc000ff, DARK.page)).toBe(0xffc000ff);
    expect(textOnPage(0x2f5496ff, LIGHT.page)).toBe(0x2f5496ff);
    expect(textOnPage(0x00000080, DARK.page) & 0xff).toBe(0x80);
  });
});
