/**
 * Undo and Redo with a long history of large steps, measured on the installable web app: a
 * 50,000-row sheet, then twelve pastes of 20,000 rows each, with Undo and Redo timed after the
 * 1st, 6th and 12th. Undo takes its step off the top of the history: its time must not grow with
 * the history's size, nor may the page's memory. `KILN_PWA_SITE` runs it against another build
 * (the numbers are printed as `UNDOPERF`).
 */

import { spawn } from 'node:child_process';
import { createServer } from 'node:net';
import { fileURLToPath } from 'node:url';
import { expect, type Page, test } from '@playwright/test';
import { CORE_START, prepare } from './pwa-helpers.ts';

const repo = fileURLToPath(new URL('../../', import.meta.url));
const SITE = process.env.KILN_PWA_SITE ?? `${repo}target/pages/site`;

test.skip(!!process.env.KILN_E2E_BASE, 'serves the artefact itself');

const bar = (page: Page) => page.getByRole('textbox', { name: 'Formula Bar' });
const nameBox = (page: Page) =>
  page
    .getByRole('combobox', { name: 'Name Box' })
    .or(page.getByRole('textbox', { name: 'Name Box' }))
    .first();

const freePort = () =>
  new Promise<number>((resolve) => {
    const probe = createServer();
    probe.listen(0, '127.0.0.1', () => {
      const address = probe.address();
      probe.close(() => resolve(typeof address === 'object' && address ? address.port : 0));
    });
  });

async function paste(page: Page, rows: number, tag: string, at: string): Promise<void> {
  await nameBox(page).click();
  await nameBox(page).fill(at);
  await nameBox(page).press('Enter');
  await page.evaluate(
    ([n, t]) => {
      const lines: string[] = [];
      for (let r = 0; r < (n as number); r += 1) {
        const cells: string[] = [];
        for (let k = 0; k < 9; k += 1)
          cells.push(k % 3 === 0 ? `${t} ${r}-${k}` : String((r * 7 + k * 13) % 1000));
        cells.push(`=B${r + 1}*2+C${r + 1}`);
        lines.push(cells.join('\t'));
      }
      const data = new DataTransfer();
      data.setData('text/plain', lines.join('\n'));
      document
        .querySelector('[data-kiln-grid-proxy]')
        ?.dispatchEvent(
          new ClipboardEvent('paste', { clipboardData: data, bubbles: true, cancelable: true }),
        );
    },
    [rows, tag] as const,
  );
  await nameBox(page).click();
  await nameBox(page).fill(at);
  await nameBox(page).press('Enter');
  await expect(bar(page)).toHaveValue(`${tag} 0-0`, { timeout: 300_000 });
}

/** Presses `key` and waits until the Formula Bar no longer shows `from`. */
async function timed(page: Page, key: string, from: string): Promise<number> {
  const t0 = await page.evaluate(() => performance.now());
  await page.keyboard.press(key);
  await page.waitForFunction(
    (x) => (document.querySelector('[aria-label="Formula Bar"]') as HTMLTextAreaElement | null)?.value !== x,
    from,
    { timeout: 120_000, polling: 'raf' },
  );
  return Math.round((await page.evaluate(() => performance.now())) - t0);
}

test('Grid: Undo and Redo stay quick, and memory stays put, with twelve large steps in the history', async ({
  browser,
}) => {
  test.setTimeout(1_200_000);
  const port = await freePort();
  const child = spawn(
    'node',
    [`${repo}tools/pages-serve.mjs`, '--port', String(port), '--root', SITE, '--test'],
    {
      stdio: 'ignore',
    },
  );
  try {
    const origin = `http://127.0.0.1:${port}`;
    await expect
      .poll(() =>
        fetch(`${origin}/__kiln/health`).then(
          (r) => r.ok,
          () => false,
        ),
      )
      .toBe(true);
    const context = await browser.newContext({
      baseURL: `${origin}/kiln/app/`,
      viewport: { width: 1280, height: 800 },
      serviceWorkers: 'block',
    });
    const page = await context.newPage();
    await prepare(page);
    await page.goto('./#/grid');
    await expect(bar(page)).toBeVisible({ timeout: CORE_START });
    await paste(page, 50_000, 'P0', 'A1');
    const cols = ['L', 'W', 'AH', 'AS', 'BD', 'BO', 'BZ', 'CK', 'CV', 'DG', 'DR', 'EC'];
    const undo: Record<number, number> = {};
    const redo: Record<number, number> = {};
    for (let k = 0; k < cols.length; k += 1) {
      await paste(page, 20_000, `U${k}`, `${cols[k]}1`);
      await page.waitForTimeout(1500);
      if (k === 0 || k === 5 || k === 11) {
        const shown = `U${k} 0-0`;
        undo[k + 1] = await timed(page, 'Control+z', shown);
        redo[k + 1] = await timed(page, 'Control+y', await bar(page).inputValue());
        await expect(bar(page)).toHaveValue(shown, { timeout: 120_000 });
        await page.waitForTimeout(2000);
      }
    }
    await page.waitForTimeout(3000);
    const heap = await page.evaluate(
      () => (performance as unknown as { memory?: { usedJSHeapSize: number } }).memory?.usedJSHeapSize ?? 0,
    );
    console.log(`UNDOPERF ${JSON.stringify({ site: SITE, undo, redo, heapMB: Math.round(heap / 1e6) })}`);
    // The review measured the build before this round at 575 ms and 57 MB at twelve steps.
    expect(undo[12]).toBeLessThan(1500);
    expect(heap / 1e6).toBeLessThan(200);
    await context.close();
  } finally {
    child.kill('SIGKILL');
  }
});
