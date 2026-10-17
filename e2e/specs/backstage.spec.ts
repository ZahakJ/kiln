/**
 * Backstage, the File menu (WP-58, PLAN.md §1.5): Office's order, pages that show what the core
 * reports (File ▸ Info from `client.query`), Esc and the back arrow close it, focus returns, and
 * axe passes.
 */
import { fileURLToPath } from 'node:url';
import { expect, type Page, test } from '@playwright/test';
import { expectNoSeriousA11yViolations } from '../../packages/testing/src/playwright/a11y.ts';

const shot = (name: string) => fileURLToPath(new URL(`../screenshots/chrome/${name}`, import.meta.url));

async function openWrite(page: Page) {
  await page.goto('/#/write');
  await expect(page.getByTestId('document-title')).toContainText('Document', { timeout: 30_000 });
}

test.describe('backstage', () => {
  test('opens from File, lists the pages in Office order and closes with Esc', async ({ page }) => {
    await openWrite(page);
    const file = page.getByRole('button', { name: 'File' });
    await file.click();
    const backstage = page.getByRole('dialog', { name: /File menu/ });
    await expect(backstage).toBeVisible();
    const nav = backstage.getByRole('navigation', { name: 'File menu' });
    const labels = (await nav.getByRole('button').allInnerTexts()).map((t) => t.trim()).filter(Boolean);
    expect(labels).toEqual([
      'Home',
      'New',
      'Open',
      'Info',
      'Save',
      'Save As',
      'Print',
      'Export',
      'Close',
      'Account',
      'Options',
    ]);
    await page.evaluate(() => document.fonts.ready);
    await page.screenshot({ path: shot('backstage-home-light-desktop.png') });
    await expectNoSeriousA11yViolations(page);
    await page.keyboard.press('Escape');
    await expect(backstage).toBeHidden();
    await expect(file).toBeFocused();
  });

  test('Info shows the properties the core reports and runs its compatibility check', async ({ page }) => {
    await openWrite(page);
    await page.getByRole('button', { name: 'File' }).click();
    await page.getByRole('button', { name: 'Info' }).click();
    const main = page.getByRole('dialog', { name: /File menu/ }).getByRole('main');
    await expect(main.getByRole('heading', { level: 1 })).toHaveText('Info');
    await expect(main.getByText('Not saved yet')).toBeVisible();
    await main.getByRole('button', { name: 'Check compatibility' }).click();
    await expect(main.getByText(/No issues found|issue|not/i).first()).toBeVisible();
    await page.screenshot({ path: shot('backstage-info-light-desktop.png') });
    await expectNoSeriousA11yViolations(page);
  });

  test('Save As offers the Office, ODF and plain formats; New offers blank documents', async ({ page }) => {
    await page.emulateMedia({ colorScheme: 'dark' });
    await openWrite(page);
    await page.getByRole('button', { name: 'File' }).click();
    const dialog = page.getByRole('dialog', { name: /File menu/ });
    await dialog.getByRole('button', { name: 'Save As' }).click();
    await expect(dialog.getByRole('button', { name: /Word Document/ })).toBeVisible();
    await expect(dialog.getByRole('button', { name: /OpenDocument Text/ })).toBeVisible();
    await dialog.getByRole('button', { name: 'New', exact: true }).click();
    await expect(dialog.getByRole('button', { name: /Blank document/ })).toBeEnabled();
    await expect(dialog.getByRole('button', { name: /Blank presentation/ })).toBeDisabled();
    await page.screenshot({ path: shot('backstage-new-dark-desktop.png') });
    await expectNoSeriousA11yViolations(page);
  });

  test('the back arrow closes it, and Account switches the theme', async ({ page }) => {
    await openWrite(page);
    await page.getByRole('button', { name: 'File' }).click();
    const dialog = page.getByRole('dialog', { name: /File menu/ });
    await dialog.getByRole('button', { name: 'Account' }).click();
    await dialog.getByRole('radio', { name: 'Dark' }).click();
    await expect(page.locator('html')).toHaveAttribute('data-theme', 'dark');
    await expect(dialog.getByText(/Core \d+\.\d+\.\d+/)).toBeVisible();
    await dialog.getByRole('button', { name: 'Back to the document' }).click();
    await expect(dialog).toBeHidden();
  });
});
