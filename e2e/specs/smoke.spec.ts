import { fileURLToPath } from 'node:url';
import { expect, test } from '@playwright/test';

/** Reviewed screenshots land in e2e/screenshots/<area>/<name>.png (gitignored except baseline/). */
const shot = (name: string) => fileURLToPath(new URL(`../screenshots/${name}`, import.meta.url));

test.describe('smoke', () => {
  test('home loads and shows Kiln (light, desktop)', async ({ page }) => {
    await page.emulateMedia({ colorScheme: 'light' });
    await page.goto('/');
    await expect(page.getByRole('heading', { level: 1 })).toContainText('Kiln');
    await expect(page).toHaveTitle('Kiln');
    await expect(page.getByTestId('suite-version')).toHaveText(/^Version \d+\.\d+\.\d+/);
    // Kiln Home: one door per app of the release, named after the product.
    for (const app of ['Kiln Write', 'Kiln Grid', 'Kiln Deck']) {
      await expect(page.locator('.kiln-door', { hasText: app })).toBeVisible();
    }
    await expect(page.locator('.kiln-door:not(.kiln-door--templates)')).toHaveCount(3);
    await page.evaluate(() => document.fonts.ready);
    await page.screenshot({ path: shot('smoke/home-light-desktop.png'), fullPage: true });
  });

  test('home renders in dark mode on a phone viewport', async ({ page }) => {
    await page.setViewportSize({ width: 390, height: 844 });
    await page.emulateMedia({ colorScheme: 'dark' });
    await page.goto('/');
    await expect(page.getByRole('heading', { level: 1 })).toContainText('Kiln');
    const bg = await page.evaluate(() => getComputedStyle(document.body).backgroundColor);
    expect(bg).toBe('rgb(23, 17, 14)'); // the brand kit's dark background
    const overflow = await page.evaluate(() => document.documentElement.scrollWidth > window.innerWidth);
    expect(overflow).toBe(false);
    await page.evaluate(() => document.fonts.ready);
    await page.screenshot({ path: shot('smoke/home-dark-phone.png'), fullPage: true });
  });
});
