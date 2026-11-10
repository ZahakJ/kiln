/** What the specs of the installable web app share (`pwa.spec.ts`, `pwa-builds.spec.ts`). */

import { type BrowserContext, expect, type Page } from '@playwright/test';

export const CORE_START = 60_000;
/** Caching every core and font after the first screen (`warm`), on a cold cache. */
export const WARM = 180_000;

/** A fresh visit: the welcome tour seen, the File System Access pickers gone (saves go to OPFS). */
export async function prepare(page: Page): Promise<void> {
  await page.addInitScript(() => {
    try {
      localStorage.setItem('kiln.pref.ui.home.tour_seen', '1');
      // The install card is tested on its own.
      localStorage.setItem('kiln.pwa.install-declined', '1');
    } catch {
      // A page that cannot keep it shows the tour; the tests dismiss nothing then.
    }
    // biome-ignore lint/suspicious/noExplicitAny: the API is not in the DOM typings
    const w = window as any;
    delete w.showOpenFilePicker;
    delete w.showSaveFilePicker;
  });
}

/** Opens the app and waits until the service worker controls it and the page is isolated. */
export async function openIsolated(page: Page): Promise<void> {
  await prepare(page);
  await page.goto('./');
  await expect(page.getByRole('heading', { level: 1 })).toBeVisible({ timeout: CORE_START });
  // The first visit reloads once when the worker takes over (cross-origin isolation).
  await page.waitForFunction(() => navigator.serviceWorker.controller !== null && crossOriginIsolated, null, {
    timeout: CORE_START,
  });
  await expect(page.getByRole('heading', { level: 1 })).toBeVisible({ timeout: CORE_START });
}

/** The size of the rayon pool of an app's core Worker (0: the single-threaded module). */
export async function coreThreads(page: Page, app: string): Promise<number> {
  await expect
    .poll(() => page.workers().some((w) => w.url().includes('/assets/worker-')), { timeout: CORE_START })
    .toBe(true);
  for (const worker of page.workers()) {
    if (!worker.url().includes('/assets/worker-')) continue;
    const [name, threads] = await worker.evaluate(() => [
      (self as unknown as { name: string }).name,
      (self as unknown as { kilnCoreThreads?: number }).kilnCoreThreads ?? -1,
    ]);
    if (name === `kiln-core-${app}`) return threads as number;
  }
  return -1;
}

export async function typeInDocument(page: Page, text: string): Promise<void> {
  const proxy = page.getByTestId('kiln-input-proxy');
  await expect(proxy).toBeAttached({ timeout: CORE_START });
  await proxy.focus();
  await page.keyboard.type(text);
  await expect(page.getByTestId('save-status').first()).toContainText(/Not saved|Unsaved|AutoSaved|Saving/, {
    timeout: 15_000,
  });
}

export async function offline(context: BrowserContext, page: Page, on: boolean): Promise<void> {
  await context.setOffline(on);
  await page.evaluate((o) => window.dispatchEvent(new Event(o ? 'offline' : 'online')), on);
}

/** The launches `installLaunchQueue` adds to the page. */
export interface LaunchHandle {
  /** A file opened with Kiln. */
  __launch(name: string, bytes: number[]): Promise<void>;
  /** A launch with no file, such as a shortcut of the icon menu: a hash under the page's URL. */
  __target(hash: string): void;
}

/**
 * The browser hands an installed app's launches to `launchQueue`; here a real file handle of the
 * Origin Private File System is handed over the same way. The first call a consumer gets is the
 * launch that loaded the page, as in an installed app.
 */
export async function installLaunchQueue(page: Page): Promise<void> {
  await page.addInitScript(() => {
    type Params = { files: FileSystemFileHandle[]; targetURL?: string };
    let consumer: ((params: Params) => void) | null = null;
    const w = window as unknown as Record<string, unknown>;
    // Chromium's own `launchQueue` is a read-only property of the window: shadow it.
    Object.defineProperty(window, 'launchQueue', {
      configurable: true,
      value: {
        setConsumer: (c: (params: Params) => void) => {
          consumer = c;
          c({ files: [], targetURL: location.href });
        },
      },
    });
    w.__launch = async (name: string, bytes: number[]) => {
      const root = await navigator.storage.getDirectory();
      const dir = await root.getDirectoryHandle('launched', { create: true });
      const file = await dir.getFileHandle(name, { create: true });
      const stream = await file.createWritable();
      await stream.write(new Uint8Array(bytes));
      await stream.close();
      consumer?.({ files: [file] });
    };
    w.__target = (hash: string) =>
      consumer?.({ files: [], targetURL: `${location.origin}${location.pathname}${hash}` });
  });
}

/**
 * Runs `fn` in the app's core Worker: the one that has `name` on its global (the rayon pool's
 * Workers are busy in WebAssembly and never answer, so each is given a moment only).
 */
export async function inCoreWorker<T>(page: Page, name: string, fn: () => T): Promise<T | undefined> {
  for (const w of page.workers()) {
    const has = await Promise.race([
      w
        .evaluate((n) => typeof (globalThis as Record<string, unknown>)[n] === 'function', name)
        .catch(() => false),
      new Promise<boolean>((resolve) => setTimeout(() => resolve(false), 2000)),
    ]);
    if (has) return w.evaluate(fn) as Promise<T>;
  }
  return undefined;
}
