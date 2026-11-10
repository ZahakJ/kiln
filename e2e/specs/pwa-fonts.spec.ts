/**
 * Fonts a document needs, offline before they were ever fetched (the CJK font's slices and Noto
 * Emoji are fetched on use, not cached ahead): the window says once, quietly, that some characters
 * need a download, and when the network is back they are fetched and drawn, and the notice goes.
 */

import { expect, test } from '@playwright/test';
import { CORE_START, inCoreWorker, openIsolated } from './pwa-helpers.ts';

test.skip(!!process.env.KILN_E2E_BASE, 'needs a page of its own origin');
test.describe.configure({ mode: 'default', timeout: 240_000 });

const NOTICE = 'Some characters need a download and appear once you are back online.';

test('CJK offline before its font was fetched: a quiet notice, then the text once online', async ({
  page,
  context,
}) => {
  await openIsolated(page);
  await page.goto('./#/grid');
  await expect(page.locator('[data-kiln-grid-proxy]')).toBeAttached({ timeout: CORE_START });
  await expect(page.getByRole('textbox', { name: 'Formula Bar' })).toBeVisible({ timeout: CORE_START });
  await context.setOffline(true);
  await page.keyboard.insertText('今年的销售额 한국어 😀');
  await page.keyboard.press('Enter');
  const notice = page.getByText(NOTICE);
  await expect(notice).toBeVisible({ timeout: 30_000 });
  await expect(notice).toHaveCount(1);
  await page.screenshot({ path: 'screenshots/fonts-offline-notice.png' });
  await context.setOffline(false);
  await expect(notice).toHaveCount(0, { timeout: 30_000 });
  let state: { slices: string[]; waiting: number; uncovered: string } | undefined;
  await expect
    .poll(
      async () => {
        state = (await inCoreWorker(page, 'kilnFonts', () =>
          (globalThis as unknown as { kilnFonts: () => unknown }).kilnFonts(),
        )) as typeof state;
        return state?.waiting;
      },
      { timeout: 30_000 },
    )
    .toBe(0);
  console.log(`FONTS after the network came back: ${JSON.stringify(state)}`);
  expect(state?.slices.length).toBeGreaterThan(0);
  expect(state?.uncovered).toBe('');
  await page.screenshot({ path: 'screenshots/fonts-online-again.png' });
});
