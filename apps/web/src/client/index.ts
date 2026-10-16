/**
 * Chooses and boots the bridge to the core once per page: the native core when the page runs in
 * the Tauri shell, otherwise the WebAssembly core in a Web Worker.
 */

import { createTauriClient, isTauri, type KilnClient, WasmWorkerClient, WebHost } from '@kiln/protocol';

let booting: Promise<KilnClient> | null = null;

async function boot(): Promise<KilnClient> {
  if (isTauri()) return createTauriClient();
  const worker = new Worker(new URL('../worker.ts', import.meta.url), { type: 'module', name: 'kiln-core' });
  const client = new WasmWorkerClient(worker, new WebHost());
  await client.ready;
  return client;
}

/** The page's client, booted on first use. A failed boot is retried on the next call. */
export function getClient(): Promise<KilnClient> {
  if (!booting) {
    booting = boot();
    booting.catch(() => {
      booting = null;
    });
  }
  return booting;
}
