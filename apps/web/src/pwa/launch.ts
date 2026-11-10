/**
 * Files that reach the installed web app from outside: a document opened with Kiln from
 * the system's file manager (the manifest's `file_handlers`, delivered through `launchQueue`)
 * and files shared to Kiln (`share_target`; the service worker keeps them in the `kiln-share`
 * cache and opens the page at `?share=<id>`).
 *
 * Each file is kept like a picked one (`adoptFile`): a launched file's handle stays writable, so
 * Save writes back to it. The core of its format opens it and the window goes to that app.
 *
 * A running installed app stays running: the manifest's `launch_handler` is `focus-existing`, so
 * the browser hands a launch to this page instead of reloading it. Documents already open stay
 * open in their cores (Back returns to them), and the page handles what a reload would have
 * done, including the icon menu's New commands (`targetURL`). A share is a navigation by nature:
 * a window with unsaved work takes it here, and the service worker ends the navigation.
 */
import { kindApp } from '@kiln/home';
import type { FileHandleLike, KilnClient } from '@kiln/protocol';
import { appHash, parseRoute } from '../App.tsx';
import { coreOfDoc, coreOfHint } from '../client/cores.ts';
import { adoptFile, getClient } from '../client/index.ts';
import { unsavedDocuments } from '../client/unsaved.ts';
import { pwaState, setPwaState } from './state.ts';

/** The cache the service worker keeps shared files in (`src/sw.ts`). */
const SHARE = 'kiln-share';

interface LaunchParams {
  files: readonly FileHandleLike[];
  /** The URL the launch asked for (`start_url`, or a shortcut's). */
  targetURL?: string;
}
interface LaunchQueue {
  setConsumer(consumer: (params: LaunchParams) => void): void;
}

/** Whether a web core opens this file name. */
function opensOnTheWeb(name: string): boolean {
  return coreOfHint({ name, path: null, format: null, read_only: false }) !== null;
}

function notOpened(name: string): void {
  setPwaState({ unopened: [...pwaState().unopened, name] });
}

/**
 * Before the window moves on: a new document's route names no document, so its history entry
 * gets the unsaved document's id, and Back comes back to that document, not to a new one.
 */
function keepPlace(): void {
  const route = parseRoute(window.location.hash);
  if (route.kind !== 'app' || route.doc !== null) return;
  const shown = unsavedDocuments().findLast((d) => coreOfDoc(d.doc) === route.app);
  if (shown) window.history.replaceState(window.history.state, '', appHash(route.app, shown.doc));
}

/** Opens one file by the path it was kept under; resolves once its window shows. */
async function openPath(client: KilnClient, path: string, name: string): Promise<void> {
  await new Promise<void>((resolve) => {
    const stop = client.subscribe((event) => {
      if ('Opened' in event) {
        stop();
        keepPlace();
        window.location.hash = appHash(kindApp(event.Opened.summary.kind), event.Opened.summary.doc);
        resolve();
      } else if ('Error' in event && event.Error.doc === null) {
        stop();
        notOpened(name);
        resolve();
      }
    });
    client.execute(null, { Suite: { Open: { path } } }).then(
      (result) => {
        if (result.outcome !== 'Applied' && typeof result.outcome === 'string') {
          stop();
          resolve();
        }
      },
      () => {
        stop();
        notOpened(name);
        resolve();
      },
    );
  });
}

/** Opens files one after another (each in its app's window; the last one stays in front). */
async function openAll(items: (FileHandleLike | File)[]): Promise<void> {
  if (items.length === 0) return;
  let client: KilnClient;
  try {
    client = await getClient();
  } catch {
    for (const item of items) notOpened(item.name);
    return;
  }
  for (const item of items) {
    if (!opensOnTheWeb(item.name)) {
      notOpened(item.name);
      continue;
    }
    const path = await adoptFile(item);
    if (path) await openPath(client, path, item.name);
    else notOpened(item.name);
  }
}

/** Files shared to the app under `id`: read from the cache, then removed from it. */
async function sharedFiles(id: string): Promise<File[]> {
  if (!/^[a-z0-9]+$/.test(id) || typeof caches === 'undefined') return [];
  try {
    const cache = await caches.open(SHARE);
    const files: File[] = [];
    for (const request of await cache.keys()) {
      if (!new URL(request.url).pathname.includes(`/share/${id}/`)) continue;
      const response = await cache.match(request);
      if (response) {
        const name = decodeURIComponent(response.headers.get('X-Kiln-Name') ?? 'Shared file');
        const blob = await response.blob();
        files.push(new File([blob], name, { type: blob.type }));
      }
      await cache.delete(request);
    }
    return files;
  } catch {
    return [];
  }
}

/** A shortcut launched into the running window: its route, as a new document where it makes one. */
function followTarget(target: string): void {
  const hash = new URL(target).hash;
  if (!hash) return;
  const path = hash.replace(/^#\/?/, '');
  keepPlace();
  window.location.hash = path === '' || path === 'home' ? hash : `${hash}?new=${Date.now().toString(36)}`;
}

/** A window with unsaved work opens a share where it stands (`takenByWindow` in `src/sw.ts`). */
function onWorkerMessage(event: MessageEvent): void {
  const data = event.data as { type?: string; id?: string } | null;
  const reply = event.ports[0];
  if (data?.type !== 'share' || !data.id || !reply) return;
  const taken = unsavedDocuments().length > 0;
  reply.postMessage(taken);
  if (taken) void sharedFiles(data.id).then(openAll);
}

/** Listens for launched files and opens shared ones. Pages build only (`__KILN_PWA__`). */
export function startLaunches(): void {
  const queue = (window as unknown as { launchQueue?: LaunchQueue }).launchQueue;
  // The first launch is the one that loaded this page, already at its URL.
  let running = false;
  queue?.setConsumer((params) => {
    const first = !running;
    running = true;
    if (params.files.length > 0) void openAll([...params.files]);
    else if (!first && params.targetURL) followTarget(params.targetURL);
  });
  navigator.serviceWorker?.addEventListener('message', onWorkerMessage);

  const params = new URLSearchParams(window.location.search);
  const share = params.get('share');
  if (share) {
    // The address loses the share id, so a reload does not open the files again.
    window.history.replaceState(
      window.history.state,
      '',
      `${window.location.pathname}${window.location.hash}`,
    );
    void sharedFiles(share).then(openAll);
  }
}
