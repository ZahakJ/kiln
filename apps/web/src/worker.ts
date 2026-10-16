/**
 * The Web Worker that owns the Kiln core (PLAN.md §3.8): it loads the WebAssembly build of
 * `kiln-bridge-wasm` (written to `./wasm/` by `cargo xtask wasm`), creates one `KilnSession` and
 * serves `WasmWorkerClient` requests in order. Parsing and editing never block the page.
 */

import {
  type PostTarget,
  serveRequest,
  type WasmSession,
  type WorkerMessage,
  type WorkerRequest,
} from '@kiln/protocol';

/** The module wasm-bindgen generates (`--target web`). */
interface BridgeModule {
  default: () => Promise<unknown>;
  KilnSession: new () => WasmSession;
  version: () => string;
}

/** The worker global scope, typed narrowly so this file compiles against the DOM library. */
const scope = globalThis as unknown as PostTarget & {
  addEventListener(type: 'message', listener: (event: MessageEvent<WorkerRequest>) => void): void;
};

// A glob, not a static import: the build and the type checker work before the core is built,
// and the page then explains how to build it instead of failing to load.
const modules = import.meta.glob<BridgeModule>('./wasm/kiln_bridge_wasm.js');

let session: WasmSession | null = null;
let failure: WorkerMessage | null = null;
const queue: WorkerRequest[] = [];

function post(message: WorkerMessage): void {
  scope.postMessage(message, []);
}

function drain(): void {
  while (queue.length > 0) {
    const request = queue.shift() as WorkerRequest;
    if (session) {
      serveRequest(session, request, scope);
    } else if (failure?.type === 'fatal') {
      scope.postMessage({ type: 'reply', id: request.id, ok: false, error: failure.error }, []);
    }
  }
}

scope.addEventListener('message', (event) => {
  queue.push(event.data);
  if (session || failure) drain();
});

async function boot(): Promise<void> {
  const load = modules['./wasm/kiln_bridge_wasm.js'];
  if (!load) {
    failure = {
      type: 'fatal',
      error: {
        kind: 'Unsupported',
        message: 'The WebAssembly core is not built. Run `cargo xtask wasm` and reload.',
      },
    };
    post(failure);
    drain();
    return;
  }
  try {
    const bridge = await load();
    await bridge.default();
    session = new bridge.KilnSession();
    post({ type: 'ready', version: bridge.version() });
  } catch (error) {
    failure = {
      type: 'fatal',
      error: { kind: 'Internal', message: `could not start the core: ${String(error)}` },
    };
    post(failure);
  }
  drain();
}

void boot();
