import { mkdirSync, readFileSync, writeFileSync } from 'node:fs';
import { fileURLToPath } from 'node:url';
import { expect, type Page, test } from '@playwright/test';
import pixelmatch from 'pixelmatch';
import {
  decodePng,
  encodePng,
  openPlayground,
  openText,
  settle,
  sizeSurface,
  surfacePixels,
} from './surface-helpers';

/**
 * Painter parity (WP-54): the canvas painter (packages/surface) and the raster painter
 * (kiln-raster) paint the same Scene identically. The canvas is compared with the golden PNGs
 * that `kiln-cli render --scratch` and the kiln-session golden tests produce
 * (`fixtures/golden/scene/*.png`, rendered by kiln-raster from the same SceneDeltas); at most 1 %
 * of the pixels may differ (pixelmatch, threshold 0.1).
 */

const golden = (name: string) =>
  fileURLToPath(new URL(`../../fixtures/golden/scene/${name}`, import.meta.url));
const outDir = fileURLToPath(new URL('../screenshots/painter-parity/', import.meta.url));
const LOREM = readFileSync(golden('lorem.txt'), 'utf8');

async function compare(page: Page, name: string): Promise<number> {
  const canvas = await surfacePixels(page);
  const expected = await decodePng(page, readFileSync(golden(`${name}.png`)));
  expect([canvas.width, canvas.height]).toEqual([expected.width, expected.height]);
  const diff = new Uint8Array(canvas.width * canvas.height * 4);
  const differing = pixelmatch(canvas.data, expected.data, diff, canvas.width, canvas.height, {
    threshold: 0.1,
  });
  const ratio = differing / (canvas.width * canvas.height);
  mkdirSync(outDir, { recursive: true });
  writeFileSync(`${outDir}${name}-canvas.png`, await encodePng(page, canvas));
  writeFileSync(
    `${outDir}${name}-diff.png`,
    await encodePng(page, { width: canvas.width, height: canvas.height, data: diff }),
  );
  test.info().annotations.push({
    type: 'parity',
    description: `${name}: ${differing} px (${(ratio * 100).toFixed(3)} %)`,
  });
  console.log(`painter parity ${name}: ${differing} differing pixels (${(ratio * 100).toFixed(3)} %)`);
  return ratio;
}

async function prepare(page: Page, width: number, height: number): Promise<void> {
  await openPlayground(page);
  await openText(page, LOREM, 'lorem.txt');
  await sizeSurface(page, width, height);
  await page.evaluate(() => {
    const c = window.__kilnPlayground?.controller;
    c?.setBlink(false);
    c?.focus();
  });
  await settle(page);
}

test.describe('painter parity', () => {
  test('canvas matches kiln-raster: lorem.txt, light', async ({ page }) => {
    await page.emulateMedia({ colorScheme: 'light' });
    await prepare(page, 1000, 900);
    expect(await compare(page, 'lorem')).toBeLessThanOrEqual(0.01);
  });

  test('canvas matches kiln-raster: lorem.txt, dark', async ({ page }) => {
    await page.emulateMedia({ colorScheme: 'dark' });
    await prepare(page, 1000, 900);
    expect(await compare(page, 'lorem-dark')).toBeLessThanOrEqual(0.01);
  });

  test.describe('at 2× device pixels', () => {
    test.use({ deviceScaleFactor: 2 });

    test('canvas matches kiln-raster: a word selection, scrolled', async ({ page }) => {
      await page.emulateMedia({ colorScheme: 'light' });
      await prepare(page, 600, 420);
      await page.evaluate(async () => {
        const pg = window.__kilnPlayground;
        if (!pg) return;
        await pg.client.execute(pg.doc, {
          Write: { SelectAt: { point: { x: 302, y: 207 }, extend: false, unit: 'Word' } },
        });
        await pg.client.execute(pg.doc, {
          Write: { SelectAt: { point: { x: 392, y: 295 }, extend: true, unit: 'Word' } },
        });
        const scroller = document.querySelector('[data-testid="kiln-surface-scroller"]') as HTMLElement;
        scroller.scrollLeft = 100;
        scroller.scrollTop = 140;
      });
      await settle(page);
      expect(await compare(page, 'selection-2x')).toBeLessThanOrEqual(0.01);
    });
  });
});
