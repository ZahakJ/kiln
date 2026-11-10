/**
 * The landing page and the manual as Pages serves them, beside the web app: the page has
 * its content, its banner and no script dependency, and the app's service worker stops at
 * /kiln/app/. Runs in the pwa project, on the local Pages emulation or a deployed site.
 */
import { expect, type Page, test } from '@playwright/test';

/** The site's root from the project's base URL (…/kiln/app/). */
const site = (baseURL: string | undefined, path: string) => new URL(`../${path}`, baseURL).href;

async function controlled(page: Page): Promise<boolean> {
  return page.evaluate(() => navigator.serviceWorker?.controller != null);
}

test.describe('the site around the web app', () => {
  test('the landing page carries the apps, the banner and its numbers', async ({ page, baseURL }) => {
    const response = await page.goto(site(baseURL, ''));
    expect(response?.status()).toBe(200);
    await expect(page.getByRole('heading', { level: 1 })).toBeVisible();
    await expect(page.getByRole('heading', { name: 'Kiln Write' })).toBeVisible();
    const banner = page.locator('.banner img');
    await expect(banner).toBeVisible();
    expect(await banner.evaluate((i: HTMLImageElement) => i.naturalWidth)).toBeGreaterThan(0);
    await expect(page.locator('[data-stat="functions"]').first()).toHaveText(/^\d{3}$/);
    await expect(page.locator('.compare table')).toContainText('LibreOffice');
    const cells = await page.locator('.compare td').allTextContents();
    expect(cells.length).toBeGreaterThan(30);
    expect(cells.filter((c) => c.trim() === '' || /tbd|n\/a/i.test(c))).toEqual([]);
    await expect(page.getByRole('link', { name: 'Open in browser' }).first()).toHaveAttribute('href', 'app/');
  });

  test('a phone width scrolls one way only, at 360 and 390 px', async ({ page, baseURL }) => {
    for (const width of [360, 390]) {
      await page.setViewportSize({ width, height: 800 });
      await page.goto(site(baseURL, ''));
      expect(await page.evaluate(() => document.documentElement.scrollWidth)).toBe(width);
      await page.goto(site(baseURL, 'manual/?p=formats'));
      await expect(page.getByRole('heading', { level: 1 })).toContainText('File formats');
      expect(await page.evaluate(() => document.documentElement.scrollWidth)).toBe(width);
    }
  });

  test('every manual page fits a 360 px phone', async ({ page, baseURL }) => {
    test.setTimeout(120_000);
    await page.setViewportSize({ width: 360, height: 800 });
    await page.goto(site(baseURL, 'manual/'));
    await expect(page.getByRole('heading', { level: 1 })).toBeVisible();
    await page.getByRole('button', { name: 'Contents' }).click();
    const names = await page
      .locator('#toc a[data-page]')
      .evaluateAll((links) => links.map((a) => (a as HTMLElement).dataset.page as string));
    expect(names.length).toBeGreaterThanOrEqual(14);
    for (const name of names) {
      await page.goto(site(baseURL, `manual/?p=${name}`));
      await expect(page.getByRole('heading', { level: 1 })).not.toHaveText('Page not found');
      expect(await page.evaluate(() => document.documentElement.scrollWidth), name).toBe(360);
    }
  });

  test('the download section offers what exists', async ({ page, baseURL }) => {
    await page.goto(site(baseURL, ''));
    const section = page.locator('#download');
    await expect(section.getByRole('link', { name: 'Open in browser' })).toBeVisible();
    await expect(section.getByRole('link', { name: 'Build from source' })).toBeVisible();
    await expect(section.locator('[data-release-card]')).toHaveCount(4);
    await expect(section.locator('[data-release-card]:visible')).toHaveCount(0);
    await expect(page.locator('#download')).not.toContainText(/releases page|checksums/);
  });

  test('the manual renders its pages and moves between them', async ({ page, baseURL }) => {
    await page.goto(site(baseURL, 'manual/'));
    await expect(page.getByRole('heading', { level: 1 })).toHaveText('The Kiln manual');
    const toc = page.getByRole('complementary');
    await toc.getByRole('link', { name: 'Keyboard shortcuts' }).click();
    await expect(page.getByRole('heading', { level: 1 })).toHaveText('Keyboard shortcuts');
    await expect(page).toHaveURL(/\?p=shortcuts/);
    await toc.getByRole('searchbox').fill('function');
    await expect(toc.getByRole('link', { name: 'Function list' })).toBeVisible();
    await expect(toc.getByRole('link', { name: 'Kiln Deck' })).toBeHidden();
    await page.goto(site(baseURL, 'manual/?p=nothing-here'));
    await expect(page.getByRole('heading', { level: 1 })).toHaveText('Page not found');
  });

  test("the app's service worker does not reach the landing page or the manual", async ({
    page,
    baseURL,
  }) => {
    await page.addInitScript(() => localStorage.setItem('kiln.pref.ui.home.tour_seen', '1'));
    await page.goto(baseURL as string);
    // The page moves to Kiln Home's address once it starts, so each try may meet a navigation.
    await expect
      .poll(
        () =>
          page
            .evaluate(async () => (await navigator.serviceWorker.getRegistration())?.scope ?? '')
            .catch(() => ''),
        { timeout: 30_000 },
      )
      .toMatch(/\/kiln\/app\/$/);
    // The worker's first reload turns the headers on: leave only after it has happened.
    await expect
      .poll(() => page.evaluate(() => crossOriginIsolated).catch(() => false), { timeout: 30_000 })
      .toBe(true);
    await page.goto(site(baseURL, ''));
    expect(await controlled(page)).toBe(false);
    await page.goto(site(baseURL, 'manual/'));
    expect(await controlled(page)).toBe(false);
  });
});
