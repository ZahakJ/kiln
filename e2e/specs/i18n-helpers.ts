import { expect, type Page } from '@playwright/test';

/**
 * Waits until the app's document has opened and drawn: the title is no longer "Opening…" and
 * the status bar shows the document, in the language of `m` (English when a message is absent).
 */
export async function loaded(page: Page, m: Record<string, string>): Promise<void> {
  const title = page.getByTestId('document-title');
  await expect(title).toBeVisible({ timeout: 45_000 });
  await expect(title).not.toContainText(m['Opening…'] ?? 'Opening…', { timeout: 45_000 });
  await expect(page.locator('.kiln-statusbar')).not.toContainText(m['No document'] ?? 'No document', {
    timeout: 45_000,
  });
  await page.waitForFunction(
    () => {
      const grid = document.querySelector<HTMLElement>('[data-kiln-grid-surface]');
      return !grid || Number(grid.dataset.kilnFrames ?? 0) > 0;
    },
    null,
    { timeout: 45_000 },
  );
  await page.evaluate(() => document.fonts.ready);
}

/** Waits for running CSS animations (a page or dialog fading in) before a screenshot. */
export async function settle(page: Page): Promise<void> {
  await page.evaluate(() =>
    Promise.all(
      document
        .getAnimations()
        .filter((a) => a.effect?.getTiming().iterations !== Number.POSITIVE_INFINITY)
        .map((a) => a.finished.catch(() => undefined)),
    ),
  );
}
