/**
 * Messages between `WasmWorkerClient` (main thread) and the Worker that owns the WebAssembly core
 * (`apps/web/src/worker.ts`, which delegates to {@link serveSession}).
 */

import type { ErrorInfo } from './generated/ErrorInfo';
import type { Event } from './generated/Event';

/** The methods the Worker serves, with their argument tuples. */
export interface WorkerMethods {
  execute: [doc: number | null, command: string];
  query: [doc: number | null, query: string];
  open: [bytes: Uint8Array, hint: string];
  newDocument: [kind: string];
  setViewport: [doc: number, viewport: string];
  frame: [doc: number];
  a11y: [doc: number];
  hostReply: [id: number, reply: string];
}

/** A call from the main thread. */
export type WorkerRequest = {
  [M in keyof WorkerMethods]: { id: number; method: M; args: WorkerMethods[M] };
}[keyof WorkerMethods];

/** A message from the Worker. */
export type WorkerMessage =
  /** The core is loaded. */
  | { type: 'ready'; version: string }
  /** The core could not be loaded; every call fails. */
  | { type: 'fatal'; error: ErrorInfo }
  /** The result of call `id`: a JSON string, bytes (frames) or nothing. */
  | { type: 'reply'; id: number; ok: true; value: string | Uint8Array | null }
  | { type: 'reply'; id: number; ok: false; error: ErrorInfo }
  /** Events emitted while serving a call, in order. */
  | { type: 'events'; events: Event[] };

/** The `KilnSession` class exported by `kiln-bridge-wasm` (JSON in, JSON or bytes out). */
export interface WasmSession {
  execute(doc: number | null | undefined, cmd: string): string;
  query(doc: number | null | undefined, query: string): string;
  open(bytes: Uint8Array, hint: string): string;
  newDocument(kind: string): string;
  setViewport(doc: number, viewport: string): void;
  frame(doc: number): Uint8Array;
  a11y(doc: number): string;
  poll(): string;
  hostReply(id: number, reply: string): void;
}

/** Where the Worker posts (the Worker's global scope). */
export interface PostTarget {
  postMessage(message: WorkerMessage, transfer: Transferable[]): void;
}
