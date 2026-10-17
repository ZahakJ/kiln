/**
 * @kiln/protocol — the typed bridge between the presentation layer and the Rust core.
 *
 * - `generated/*`: every protocol type, generated from `kiln-protocol` by `cargo xtask protocol`
 *   (never edit by hand); `catalog.json` lists every command's metadata.
 * - {@link KilnClient}: the one interface the UI talks to; {@link WasmWorkerClient} (web) and
 *   `TauriClient` (desktop/Android, loaded lazily with {@link createTauriClient}).
 * - {@link WebHost}: answers the core's host requests in browsers.
 * - {@link decode}: the schema-driven postcard decoder for frames.
 */

export { EMPTY_VIEWPORT, type EventListener, type KilnClient, Listeners } from './client';
export { isErrorInfo, KilnError } from './errors';
export * from './generated/index';
export {
  failed,
  type HostHandler,
  handleCommon,
  isExternalUrl,
  OPFS_PREFIX,
  type PrefStore,
  WebHost,
} from './host';
export { pingInfo } from './ping';
export {
  type ContainerFormat,
  type DecodeOptions,
  decode,
  decodeAs,
  type Format,
  PostcardError,
  type Registry,
  SCHEMA,
  shortestF32,
} from './postcard';
export { printPdfInFrame } from './print-frame';
export { WasmWorkerClient, type WorkerLike } from './wasm-worker-client';
export { serveRequest } from './worker-host';
export type { PostTarget, WasmSession, WorkerMessage, WorkerMethods, WorkerRequest } from './worker-protocol';

/** Whether the page runs inside the Tauri shell. */
export function isTauri(
  win: object | undefined = typeof window === 'undefined' ? undefined : window,
): boolean {
  return win !== undefined && ('__TAURI_INTERNALS__' in win || '__TAURI__' in win);
}

/** Connects to the native core (desktop/Android); the Tauri API is loaded on demand. */
export async function createTauriClient(): Promise<import('./client').KilnClient> {
  const { TauriClient } = await import('./tauri-client');
  return TauriClient.connect();
}
