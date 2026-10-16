/**
 * The Worker side of the WebAssembly bridge: dispatches {@link WorkerRequest}s to the core and
 * posts replies and events back. `apps/web/src/worker.ts` loads the wasm module and calls this.
 */

import { KilnError } from './errors';
import type { Event } from './generated/Event';
import type { PostTarget, WasmSession, WorkerRequest } from './worker-protocol';

function call(session: WasmSession, request: WorkerRequest): string | Uint8Array | null {
  switch (request.method) {
    case 'execute':
      return session.execute(request.args[0], request.args[1]);
    case 'query':
      return session.query(request.args[0], request.args[1]);
    case 'open':
      return session.open(request.args[0], request.args[1]);
    case 'newDocument':
      return session.newDocument(request.args[0]);
    case 'setViewport':
      session.setViewport(request.args[0], request.args[1]);
      return null;
    case 'frame':
      return session.frame(request.args[0]);
    case 'a11y':
      return session.a11y(request.args[0]);
    case 'hostReply':
      session.hostReply(request.args[0], request.args[1]);
      return null;
  }
}

/**
 * Serves one request: runs it, posts the reply (frames as transferred buffers), then posts the
 * events the call produced.
 */
export function serveRequest(session: WasmSession, request: WorkerRequest, target: PostTarget): void {
  try {
    const value = call(session, request);
    const transfer = value instanceof Uint8Array ? [value.buffer] : [];
    target.postMessage({ type: 'reply', id: request.id, ok: true, value }, transfer);
  } catch (error) {
    target.postMessage(
      { type: 'reply', id: request.id, ok: false, error: KilnError.from(error).toInfo() },
      [],
    );
  }
  try {
    const events = JSON.parse(session.poll()) as Event[];
    if (events.length > 0) target.postMessage({ type: 'events', events }, []);
  } catch (error) {
    target.postMessage(
      {
        type: 'events',
        events: [{ Error: { doc: null, error: KilnError.from(error).toInfo() } }],
      },
      [],
    );
  }
}
