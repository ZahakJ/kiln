/**
 * File ▸ Print and File ▸ Export ▸ PDF (WP-67): the Backstage Print pane paints the core's
 * preview of the sheet, a page-range change asks the core for a new preview, and Export ▸ PDF
 * downloads a PDF the core wrote (the web build has no save dialog the test could drive, so the
 * browser download is the web's Export).
 */
import { readFileSync } from 'node:fs';
import { fileURLToPath } from 'node:url';
import { expect, type Page, test } from '@playwright/test';
import { expectNoSeriousA11yViolations } from '../../packages/testing/src/playwright/a11y.ts';

const shot = (name: string) => fileURLToPath(new URL(`../screenshots/print/${name}`, import.meta.url));
const fixture = (name: string) =>
  Array.from(readFileSync(fileURLToPath(new URL(`../../fixtures/docx/features/${name}`, import.meta.url))));

/** Opens a fixture in the web core and shows it in Kiln Write. */
async function openFixture(page: Page, name: string) {
  await page.goto('/#/write');
  await expect(page.getByTestId('document-title')).toContainText('Document', { timeout: 30_000 });
  const doc = await page.evaluate(
    async ([bytes, file]) => {
      type Opener = {
        open(bytes: Uint8Array, hint: object): Promise<{ doc: number }>;
      };
      const client = (window as unknown as { __kiln?: { client?: Opener | null } }).__kiln?.client;
      if (!client) throw new Error('window.__kiln.client is missing (development build only)');
      const s = await client.open(Uint8Array.from(bytes as number[]), {
        name: file as string,
        path: null,
        format: 'Docx',
        read_only: false,
      });
      return s.doc;
    },
    [fixture(name), name] as const,
  );
  await page.goto(`/#/write?doc=${doc}`);
  await expect(page.getByTestId('document-title')).toContainText(name.replace(/\.docx$/, ''), {
    timeout: 30_000,
  });
}

async function openPrint(page: Page) {
  await page.getByRole('button', { name: 'File' }).click();
  const dialog = page.getByRole('dialog', { name: /File menu/ });
  await dialog.getByRole('button', { name: 'Print', exact: true }).click();
  await expect(dialog.getByRole('heading', { level: 1 })).toHaveText('Print');
  return dialog;
}

/** A fingerprint of the preview canvas's pixels (changes when the sheet does). */
function previewPixels(page: Page) {
  return page.getByTestId('print-preview').evaluate((c) => {
    const canvas = c as HTMLCanvasElement;
    const ctx = canvas.getContext('2d');
    if (!ctx || canvas.width === 0) return { w: 0, h: 0, sum: 0, ink: 0 };
    const data = ctx.getImageData(0, 0, canvas.width, canvas.height).data;
    let sum = 0;
    let ink = 0;
    for (let i = 0; i < data.length; i += 4) {
      const v = (data[i] ?? 0) + (data[i + 1] ?? 0) + (data[i + 2] ?? 0);
      sum = (sum + v * ((i >> 2) % 251)) % 1_000_000_007;
      if ((data[i + 3] ?? 0) > 0 && v < 600) ink += 1;
    }
    return { w: canvas.width, h: canvas.height, sum, ink };
  });
}

test.describe('print', () => {
  test('Backstage Print paints the preview and a page range updates it', async ({ page }) => {
    await openFixture(page, 'long-text.docx');
    const dialog = await openPrint(page);
    const sheet = dialog.getByTestId('print-sheet');
    await expect(sheet).toHaveText(/Page 1 of (\d+)/, { timeout: 30_000 });
    const total = Number((await sheet.textContent())?.match(/of (\d+)/)?.[1]);
    expect(total).toBeGreaterThanOrEqual(3);
    await expect.poll(async () => (await previewPixels(page)).ink, { timeout: 15_000 }).toBeGreaterThan(500);
    const first = await previewPixels(page);
    await page.evaluate(() => document.fonts.ready);
    await page.screenshot({ path: shot('print-light-desktop.png') });
    await expectNoSeriousA11yViolations(page);

    // Pages 2-3 only: two sheets, and the preview shows page 2.
    const pages = dialog.getByTestId('print-pages');
    await pages.fill('2-3');
    await pages.press('Enter');
    await expect(sheet).toHaveText('Page 1 of 2', { timeout: 15_000 });
    await expect.poll(async () => (await previewPixels(page)).sum, { timeout: 15_000 }).not.toBe(first.sum);
    await dialog.getByRole('button', { name: 'Next page' }).click();
    await expect(sheet).toHaveText('Page 2 of 2');

    // A page box the core cannot read is reported under the field.
    await pages.fill('9-2');
    await pages.press('Enter');
    await expect(dialog.getByRole('alert')).toContainText(/page/i);
  });

  test('the preview follows the theme and pages per sheet', async ({ page }) => {
    await page.emulateMedia({ colorScheme: 'dark' });
    await openFixture(page, 'long-text.docx');
    const dialog = await openPrint(page);
    await expect(dialog.getByTestId('print-sheet')).toHaveText(/Page 1 of/, { timeout: 30_000 });
    await dialog.getByRole('combobox', { name: 'Pages per sheet' }).click();
    await page.getByRole('option', { name: '2 Pages Per Sheet' }).click();
    await expect(dialog.getByTestId('print-sheet')).toHaveText(/Sheet 1 of \d+ \(pages 1–2\)/, {
      timeout: 15_000,
    });
    await expect
      .poll(async () => (await previewPixels(page)).w, { timeout: 15_000 })
      .toBeGreaterThan((await previewPixels(page)).h);
    await page.evaluate(() => document.fonts.ready);
    await page.screenshot({ path: shot('print-2up-dark-desktop.png') });
  });

  test('Export PDF downloads a PDF', async ({ page }) => {
    await openFixture(page, 'headers-footers.docx');
    await page.getByRole('button', { name: 'File' }).click();
    const dialog = page.getByRole('dialog', { name: /File menu/ });
    await dialog.getByRole('button', { name: 'Export', exact: true }).click();
    const download = page.waitForEvent('download');
    await dialog.getByTestId('pdf-export-go').click();
    const file = await download;
    expect(file.suggestedFilename()).toBe('headers-footers.pdf');
    const path = await file.path();
    const bytes = readFileSync(path);
    expect(bytes.subarray(0, 5).toString('latin1')).toBe('%PDF-');
    await expect(dialog.getByTestId('pdf-export-result')).toContainText('Saved');
    await page.screenshot({ path: shot('export-pdf-light-desktop.png') });
    await expectNoSeriousA11yViolations(page);
  });

  test('Print on a phone stacks the preview over the settings', async ({ page }) => {
    await page.setViewportSize({ width: 390, height: 844 });
    await openFixture(page, 'long-text.docx');
    const dialog = await openPrint(page);
    await expect(dialog.getByTestId('print-sheet')).toHaveText(/Page 1 of/, { timeout: 30_000 });
    await expect.poll(async () => (await previewPixels(page)).ink, { timeout: 15_000 }).toBeGreaterThan(200);
    await page.screenshot({ path: shot('print-light-phone.png') });
  });
});
