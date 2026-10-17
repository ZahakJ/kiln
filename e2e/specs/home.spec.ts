/**
 * Kiln Home (WP-58, PLAN.md §1.6): the start screen in light and dark on desktop and phone
 * (reviewed screenshots in e2e/screenshots/chrome/), axe, creating a document through the core,
 * search, the app filters and Options.
 */
import { fileURLToPath } from 'node:url';
import { expect, type Page, test } from '@playwright/test';
import { expectNoSeriousA11yViolations } from '../../packages/testing/src/playwright/a11y.ts';

const shot = (name: string) => fileURLToPath(new URL(`../screenshots/chrome/${name}`, import.meta.url));
/** Starting the wasm core (first document) can take well over the 5 s expect default under CPU load. */
const CORE_START = 30_000;

async function openHome(page: Page) {
  await page.goto('/#/home');
  await expect(page.getByRole('heading', { level: 1 })).toContainText('What are we making today?');
  await expect(page.getByText('Your recent files will gather here')).toBeVisible({ timeout: CORE_START });
  await page.evaluate(() => document.fonts.ready);
}

test.describe('home', () => {
  for (const scheme of ['light', 'dark'] as const) {
    test(`Home (${scheme}, desktop) matches the brand layout and passes axe`, async ({ page }) => {
      await page.emulateMedia({ colorScheme: scheme });
      await openHome(page);
      for (const name of [/^New document in Kiln Write:/, /^New workbook in Kiln Grid:/]) {
        await expect(page.getByRole('button', { name })).toBeVisible();
      }
      await expect(page.getByRole('navigation', { name: 'Kiln Home' })).toBeVisible();
      await expect(page.getByText('Coming from Microsoft Office?')).toBeVisible();
      expect(await page.evaluate(() => document.documentElement.scrollWidth > window.innerWidth)).toBe(false);
      await page.screenshot({ path: shot(`home-${scheme}-desktop.png`) });
      await expectNoSeriousA11yViolations(page);
    });

    test(`Home (${scheme}, phone) stacks the doors and shows the tab bar`, async ({ page }) => {
      await page.setViewportSize({ width: 390, height: 844 });
      await page.emulateMedia({ colorScheme: scheme });
      await openHome(page);
      await expect(page.getByRole('navigation', { name: 'Kiln Home sections' })).toBeVisible();
      await expect(page.locator('.kiln-home__new-btn')).toBeVisible();
      expect(await page.evaluate(() => document.documentElement.scrollWidth > window.innerWidth)).toBe(false);
      await page.screenshot({ path: shot(`home-${scheme}-phone.png`) });
      await expectNoSeriousA11yViolations(page);
    });
  }

  test('the Write door asks the core for a new document and opens the Write frame', async ({ page }) => {
    await openHome(page);
    await page.getByRole('button', { name: /^New document in Kiln Write:/ }).click();
    await expect(page).toHaveURL(/#\/write$/);
    // The wasm core starts on first use: allow for a loaded machine (as openFrame in chrome.spec).
    await expect(page.getByTestId('document-title')).toContainText('Document', { timeout: CORE_START });
    await expect(page.locator('.kiln-appframe')).toHaveAttribute('data-app', 'write');
  });

  test('New ▸ Blank workbook opens Grid; apps the core cannot create yet are marked', async ({ page }) => {
    await openHome(page);
    await page.locator('.kiln-home__new-btn').click();
    await expect(page.getByRole('menuitem', { name: /Blank deck/ })).toBeDisabled();
    await page.getByRole('menuitem', { name: /Blank workbook/ }).click();
    await expect(page).toHaveURL(/#\/grid$/);
    await expect(page.getByTestId('document-title')).toContainText('Book', { timeout: CORE_START });
  });

  test('Ctrl+K searches, the app filters narrow the lists, Settings opens Options', async ({ page }) => {
    await openHome(page);
    await page.keyboard.press('Control+K');
    await expect(page.getByRole('searchbox')).toBeFocused();
    await page.keyboard.type('budget');
    await expect(page.getByText('No matches')).toBeVisible();
    await page.getByRole('searchbox').fill('');
    await page.getByRole('button', { name: 'Grid', exact: true }).click();
    await expect(page.getByRole('button', { name: /Grid only/ })).toBeVisible();
    await page.getByRole('button', { name: 'Settings', exact: true }).click();
    await expect(page.getByRole('dialog', { name: /Options/ })).toBeVisible();
    await expectNoSeriousA11yViolations(page);
  });

  test('dismissing the Office banner is remembered through the host', async ({ page }) => {
    await openHome(page);
    await page.getByRole('button', { name: 'Dismiss' }).click();
    await expect(page.getByText('Coming from Microsoft Office?')).toHaveCount(0);
    expect(await page.evaluate(() => localStorage.getItem('kiln.pref.ui.home.switcher_dismissed'))).toBe('1');
    await page.reload();
    await expect(page.getByRole('heading', { level: 1 })).toBeVisible();
    await expect(page.getByText('Coming from Microsoft Office?')).toHaveCount(0);
  });
});
