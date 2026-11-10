/**
 * Registers Kiln's service worker (`src/sw.ts`) in the GitHub Pages build and follows it:
 *
 * - **Cross-origin isolation.** The worker adds COOP and COEP to what it serves. A page loaded
 *   before the worker controlled it is not isolated; on the first visit the page reloads once
 *   when the worker takes over, while Kiln Home shows and no document is open, so the threads core
 *   can run from then on. With a document open it waits for the next visit.
 * - **Updates.** A new version installs in the background, with every core and font when this
 *   device had them, and waits. The page shows "Kiln has been updated" with Reload; Reload lets
 *   the new worker take over, then reloads. Documents with unsaved changes keep their autosave
 *   journals in the Origin Private File System, so Document Recovery offers them after the reload
 *   and the notice says so. With "Keep recovery info" off there is no journal, so the notice says
 *   Kiln will ask, and Reload asks the save-changes question for each document first.
 * - **Offline files.** Once the page has been quiet for a while after it loaded (or at once in the
 *   installed app), it asks the worker to cache every core and font, so every app opens offline;
 *   `data-kiln-offline="ready"` on the root element says when that is done, and the first time it
 *   is, the page says so. A browser asking to save data is left alone.
 * - **Install.** The browser's install offer is kept and shown as Kiln's own card (`PwaLayer`).
 */
import { journalsWritten } from '../client/index.ts';
import { allowLeaving, keepsRecovery, unsavedDocuments } from '../client/unsaved.ts';
import { type InstallPrompt, remember, remembered, setPwaState } from './state.ts';

/** Where the service worker is, next to the page (`/kiln/app/sw.js` on Pages). */
const SW_URL = `${import.meta.env.BASE_URL}sw.js`;
/** Set for the one reload that turns cross-origin isolation on, so it never loops. */
const ISOLATE_RELOAD = 'kiln.pwa.isolate-reload';

let registration: ServiceWorkerRegistration | null = null;

function onHome(): boolean {
  const name = window.location.hash.replace(/^#\/?/, '').split('?')[0] ?? '';
  return name === '' || name === 'home';
}

/** A version waits: tell the page, unless it is the first install (nothing to update). */
function watchWaiting(reg: ServiceWorkerRegistration): void {
  const waiting = () => {
    if (reg.waiting && navigator.serviceWorker.controller) setPwaState({ update: true });
  };
  waiting();
  reg.addEventListener('updatefound', () => {
    const next = reg.installing;
    next?.addEventListener('statechange', () => {
      if (next.state === 'installed') waiting();
    });
  });
}

/** How long after the load the page waits before caching the rest (the first screen comes first). */
const WARM_AFTER_MS = 10_000;

/** Asks the worker to cache every core and font (every app then opens offline). */
export function warmOfflineFiles(): void {
  navigator.serviceWorker?.controller?.postMessage({ type: 'warm' });
}

function saveData(): boolean {
  return (navigator as { connection?: { saveData?: boolean } }).connection?.saveData === true;
}

/** The worker's answer to `warm`: every on-demand file is cached (or not, when offline). */
function onWorkerMessage(event: MessageEvent): void {
  const data = event.data as { type?: string; complete?: boolean } | null;
  if (data?.type !== 'warmed' || !data.complete) return;
  document.documentElement.dataset.kilnOffline = 'ready';
  if (remembered('offline-ready') !== '1') {
    remember('offline-ready', '1');
    setPwaState({ offlineReady: true });
    setTimeout(() => setPwaState({ offlineReady: false }), 5000);
  }
}

function installedApp(): boolean {
  return globalThis.matchMedia?.('(display-mode: standalone)').matches === true;
}

/** How long a new version that is still installing may hold up Reload. */
const INSTALL_WAIT_MS = 120_000;

/** The new version: the one waiting, or the one the server has, once it has installed. */
async function newVersion(): Promise<ServiceWorker | null> {
  const reg = registration;
  if (!reg) return null;
  if (reg.waiting) return reg.waiting;
  await reg.update().catch(() => undefined);
  const next = reg.installing ?? reg.waiting;
  if (!next || next.state === 'installed') return next;
  return new Promise((resolve) => {
    const timer = setTimeout(() => resolve(null), INSTALL_WAIT_MS);
    next.addEventListener('statechange', () => {
      if (next.state !== 'installed' && next.state !== 'redundant') return;
      clearTimeout(timer);
      resolve(next.state === 'installed' ? next : null);
    });
  });
}

/** Lets the new version take over and reloads into it; the unsaved work is answered for. */
export async function applyUpdate(): Promise<void> {
  allowLeaving();
  const waiting = await newVersion();
  if (!waiting) {
    window.location.reload();
    return;
  }
  navigator.serviceWorker.addEventListener('controllerchange', () => window.location.reload(), {
    once: true,
  });
  waiting.postMessage({ type: 'skip-waiting' });
}

/**
 * Reload, from the update notice or from a window whose old files are gone: it goes once the
 * journals are written, or, with no journals to come back from, the save-changes question comes
 * first. Returns whether the page is on its way.
 */
export function reloadIntoUpdate(): boolean {
  const unsaved = unsavedDocuments();
  if (unsaved.length > 0 && !keepsRecovery()) {
    setPwaState({ asking: unsaved });
    return false;
  }
  void journalsWritten().finally(() => void applyUpdate());
  return true;
}

/** Starts the service worker and the page's side of it. Pages build only (`__KILN_PWA__`). */
export function startPwa(): void {
  const sw = navigator.serviceWorker;
  if (!sw) return;
  const firstVisit = !sw.controller;
  sw.addEventListener('message', onWorkerMessage);
  sw.addEventListener('controllerchange', () => {
    // The worker took over a page it did not serve: it is isolated after one reload.
    if (!firstVisit || globalThis.crossOriginIsolated) return;
    let reloaded = false;
    try {
      reloaded = sessionStorage.getItem(ISOLATE_RELOAD) === '1';
      sessionStorage.setItem(ISOLATE_RELOAD, '1');
    } catch {
      reloaded = true;
    }
    if (!reloaded && onHome() && unsavedDocuments().length === 0) window.location.reload();
  });

  // After the page has loaded and gone quiet, so registering costs the first screen nothing.
  const register = () =>
    sw.register(SW_URL, { scope: import.meta.env.BASE_URL }).then(
      (reg) => {
        registration = reg;
        watchWaiting(reg);
        if (!saveData())
          void sw.ready.then(() => setTimeout(warmOfflineFiles, installedApp() ? 0 : WARM_AFTER_MS));
        // A long-lived window still hears of new versions.
        setInterval(() => void reg.update().catch(() => undefined), 60 * 60 * 1000);
      },
      () => undefined,
    );
  if (document.readyState === 'complete') void register();
  else window.addEventListener('load', () => void register(), { once: true });

  window.addEventListener('online', () => {
    setPwaState({ offline: false, backOnline: true });
    setTimeout(() => setPwaState({ backOnline: false }), 4000);
  });
  window.addEventListener('offline', () => setPwaState({ offline: true, backOnline: false }));

  window.addEventListener('beforeinstallprompt', (event) => {
    // Kiln's own card offers it instead of the browser's bar.
    event.preventDefault();
    if (remembered('install-declined') !== '1') setPwaState({ install: event as InstallPrompt });
  });
  window.addEventListener('appinstalled', () => {
    setPwaState({ install: null });
    void sw.ready.then(warmOfflineFiles);
  });
}
