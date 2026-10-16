import { readFileSync } from 'node:fs';
import { fileURLToPath } from 'node:url';
import { expect, type Page, test } from '@playwright/test';

/**
 * The bridge end to end (WP-52): the page boots the WebAssembly core in a Worker, shows the core
 * version from a Ping, and a scratch document round-trips typing through the core.
 * Needs the wasm build: `cargo xtask wasm` before `pnpm e2e`.
 */

const shot = (name: string) => fileURLToPath(new URL(`../screenshots/bridge/${name}`, import.meta.url));
const suiteVersion: string = JSON.parse(
  readFileSync(fileURLToPath(new URL('../../package.json', import.meta.url)), 'utf8'),
).version;

async function paragraphs(page: Page): Promise<string[]> {
  return page
    .getByTestId('scratch-paragraph')
    .evaluateAll((ps) => ps.map((p) => p.getAttribute('data-text') ?? ''));
}

test.describe('bridge', () => {
  test('shows the core version from a Ping through the Worker', async ({ page }) => {
    const errors: string[] = [];
    page.on('pageerror', (e) => errors.push(e.message));
    await page.emulateMedia({ colorScheme: 'light' });
    await page.goto('/#/frame');
    await expect(page.getByTestId('core-version')).toHaveText(`Kiln core ${suiteVersion}`);
    await expect(page.getByTestId('core-protocol')).toHaveText('v1');
    await expect(page.getByTestId('bridge-kind')).toHaveText('WebAssembly in a Worker');
    // The core really runs in a Worker, not on the page.
    expect(page.workers().some((w) => w.url().includes('worker'))).toBe(true);
    expect(errors).toEqual([]);
    await page.screenshot({ path: shot('frame-light-desktop.png'), fullPage: true });
  });

  test('typing in the scratch route round-trips text through the core', async ({ page }) => {
    await page.goto('/#/scratch');
    const editor = page.getByTestId('scratch-editor');
    await expect(editor).toHaveAttribute('data-ready', 'true');
    await expect(page.getByTestId('scratch-title')).toContainText('Scratch1');
    await editor.click();

    await page.keyboard.type('Hello world');
    await page.keyboard.press('Enter');
    await page.keyboard.type('Kiln');
    await expect.poll(() => paragraphs(page)).toEqual(['Hello world', 'Kiln']);
    await expect(page.getByTestId('scratch-words')).toHaveText('3 words');
    await expect(page.getByRole('img', { name: 'Unsaved changes' })).toBeVisible();

    await page.keyboard.press('Backspace');
    await page.keyboard.press('Backspace');
    await expect.poll(() => paragraphs(page)).toEqual(['Hello world', 'Ki']);

    // Undo is one step per typing or deleting run, as in Word.
    await page.keyboard.press('Control+z');
    await expect.poll(() => paragraphs(page)).toEqual(['Hello world', 'Kiln']);
    await page.keyboard.press('Control+y');
    await expect.poll(() => paragraphs(page)).toEqual(['Hello world', 'Ki']);

    // Selection and replacement are the core's too.
    await page.keyboard.press('Control+Home');
    await page.keyboard.press('Control+Shift+ArrowRight');
    await page.keyboard.type('Hi ');
    await expect.poll(() => paragraphs(page)).toEqual(['Hi world', 'Ki']);
    await page.keyboard.insertText('é👋');
    await expect.poll(() => paragraphs(page)).toEqual(['Hi é👋world', 'Ki']);
    await page.keyboard.press('Backspace');
    await expect.poll(() => paragraphs(page)).toEqual(['Hi éworld', 'Ki']);
    await expect(page.getByTestId('scratch-caret')).toHaveCount(1);

    await page.getByRole('button', { name: 'Undo' }).click();
    await expect.poll(() => paragraphs(page)).toEqual(['Hi é👋world', 'Ki']);
    await page.screenshot({ path: shot('scratch-light-desktop.png'), fullPage: true });
  });

  test('the scratch route works in dark mode on a phone', async ({ page }) => {
    await page.setViewportSize({ width: 390, height: 844 });
    await page.emulateMedia({ colorScheme: 'dark' });
    await page.goto('/#/scratch');
    const editor = page.getByTestId('scratch-editor');
    await expect(editor).toHaveAttribute('data-ready', 'true');
    await editor.click();
    await page.keyboard.type('Dark mode works');
    await expect.poll(() => paragraphs(page)).toEqual(['Dark mode works']);
    const overflow = await page.evaluate(() => document.documentElement.scrollWidth > window.innerWidth);
    expect(overflow).toBe(false);
    await page.screenshot({ path: shot('scratch-dark-phone.png'), fullPage: true });
  });
});
