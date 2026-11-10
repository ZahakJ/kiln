/**
 * A fault inside the core, on the installable web app: a WebAssembly core cannot go on after a
 * panic, and every later call would fail with wasm-bindgen's own words ("recursive use of an
 * object detected…"). The window says what happened in written words, in the interface's
 * language, with Reload, and never shows the core's text; what was typed comes back in Document
 * Recovery after the reload.
 */

import { expect, type Page, test } from '@playwright/test';
import { CORE_START, inCoreWorker, openIsolated } from './pwa-helpers.ts';

test.skip(!!process.env.KILN_E2E_BASE, 'needs a page of its own origin');
test.describe.configure({ mode: 'default', timeout: 240_000 });

const RAW = /recursive use|unsafe aliasing|unreachable|panicked|RuntimeError|a panic forced/i;

const WORDS = {
  en: { title: 'Kiln Grid stopped working', reload: 'Reload' },
  de: { title: 'Kiln Grid funktioniert nicht mehr', reload: 'Neu laden' },
  ar: { title: 'توقّف Kiln Grid عن العمل', reload: 'إعادة التحميل' },
} as const;

async function workbook(page: Page, lang: string): Promise<void> {
  await page.addInitScript((l) => {
    localStorage.setItem('kiln.options', JSON.stringify({ language: { ui: l } }));
  }, lang);
  await openIsolated(page);
  await page.goto('./#/grid');
  await expect(page.locator('[data-kiln-grid-proxy]')).toBeAttached({ timeout: CORE_START });
  await expect(page.getByTestId('document-title')).not.toContainText(/Opening|Wird geöffnet|جارٍ/, {
    timeout: CORE_START,
  });
}

/** Panics inside the Grid core's session, as a bug in it would. */
async function panicTheCore(page: Page): Promise<void> {
  const done = await inCoreWorker(page, 'kilnPanicForTest', () => {
    (globalThis as unknown as { kilnPanicForTest: () => void }).kilnPanicForTest();
    return true;
  });
  expect(done, 'the Grid core Worker').toBe(true);
}

for (const lang of ['en', 'de', 'ar'] as const) {
  test(`${lang}: a panic in the core shows a written message, never the core's text, and the work comes back`, async ({
    page,
  }) => {
    const panics: string[] = [];
    page.on('console', (m) => {
      if (m.type() === 'error' && m.text().includes('the core stopped')) panics.push(m.text());
    });
    await workbook(page, lang);
    await page.keyboard.type('kept');
    await page.keyboard.press('Enter');
    // The edit's journal write lands before the fault.
    await page.waitForTimeout(1500);
    await panicTheCore(page);
    // The next call meets the broken core.
    await page.keyboard.type('x');
    const card = page.getByTestId('load-problem');
    await expect(card).toBeVisible({ timeout: 30_000 });
    await expect(card).toHaveAttribute('data-failure', 'stopped');
    await expect(card).toContainText(WORDS[lang].title);
    await expect(card.getByRole('button', { name: WORDS[lang].reload })).toBeVisible();
    // Nothing on the page is the core's own text: not the card, a toast or a notice.
    await page.waitForTimeout(1000);
    expect(await page.locator('body').innerText()).not.toMatch(RAW);
    expect(panics.join('\n'), 'the panic is in the console for a test run').toContain(
      'a panic forced by a test',
    );
    await page.screenshot({ path: `screenshots/core-stopped-${lang}.png` });
    // Reload: Document Recovery offers what was typed.
    await card.getByRole('button', { name: WORDS[lang].reload }).click();
    const row = page.getByTestId('document-recovery').locator('[data-recovery]');
    await expect(row).toHaveCount(1, { timeout: CORE_START });
  });
}
