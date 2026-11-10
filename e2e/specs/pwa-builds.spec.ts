/**
 * Updates of the installable web app between real builds: the Pages artefact and variants
 * of it in which every content-hashed file has a new name and new bytes (`tools/pages-variant.mjs`),
 * served as Pages serves them, with each deployment replacing the last on the server.
 *
 * - an update on a device that was offline-ready stays offline-ready: all three apps open with no
 *   network after the window closes, and after Reload pressed with no network;
 * - a failed fetch fails the install: the old version keeps working, and the next check succeeds;
 * - old caches go, one version behind (a tab may still be running it);
 * - unsaved work written just before Reload comes back after the switch;
 * - a tab left open across a deployment says "Kiln has been updated", never an error.
 */

import { execFileSync, spawn } from 'node:child_process';
import { createServer } from 'node:net';
import { fileURLToPath } from 'node:url';
import { type BrowserContext, expect, type Page, test } from '@playwright/test';
import { settle } from './i18n-helpers.ts';
import { CORE_START, coreThreads, openIsolated, prepare, typeInDocument, WARM } from './pwa-helpers.ts';

const repo = fileURLToPath(new URL('../../', import.meta.url));
const shot = (name: string) => fileURLToPath(new URL(`../screenshots/pwa/${name}`, import.meta.url));
/** The artefact under test (`KILN_PWA_SITE` to run these against another build). */
const SITE = process.env.KILN_PWA_SITE ?? `${repo}target/pages/site`;
const variant = (name: string) => `${repo}target/pages/variants/${name}`;

test.skip(!!process.env.KILN_E2E_BASE, 'deploys to the local Pages emulation');
// Each test takes its own server, and these are long: one after another.
test.describe.configure({ mode: 'default', timeout: 420_000 });

test.beforeAll(() => {
  for (const name of ['b', 'c'])
    execFileSync('node', [`${repo}tools/pages-variant.mjs`, SITE, variant(name), name], { stdio: 'pipe' });
});

interface Site {
  base: string;
  deploy(name: 'b' | 'c'): Promise<void>;
  block(files: string | null): Promise<void>;
  stop(): Promise<void>;
}

const freePort = () =>
  new Promise<number>((resolve) => {
    const probe = createServer();
    probe.listen(0, '127.0.0.1', () => {
      const address = probe.address();
      probe.close(() => resolve(typeof address === 'object' && address ? address.port : 0));
    });
  });

/** The test's servers: a failing test does not reach its own `stop()`. */
const running = new Set<Site>();
test.afterEach(async () => {
  for (const site of running) await site.stop();
  running.clear();
});

/** A server of its own (the origin's service worker and caches are the test's own too). */
async function startSite(): Promise<Site> {
  const port = await freePort();
  const child = spawn(
    'node',
    [`${repo}tools/pages-serve.mjs`, '--port', String(port), '--root', SITE, '--test'],
    {
      stdio: 'ignore',
    },
  );
  const origin = `http://127.0.0.1:${port}`;
  await expect
    .poll(() =>
      fetch(`${origin}/__kiln/health`).then(
        (r) => r.ok,
        () => false,
      ),
    )
    .toBe(true);
  const stopped = new Promise<void>((resolve) => child.once('exit', () => resolve()));
  const site: Site = {
    base: `${origin}/kiln/app/`,
    deploy: async (name) => {
      await fetch(`${origin}/__kiln/deploy?root=${encodeURIComponent(variant(name))}`, { method: 'POST' });
    },
    block: async (files) => {
      await fetch(`${origin}/__kiln/block?files=${encodeURIComponent(files ?? '')}`, { method: 'POST' });
    },
    stop: async () => {
      if (child.exitCode !== null || child.signalCode !== null) return;
      child.kill('SIGKILL');
      await stopped;
    },
  };
  running.add(site);
  return site;
}

/** A page of a fresh profile on `site`. */
async function visit(
  browser: import('@playwright/test').Browser,
  site: Site,
): Promise<{ context: BrowserContext; page: Page }> {
  const context = await browser.newContext({ baseURL: site.base, viewport: { width: 1280, height: 800 } });
  return { context, page: await context.newPage() };
}

const readyOffline = (page: Page) =>
  expect(page.locator('html')).toHaveAttribute('data-kiln-offline', 'ready', { timeout: WARM });

/** The page's own script: which build a window runs. */
const buildOf = (page: Page) =>
  page.evaluate(() =>
    document.querySelector<HTMLScriptElement>('script[type="module"][src]')?.src.split('/').pop(),
  );

const checkForUpdate = (page: Page) =>
  page.evaluate(async () => {
    await (await navigator.serviceWorker.getRegistration())?.update();
  });

/** The app's window has its document: no core failure, no "Opening…". */
async function appOpens(page: Page): Promise<void> {
  const title = page.getByTestId('document-title');
  await expect(title).not.toContainText('Opening', { timeout: CORE_START });
  await expect(page.locator('[data-problem]')).toHaveCount(0);
  await expect(page.getByTestId('load-problem')).toHaveCount(0);
  await expect(page.getByTestId('save-status').first()).not.toContainText('Not available');
}

/** Write, Grid and Deck each start with no network, on the threads module for Grid. */
async function everyAppOpens(page: Page): Promise<void> {
  for (const app of ['write', 'grid', 'deck']) {
    await page.goto(`./#/${app}`);
    await page.reload();
    await appOpens(page);
    if (app === 'grid') expect(await coreThreads(page, 'grid')).toBeGreaterThan(0);
  }
}

/** Every cache and the wasm files it holds. */
const cached = (page: Page) =>
  page.evaluate(async () => {
    const names = await caches.keys();
    const wasm: string[] = [];
    for (const name of names) {
      const cache = await caches.open(name);
      for (const request of await cache.keys())
        if (request.url.endsWith('.wasm')) wasm.push(request.url.split('/').pop() as string);
    }
    return { shells: names.filter((n) => n.startsWith('kiln-shell-')).sort(), wasm };
  });

test('an update stays offline-ready when the window closes and the network goes', async ({ browser }) => {
  const site = await startSite();
  const { context, page } = await visit(browser, site);
  await openIsolated(page);
  await readyOffline(page);
  const before = await buildOf(page);

  await site.deploy('b');
  await checkForUpdate(page);
  await expect(page.getByTestId('pwa-update')).toBeVisible({ timeout: WARM });

  // Nobody presses Reload: the waiting version takes over by itself when the last window closes.
  await page.close();
  await new Promise((r) => setTimeout(r, 1500));
  await site.stop();
  await context.setOffline(true);
  const after = await context.newPage();
  await prepare(after);
  await after.goto('./');
  expect(await buildOf(after)).not.toBe(before);
  await everyAppOpens(after);
  await context.close();
});

test('Reload pressed after the network has gone lands in the new version, and it works offline', async ({
  browser,
}) => {
  const site = await startSite();
  const { context, page } = await visit(browser, site);
  await openIsolated(page);
  await readyOffline(page);
  const before = await buildOf(page);

  await site.deploy('b');
  await checkForUpdate(page);
  const reload = page.getByTestId('pwa-reload');
  await expect(reload).toBeVisible({ timeout: WARM });
  await site.stop();
  await context.setOffline(true);
  await Promise.all([page.waitForEvent('load', { timeout: 60_000 }), reload.click()]);
  await expect(page.getByRole('heading', { level: 1 })).toBeVisible({ timeout: CORE_START });
  expect(await buildOf(page)).not.toBe(before);
  await everyAppOpens(page);
  await context.close();
});

test('a failed fetch fails the install: the old version keeps working, and the next check succeeds', async ({
  browser,
}) => {
  const site = await startSite();
  const { context, page } = await visit(browser, site);
  await openIsolated(page);
  await readyOffline(page);
  const before = await buildOf(page);

  // One core of the new build is missing.
  await site.block('kiln_bridge_wasm_bg.*\\.wasm$');
  await site.deploy('b');
  const outcome = await page.evaluate(async () => {
    const reg = await navigator.serviceWorker.getRegistration();
    await reg?.update();
    const worker = reg?.installing;
    if (!worker) return 'none';
    return new Promise<string>((resolve) => {
      worker.addEventListener('statechange', () => {
        if (worker.state === 'redundant' || worker.state === 'installed') resolve(worker.state);
      });
    });
  });
  expect(outcome).toBe('redundant');
  await expect(page.getByTestId('pwa-update')).toHaveCount(0);

  await page.reload();
  await expect(page.getByRole('heading', { level: 1 })).toBeVisible({ timeout: CORE_START });
  expect(await buildOf(page)).toBe(before);
  await context.setOffline(true);
  await everyAppOpens(page);
  await context.setOffline(false);

  await site.block(null);
  await page.goto('./#/home');
  await checkForUpdate(page);
  await expect(page.getByTestId('pwa-update')).toBeVisible({ timeout: WARM });
  await site.stop();
  await context.close();
});

test('old caches go one version behind', async ({ browser }) => {
  const site = await startSite();
  const { context, page } = await visit(browser, site);
  await openIsolated(page);
  await readyOffline(page);
  const first = await cached(page);
  expect(first.shells).toHaveLength(1);
  expect(first.wasm).toHaveLength(5);

  const update = async (name: 'b' | 'c') => {
    await site.deploy(name);
    await checkForUpdate(page);
    const reload = page.getByTestId('pwa-reload');
    await expect(reload).toBeVisible({ timeout: WARM });
    await Promise.all([page.waitForEvent('load', { timeout: 60_000 }), reload.click()]);
    await expect(page.getByRole('heading', { level: 1 })).toBeVisible({ timeout: CORE_START });
    await readyOffline(page);
  };

  // The version before stays, for a tab still running it.
  await update('b');
  const second = await cached(page);
  expect(second.shells).toEqual(
    [...first.shells, ...second.shells.filter((s) => !first.shells.includes(s))].sort(),
  );
  expect(second.shells).toHaveLength(2);
  expect(second.wasm).toHaveLength(10);
  for (const file of first.wasm) expect(second.wasm).toContain(file);

  await update('c');
  const third = await cached(page);
  expect(third.shells).toHaveLength(2);
  for (const shell of first.shells) expect(third.shells).not.toContain(shell);
  expect(third.wasm).toHaveLength(10);
  for (const file of first.wasm) expect(third.wasm).not.toContain(file);
  await site.stop();
  await context.close();
});

for (const app of ['write', 'grid'] as const)
  test(`${app}: what was typed just before Reload comes back after the switch`, async ({ browser }) => {
    const site = await startSite();
    const { context, page } = await visit(browser, site);
    await openIsolated(page);
    await page.goto(`./#/${app}`);
    await appOpens(page);
    await site.deploy('b');
    await checkForUpdate(page);
    const notice = page.getByTestId('pwa-update');
    await expect(notice).toBeVisible({ timeout: WARM });
    await expect(notice).not.toContainText('Document Recovery');

    // The notice was already up, and nothing was journalled yet when it appeared.
    if (app === 'write') await typeInDocument(page, 'Words the update must not lose');
    else {
      await page.mouse.click(420, 420);
      await page.keyboard.type('1111\n');
      await expect(page.getByTestId('save-status').first()).toContainText(/Not saved|Unsaved/, {
        timeout: 15_000,
      });
      await page.waitForTimeout(1500);
      // The second edit lands on a journal already started.
      await page.mouse.click(420, 420);
      await page.keyboard.type('4242\n');
      await page.waitForTimeout(100);
      await page.mouse.dblclick(420, 420);
    }
    await expect(notice).toContainText('Document Recovery');
    await Promise.all([
      page.waitForEvent('load', { timeout: 60_000 }),
      page.getByTestId('pwa-reload').click(),
    ]);

    const row = page.getByTestId('document-recovery').locator('[data-recovery]');
    await expect(row).toHaveCount(1, { timeout: CORE_START });
    await row.getByRole('button', { name: 'Open' }).click();
    if (app === 'write') await expect(page.getByText('6 words')).toBeVisible({ timeout: CORE_START });
    else {
      await expect(page).toHaveURL(/#\/grid\?doc=/, { timeout: CORE_START });
      await page.waitForTimeout(800);
      await page.mouse.click(420, 420);
      await expect(page.getByRole('textbox', { name: 'Formula Bar' })).toHaveValue('4242', {
        timeout: 15_000,
      });
    }
    await site.stop();
    await context.close();
  });

test('a tab left open across a deployment says Kiln has been updated, and Reload brings the new version', async ({
  browser,
}) => {
  const site = await startSite();
  const { context, page } = await visit(browser, site);
  await openIsolated(page);
  const before = await buildOf(page);
  await page.goto('./#/write');
  await appOpens(page);
  await typeInDocument(page, 'Words in the old tab');
  await page.waitForTimeout(300);

  // The server moves on while Deck's core was never fetched.
  await site.deploy('b');
  await page.evaluate(() => {
    window.location.hash = '#/deck';
  });
  const card = page.getByTestId('load-problem');
  await expect(card).toHaveAttribute('data-failure', 'updated', { timeout: CORE_START });
  await expect(card).toContainText('Kiln has been updated');
  await expect(card).toContainText('Document Recovery');
  await expect(page.locator('body')).not.toContainText(/failed to fetch|did not start|Wasm|404/);
  // The corner notice does not repeat it.
  await expect(page.getByTestId('pwa-update')).toHaveCount(0);
  await page.waitForTimeout(400);
  await settle(page);
  await page.screenshot({ path: shot('stale-tab-light.png') });
  await page.emulateMedia({ colorScheme: 'dark' });
  await page.waitForTimeout(600);
  await settle(page);
  await page.screenshot({ path: shot('stale-tab-dark.png') });
  await page.setViewportSize({ width: 412, height: 915 });
  await page.waitForTimeout(400);
  await page.screenshot({ path: shot('stale-tab-dark-phone.png') });
  await page.setViewportSize({ width: 1280, height: 800 });
  await page.emulateMedia({ colorScheme: 'light' });

  await Promise.all([
    page.waitForEvent('load', { timeout: WARM }),
    page.getByTestId('load-problem-reload').click(),
  ]);
  await expect(page.getByTestId('document-recovery').locator('[data-recovery]')).toHaveCount(1, {
    timeout: CORE_START,
  });
  expect(await buildOf(page)).not.toBe(before);
  await appOpens(page);
  await site.stop();
  await context.close();
});

test('the state reads in German and Arabic', async ({ browser }) => {
  const site = await startSite();
  for (const [language, title] of [
    ['de', 'Kiln wurde aktualisiert'],
    ['ar', ''],
  ] as const) {
    const { context, page } = await visit(browser, site);
    await page.addInitScript((ui) => {
      localStorage.setItem('kiln.options', JSON.stringify({ language: { ui } }));
    }, language);
    await openIsolated(page);
    await site.deploy('b');
    await page.evaluate(() => {
      window.location.hash = '#/deck';
    });
    const card = page.getByTestId('load-problem');
    await expect(card).toHaveAttribute('data-failure', 'updated', { timeout: CORE_START });
    if (title) await expect(card).toContainText(title);
    else await expect(card).not.toContainText('Kiln has been updated');
    await page.waitForTimeout(400);
    await settle(page);
    await page.screenshot({ path: shot(`stale-tab-${language}.png`) });
    await context.close();
    await site.block(null);
    await fetch(`${site.base.replace(/\/kiln\/app\/$/, '')}/__kiln/deploy?root=${encodeURIComponent(SITE)}`, {
      method: 'POST',
    });
  }
  await site.stop();
});

test('with no service worker, a screen whose chunk is gone says Kiln has been updated too', async ({
  browser,
}) => {
  const site = await startSite();
  const context = await browser.newContext({
    baseURL: site.base,
    serviceWorkers: 'block',
    viewport: { width: 1280, height: 800 },
  });
  const page = await context.newPage();
  await prepare(page);
  // The editors' chunk of the old build was never fetched (the page can start fetching it early).
  await page.route(/\/assets\/app-frame-/, (route) => route.fulfill({ status: 404 }));
  await page.goto('./');
  await expect(page.getByRole('heading', { level: 1 })).toBeVisible({ timeout: CORE_START });
  const before = await buildOf(page);
  await site.deploy('b');
  await page.evaluate(() => {
    window.location.hash = '#/write';
  });
  const card = page.getByTestId('load-problem');
  await expect(card).toHaveAttribute('data-failure', 'updated', { timeout: CORE_START });
  await page.waitForTimeout(400);
  await settle(page);
  await expect(page.locator('.kiln-load-screen')).toBeVisible();
  await page.screenshot({ path: shot('stale-chunk.png') });
  await page.unroute(/\/assets\/app-frame-/);
  await Promise.all([
    page.waitForEvent('load', { timeout: 60_000 }),
    page.getByTestId('load-problem-reload').click(),
  ]);
  expect(await buildOf(page)).not.toBe(before);
  await appOpens(page);
  await site.stop();
  await context.close();
});
