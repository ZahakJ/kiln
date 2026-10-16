import type { A11yTree } from './generated/A11yTree';
import type { Command } from './generated/Command';
import type { CoreInfo } from './generated/CoreInfo';
import type { DocId } from './generated/DocId';
import type { DocKind } from './generated/DocKind';
import type { DocSummary } from './generated/DocSummary';
import type { Event } from './generated/Event';
import type { ExecResult } from './generated/ExecResult';
import type { HostReply } from './generated/HostReply';
import type { OpenHint } from './generated/OpenHint';
import type { Query } from './generated/Query';
import type { QueryResult } from './generated/QueryResult';
import type { RequestId } from './generated/RequestId';
import type { SceneDelta } from './generated/SceneDelta';
import type { Viewport } from './generated/Viewport';

/** Receives every event of the session, in order. */
export type EventListener = (event: Event) => void;

/**
 * The one interface the UI uses to talk to the core, whatever the host (PLAN.md §3.3).
 * `WasmWorkerClient` runs the core in a Web Worker; `TauriClient` talks to the native core.
 *
 * Every method rejects with a {@link KilnError}.
 */
export interface KilnClient {
  /** Which bridge this is. */
  readonly bridge: 'wasm' | 'tauri';
  /** Round trip that returns the core's version, protocol and target. */
  ping(): Promise<CoreInfo>;
  /** Runs a command on a document, or on the session with `doc: null`. */
  execute(doc: DocId | null, command: Command): Promise<ExecResult>;
  /** Asks a read-only question about a document, or the session with `doc: null`. */
  query(doc: DocId | null, query: Query): Promise<QueryResult>;
  /** Opens a file from its bytes. */
  open(bytes: Uint8Array, hint: OpenHint): Promise<DocSummary>;
  /** Creates an empty document. */
  'new'(kind: DocKind): Promise<DocSummary>;
  /** Tells the core what part of a document is visible. */
  setViewport(doc: DocId, viewport: Viewport): Promise<void>;
  /** The changes to paint since the previous frame. */
  frame(doc: DocId): Promise<SceneDelta>;
  /** The accessibility tree of the visible region. */
  a11y(doc: DocId): Promise<A11yTree>;
  /** Listens to every event; returns the unsubscribe function. */
  subscribe(listener: EventListener): () => void;
  /** Answers a host request the core sent as `Event.HostRequest` (clients do this automatically). */
  hostReply(id: RequestId, reply: HostReply): Promise<void>;
  /** Releases the bridge (terminates the Worker, closes the event channel). */
  dispose(): void;
}

/** An empty viewport, for surfaces that have not been measured yet. */
export const EMPTY_VIEWPORT: Viewport = {
  width_px: 0,
  height_px: 0,
  scroll: { x: 0, y: 0 },
  zoom: 1,
  dpr: 1,
  mode: 'PrintLayout',
};

/** Fans events out to listeners; a throwing listener does not stop the others. */
export class Listeners {
  private readonly listeners = new Set<EventListener>();

  add(listener: EventListener): () => void {
    this.listeners.add(listener);
    return () => {
      this.listeners.delete(listener);
    };
  }

  emit(event: Event): void {
    for (const listener of [...this.listeners]) {
      try {
        listener(event);
      } catch (error) {
        console.error('Kiln: an event listener failed', error);
      }
    }
  }

  clear(): void {
    this.listeners.clear();
  }
}
