import { fileURLToPath } from 'node:url';
import { expect, test } from '@playwright/test';

/** Reviewed screenshots land in e2e/screenshots/<area>/<name>.png (gitignored except baseline/). */
const shot = (name: string) => fileURLToPath(new URL(`../screenshots/${name}`, import.meta.url));

test.describe('smoke', () => {
  test('hub loads and shows Officina (light, desktop)', async ({ page }) => {
    await page.emulateMedia({ colorScheme: 'light' });
    await page.goto('/');
    await expect(page.getByRole('heading', { level: 1 })).toContainText('Officina');
    await expect(page.getByTestId('suite-version')).toHaveText(/^Version \d+\.\d+\.\d+/);
    await page.evaluate(() => document.fonts.ready);
    await page.screenshot({ path: shot('smoke/hub-light-desktop.png'), fullPage: true });
  });

  test('hub renders in dark mode on a phone viewport', async ({ page }) => {
    await page.setViewportSize({ width: 390, height: 844 });
    await page.emulateMedia({ colorScheme: 'dark' });
    await page.goto('/');
    await expect(page.getByRole('heading', { level: 1 })).toContainText('Officina');
    const bg = await page.evaluate(() => getComputedStyle(document.body).backgroundColor);
    expect(bg).toBe('rgb(21, 23, 28)');
    const overflow = await page.evaluate(() => document.documentElement.scrollWidth > window.innerWidth);
    expect(overflow).toBe(false);
    await page.evaluate(() => document.fonts.ready);
    await page.screenshot({ path: shot('smoke/hub-dark-phone.png'), fullPage: true });
  });
});
