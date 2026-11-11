/**
 * The web app's routes (hash routing, so they work on GitHub Pages and in the Tauri webview):
 * `#/home` (Kiln Home, also `#/` and no hash), `#/write`, `#/grid`, `#/deck` (the app frame, for
 * the apps of the release: `RELEASE_APPS`), `#/present` (Kiln Deck's slide show and Presenter View), plus the developer routes
 * (`#/frame`, `#/scratch`, `#/playground`).
 * A query after the route addresses a document: `#/write?doc=3` shows an open document, and
 * `#/write?doc=<room>&server=<url>&token=<token>` opens a shared one, and
 * `#/write?template=write/letter.kwrite` a new document from a template.
 */
import { type AppId, isReleasedApp } from '@kiln/commands';
import { ThemeProvider, useInterfaceLanguage } from '@kiln/ui-kit';
import { lazy, type ReactNode, Suspense, useEffect, useState, useSyncExternalStore } from 'react';
import { useDesktopRoute } from './desktop.ts';
import { LoadBoundary } from './LoadProblem.tsx';
import type { EditorApp } from './routes/app-frame/index.tsx';
import { DEFAULT_PREFS, loadPrefs, type UiPrefs } from './routes/app-frame/prefs.ts';
import { bridgeRoute } from './routes/frame/index.tsx';
import { HomeRoute } from './routes/home/index.tsx';
import { type PresentParams, parsePresent, type ShareLink, shareLinkOf } from './routes/params.ts';
import { playgroundRoute } from './routes/playground/index.tsx';

// The editors and the slide show load when a window first shows them, so Kiln Home (the first
// screen) does not wait for every app's code (budget B12: the page's first script).
const loadAppFrame = () => import('./routes/app-frame/index.tsx');
const AppFrameRoute = lazy(() => loadAppFrame().then((m) => ({ default: m.AppFrameRoute })));

/**
 * Fetches the editors' chunk at the first sign that the user is about to open something (a
 * pointer over the window, a key, a touch), so the window that opens has its code already; a
 * page nobody touches loads nothing more.
 */
function useEditorsSoon(): void {
  useEffect(() => {
    const events = ['pointerover', 'pointerdown', 'keydown', 'touchstart'] as const;
    const fetchOnce = () => {
      for (const e of events) window.removeEventListener(e, fetchOnce, true);
      void loadAppFrame().catch(() => undefined);
    };
    for (const e of events) window.addEventListener(e, fetchOnce, { capture: true, passive: true });
    return () => {
      for (const e of events) window.removeEventListener(e, fetchOnce, true);
    };
  }, []);
}
// the update, offline and install notices of the GitHub Pages build.
const PwaLayer = __KILN_PWA__
  ? lazy(() =>
      import('./pwa/PwaLayer.tsx').catch(() => ({ default: (_: { onHome: boolean }) => null as ReactNode })),
    )
  : null;
const PresentRoute = lazy(() =>
  import('./routes/present/index.tsx').then((m) => ({ default: m.PresentRoute })),
);

export type Route =
  | { kind: 'home' }
  | {
      kind: 'app';
      app: EditorApp;
      doc: number | null;
      nonce: string | null;
      share: ShareLink | null;
      /** A template (its file in the templates folder) to start the new document from. */
      template: string | null;
    }
  | { kind: 'present'; params: PresentParams }
  | { kind: 'unknown'; hash: string };

/** The app frames this release routes to (an app outside `RELEASE_APPS` has no route). */
const EDITORS: readonly EditorApp[] = (['write', 'grid', 'deck', 'notes'] as const).filter(isReleasedApp);

/** The route of a location hash. */
export function parseRoute(hash: string): Route {
  const [path = '', query = ''] = hash.replace(/^#/, '').split('?');
  const name = path.replace(/^\/+/, '').replace(/\/+$/, '');
  if (name === '' || name === 'home') return { kind: 'home' };
  if (name === 'present') return { kind: 'present', params: parsePresent(query) };
  if ((EDITORS as readonly string[]).includes(name)) {
    const params = new URLSearchParams(query);
    const doc = Number(params.get('doc'));
    return {
      kind: 'app',
      app: name as EditorApp,
      doc: Number.isInteger(doc) && doc > 0 ? doc : null,
      nonce: params.get('new'),
      share: shareLinkOf(query),
      template: params.get('template'),
    };
  }
  return { kind: 'unknown', hash };
}

/** The hash of an app route. */
export function appHash(app: AppId, doc?: number | null): string {
  if (app === 'hub') return '#/home';
  return doc ? `#/${app}?doc=${doc}` : `#/${app}`;
}

/**
 * The frames that created their document and then took its address (`#/write?new=…` becomes
 * `#/write?doc=N`): the frame keeps its key, so the window does not open the document again.
 */
const adopted = new Map<string, string>();

function frameKey(route: Extract<Route, { kind: 'app' }>): string {
  const own = `${route.app}:${route.doc ?? route.share?.room ?? route.template ?? 'new'}:${route.nonce ?? ''}`;
  return (route.doc !== null && adopted.get(`${route.app}:${route.doc}`)) || own;
}

/**
 * Gives a new document's frame the document's own address, in place (Back and Forward, and Kiln
 * Home's Open in this window, then come back to this document, not to another new one). The
 * address keeps its other parameters.
 */
export function adoptAddress(app: AppId, doc: number): void {
  const route = parseRoute(window.location.hash);
  // A shared document keeps its link (`doc` is the room there), which opens it again on a reload.
  if (route.kind !== 'app' || route.app !== app || route.doc !== null || route.share) return;
  const [path, query = ''] = window.location.hash.split('?');
  const params = new URLSearchParams(query);
  params.delete('new');
  params.delete('template');
  params.delete('doc');
  const rest = params.toString();
  adopted.set(`${app}:${doc}`, frameKey(route));
  window.history.replaceState(window.history.state, '', `${path}?doc=${doc}${rest ? `&${rest}` : ''}`);
}

function subscribeHash(onChange: () => void) {
  window.addEventListener('hashchange', onChange);
  return () => window.removeEventListener('hashchange', onChange);
}

/** Loads the UI preferences from the host before the first paint of the product routes. */
function usePrefs(): UiPrefs | null {
  const [prefs, setPrefs] = useState<UiPrefs | null>(null);
  useEffect(() => {
    let live = true;
    loadPrefs().then(
      (p) => live && setPrefs(p),
      () => live && setPrefs(DEFAULT_PREFS),
    );
    return () => {
      live = false;
    };
  }, []);
  return prefs;
}

export function App() {
  // The whole window renders again in the new language after a switch (Options ▸ Language).
  useInterfaceLanguage();
  useEditorsSoon();
  const hash = useSyncExternalStore(subscribeHash, () => window.location.hash);
  const prefs = usePrefs();
  const dev = bridgeRoute(hash) ?? playgroundRoute(hash);
  const route = parseRoute(hash);
  // The desktop shell gives the window the menu bar of the app it shows.
  useDesktopRoute(
    dev ? 'home' : route.kind === 'app' ? route.app : route.kind === 'present' ? 'present' : 'home',
  );
  useEffect(() => {
    const root = document.getElementById('root');
    if (root)
      root.dataset.route = dev
        ? 'dev'
        : route.kind === 'app'
          ? 'app'
          : route.kind === 'present'
            ? 'present'
            : 'home';
  }, [dev, route.kind]);
  if (dev) return dev;
  // The slide show needs no preferences: it paints the slides only.
  if (route.kind === 'present')
    return (
      <ThemeProvider app="deck" defaultTheme="light" persist={false}>
        <LoadBoundary app="deck" hash={hash}>
          <Suspense fallback={null}>
            <PresentRoute params={route.params} />
          </Suspense>
        </LoadBoundary>
      </ThemeProvider>
    );
  if (!prefs) return null;
  const app: AppId = route.kind === 'app' ? route.app : 'hub';
  return (
    <ThemeProvider app={app} defaultTheme={prefs.theme} persist={false}>
      {route.kind === 'app' ? (
        <LoadBoundary app={app} hash={hash}>
          <Suspense fallback={null}>
            <AppFrameRoute
              key={frameKey(route)}
              app={route.app}
              doc={route.doc}
              template={route.template}
              share={route.share}
              prefs={prefs}
            />
          </Suspense>
        </LoadBoundary>
      ) : (
        <HomeRoute prefs={prefs} />
      )}
      {PwaLayer ? (
        <Suspense fallback={null}>
          <PwaLayer onHome={route.kind === 'home'} />
        </Suspense>
      ) : null}
    </ThemeProvider>
  );
}
