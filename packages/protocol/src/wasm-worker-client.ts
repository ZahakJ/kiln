import { type EventListener, type KilnClient, Listeners } from './client';
import { KilnError } from './errors';
import type { A11yTree } from './generated/A11yTree';
import type { Command } from './generated/Command';
import type { CoreInfo } from './generated/CoreInfo';
import type { DocId } from './generated/DocId';
import type { DocKind } from './generated/DocKind';
import type { DocSummary } from './generated/DocSummary';
import type { ErrorInfo } from './generated/ErrorInfo';
import type { ExecResult } from './generated/ExecResult';
import type { HostReply } from './generated/HostReply';
import type { OpenHint } from './generated/OpenHint';
import type { Query } from './generated/Query';
import type { QueryResult } from './generated/QueryResult';
import type { RequestId } from './generated/RequestId';
import type { SceneDelta } from './generated/SceneDelta';
import type { Viewport } from './generated/Viewport';
import type { HostHandler } from './host';
import { pingInfo } from './ping';
import { decodeAs } from './postcard';
import type { WorkerMessage, WorkerMethods, WorkerRequest } from './worker-protocol';

/** The part of `Worker` the client uses (a fake in tests). */
export interface WorkerLike {
  postMessage(message: WorkerRequest, transfer: Transferable[]): void;
  addEventListener(type: 'message', listener: (event: MessageEvent<WorkerMessage>) => void): void;
  addEventListener(type: 'error', listener: (event: ErrorEvent) => void): void;
  terminate(): void;
}

interface Pending {
  resolve: (value: string | Uint8Array | null) => void;
  reject: (error: KilnError) => void;
}

/**
 * The web client: the core runs in a dedicated Worker (so parsing a large file never blocks the
 * chrome) and is reached with `postMessage`; frames come back as transferred buffers and are
 * decoded here with the postcard decoder. Host requests are answered on this (main) thread by the
 * given {@link HostHandler}.
 */
export class WasmWorkerClient implements KilnClient {
  readonly bridge = 'wasm' as const;
  private readonly listeners = new Listeners();
  private readonly pending = new Map<number, Pending>();
  private nextId = 1;
  private fatal: ErrorInfo | null = null;
  /** Resolves with the core version once the Worker has loaded the wasm module. */
  readonly ready: Promise<string>;

  constructor(
    private readonly worker: WorkerLike,
    private readonly host: HostHandler,
  ) {
    let resolveReady: (version: string) => void = () => {};
    let rejectReady: (error: KilnError) => void = () => {};
    this.ready = new Promise((resolve, reject) => {
      resolveReady = resolve;
      rejectReady = reject;
    });
    // Callers that never await `ready` must not see an unhandled rejection.
    this.ready.catch(() => {});
    worker.addEventListener('message', (event) => {
      const msg = event.data;
      switch (msg.type) {
        case 'ready':
          resolveReady(msg.version);
          break;
        case 'fatal':
          this.fail(msg.error);
          rejectReady(new KilnError(msg.error));
          break;
        case 'reply': {
          const p = this.pending.get(msg.id);
          if (!p) break;
          this.pending.delete(msg.id);
          if (msg.ok) p.resolve(msg.value);
          else p.reject(new KilnError(msg.error));
          break;
        }
        case 'events':
          for (const e of msg.events) this.dispatch(e);
          break;
      }
    });
    worker.addEventListener('error', (event) => {
      const info: ErrorInfo = { kind: 'Internal', message: `the core Worker failed: ${event.message}` };
      this.fail(info);
      rejectReady(new KilnError(info));
    });
  }

  private fail(info: ErrorInfo): void {
    this.fatal = info;
    for (const p of this.pending.values()) p.reject(new KilnError(info));
    this.pending.clear();
  }

  private dispatch(event: Parameters<EventListener>[0]): void {
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

  private call<M extends keyof WorkerMethods>(
    method: M,
    args: WorkerMethods[M],
    transfer: Transferable[] = [],
  ) {
    if (this.fatal) return Promise.reject(new KilnError(this.fatal));
    const id = this.nextId++;
    return new Promise<string | Uint8Array | null>((resolve, reject) => {
      this.pending.set(id, { resolve, reject });
      this.worker.postMessage({ id, method, args } as WorkerRequest, transfer);
    });
  }

  private async json<T>(method: keyof WorkerMethods, args: WorkerMethods[keyof WorkerMethods]): Promise<T> {
    const value = await this.call(method, args as never);
    if (typeof value !== 'string')
      throw new KilnError({ kind: 'Internal', message: `${method}: expected JSON` });
    return JSON.parse(value) as T;
  }

  async ping(): Promise<CoreInfo> {
    return pingInfo(await this.execute(null, { Suite: 'Ping' }));
  }

  execute(doc: DocId | null, command: Command): Promise<ExecResult> {
    return this.json('execute', [doc, JSON.stringify(command)]);
  }

  query(doc: DocId | null, query: Query): Promise<QueryResult> {
    return this.json('query', [doc, JSON.stringify(query)]);
  }

  async open(bytes: Uint8Array, hint: OpenHint): Promise<DocSummary> {
    // Copy so the caller keeps its buffer; the copy is transferred, not cloned.
    const copy = bytes.slice();
    const value = await this.call('open', [copy, JSON.stringify(hint)], [copy.buffer]);
    return JSON.parse(value as string) as DocSummary;
  }

  new(kind: DocKind): Promise<DocSummary> {
    return this.json('newDocument', [JSON.stringify(kind)]);
  }

  async setViewport(doc: DocId, viewport: Viewport): Promise<void> {
    await this.call('setViewport', [doc, JSON.stringify(viewport)]);
  }

  async frame(doc: DocId): Promise<SceneDelta> {
    const bytes = await this.call('frame', [doc]);
    if (!(bytes instanceof Uint8Array))
      throw new KilnError({ kind: 'Internal', message: 'frame: expected bytes' });
    return decodeAs<SceneDelta>('SceneDelta', bytes);
  }

  a11y(doc: DocId): Promise<A11yTree> {
    return this.json('a11y', [doc]);
  }

  subscribe(listener: EventListener): () => void {
    return this.listeners.add(listener);
  }

  async hostReply(id: RequestId, reply: HostReply): Promise<void> {
    await this.call('hostReply', [id, JSON.stringify(reply)]);
  }

  dispose(): void {
    this.fail({ kind: 'Closed', message: 'the client was disposed' });
    this.listeners.clear();
    this.worker.terminate();
  }
}
