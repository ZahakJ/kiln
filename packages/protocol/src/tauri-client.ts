/**
 * The native client: the core runs in the Tauri process (desktop and Android) and is reached
 * with `invoke` of the `kiln_*` commands registered by `kiln-desktop`; events stream over an
 * ordered IPC `Channel`; frames arrive as raw postcard bytes.
 *
 * Loaded lazily (see `createTauriClient`) so the web bundle does not carry the Tauri API.
 */

import { Channel, invoke } from '@tauri-apps/api/core';
import { type EventListener, type KilnClient, Listeners } from './client';
import { KilnError } from './errors';
import type { A11yTree } from './generated/A11yTree';
import type { Command } from './generated/Command';
import type { CoreInfo } from './generated/CoreInfo';
import type { DocId } from './generated/DocId';
import type { DocKind } from './generated/DocKind';
import type { DocSummary } from './generated/DocSummary';
import type { Event } from './generated/Event';
import type { ExecResult } from './generated/ExecResult';
import type { HostOp } from './generated/HostOp';
import type { HostReply } from './generated/HostReply';
import type { OpenHint } from './generated/OpenHint';
import type { Query } from './generated/Query';
import type { QueryResult } from './generated/QueryResult';
import type { RequestId } from './generated/RequestId';
import type { SceneDelta } from './generated/SceneDelta';
import type { Viewport } from './generated/Viewport';
import { failed, type HostHandler, handleCommon } from './host';
import { pingInfo } from './ping';
import { decodeAs } from './postcard';

/** `invoke` with errors converted to {@link KilnError}. */
async function call<T>(
  command: string,
  args?: Record<string, unknown> | Uint8Array,
  headers?: Record<string, string>,
) {
  try {
    return await invoke<T>(command, args as Record<string, unknown>, headers ? { headers } : undefined);
  } catch (error) {
    throw KilnError.from(error);
  }
}

/**
 * The Tauri host: the core reads and writes files itself (`NativeHost`), so only dialogs,
 * preferences, clipboard, URLs and files to show reach the webview. Dialogs use
 * `@tauri-apps/plugin-dialog`, URLs and files (a PDF to print) `@tauri-apps/plugin-opener`.
 */
export class TauriHost implements HostHandler {
  async handle(op: HostOp): Promise<HostReply> {
    const common = await handleCommon(op, {
      prefs: typeof localStorage === 'undefined' ? undefined : localStorage,
      clipboard: typeof navigator === 'undefined' ? undefined : navigator.clipboard,
      openUrl: async (url) => {
        const { openUrl } = await import('@tauri-apps/plugin-opener');
        await openUrl(url);
      },
    });
    if (common !== undefined) return common;
    if (typeof op === 'object' && 'PickOpen' in op) {
      const { open } = await import('@tauri-apps/plugin-dialog');
      const path = await open({ multiple: false, directory: false, filters: op.PickOpen.filters });
      return typeof path === 'string' ? { Path: path } : 'Cancelled';
    }
    if (typeof op === 'object' && 'OpenFile' in op) {
      // Print (WP-67): the core wrote the PDF to a temporary file; the system viewer prints it.
      const { openPath } = await import('@tauri-apps/plugin-opener');
      await openPath(op.OpenFile.path);
      return 'Done';
    }
    if (typeof op === 'object' && 'PickSave' in op) {
      const { save } = await import('@tauri-apps/plugin-dialog');
      const path = await save({ defaultPath: op.PickSave.suggested_name, filters: op.PickSave.filters });
      return path ? { Path: path } : 'Cancelled';
    }
    return failed('Unsupported', 'the native core handles files itself');
  }
}

/** Talks to `kiln-desktop` over Tauri IPC. Create it with {@link TauriClient.connect}. */
export class TauriClient implements KilnClient {
  readonly bridge = 'tauri' as const;
  private readonly listeners = new Listeners();
  /** The last command sent: the next one waits for it. */
  private order: Promise<void> = Promise.resolve();
  private channel: Channel<Event> | null = null;

  private constructor(private readonly host: HostHandler) {}

  /** Connects and subscribes to the session's events. */
  static async connect(host: HostHandler = new TauriHost()): Promise<TauriClient> {
    const client = new TauriClient(host);
    const channel = new Channel<Event>();
    channel.onmessage = (event) => client.dispatch(event);
    await call<void>('kiln_subscribe', { doc: null, onEvent: channel });
    client.channel = channel;
    return client;
  }

  private dispatch(event: Event): void {
    if (typeof event === 'object' && 'HostRequest' in event) {
      const { id, op } = event.HostRequest;
      void this.host
        .handle(op)
        .catch((error: unknown) => ({ Failed: KilnError.from(error).toInfo() }) as HostReply)
        .then((reply) => this.hostReply(id, reply))
        .catch((error: unknown) => console.error('Kiln: could not answer a host request', error));
    }
    this.listeners.emit(event);
  }

  async ping(): Promise<CoreInfo> {
    return pingInfo(await this.execute(null, { Suite: 'Ping' }));
  }

  execute(doc: DocId | null, command: Command): Promise<ExecResult> {
    // Tauri may run concurrent invocations in any order: commands go one after the other, in
    // the order of the calls (the contract of `KilnClient.execute`).
    const run = this.order.then(() => call<ExecResult>('kiln_execute', { doc, cmd: command }));
    this.order = run.then(
      () => undefined,
      () => undefined,
    );
    return run;
  }

  query(doc: DocId | null, query: Query): Promise<QueryResult> {
    return call('kiln_query', { doc, query });
  }

  open(bytes: Uint8Array, hint: OpenHint): Promise<DocSummary> {
    // The bytes travel as the raw request body; the hint rides in a header (percent-encoded JSON,
    // since header values must be ASCII and file names need not be).
    return call('kiln_open', bytes, { 'kiln-hint': encodeURIComponent(JSON.stringify(hint)) });
  }

  new(kind: DocKind): Promise<DocSummary> {
    return call('kiln_new', { kind });
  }

  setViewport(doc: DocId, viewport: Viewport): Promise<void> {
    return call('kiln_viewport', { doc, vp: viewport });
  }

  async frame(doc: DocId): Promise<SceneDelta> {
    const buffer = await call<ArrayBuffer>('kiln_frame', { doc });
    return decodeAs<SceneDelta>('SceneDelta', new Uint8Array(buffer));
  }

  a11y(doc: DocId): Promise<A11yTree> {
    return call('kiln_a11y', { doc });
  }

  subscribe(listener: EventListener): () => void {
    return this.listeners.add(listener);
  }

  hostReply(id: RequestId, reply: HostReply): Promise<void> {
    return call('kiln_host_reply', { id, reply });
  }

  dispose(): void {
    if (this.channel) this.channel.onmessage = () => {};
    this.channel = null;
    this.listeners.clear();
  }
}
