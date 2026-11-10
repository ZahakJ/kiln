/**
 * A Web Worker that owns one app's Kiln core: the Worker's name (`kiln-core-<app>`,
 * given by `client/index.ts`) picks the WebAssembly build of `kiln-bridge-wasm` for that app
 * (written to `./wasm/<app>/` by `cargo xtask wasm`). It creates one `KilnSession` and serves
 * `WasmWorkerClient` requests in order. Parsing and editing never block the page.
 */

import {
  CORE_STOPPED,
  type PostTarget,
  serveRequest,
  type WasmSession,
  type WorkerMessage,
  type WorkerRequest,
} from '@kiln/protocol';

/**
 * The document fonts (static assets, cached by the browser and the service worker): Kiln's
 * metric-compatible set for Calibri (Carlito), Cambria (Caladea), Arial, Times New Roman and
 * Courier New (Liberation), every weight and style, as the native core bundles them.
 */
// the brand faces too (Fraunces and Figtree), which the Kiln and Tenmoku slide themes use.
const FONT_URLS = import.meta.glob<string>(
  '../../../assets/fonts/{carlito,caladea,liberation,brand/Figtree,brand/Fraunces}/*.ttf',
  {
    query: '?url',
    import: 'default',
    eager: true,
  },
);
/**
 * Noto Naskh Arabic, Noto Sans Symbols 2 and Noto Sans Math, fetched only when a document
 * has characters that no loaded font covers (the core reports their kinds after each call); the
 * same for emoji (Noto Emoji). Chinese, Japanese and Korean come from Noto Sans CJK SC, whole,
 * in slices (`tools/fonts/cjk-slices.py`): the core reports the characters it missed, and only
 * the slices that have them are fetched.
 */
const FALLBACK_URLS = import.meta.glob<string>('../../../assets/fonts/noto/*.{ttf,otf}', {
  query: '?url',
  import: 'default',
  eager: true,
});
/** The fallback fonts for each kind the core reports (bits of `takeMissingFonts`). */
const FALLBACKS: [bit: number, files: string[]][] = [
  [1, ['NotoNaskhArabic-Regular.ttf', 'NotoNaskhArabic-Bold.ttf']],
  [2, ['NotoSansSymbols2-Regular.ttf']],
  [4, ['NotoSansMath-Regular.ttf']],
  [32, ['NotoEmoji-Regular.ttf']],
];
const CJK_SLICE_URLS = import.meta.glob<string>('../../../assets/fonts/noto/cjk/*.otf', {
  query: '?url',
  import: 'default',
  eager: true,
});
const CJK_MAP_URL = Object.values(
  import.meta.glob<string>('../../../assets/fonts/noto/cjk/slices.json', {
    query: '?url',
    import: 'default',
    eager: true,
  }),
)[0];
/** The font files registered, and those wanted and not yet fetched (by URL). */
const fetchedFonts = new Set<string>();
const wantedFonts = new Set<string>();
/** The CJK slices registered (by file name), and when. */
const fetchedSlices = new Map<string, number>();
/** A document lays its text out again with a font a moment after it is registered. */
const RELAYOUT_MS = 3_000;
/** CJK characters missed whose slice is not known yet (the map is fetched with the first). */
const wantedChars = new Set<number>();
/** CJK characters still missed with their slice registered, or in no slice (drawn as boxes). */
const uncovered = new Set<number>();
/** Diagnostics (`e2e/specs/scripts.spec.ts`): the slices registered and what still has no font. */
(globalThis as { kilnFonts?: () => unknown }).kilnFonts = () => ({
  slices: [...fetchedSlices.keys()].sort(),
  waiting: wantedFonts.size + wantedChars.size,
  uncovered: [...uncovered].map((c) => String.fromCodePoint(c)).join(''),
});
/** Fonts are being fetched; a fetch failed and waits for the network (the page says so). */
let fetching = false;
let waiting = false;

/**
 * The formula function documentation (inline docs, Insert Function, Formula Lab): static assets,
 * not compiled into the core (budget B12). The Grid core fetches it with the fonts; the core
 * works without it.
 */
const DOC_URLS = import.meta.glob<string>('../../../crates/kiln-formula/docs/*.json', {
  query: '?url',
  import: 'default',
  eager: true,
});

/** Without this one the core cannot lay out Kiln's default text. */
const REQUIRED_FONT = '../../../assets/fonts/carlito/Carlito-Regular.ttf';

/** The module wasm-bindgen generates (`--target web`). */
interface BridgeModule {
  default: () => Promise<unknown>;
  KilnSession: new () => WasmSession;
  version: () => string;
  /** Gives the core a document font (it embeds none, budget B12). */
  registerFont: (bytes: Uint8Array) => boolean;
  /** The apps this module serves (`write`, `grid` or `deck`). */
  coreApps: () => string;
  /** Gives the core its function documentation: `{ "MATH": [...], ... }` (the Grid core only). */
  loadFunctionDocs?: (json: string) => boolean;
  /** The kinds of characters laid out without a font since the last call (bits). */
  takeMissingFonts: () => number;
  /** The threads module only (wasm-bindgen-rayon): starts the rayon pool of Workers. */
  initThreadPool?: (threads: number) => Promise<unknown>;
  /** The CJK characters laid out without a font since the last call (code points). */
  takeMissingChars?: () => Uint32Array;
  /** The message of a panic in the core since the last call. */
  takePanic?: () => string | undefined;
}

/** The worker global scope, typed narrowly so this file compiles against the DOM library. */
const scope = globalThis as unknown as PostTarget & {
  addEventListener(type: 'message', listener: (event: MessageEvent<WorkerRequest>) => void): void;
};

// A glob, not a static import: the build and the type checker work before the cores are built,
// and the page then explains how to build them instead of failing to load. Each app's module is
// its own chunk (and content-hashed wasm asset), fetched only by that app's Worker.
const modules = import.meta.glob<BridgeModule>('./wasm/*/kiln_bridge_wasm.js');

/** the collaboration module (`kiln-collab-wasm`), loaded when a document is shared. */
interface CollabModule {
  default: () => Promise<unknown>;
  CollabEngines: new () => { call(request: Uint8Array): Uint8Array };
}
const collabModules = import.meta.glob<CollabModule>('./wasm/collab/kiln_collab_wasm.js');
let collabLoading: Promise<void> | null = null;

/** Loads the collaboration module once and gives the core its engines. */
function loadCollab(): Promise<void> {
  collabLoading ??= (async () => {
    const load = collabModules['./wasm/collab/kiln_collab_wasm.js'];
    if (!load) throw new Error('The collaboration module is not built. Run `cargo xtask wasm` and reload.');
    const m = await load();
    await m.default();
    const engines = new m.CollabEngines();
    session?.setCollab?.((request) => engines.call(request));
  })();
  collabLoading.catch(() => {
    collabLoading = null;
  });
  return collabLoading;
}

/** The app whose core this Worker runs. */
const app = (globalThis as { name?: string }).name?.replace(/^kiln-core-/, '') ?? '';

/**
 * the threads module of this app (`./wasm/<app>-threads/`, Grid's: recalculation on a pool
 * of Workers sharing the module's memory), used when the page is cross-origin isolated (the
 * service worker's headers on GitHub Pages, or a server that sends COOP and COEP). Anything that
 * fails on the way falls back to the single-threaded module without a word to the user.
 */
const THREADS_MODULE = `./wasm/${app}-threads/kiln_bridge_wasm.js`;
/**
 * Pool size. Doubling it from four to eight gained about 10 % on a 500,000-formula recalculation
 * (331 ms to 298 ms, docs/qa/perf.md), not worth four more Workers and their share of the memory.
 */
const POOL_MAX = 4;
/** How long the pool may take to start; a Worker that fails says so at once. */
const POOL_START_MS = 20_000;

function threadsWanted(): boolean {
  const g = globalThis as { crossOriginIsolated?: boolean };
  return (
    g.crossOriginIsolated === true && typeof SharedArrayBuffer === 'function' && THREADS_MODULE in modules
  );
}

/**
 * Starts the rayon pool. `initThreadPool` waits for each Worker's first message and has no error
 * path, so a Worker that cannot load would leave it waiting for ever: while it runs, the Workers
 * it creates report failures here, and it has a time limit. On failure they are stopped.
 */
async function startPool(bridge: BridgeModule, threads: number): Promise<void> {
  const g = globalThis as unknown as { Worker: typeof Worker };
  const Native = g.Worker;
  const started: Worker[] = [];
  let timer: ReturnType<typeof setTimeout> | undefined;
  const failed = new Promise<never>((_, reject) => {
    timer = setTimeout(() => reject(new Error('the pool did not start')), POOL_START_MS);
    g.Worker = class extends Native {
      constructor(url: string | URL, options?: WorkerOptions) {
        super(url, options);
        started.push(this);
        this.addEventListener('error', () => reject(new Error('a pool Worker failed')));
      }
    };
  });
  try {
    await Promise.race([bridge.initThreadPool?.(threads), failed]);
  } catch (error) {
    for (const worker of started) worker.terminate();
    throw error;
  } finally {
    clearTimeout(timer);
    g.Worker = Native;
  }
}

/** Loads and instantiates this app's module: the threads one when it can run, else the single one. */
async function loadBridge(): Promise<{ bridge: BridgeModule; instance: unknown; threads: number }> {
  if (threadsWanted()) {
    try {
      const bridge = await (modules[THREADS_MODULE] as () => Promise<BridgeModule>)();
      const instance = await bridge.default();
      const threads = Math.max(1, Math.min((navigator.hardwareConcurrency || 2) - 1, POOL_MAX));
      await startPool(bridge, threads);
      return { bridge, instance, threads };
    } catch (error) {
      // The single-threaded module below is a separate instance, untouched by the attempt.
      console.warn('Kiln: the threads module did not start', error);
    }
  }
  const load = modules[`./wasm/${app}/kiln_bridge_wasm.js`] as () => Promise<BridgeModule>;
  const bridge = await load();
  return { bridge, instance: await bridge.default(), threads: 0 };
}

let session: WasmSession | null = null;
let core: BridgeModule | null = null;
let failure: WorkerMessage | null = null;
const queue: WorkerRequest[] = [];

function post(message: WorkerMessage): void {
  scope.postMessage(message, []);
}

/** A call the Worker is waiting on (loading the collaboration module): later calls wait too. */
let busy = false;

function drain(): void {
  while (queue.length > 0 && !busy) {
    const request = queue.shift() as WorkerRequest;
    if (session && request.method === 'loadCollab') {
      busy = true;
      void loadCollab()
        .then(
          () => serveRequest(session as WasmSession, request, scope),
          (error: unknown) =>
            scope.postMessage(
              {
                type: 'reply',
                id: request.id,
                ok: false,
                error: {
                  kind: 'Unavailable',
                  message: error instanceof Error ? error.message : String(error),
                },
              },
              [],
            ),
        )
        .finally(() => {
          busy = false;
          drain();
        });
      return;
    }
    if (stopped !== null) {
      stoppedReply(request.id);
    } else if (session) {
      serve(session, request);
      if (stopped === null) fetchMissingFonts();
    } else if (failure?.type === 'fatal') {
      scope.postMessage({ type: 'reply', id: request.id, ok: false, error: failure.error }, []);
    }
  }
}

/**
 * A panic's message once the core has panicked: a WebAssembly instance cannot go on after one (its
 * objects stay borrowed, and every later call fails with wasm-bindgen's own words), so the Worker
 * answers each call itself from then on, and the page says what happened in written words.
 */
let stopped: string | null = null;

function stoppedReply(id: number): void {
  scope.postMessage({ type: 'reply', id, ok: false, error: { kind: 'Closed', message: CORE_STOPPED } }, []);
}

/** Serves one call; if the core panicked in it, its reply and events give way to the stop. */
function serve(live: WasmSession, request: WorkerRequest): void {
  const held: [WorkerMessage, Transferable[]][] = [];
  serveRequest(live, request, { postMessage: (m, transfer) => held.push([m, transfer]) });
  const panic = core?.takePanic?.();
  if (panic === undefined || panic === null) {
    for (const [m, transfer] of held) scope.postMessage(m, transfer);
    return;
  }
  stopped = panic;
  stoppedReply(request.id);
  post({ type: 'stopped', detail: panic });
}

/** Tests only (`e2e/specs/pwa-stopped.spec.ts`): a panic inside the session, as a core bug would. */
(globalThis as { kilnPanicForTest?: () => void }).kilnPanicForTest = () => {
  try {
    (session as unknown as { panicForTest?: () => void } | null)?.panicForTest?.();
  } catch {
    // The panic is seen on the next call, as a real one would be.
  }
};

scope.addEventListener('message', (event) => {
  queue.push(event.data);
  if (session || failure) drain();
});

/** Where each CJK character is: sorted `[first, last, file]` ranges of `slices.json`. */
let cjkMap: Promise<[number, number, string][]> | null = null;
/** The map once loaded. */
let cjkRanges: [number, number, string][] | null = null;

function loadCjkMap(): Promise<[number, number, string][]> {
  cjkMap ??= (async () => {
    if (!CJK_MAP_URL) return [];
    const response = await fetch(CJK_MAP_URL);
    if (!response.ok) throw new Error(`slices.json: HTTP ${response.status}`);
    const map = (await response.json()) as { slices: { file: string; ranges: string }[] };
    const out: [number, number, string][] = [];
    for (const slice of map.slices) {
      let end = 0;
      for (const token of slice.ranges.split(' ')) {
        const [gap, length] = token.split('+');
        const first = end + Number.parseInt(gap ?? '0', 16);
        end = first + (length ? Number.parseInt(length, 16) : 0);
        out.push([first, end, slice.file]);
      }
    }
    cjkRanges = out.sort((a, b) => a[0] - b[0]);
    return cjkRanges;
  })();
  cjkMap.catch(() => {
    cjkMap = null;
  });
  return cjkMap;
}

function sliceOf(map: [number, number, string][], c: number): string | null {
  let lo = 0;
  let hi = map.length - 1;
  while (lo <= hi) {
    const mid = (lo + hi) >> 1;
    const r = map[mid] as [number, number, string];
    if (c < r[0]) hi = mid - 1;
    else if (c > r[1]) lo = mid + 1;
    else return r[2];
  }
  return null;
}

/**
 * Notes the fonts the core lacked in the last call (kinds, and CJK characters by slice) and
 * fetches them. A fetch that fails (offline) is kept: the page shows a quiet notice, and the
 * fonts are fetched again when the network comes back, and the documents drawn again with them.
 */
function fetchMissingFonts(): void {
  if (!core) return;
  const kinds = core.takeMissingFonts();
  const chars = core.takeMissingChars?.() ?? new Uint32Array(0);
  for (const [bit, files] of FALLBACKS) {
    if (!(kinds & bit)) continue;
    for (const f of files) {
      const url = FALLBACK_URLS[`../../../assets/fonts/noto/${f}`];
      if (url && !fetchedFonts.has(url)) wantedFonts.add(url);
    }
  }
  for (const c of chars) {
    // Missed well after its slice was registered (or in no slice): it has no font.
    const file = cjkRanges ? sliceOf(cjkRanges, c) : undefined;
    const landed = file ? fetchedSlices.get(file) : undefined;
    if (file === null || (landed !== undefined && Date.now() - landed > RELAYOUT_MS)) uncovered.add(c);
    else if (landed === undefined) wantedChars.add(c);
  }
  void fetchWanted();
}

/** While fonts wait, the network is looked at now and then too (not every browser tells a Worker). */
let retry: ReturnType<typeof setInterval> | undefined;
const RETRY_MS = 5_000;

function offlineFonts(): void {
  if (!waiting) post({ type: 'fonts-pending' });
  waiting = true;
  retry ??= setInterval(() => {
    if (navigator.onLine !== false) void fetchWanted();
  }, RETRY_MS);
}

async function fetchWanted(): Promise<void> {
  if (!core || fetching || (wantedFonts.size === 0 && wantedChars.size === 0)) return;
  const bridge = core;
  fetching = true;
  let failed = false;
  if (wantedChars.size > 0) {
    try {
      const map = await loadCjkMap();
      for (const c of wantedChars) {
        const file = sliceOf(map, c);
        const url = file === null ? undefined : CJK_SLICE_URLS[`../../../assets/fonts/noto/cjk/${file}`];
        if (url === undefined) uncovered.add(c);
        else if (!fetchedFonts.has(url)) wantedFonts.add(url);
      }
      wantedChars.clear();
    } catch {
      failed = true;
    }
  }
  const urls = [...wantedFonts];
  let added = false;
  await Promise.all(
    urls.map(async (url) => {
      try {
        const response = await fetch(url);
        if (!response.ok) throw new Error(`HTTP ${response.status}`);
        const bytes = new Uint8Array(await response.arrayBuffer());
        if (bridge.registerFont(bytes)) added = true;
        wantedFonts.delete(url);
        fetchedFonts.add(url);
        const slice = /NotoSansCJKsc-\d+/.exec(url)?.[0];
        if (slice) fetchedSlices.set(`${slice}.otf`, Date.now());
      } catch {
        failed = true;
      }
    }),
  );
  fetching = false;
  if (failed) offlineFonts();
  if (added || (waiting && !failed)) {
    waiting = waiting && failed;
    post({ type: 'fonts' });
  }
  if (!waiting && retry !== undefined) {
    clearInterval(retry);
    retry = undefined;
  }
  if (!failed && (wantedFonts.size > 0 || wantedChars.size > 0)) void fetchWanted();
}

// Back online: what could not be fetched is fetched now.
(scope as unknown as EventTarget).addEventListener('online', () => {
  void fetchWanted();
});

async function boot(): Promise<void> {
  const load = modules[`./wasm/${app}/kiln_bridge_wasm.js`];
  if (!load) {
    failure = {
      type: 'fatal',
      error: {
        kind: 'Unsupported',
        message: `The WebAssembly core (${app || 'no app'}) is not built. Run \`cargo xtask wasm\` and reload.`,
      },
    };
    post(failure);
    drain();
    return;
  }
  // The document fonts are fetched while the core loads; a missing optional one only means
  // that typeface falls back (as it would natively without it).
  const fonts = Object.entries(FONT_URLS).map(([path, url]) =>
    fetch(url)
      .then((r) => (r.ok ? r.arrayBuffer() : Promise.reject(new Error(`${path}: HTTP ${r.status}`))))
      .then(
        (bytes) => ({ path, bytes }),
        (error: unknown) => {
          if (path === REQUIRED_FONT) throw new Error(`the document font: ${String(error)}`);
          return null;
        },
      ),
  );
  const docs = Promise.all(
    Object.entries(app === 'grid' ? DOC_URLS : {}).map(([path, url]) =>
      fetch(url)
        .then((r) => (r.ok ? r.text() : null))
        .then(
          (text) => (text === null ? null : `${JSON.stringify(path.replace(/^.*\/|\.json$/g, ''))}:${text}`),
          () => null,
        ),
    ),
  );
  try {
    const loaded = await loadBridge();
    const bridge = loaded.bridge;
    const instance = loaded.instance as { memory?: WebAssembly.Memory } | undefined;
    if (bridge.coreApps() !== app) throw new Error(`expected the ${app} core, loaded ${bridge.coreApps()}`);
    // Diagnostics: the size of the rayon pool (0: the single-threaded module).
    (globalThis as { kilnCoreThreads?: number }).kilnCoreThreads = loaded.threads;
    // Diagnostics (the B4 memory probe): the bytes of the core's WebAssembly memory.
    (globalThis as { kilnCoreBytes?: () => number }).kilnCoreBytes = () =>
      instance?.memory?.buffer.byteLength ?? 0;
    const faces = (await Promise.all(fonts)).filter((f) => f !== null);
    if (!faces.some((f) => f.path === REQUIRED_FONT)) throw new Error('the document font is missing');
    for (const f of faces) bridge.registerFont(new Uint8Array(f.bytes));
    const families = (await docs).filter((d) => d !== null);
    if (families.length > 0) bridge.loadFunctionDocs?.(`{${families.join(',')}}`);
    session = new bridge.KilnSession();
    // the page keeps the app storage (autosave journals, versions, recent files).
    session.enableStorage?.();
    core = bridge;
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
