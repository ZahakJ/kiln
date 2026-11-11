/**
 * The installable web app on the GitHub Pages artefact (`tools/pages-build.sh`), served as
 * Pages serves it, under /kiln/app/ with no COOP or COEP headers (`tools/pages-serve.mjs`), or on a
 * deployed site (`KILN_E2E_BASE=https://zahakj.github.io/kiln/app/`):
 *
 * - the page boots, the service worker takes over and its headers make the page cross-origin
 *   isolated, so the Grid core runs on its threads module (and on the single one without them);
 * - the manifest is installable (Chromium's own installability check);
 * - offline after the first visit: a template opens, is edited and saved to the Origin Private File
 *   System, and the page reloads offline and opens the saved file again;
 * - a new deployment shows Kiln's update notice, and Reload brings back an unsaved document through
 *   Document Recovery;
 * - with "Keep recovery info" off the notice promises nothing, Reload asks the save-changes
 *   question, and leaving the page warns;
 * - a pool Worker that cannot start leaves Grid on its single module, not on "Opening…";
 * - a file launched through the file handlers opens in its app, and Back leaves it for Kiln Home;
 *   launched or shared into a window with unsaved work, it leaves that work where it was.
 *
 * `pwa-builds.spec.ts` switches between two real builds.
 *
 * The build has no development hooks, so the tests use the interface and the browser's storage.
 */

import { readFileSync } from 'node:fs';
import { createServer } from 'node:http';
import type { AddressInfo } from 'node:net';
import { extname, join } from 'node:path';
import { fileURLToPath } from 'node:url';
import { inflateRawSync } from 'node:zlib';
import { expect, type Page, test } from '@playwright/test';
import { settle } from './i18n-helpers.ts';
import {
  CORE_START,
  coreThreads,
  installLaunchQueue,
  type LaunchHandle,
  offline,
  openIsolated,
  prepare,
  typeInDocument,
  WARM,
} from './pwa-helpers.ts';

const local = !process.env.KILN_E2E_BASE;
const AR = JSON.parse(
  readFileSync(fileURLToPath(new URL('../../packages/i18n/locales/ar.json', import.meta.url)), 'utf8'),
) as Record<string, string>;
const shot = (name: string) => fileURLToPath(new URL(`../screenshots/pwa/${name}`, import.meta.url));

test.beforeAll(async ({ request, baseURL }) => {
  if (!local) return;
  const health = await request.get(new URL('/__kiln/health', baseURL).href);
  const text = await health.text();
  if (!text.startsWith('built'))
    throw new Error('The Pages artefact is not built: run `bash tools/pages-build.sh` (target/pages/site).');
});

/** The files in the Origin Private File System's `documents/` (the web's saves without a picker). */
async function savedDocuments(page: Page): Promise<Record<string, number[]>> {
  return page.evaluate(async () => {
    const out: Record<string, number[]> = {};
    const root = await navigator.storage.getDirectory();
    const dir = await root.getDirectoryHandle('documents', { create: true });
    // biome-ignore lint/suspicious/noExplicitAny: async iteration of a directory is not in the DOM typings
    for await (const [name, handle] of (dir as any).entries()) {
      if (handle.kind !== 'file') continue;
      out[name] = Array.from(new Uint8Array(await (await handle.getFile()).arrayBuffer()));
    }
    return out;
  });
}

/** One part of a zip package, inflated (stored or deflated entries; enough for a saved .docx). */
function zipPart(bytes: Buffer, name: string): string | null {
  const eocd = bytes.lastIndexOf(Buffer.from([0x50, 0x4b, 0x05, 0x06]));
  if (eocd < 0) return null;
  let at = bytes.readUInt32LE(eocd + 16);
  const count = bytes.readUInt16LE(eocd + 10);
  for (let i = 0; i < count; i++) {
    const method = bytes.readUInt16LE(at + 10);
    const size = bytes.readUInt32LE(at + 20);
    const nameLen = bytes.readUInt16LE(at + 28);
    const extra = bytes.readUInt16LE(at + 30);
    const comment = bytes.readUInt16LE(at + 32);
    const local = bytes.readUInt32LE(at + 42);
    const entry = bytes.toString('utf8', at + 46, at + 46 + nameLen);
    if (entry === name) {
      const start = local + 30 + bytes.readUInt16LE(local + 26) + bytes.readUInt16LE(local + 28);
      const data = bytes.subarray(start, start + size);
      return (method === 8 ? inflateRawSync(data) : data).toString('utf8');
    }
    at += 46 + nameLen + extra + comment;
  }
  return null;
}

/** Past every entrance animation, for a screenshot (a card can mount a frame after `settle`). */
async function settled(page: Page): Promise<void> {
  await page.waitForTimeout(400);
  await settle(page);
}

test('the Pages build boots, isolates itself through the service worker and runs Grid on threads', async ({
  page,
}) => {
  await openIsolated(page);
  expect(page.url()).toContain('/kiln/app/');
  await page.goto('./#/grid');
  await expect(page.getByTestId('document-title')).toContainText('Book', { timeout: CORE_START });
  expect(await coreThreads(page, 'grid')).toBeGreaterThan(0);
  // Write has no rayon work on the web: one module.
  await page.goto('./#/write');
  await expect(page.getByTestId('document-title')).toContainText('Document', { timeout: CORE_START });
  expect(await coreThreads(page, 'write')).toBe(0);
});

test('without the service worker the page is not isolated and Grid falls back to its single module', async ({
  browser,
  baseURL,
}) => {
  const context = await browser.newContext({ serviceWorkers: 'block', baseURL });
  const page = await context.newPage();
  await prepare(page);
  await page.goto('./#/grid');
  await expect(page.getByTestId('document-title')).toContainText('Book', { timeout: CORE_START });
  expect(await page.evaluate(() => crossOriginIsolated)).toBe(false);
  expect(await coreThreads(page, 'grid')).toBe(0);
  await context.close();
});

test('the manifest is installable: Chromium reports no installability errors', async ({ page }) => {
  await openIsolated(page);
  const cdp = await page.context().newCDPSession(page);
  const manifest = await cdp.send('Page.getAppManifest');
  expect(manifest.errors).toEqual([]);
  expect(manifest.url).toMatch(/\/kiln\/app\/manifest\.webmanifest$/);
  const parsed = JSON.parse(manifest.data ?? '{}') as {
    display: string;
    theme_color: string;
    file_handlers: { accept: Record<string, string[]> }[];
    launch_handler: { client_mode: string[] };
  };
  expect(parsed.display).toBe('standalone');
  expect(parsed.theme_color).toBe('#F6EFE6');
  // A launch reaches the running window (`launchQueue`) instead of reloading it.
  expect(parsed.launch_handler.client_mode[0]).toBe('focus-existing');
  const extensions = parsed.file_handlers.flatMap((h) => Object.values(h.accept).flat());
  for (const ext of ['.docx', '.xlsx', '.pptx', '.kwrite', '.kgrid', '.kdeck', '.csv'])
    expect(extensions).toContain(ext);
  await expect
    .poll(async () => (await cdp.send('Page.getInstallabilityErrors')).installabilityErrors, {
      timeout: 30_000,
    })
    .toEqual([]);
});

test('offline after the first visit: a template opens, is edited and saved, and opens again after an offline reload', async ({
  page,
  context,
}) => {
  await openIsolated(page);
  // Every core and font is cached a little after the first screen.
  await expect(page.locator('html')).toHaveAttribute('data-kiln-offline', 'ready', { timeout: WARM });
  await expect(page.getByTestId('pwa-offline-ready')).toBeVisible();
  await settle(page);
  await page.screenshot({ path: shot('offline-ready.png') });

  await offline(context, page, true);
  await expect(page.getByTestId('pwa-network')).toContainText('Offline');
  await settle(page);
  await page.screenshot({ path: shot('offline-home.png') });
  await page.getByRole('button', { name: /^Browse templates:/ }).click();
  await page
    .locator('[data-template]')
    .filter({ hasText: /Letter|letter/ })
    .first()
    .click();
  await expect(page).toHaveURL(/#\/write\?template=/, { timeout: CORE_START });
  await expect(page.getByTestId('document-title')).not.toContainText('Opening', { timeout: CORE_START });
  await typeInDocument(page, 'Written offline. ');
  await settle(page);
  await page.screenshot({ path: shot('offline-write.png') });
  await page.keyboard.press('Control+s');
  // The web saves to the Origin Private File System when there is no file picker.
  let name = '';
  await expect
    .poll(
      async () => {
        const files = await savedDocuments(page);
        for (const [file, bytes] of Object.entries(files)) {
          const xml = zipPart(Buffer.from(bytes), 'word/document.xml');
          if (xml?.includes('Written offline.')) {
            name = file;
            return true;
          }
        }
        return false;
      },
      { timeout: 30_000 },
    )
    .toBe(true);
  await expect(page.getByTestId('save-status').first()).toContainText(/Saved/, { timeout: 15_000 });

  // Reload while offline: the service worker serves the app, Home lists the file, it opens.
  await page.goto('./#/home');
  await page.reload();
  await expect(page.getByRole('heading', { level: 1 })).toBeVisible({ timeout: CORE_START });
  const row = page.getByRole('row', { name: new RegExp(name.replace(/\.docx$/, '')) });
  await expect(row).toBeVisible({ timeout: CORE_START });
  await row.locator('.kiln-home__file').click();
  await expect(page).toHaveURL(/#\/write\?doc=/, { timeout: CORE_START });
  await expect(page.getByTestId('document-title')).toContainText(name.replace(/\.docx$/, ''), {
    timeout: CORE_START,
  });
  await page.emulateMedia({ colorScheme: 'dark' });
  await page.waitForTimeout(600);
  await settle(page);
  await page.screenshot({ path: shot('offline-reopened-dark.png') });
  await page.emulateMedia({ colorScheme: 'light' });
  await offline(context, page, false);
  await expect(page.getByTestId('pwa-network')).toContainText('Back online');
});

/** A form posts to the share target as the system share sheet does. */
async function submitShare(page: Page, name: string, content: string): Promise<void> {
  await page.evaluate(() => {
    const form = document.createElement('form');
    form.method = 'POST';
    form.action = 'share-target';
    form.enctype = 'multipart/form-data';
    const input = document.createElement('input');
    input.type = 'file';
    input.name = 'files';
    input.id = 'kiln-share-input';
    form.append(input);
    document.body.append(form);
  });
  await page.locator('#kiln-share-input').setInputFiles({
    name,
    mimeType: 'text/csv',
    buffer: Buffer.from(content),
  });
  await page.evaluate(() => document.querySelector<HTMLInputElement>('#kiln-share-input')?.form?.submit());
}

test('a new deployment shows the update notice, and Reload brings an unsaved document back', async ({
  page,
  request,
  baseURL,
}) => {
  test.skip(!local, 'a deployment of the local Pages emulation only');
  await openIsolated(page);
  await page.goto('./#/write');
  await expect(page.getByTestId('document-title')).toContainText('Document', { timeout: CORE_START });
  await typeInDocument(page, 'Words the update must not lose');
  // The journal reaches the disk.
  await page.waitForTimeout(1500);
  await request.post(new URL('/__kiln/next-version', baseURL).href);
  await page.evaluate(async () => (await navigator.serviceWorker.getRegistration())?.update());
  const notice = page.getByTestId('pwa-update');
  await expect(notice).toBeVisible({ timeout: 60_000 });
  await expect(notice).toContainText('Kiln has been updated');
  await expect(notice).toContainText('Document Recovery');
  await settle(page);
  await page.screenshot({ path: shot('update-light.png') });
  await page.emulateMedia({ colorScheme: 'dark' });
  await page.waitForTimeout(600);
  await settle(page);
  await page.screenshot({ path: shot('update-dark.png') });
  await page.emulateMedia({ colorScheme: 'light' });
  await Promise.all([page.waitForEvent('load', { timeout: 60_000 }), page.getByTestId('pwa-reload').click()]);
  await expect(page.getByTestId('pwa-update')).toHaveCount(0);
  const pane = page.getByTestId('document-recovery');
  await expect(pane).toBeVisible({ timeout: CORE_START });
  await expect(pane.locator('[data-recovery]')).toHaveCount(1);
  await expect(pane.locator('[data-recovery]')).toContainText('Document1');
});

test('a document launched from the file manager opens in its app, and Back returns to Kiln Home', async ({
  page,
}) => {
  await installLaunchQueue(page);
  await openIsolated(page);
  const csv = Array.from(Buffer.from('Item,Qty\nClay,12\nGlaze,4\n'));
  await page.evaluate((b) => (window as unknown as LaunchHandle).__launch('Stock.csv', b), csv);
  await expect(page).toHaveURL(/#\/grid\?doc=/, { timeout: CORE_START });
  await expect(page.getByTestId('document-title')).toContainText('Stock', { timeout: CORE_START });
  await settle(page);
  await page.screenshot({ path: shot('launched-grid.png') });
  // A file no web core opens says so.
  await page.evaluate((b) => (window as unknown as LaunchHandle).__launch('Notes.odt', b), csv);
  await expect(page.getByTestId('pwa-unopened')).toContainText('Notes.odt');
  await settled(page);
  await page.screenshot({ path: shot('launch-unopened.png') });
  await page.goBack();
  await expect(page.getByRole('heading', { level: 1 })).toBeVisible({ timeout: CORE_START });
  await expect(page).toHaveURL(/#\/home|\/kiln\/app\/(#\/?)?$/);
});

test('with recovery off the notice promises nothing, follows the document, and Reload asks first', async ({
  page,
  request,
  baseURL,
}) => {
  test.skip(!local, 'a deployment of the local Pages emulation only');
  await page.addInitScript(() => {
    localStorage.setItem('kiln.options', JSON.stringify({ save: { keepRecoveryInfo: false } }));
    sessionStorage.setItem('kiln.test.page', String(performance.timeOrigin));
  });
  await openIsolated(page);
  await page.goto('./#/write');
  await expect(page.getByTestId('document-title')).toContainText('Document', { timeout: CORE_START });
  await request.post(new URL('/__kiln/next-version', baseURL).href);
  await page.evaluate(async () => (await navigator.serviceWorker.getRegistration())?.update());
  const notice = page.getByTestId('pwa-update');
  await expect(notice).toBeVisible({ timeout: 60_000 });
  await expect(notice).toContainText('Reload to start the new version.');
  await expect(notice).not.toContainText('unsaved');

  // The notice follows the typing that comes after it appeared, and promises no recovery.
  await typeInDocument(page, 'Words with no recovery copy');
  await expect(notice).toContainText('Kiln asks about your unsaved changes first');
  await expect(notice).not.toContainText('Document Recovery');

  // Leaving the page warns: nothing would bring these words back.
  let asked = '';
  page.once('dialog', (d) => {
    asked = d.type();
    void d.dismiss();
  });
  await page.close({ runBeforeUnload: true });
  await expect.poll(() => asked).toBe('beforeunload');
  expect(page.isClosed()).toBe(false);

  // Reload asks; Cancel stays, with the words.
  const question = page.getByTestId('save-changes');
  await page.getByTestId('pwa-reload').click();
  await expect(question).toBeVisible();
  await expect(question).toContainText('Document1');
  await settled(page);
  await page.screenshot({ path: shot('update-recovery-off.png') });
  await question.getByRole('button', { name: 'Cancel' }).click();
  await expect(question).toHaveCount(0);
  await page.waitForTimeout(800);
  await expect(page.getByTestId('save-status').first()).toContainText(/Not saved|Unsaved/);

  // Save writes the document, and then the page reloads into the new version.
  await page.getByTestId('pwa-reload').click();
  await Promise.all([
    page.waitForEvent('load', { timeout: 60_000 }),
    page.getByTestId('save-changes-save').click(),
  ]);
  await expect(page.getByTestId('pwa-update')).toHaveCount(0);
  await expect
    .poll(
      async () =>
        Object.values(await savedDocuments(page)).some((bytes) =>
          zipPart(Buffer.from(bytes), 'word/document.xml')?.includes('Words with no recovery copy'),
        ),
      { timeout: 30_000 },
    )
    .toBe(true);
});

test("with recovery off, Don't Save goes through with no second warning", async ({
  page,
  request,
  baseURL,
}) => {
  test.skip(!local, 'a deployment of the local Pages emulation only');
  await page.addInitScript(() => {
    localStorage.setItem('kiln.options', JSON.stringify({ save: { keepRecoveryInfo: false } }));
  });
  await openIsolated(page);
  await page.goto('./#/write');
  await expect(page.getByTestId('document-title')).toContainText('Document', { timeout: CORE_START });
  await typeInDocument(page, 'Words to let go');
  await request.post(new URL('/__kiln/next-version', baseURL).href);
  await page.evaluate(async () => (await navigator.serviceWorker.getRegistration())?.update());
  await page.getByTestId('pwa-reload').click({ timeout: 60_000 });
  let warned = false;
  page.on('dialog', (d) => {
    warned = true;
    void d.accept();
  });
  await Promise.all([
    page.waitForEvent('load', { timeout: 60_000 }),
    page.getByTestId('save-changes-discard').click(),
  ]);
  expect(warned).toBe(false);
  await expect(page.getByTestId('pwa-update')).toHaveCount(0);
  await expect(page.locator('[data-recovery]')).toHaveCount(0);
});

test('with recovery on, leaving the page does not warn: the journal has the work', async ({ page }) => {
  await openIsolated(page);
  await page.goto('./#/write');
  await expect(page.getByTestId('document-title')).toContainText('Document', { timeout: CORE_START });
  await typeInDocument(page, 'Words with a recovery copy');
  // The journal reaches the disk.
  await page.waitForTimeout(1500);
  let asked = false;
  page.on('dialog', (d) => {
    asked = true;
    void d.accept();
  });
  await page.reload();
  await expect(page.getByTestId('document-recovery')).toBeVisible({ timeout: CORE_START });
  expect(asked).toBe(false);
});

test('a pool Worker that cannot start leaves Grid on its single module, ready to use', async ({
  browser,
}) => {
  test.skip(!local, 'blocks a file of the local Pages artefact');
  // An isolated page with no service worker (COOP and COEP from this server), where the rayon
  // Workers' script is missing, so `initThreadPool` would wait for them for ever.
  const site = `${process.env.KILN_PWA_SITE ?? fileURLToPath(new URL('../../target/pages/site', import.meta.url))}/`;
  const types: Record<string, string> = {
    '.html': 'text/html',
    '.js': 'text/javascript',
    '.css': 'text/css',
    '.wasm': 'application/wasm',
    '.json': 'application/json',
    '.svg': 'image/svg+xml',
    '.png': 'image/png',
    '.ttf': 'font/ttf',
  };
  const server = createServer((req, res) => {
    const path = new URL(req.url ?? '/', 'http://localhost').pathname.replace(/^\/kiln\//, '');
    const headers = {
      'Cross-Origin-Opener-Policy': 'same-origin',
      'Cross-Origin-Embedder-Policy': 'require-corp',
      'Cross-Origin-Resource-Policy': 'same-origin',
    };
    try {
      if (/workerHelpers/.test(path)) throw new Error('blocked');
      const file = join(site, path === 'app/' ? 'app/index.html' : path);
      const body = readFileSync(file);
      res.writeHead(200, { ...headers, 'Content-Type': types[extname(file)] ?? 'application/octet-stream' });
      res.end(body);
    } catch {
      res.writeHead(404, headers);
      res.end();
    }
  });
  await new Promise<void>((resolve) => server.listen(0, '127.0.0.1', resolve));
  const { port } = server.address() as AddressInfo;
  const context = await browser.newContext({
    serviceWorkers: 'block',
    baseURL: `http://127.0.0.1:${port}/kiln/app/`,
  });
  try {
    const page = await context.newPage();
    await prepare(page);
    await page.goto('./#/grid');
    await expect(page.getByTestId('document-title')).toContainText('Book', { timeout: CORE_START });
    expect(await page.evaluate(() => crossOriginIsolated)).toBe(true);
    expect(await coreThreads(page, 'grid')).toBe(0);
    await page.mouse.click(420, 420);
    await page.keyboard.type('=6*7\n');
    await expect(page.getByTestId('save-status').first()).toContainText(/Not saved|Unsaved/, {
      timeout: 15_000,
    });
  } finally {
    await context.close();
    server.close();
  }
});

test('a file launched into a window with unsaved work opens there, and the work stays', async ({ page }) => {
  await installLaunchQueue(page);
  await openIsolated(page);
  await page.goto('./#/write');
  await expect(page.getByTestId('document-title')).toContainText('Document', { timeout: CORE_START });
  await typeInDocument(page, 'Words a launch must not drop');
  await expect(page.getByText('6 words')).toBeVisible();
  await page.evaluate(() => {
    (window as unknown as { __sameWindow: boolean }).__sameWindow = true;
  });
  const csv = Array.from(Buffer.from('Item,Qty\nClay,12\n'));
  await page.evaluate((b) => (window as unknown as LaunchHandle).__launch('Stock.csv', b), csv);
  await expect(page).toHaveURL(/#\/grid\?doc=/, { timeout: CORE_START });
  await expect(page.getByTestId('document-title')).toContainText('Stock', { timeout: CORE_START });

  // Back closes the launched file and returns to the document, unsaved, in the same window.
  const stillThere = async () => {
    await expect(page.getByTestId('document-title')).toContainText('Document', { timeout: CORE_START });
    await expect(page.getByTestId('save-status').first()).toContainText(/Not saved|Unsaved/);
    await expect(page.getByText('6 words')).toBeVisible();
    expect(await page.evaluate(() => (window as unknown as { __sameWindow?: boolean }).__sameWindow)).toBe(
      true,
    );
  };
  await page.evaluate(() => history.back());
  await stillThere();

  // A New command of the icon menu in the running window: the route moves, nothing reloads.
  await page.evaluate(() => (window as unknown as LaunchHandle).__target('#/deck'));
  await expect(page).toHaveURL(/#\/deck\?(new|doc)=/, { timeout: CORE_START });
  await expect(page.getByTestId('document-title')).not.toContainText(/Opening|Document/, {
    timeout: CORE_START,
  });
  await page.evaluate(() => history.back());
  await stillThere();
});

test('a file shared to a window with unsaved work opens there, and the work stays', async ({ page }) => {
  await page.addInitScript(() => {
    localStorage.setItem('kiln.options', JSON.stringify({ save: { keepRecoveryInfo: false } }));
  });
  await openIsolated(page);
  await page.goto('./#/write');
  await expect(page.getByTestId('document-title')).toContainText('Document', { timeout: CORE_START });
  await typeInDocument(page, 'Words a share must not drop');
  await page.evaluate(() => {
    (window as unknown as { __sameWindow: boolean }).__sameWindow = true;
  });
  // The browser may ask before the navigation starts; the worker then ends it.
  page.on('dialog', (d) => void d.accept());
  await submitShare(page, 'Shared list.csv', 'Name,Kiln\nAda,1\n');
  // Playwright still counts the navigation the worker ended as pending, so its locators can wait
  // for ever from here: the page is read directly.
  await expect(page).toHaveURL(/#\/grid\?doc=/, { timeout: CORE_START });
  const title = () =>
    page.evaluate(() => document.querySelector('[data-testid="document-title"]')?.textContent ?? '');
  await expect.poll(title, { timeout: CORE_START }).toContain('Shared list');
  expect(await page.evaluate(() => (window as unknown as { __sameWindow?: boolean }).__sameWindow)).toBe(
    true,
  );
  await page.evaluate(() => history.back());
  await expect
    .poll(() => page.evaluate(() => document.querySelector('[data-testid="document-title"]')?.textContent), {
      timeout: 20_000,
    })
    .toContain('Document');
  await expect.poll(() => page.evaluate(() => document.body.innerText)).toContain('6 words');
});

test('files shared to the app open from the share target', async ({ page }) => {
  await openIsolated(page);
  await submitShare(page, 'Shared list.csv', 'Name,Kiln\nAda,1\n');
  await expect(page).toHaveURL(/#\/grid\?doc=/, { timeout: CORE_START });
  await expect(page.getByTestId('document-title')).toContainText('Shared list', { timeout: CORE_START });
  expect(page.url()).not.toContain('share=');
});

test('Kiln offers its own install card on Kiln Home', async ({ page }) => {
  await page.addInitScript(() => {
    localStorage.setItem('kiln.pref.ui.home.tour_seen', '1');
  });
  await page.goto('./');
  await page.waitForFunction(() => navigator.serviceWorker.controller !== null && crossOriginIsolated, null, {
    timeout: CORE_START,
  });
  // The browser's offer, as Chromium fires it for an installable page.
  await page.evaluate(() => {
    const event = new Event('beforeinstallprompt', { cancelable: true }) as Event & Record<string, unknown>;
    event.prompt = async () => undefined;
    event.userChoice = Promise.resolve({ outcome: 'dismissed' });
    window.dispatchEvent(event);
  });
  const card = page.getByTestId('pwa-install');
  await expect(card).toBeVisible({ timeout: 10_000 });
  await expect(card).toContainText('Install Kiln');
  await settled(page);
  await page.screenshot({ path: shot('install-light.png') });
  await page.emulateMedia({ colorScheme: 'dark' });
  await page.waitForTimeout(600);
  await settle(page);
  await page.screenshot({ path: shot('install-dark.png') });
  await page.setViewportSize({ width: 412, height: 915 });
  await page.waitForTimeout(300);
  await settle(page);
  await page.screenshot({ path: shot('install-dark-phone.png') });
  await page.emulateMedia({ colorScheme: 'light' });
  await card.getByRole('button', { name: 'Not now' }).first().click();
  await expect(card).toHaveCount(0);
  await page.reload();
  await page.evaluate(() => window.dispatchEvent(new Event('beforeinstallprompt', { cancelable: true })));
  await page.waitForTimeout(3500);
  await expect(card).toHaveCount(0);
});

test('the notices in Arabic: mirrored, in Arabic, on Kiln Home', async ({ page }) => {
  await page.addInitScript(() => {
    localStorage.setItem('kiln.pref.ui.home.tour_seen', '1');
    localStorage.setItem('kiln.options', JSON.stringify({ language: { ui: 'ar' } }));
  });
  await page.goto('./');
  await page.waitForFunction(() => navigator.serviceWorker.controller !== null && crossOriginIsolated, null, {
    timeout: CORE_START,
  });
  await expect(page.locator('html')).toHaveAttribute('dir', 'rtl');
  await page.evaluate(() => {
    const event = new Event('beforeinstallprompt', { cancelable: true }) as Event & Record<string, unknown>;
    event.prompt = async () => undefined;
    event.userChoice = Promise.resolve({ outcome: 'dismissed' });
    window.dispatchEvent(event);
    window.dispatchEvent(new Event('offline'));
  });
  const card = page.getByTestId('pwa-install');
  await expect(card).toContainText(AR['Install Kiln'] ?? '', { timeout: 10_000 });
  await expect(page.getByTestId('pwa-network')).toContainText(AR.Offline ?? '');
  // The cards sit at the window's end: the left of a right-to-left window.
  const box = await card.boundingBox();
  expect((box?.x ?? 999) < 100).toBe(true);
  await settle(page);
  await page.screenshot({ path: shot('install-offline-ar.png') });
});
