/**
 * Kiln Write keeps the caret on screen while text flows onto new pages.
 *
 * The caret is brought into view when the change that moves it arrives. When text has just
 * flowed onto a new page, that change can come before the frame that makes the document taller,
 * and the browser stops the scroll short of the caret; keys typed in a burst are all sent before
 * the first change comes back. In both cases the caret must still end up on screen without another
 * key: the new page shows where the typing goes.
 */
import { expect, type Page, test } from '@playwright/test';

type WriteHandle = {
  client: { frame: (...a: unknown[]) => Promise<unknown> };
  controller: { getStats(): { tiles: number } } | null;
};
type KilnWindow = { __kilnWrite?: WriteHandle };

test.describe.configure({ timeout: 240_000 });

async function openWrite(page: Page): Promise<void> {
  await page.goto(`/#/write?new=${Date.now()}`);
  await page.waitForFunction(
    () => ((window as unknown as KilnWindow).__kilnWrite?.controller?.getStats().tiles ?? 0) > 0,
    null,
    { timeout: 45_000 },
  );
  await page.getByTestId('kiln-input-proxy').focus();
}

/** Every frame of the document reaches the page `ms` late, as on a busy machine. */
async function lateFrames(page: Page, ms: number): Promise<void> {
  await page.evaluate((delay) => {
    const c = (window as unknown as KilnWindow).__kilnWrite?.client;
    if (!c) throw new Error('no Write client');
    const frame = c.frame.bind(c);
    c.frame = (...a: unknown[]) => new Promise((ok) => setTimeout(ok, delay)).then(() => frame(...a));
  }, ms);
}

/** The status bar's "Page x of y". */
async function pages(page: Page): Promise<{ at: number; of: number }> {
  const text = await page
    .getByText(/^Page \d+ of \d+$/)
    .first()
    .textContent();
  const m = /Page (\d+) of (\d+)/.exec(text ?? '');
  return { at: Number(m?.[1] ?? 0), of: Number(m?.[2] ?? 0) };
}

/** Whether the caret (the input proxy sits on it) is inside the visible part of the document. */
async function caretOnScreen(page: Page): Promise<boolean> {
  return page.evaluate(() => {
    const proxy = document.querySelector('[data-testid="kiln-input-proxy"]');
    const doc = document.querySelector('[data-testid="kiln-surface"]');
    if (!proxy || !doc) return false;
    const c = proxy.getBoundingClientRect();
    const v = doc.getBoundingClientRect();
    return c.top >= v.top && c.top + Math.max(c.height, 1) <= v.bottom;
  });
}

const line = (i: number) => `Line ${i}: the quick brown fox jumps over the lazy dog`;

for (const late of [0, 400])
  test(`the caret stays on screen as typing flows onto new pages${late ? `, frames ${late} ms late` : ''}`, async ({
    page,
  }) => {
    await openWrite(page);
    if (late) await lateFrames(page, late);
    for (let i = 1; i <= 60; i++) {
      await page.keyboard.type(line(i));
      await page.keyboard.press('Enter');
    }
    // No key after the last one: the view has to follow by itself.
    await expect.poll(async () => (await pages(page)).of, { timeout: 30_000 }).toBeGreaterThanOrEqual(3);
    await expect.poll(() => caretOnScreen(page), { timeout: 10_000 }).toBe(true);
    const p = await pages(page);
    expect(p.at).toBe(p.of);
  });

test('the caret is on screen after each line typed onto a new page, 20 rounds', async ({ page }) => {
  await openWrite(page);
  await lateFrames(page, 250);
  let i = 0;
  for (let round = 1; round <= 20; round++) {
    const before = (await pages(page)).of;
    // Lines until the text has flowed onto the next page.
    while ((await pages(page)).of === before) {
      i += 1;
      await page.keyboard.type(line(i));
      await page.keyboard.press('Enter');
      if (i > 2000) throw new Error('no new page');
    }
    await expect
      .poll(() => caretOnScreen(page), { timeout: 10_000, message: `round ${round}, line ${i}` })
      .toBe(true);
  }
});

test('the caret is on screen after a paste that runs onto a new page, frames late', async ({ page }) => {
  await openWrite(page);
  await lateFrames(page, 250);
  await page.keyboard.insertText(Array.from({ length: 70 }, (_, i) => line(i + 1)).join('\n'));
  await expect.poll(async () => (await pages(page)).of, { timeout: 30_000 }).toBeGreaterThanOrEqual(3);
  await expect.poll(() => caretOnScreen(page), { timeout: 10_000 }).toBe(true);
});
