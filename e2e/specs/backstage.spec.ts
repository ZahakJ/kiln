/**
 * Backstage, the File menu: Office's order, pages that show what the core
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

  test('File ▸ Options, then Esc: the keys go back to the document', async ({ page }) => {
    await openWrite(page);
    await page.waitForFunction(
      () =>
        // biome-ignore lint/suspicious/noExplicitAny: the test handle
        ((window as any).__kilnWrite?.controller?.getStats().tiles ?? 0) > 0,
      null,
      { timeout: 45_000 },
    );
    const proxy = page.getByTestId('kiln-input-proxy');
    await proxy.focus();
    await page.getByRole('button', { name: 'File' }).click();
    await page.getByRole('button', { name: 'Options' }).click();
    const options = page.getByRole('dialog', { name: /Options/ });
    await expect(options).toBeVisible();
    await page.keyboard.press('Escape');
    await expect(options).toBeHidden();
    // The Backstage that opened Options has gone: the document has the keys, not the page body.
    await expect(proxy).toBeFocused();
    await page.keyboard.type('Kept');
    await expect
      .poll(() =>
        page.evaluate(async () => {
          // biome-ignore lint/suspicious/noExplicitAny: the test handle
          const w = (window as any).__kilnWrite;
          const r = await w.client.query(w.doc, 'Paragraphs');
          return (r.Paragraphs ?? []).map((x: { text: string }) => x.text).join('\n');
        }),
      )
      .toContain('Kept');
  });

  test('Info shows the properties the core reports and runs its compatibility check', async ({ page }) => {
    await openWrite(page);
    await page.getByRole('button', { name: 'File' }).click();
    await page.getByRole('button', { name: 'Info' }).click();
    const main = page.getByRole('dialog', { name: /File menu/ }).getByRole('main');
    await expect(main.getByRole('heading', { level: 1 })).toHaveText('Info');
    await expect(main.getByText('Not saved yet')).toBeVisible();
    await page.screenshot({ path: shot('backstage-info-light-desktop.png') });
    await expectNoSeriousA11yViolations(page);
    // the Compatibility Checker opens in its own dialog.
    await main.getByRole('button', { name: /Check compatibility/ }).click();
    const compat = page.getByTestId('compat-dialog');
    await expect(
      compat.getByText(/Nothing will be lost|Will be lost|saved differently|Kept as it was read/).first(),
    ).toBeVisible();
    await compat.getByRole('button', { name: 'Done' }).click();
  });

  test('Save As offers what the web core saves; New offers blank documents and templates', async ({
    page,
  }) => {
    await page.emulateMedia({ colorScheme: 'dark' });
    await openWrite(page);
    await page.getByRole('button', { name: 'File' }).click();
    const dialog = page.getByRole('dialog', { name: /File menu/ });
    await dialog.getByRole('button', { name: 'Save As' }).click();
    await expect(dialog.getByRole('button', { name: /Word Document/ })).toBeVisible();
    // OpenDocument is saved by the native core only.
    await expect(dialog.getByRole('button', { name: /OpenDocument Text/ })).toHaveCount(0);
    await dialog.getByRole('button', { name: 'New', exact: true }).click();
    await expect(dialog.getByRole('button', { name: /Blank document/ })).toBeEnabled();
    // The apps of the release (Kiln Notes follows after v0.1), then the templates.
    await expect(dialog.getByRole('button', { name: /Blank presentation/ })).toBeEnabled();
    await expect(dialog.getByRole('button', { name: /notebook/i })).toHaveCount(0);
    await expect(dialog.locator('[data-template]').first()).toBeVisible();
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
