/**
 * Chooses and boots the bridge to the core once per page: the native core when the page runs in
 * the Tauri shell, otherwise the WebAssembly cores, one per app, each in its own Web Worker and
 * started when its app first needs it ({@link CoreRouter}).
 */

import { t } from '@kiln/i18n';
import {
  createTauriClient,
  type FileHandleLike,
  isTauri,
  type KilnClient,
  KilnError,
  WasmWorkerClient,
  type WebHost,
} from '@kiln/protocol';
import { APP_NAMES } from '@kiln/ui-kit';
import { AppHost } from '../host/index.ts';
import { type CoreApp, CoreRouter } from './cores';
import { guardLeaving, watchDocuments } from './unsaved.ts';

export { CORE_APPS, type CoreApp, CoreRouter } from './cores';

let booting: Promise<KilnClient> | null = null;
let router: CoreRouter | null = null;
/** The web cores' host (the browser's files and the app storage); none in the Tauri shell. */
let webHost: AppHost | null = null;

/** Starts one app's core in a Worker; the Worker loads `./wasm/<app>/` (see `worker.ts`). */
async function startCore(app: CoreApp, host: WebHost): Promise<KilnClient> {
  const worker = new Worker(new URL('../worker.ts', import.meta.url), {
    type: 'module',
    name: `kiln-core-${app}`,
  });
  const client = new WasmWorkerClient(worker, host, () =>
    t('{app} stopped working. Reload Kiln to go on.', { app: APP_NAMES[app] }),
  );
  try {
    await client.ready;
  } catch (error) {
    client.dispose();
    // The reason is for the console (and the dev server's page, which says how to build the
    // cores); people get a sentence they can act on.
    console.error(`Kiln: the ${app} core did not start`, error);
    if (import.meta.env.DEV) throw error;
    throw new KilnError({ kind: 'Unavailable', message: t('Reload Kiln and try again.') });
  }
  return client;
}

async function boot(): Promise<KilnClient> {
  if (isTauri()) return createTauriClient();
  // One host for every core: a file picked in one is readable by the core that opens it, and
  // every core keeps its journals, versions and recent files in the same app storage.
  const host = new AppHost();
  webHost = host;
  router = new CoreRouter((app) => startCore(app, host), host);
  watchDocuments(router);
  guardLeaving(() => host.unprotectedWrites() > 0);
  return router;
}

/**
 * The page's client, booted on first use. With `app`, that app's web core is started (and
 * answers the session calls that name no document) before the client resolves, so a core that
 * cannot start shows as a failed boot. A failed boot is retried on the next call.
 */
export async function getClient(app?: CoreApp): Promise<KilnClient> {
  if (!booting) {
    booting = boot();
    booting.catch(() => {
      booting = null;
    });
  }
  const client = await booting;
  if (app && router && client === router) {
    router.prefer(app);
    await router.core(app);
  }
  return client;
}

/**
 * Resolves once every edit made so far is on the disk: the cores have handled what they were sent,
 * and the journal writes that came of it have finished.
 */
export async function journalsWritten(): Promise<void> {
  await router?.caughtUp();
  await webHost?.written();
}

/**
 * The bytes of a file the web cores saved (a picked file, or one in the app storage), as the host
 * reads it for a core; `null` in the Tauri shell or when the browser cannot read it now.
 */
export async function readSavedFile(path: string): Promise<Uint8Array | null> {
  if (!webHost) return null;
  const reply = await webHost.handle({ ReadFile: { path } }).catch(() => null);
  return reply && typeof reply === 'object' && 'Bytes' in reply ? Uint8Array.from(reply.Bytes) : null;
}

/**
 * a file the browser gave the page (a file handler launch, a share target), kept like a
 * picked file; returns the path a core opens it by, or `null` in the Tauri shell.
 */
export async function adoptFile(item: FileHandleLike | File): Promise<string | null> {
  await getClient();
  return webHost ? webHost.adoptLaunched(item) : null;
}
