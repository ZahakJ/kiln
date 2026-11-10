/**
 * Kiln's service worker, built only for the GitHub Pages build (`vite build --mode
 * pages`, which injects the precache manifest through vite-plugin-pwa).
 *
 * - **One version, one app.** The version is a hash of the build's file list, and every file the
 *   build makes is content-hashed by Vite, so a page never mixes the chrome of one build with the
 *   WebAssembly cores of another. The page shell (HTML, scripts, styles, icons) is cached when the
 *   worker installs, in a cache of its own version. A new build's worker waits until the page
 *   asks it to take over (the "Kiln has been updated" notice), so a running page keeps its files.
 * - **Cores and fonts after the first screen.** The per-app cores, the threads core, the
 *   collaboration module, the document fonts, dictionaries and formula help are not part of the
 *   install: a page fetches only its app's core first (the split of the cores, budget B12), and
 *   the worker caches what a page fetches. Once the page has been quiet for a while it asks for
 *   the rest (`warm`, answered with `warmed`), so every app then opens offline.
 * - **An update stays offline-ready.** When a version is already active, installing the next one
 *   also fetches its cores and fonts, because a waiting worker takes over by itself when the last
 *   window closes, and nothing may then be missing. One failed fetch fails the install: the old
 *   version keeps working, and the next check tries again and resumes (what was fetched stays).
 *   A browser asking to save data gets this only if it held all of the old version's files;
 *   otherwise the cores come on use, as on a first visit.
 * - **Cross-origin isolation.** Every response it serves carries COOP and COEP, as a server would
 *   send them: GitHub Pages cannot set headers, and with them the page is `crossOriginIsolated`,
 *   so the Grid core can run its recalculation on a pool of Workers (the threads core).
 * - **Share target.** Files shared to the installed app arrive as a POST to `share-target`; they
 *   are kept in a cache and the page opens them (`src/pwa/launch.ts`).
 */

declare const self: ServiceWorkerGlobalScope & {
  __WB_MANIFEST: Array<{ url: string; revision: string | null }>;
};

/** The build's files, relative to the scope (`/kiln/app/` on Pages). */
const MANIFEST = self.__WB_MANIFEST;

/** FNV-1a, 32 bits, as hex: the version of a list of files. */
function hash(text: string): string {
  let h = 0x811c9dc5;
  for (let i = 0; i < text.length; i++) {
    h ^= text.charCodeAt(i);
    h = Math.imul(h, 0x01000193);
  }
  return (h >>> 0).toString(16).padStart(8, '0');
}

const VERSION = hash(MANIFEST.map((e) => `${e.url} ${e.revision ?? ''}`).join('\n'));
const SHELL = `kiln-shell-${VERSION}`;
/** Content-hashed files cached on use (shared by the versions that list them). */
const ASSETS = 'kiln-assets';
/** Files shared to the app, until the page opens them (`src/pwa/launch.ts` reads it too). */
const SHARE = 'kiln-share';
/** What the worker remembers between versions: the version before this one and its files. */
const META = 'kiln-meta';
const SHELL_PREFIX = 'kiln-shell-';

const scope = new URL(self.registration.scope);
const absolute = (url: string) => new URL(url, scope).href;

/** Large binaries fetched only when a document needs them: not part of the installed shell. */
const ON_DEMAND = /\.(wasm|ttf|otf|aff|dic|json|png|jpe?g|webp)$/i;
const SHELL_ALWAYS = /(^|\/)(icons\/[^/]+\.png|favicon\.[a-z]+)$/;
const isOnDemand = (url: string) => ON_DEMAND.test(url) && !SHELL_ALWAYS.test(url);

/**
 * The CJK font's slices and Noto Emoji: fetched by a page when a document's text needs them, and
 * cached then, but neither part of an install nor of the quiet-time cache (about 16 MB that most
 * visitors never use). A page that needs one offline says so and fetches it when it is back online.
 */
const ON_USE = /(^|\/)(NotoSansCJKsc-\d+|NotoEmoji-Regular)[-.][^/]*$/;

const SHELL_URLS = MANIFEST.filter((e) => !isOnDemand(e.url)).map((e) => absolute(e.url));
/** Every on-demand file a version lists (kept across an update when unchanged). */
const LISTED_URLS = MANIFEST.filter((e) => isOnDemand(e.url)).map((e) => absolute(e.url));
/** The on-demand files an install and the quiet-time cache fetch. */
const ON_DEMAND_URLS = LISTED_URLS.filter((url) => !ON_USE.test(url));
const KNOWN = new Set([...SHELL_URLS, ...LISTED_URLS]);
const INDEX = absolute('index.html');

/** Adds the cross-origin isolation headers to a response (same-origin responses only). */
function isolate(response: Response): Response {
  if (response.type === 'opaque' || response.type === 'opaqueredirect' || response.status === 0)
    return response;
  const headers = new Headers(response.headers);
  headers.set('Cross-Origin-Opener-Policy', 'same-origin');
  headers.set('Cross-Origin-Embedder-Policy', 'require-corp');
  headers.set('Cross-Origin-Resource-Policy', 'same-origin');
  return new Response(response.body, { status: response.status, statusText: response.statusText, headers });
}

self.addEventListener('install', (event) => {
  event.waitUntil(
    (async () => {
      const cache = await caches.open(SHELL);
      // Revalidated with the server: Pages lets browsers keep its files for ten minutes, and the
      // shell of this version must be this build's.
      await cache.addAll(SHELL_URLS.map((url) => new Request(url, { cache: 'no-cache' })));
      if (self.registration.active && (await updateFetchesCores())) await cacheOnDemand({ required: true });
    })(),
  );
});

self.addEventListener('activate', (event) => {
  event.waitUntil(
    (async () => {
      const before = await activeRecord();
      const keep = new Set([
        SHELL,
        before && before.version !== VERSION ? `${SHELL_PREFIX}${before.version}` : '',
      ]);
      // Pages that still run the version before this one can finish loading its files.
      for (const name of await caches.keys())
        if (name.startsWith(SHELL_PREFIX) && !keep.has(name)) await caches.delete(name);
      const previous = before && before.version !== VERSION ? before.files : [];
      const wanted = new Set([...LISTED_URLS, ...previous]);
      const assets = await caches.open(ASSETS);
      for (const request of await assets.keys()) if (!wanted.has(request.url)) await assets.delete(request);
      const record: ActiveRecord = { version: VERSION, files: ON_DEMAND_URLS };
      const meta = await caches.open(META);
      await meta.put('active', new Response(JSON.stringify(record)));
      await self.clients.claim();
    })(),
  );
});

interface ActiveRecord {
  version: string;
  files: string[];
}

/** The version that is active and its on-demand files, as it recorded them. */
async function activeRecord(): Promise<ActiveRecord | null> {
  const meta = await caches.open(META);
  const record = await meta.match('active');
  return record ? (record.json() as Promise<ActiveRecord>) : null;
}

/**
 * Whether installing over an active version fetches every core and font. A browser asking to save
 * data is spared that, unless this device holds all of the old version's files: then it is
 * offline-ready, and the update must not take that away.
 */
async function updateFetchesCores(): Promise<boolean> {
  if ((navigator as { connection?: { saveData?: boolean } }).connection?.saveData !== true) return true;
  const record = await activeRecord();
  if (!record) return false;
  const assets = await caches.open(ASSETS);
  for (const url of record.files) if (!(await assets.match(url))) return false;
  return true;
}

/**
 * Caches the on-demand files not cached yet, a few at a time. With `required` the first failure
 * rejects (an install must not finish half-cached); otherwise a file that fails waits for its use.
 */
async function cacheOnDemand({ required }: { required: boolean }): Promise<void> {
  const assets = await caches.open(ASSETS);
  const missing: string[] = [];
  for (const url of ON_DEMAND_URLS) if (!(await assets.match(url))) missing.push(url);
  let failure: Error | null = null;
  const next = async (): Promise<void> => {
    for (let url = missing.shift(); url !== undefined; url = missing.shift()) {
      if (required && failure) return;
      try {
        const response = await fetch(url);
        if (!response.ok) throw new Error(`${url}: HTTP ${response.status}`);
        await assets.put(url, response);
      } catch (error) {
        failure ??= error instanceof Error ? error : new Error(String(error));
      }
    }
  };
  await Promise.all([next(), next(), next(), next()]);
  if (required && failure) throw failure;
}

/** Caches the rest for a page that has gone quiet, and tells the pages when it is all there. */
async function warm(): Promise<void> {
  await cacheOnDemand({ required: false });
  const assets = await caches.open(ASSETS);
  let complete = true;
  for (const url of ON_DEMAND_URLS) if (!(await assets.match(url))) complete = false;
  for (const client of await self.clients.matchAll({ type: 'window' }))
    client.postMessage({ type: 'warmed', complete, version: VERSION });
}

self.addEventListener('message', (event) => {
  const data = event.data as { type?: string } | null;
  if (data?.type === 'skip-waiting') void self.skipWaiting();
  else if (data?.type === 'warm') event.waitUntil(warm());
  else if (data?.type === 'version') event.source?.postMessage({ type: 'version', version: VERSION });
});

/** A file from this version's shell, the on-demand cache, or the network (then cached). */
async function fromCaches(request: Request): Promise<Response> {
  const shell = await caches.open(SHELL);
  const hit =
    (await shell.match(request, { ignoreSearch: true })) ??
    (await caches.match(request, { ignoreSearch: true }));
  if (hit) return hit;
  const response = await fetch(request);
  // Only this build's files are kept (they never change under their name).
  if (response.ok && KNOWN.has(new URL(request.url).origin + new URL(request.url).pathname)) {
    const copy = response.clone();
    void caches.open(ASSETS).then((assets) => assets.put(request.url.split('?')[0] as string, copy));
  }
  return response;
}

/** The page: always this version's shell (the hash route picks the screen). */
async function page(request: Request): Promise<Response> {
  const shell = await caches.open(SHELL);
  const hit = await shell.match(INDEX);
  if (hit) return hit;
  try {
    return await fetch(request);
  } catch {
    // Not installed yet and offline: the previous version's shell, if any.
    return (await caches.match(INDEX)) ?? Response.error();
  }
}

/** How long a window has to answer whether it takes a share. */
const SHARE_ANSWER_MS = 500;

/**
 * Asks each window whether it takes the share itself. A share is a navigation, and it would
 * replace a window with unsaved work, so such a window opens the files where it stands.
 */
async function takenByWindow(id: string): Promise<boolean> {
  const windows = await self.clients.matchAll({ type: 'window' });
  const answers = windows.map(
    (client) =>
      new Promise<boolean>((resolve) => {
        const channel = new MessageChannel();
        channel.port1.onmessage = (event) => resolve(event.data === true);
        setTimeout(() => resolve(false), SHARE_ANSWER_MS);
        client.postMessage({ type: 'share', id }, [channel.port2]);
      }),
  );
  return (await Promise.all(answers)).some(Boolean);
}

/** Files shared to the app: kept until a page opens them, in a window that takes them or a new page. */
async function share(request: Request): Promise<Response> {
  const id = Date.now().toString(36);
  try {
    const form = await request.formData();
    const cache = await caches.open(SHARE);
    let n = 0;
    for (const item of form.getAll('files')) {
      if (!(item instanceof File)) continue;
      const headers = {
        'Content-Type': item.type || 'application/octet-stream',
        'X-Kiln-Name': encodeURIComponent(item.name),
      };
      await cache.put(absolute(`share/${id}/${n++}`), new Response(item, { headers }));
    }
  } catch {
    // Nothing usable was shared: Kiln Home opens.
  }
  // No content: the navigation ends here and the window stays as it is.
  if (await takenByWindow(id)) return new Response(null, { status: 204 });
  return Response.redirect(absolute(`./?share=${id}`), 303);
}

self.addEventListener('fetch', (event) => {
  const request = event.request;
  const url = new URL(request.url);
  if (url.origin !== scope.origin || !url.pathname.startsWith(scope.pathname)) return;
  if (request.method === 'POST' && url.pathname === `${scope.pathname}share-target`) {
    event.respondWith(share(request));
    return;
  }
  if (request.method !== 'GET' || request.headers.has('range')) return;
  if (
    request.mode === 'navigate' &&
    (url.pathname === scope.pathname || url.href.split(/[?#]/)[0] === INDEX)
  ) {
    event.respondWith(page(request).then(isolate));
    return;
  }
  event.respondWith(fromCaches(request).then(isolate));
});
