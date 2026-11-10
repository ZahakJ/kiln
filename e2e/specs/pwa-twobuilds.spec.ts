/**
 * What one build leaves, the next reads: a tail and a journal written by build A, replayed by build
 * B on the same origin. `KILN_OLD_SITE` names build A's Pages artefact; the current one is B.
 */
import { spawn } from 'node:child_process';
import { createServer } from 'node:net';
import { fileURLToPath } from 'node:url';
import { expect, test } from '@playwright/test';
import { CORE_START, prepare } from './pwa-helpers.ts';

const repo = fileURLToPath(new URL('../../', import.meta.url));
const OLD = process.env.KILN_OLD_SITE;
const NEW = process.env.KILN_PWA_SITE ?? `${repo}target/pages/site`;

test.skip(!OLD, 'needs an earlier build: KILN_OLD_SITE');

const freePort = () =>
  new Promise<number>((resolve) => {
    const probe = createServer();
    probe.listen(0, '127.0.0.1', () => {
      const address = probe.address();
      probe.close(() => resolve(typeof address === 'object' && address ? address.port : 0));
    });
  });

test('a tail and a journal of the build before are replayed by this one', async ({ browser }) => {
  test.setTimeout(240_000);
  const port = await freePort();
  const child = spawn(
    'node',
    [`${repo}tools/pages-serve.mjs`, '--port', String(port), '--root', OLD as string, '--test'],
    { stdio: 'ignore' },
  );
  const origin = `http://127.0.0.1:${port}`;
  try {
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
    const first = await context.newPage();
    await prepare(first);
    await first.goto('./#/grid');
    const bar = (p: typeof first) => p.getByRole('textbox', { name: 'Formula Bar' });
    await expect(bar(first)).toBeVisible({ timeout: CORE_START });
    await first.mouse.click(420, 420);
    await first.keyboard.type('1111\n');
    await first.waitForTimeout(400);
    // Build A's last edit is only in its tail.
    await first.evaluate(() => {
      FileSystemWritableFileStream.prototype.close = () => new Promise(() => undefined);
    });
    await first.mouse.click(420, 420);
    await first.keyboard.type('4242\n');
    await first.waitForTimeout(150);
    await first.mouse.click(420, 420);
    await expect(bar(first)).toHaveValue('4242');
    const cdp = await context.newCDPSession(first);
    const crashed = first.waitForEvent('crash');
    cdp.send('Page.crash').catch(() => undefined);
    await crashed;

    await fetch(`${origin}/__kiln/deploy?root=${encodeURIComponent(NEW)}`, { method: 'POST' });
    const page = await context.newPage();
    await prepare(page);
    await page.goto('./#/grid');
    await first.close().catch(() => undefined);
    const row = page.getByTestId('document-recovery').locator('[data-recovery]');
    await expect(row).toHaveCount(1, { timeout: CORE_START });
    await row.getByRole('button', { name: 'Open' }).click();
    await expect(row).toHaveCount(0, { timeout: CORE_START });
    await expect
      .poll(
        async () => {
          await page.mouse.click(420, 420);
          return bar(page).inputValue();
        },
        { timeout: 20_000 },
      )
      .toBe('4242');
    await context.close();
  } finally {
    child.kill('SIGKILL');
  }
});
