import { join } from 'node:path';
import { describe, expect, it } from 'vitest';
import { analyzeLayout, type ElementBox } from '../src/playwright/layout-scan.ts';
import {
  parseScreenshotName,
  SCREENSHOTS_DIR,
  screenshotName,
  screenshotPath,
  slugify,
  VIEWPORTS,
} from '../src/playwright/screenshots.ts';

describe('screenshot naming', () => {
  it('follows <spec>/<name>-<theme>-<viewport>.png', () => {
    expect(slugify('Ribbon: Home tab')).toBe('ribbon-home-tab');
    expect(screenshotName('IME composing', 'dark', 'phone')).toBe('ime-composing-dark-phone.png');
    expect(screenshotPath('specs/surface.spec.ts', 'caret', 'light', 'desktop')).toBe(
      join(SCREENSHOTS_DIR, 'surface', 'caret-light-desktop.png'),
    );
    expect(screenshotPath('home', 'start', 'hc', 'tablet', '/tmp/s')).toBe('/tmp/s/home/start-hc-tablet.png');
    expect(() => screenshotName('!!!', 'light', 'desktop')).toThrow(/no letters/);
    expect(VIEWPORTS.phone).toEqual({ width: 412, height: 915 });
  });

  it('parses names back', () => {
    expect(parseScreenshotName('ime-composing-dark-phone.png')).toEqual({
      name: 'ime-composing',
      theme: 'dark',
      viewport: 'phone',
    });
    expect(parseScreenshotName('ime-composing-sepia-phone.png')).toBeNull();
    expect(parseScreenshotName('Bad Name-light-desktop.png')).toBeNull();
  });
});

function box(
  label: string,
  x: number,
  y: number,
  w: number,
  h: number,
  extra: Partial<ElementBox> = {},
): ElementBox {
  return {
    label,
    x,
    y,
    width: w,
    height: h,
    interactive: false,
    hasText: false,
    clipsText: false,
    ellipsis: false,
    visible: true,
    ...extra,
  };
}

describe('layout scan', () => {
  const viewport = { width: 412, height: 915 };

  it('passes a clean layout', () => {
    const boxes = [
      box('button.bold', 0, 0, 40, 40, { interactive: true }),
      box('button.italic', 44, 0, 40, 40, { interactive: true }),
      box('span.label', 0, 50, 200, 20, { hasText: true, clipsText: true, ellipsis: true }),
    ];
    expect(analyzeLayout(boxes, { viewport, touch: true })).toEqual([]);
  });

  it('finds clipped text, overflow, overlaps and small targets', () => {
    const boxes = [
      box('span.title', 0, 0, 100, 20, { hasText: true, clipsText: true }),
      box('div.ribbon', 0, 30, 600, 90),
      box('button.a', 0, 200, 40, 40, { interactive: true }),
      box('button.b', 20, 210, 40, 40, { interactive: true }),
      box('a.tiny', 100, 300, 16, 16, { interactive: true }),
      box('button.hidden', 0, 200, 40, 40, { interactive: true, visible: false }),
    ];
    const issues = analyzeLayout(boxes, { viewport, touch: true });
    expect(issues.map((i) => `${i.kind} ${i.label}`)).toEqual([
      'clipped-text span.title',
      'outside-viewport div.ribbon',
      'small-target a.tiny',
      'overlap button.a',
    ]);
    expect(issues[3]?.detail).toContain('button.b');
    // Desktop: no touch-target check; nested controls are not overlaps.
    const nested = [
      box('div.split', 0, 0, 80, 30, { interactive: true }),
      box('button.arrow', 60, 0, 20, 30, { interactive: true }),
    ];
    expect(analyzeLayout(nested, { viewport: { width: 1440, height: 900 } })).toEqual([]);
  });
});
