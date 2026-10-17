/**
 * The web app's routes (hash routing, so they work on GitHub Pages and in the Tauri webview):
 * `#/home` (Kiln Home, also `#/` and no hash), `#/write`, `#/grid`, `#/deck`, `#/notes` (the app
 * frame), plus the developer routes of WP-52/WP-54 (`#/frame`, `#/scratch`, `#/playground`).
 * A query after the route addresses a document: `#/write?doc=3` shows an open document.
 */
import type { AppId } from '@kiln/commands';
import { ThemeProvider } from '@kiln/ui-kit';
import { useEffect, useState, useSyncExternalStore } from 'react';
import { AppFrameRoute, type EditorApp } from './routes/app-frame/index.tsx';
import { DEFAULT_PREFS, loadPrefs, type UiPrefs } from './routes/app-frame/prefs.ts';
import { bridgeRoute } from './routes/frame/index.tsx';
import { HomeRoute } from './routes/home/index.tsx';
import { playgroundRoute } from './routes/playground/index.tsx';

export type Route =
  | { kind: 'home' }
  | { kind: 'app'; app: EditorApp; doc: number | null; nonce: string | null }
  | { kind: 'unknown'; hash: string };

const EDITORS: readonly EditorApp[] = ['write', 'grid', 'deck', 'notes'];

/** The route of a location hash. */
export function parseRoute(hash: string): Route {
  const [path = '', query = ''] = hash.replace(/^#/, '').split('?');
  const name = path.replace(/^\/+/, '').replace(/\/+$/, '');
  if (name === '' || name === 'home') return { kind: 'home' };
  if ((EDITORS as readonly string[]).includes(name)) {
    const params = new URLSearchParams(query);
    const doc = Number(params.get('doc'));
    return {
      kind: 'app',
      app: name as EditorApp,
      doc: Number.isInteger(doc) && doc > 0 ? doc : null,
      nonce: params.get('new'),
    };
  }
  return { kind: 'unknown', hash };
}

/** The hash of an app route. */
export function appHash(app: AppId, doc?: number | null): string {
  if (app === 'hub') return '#/home';
  return doc ? `#/${app}?doc=${doc}` : `#/${app}`;
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
  const hash = useSyncExternalStore(subscribeHash, () => window.location.hash);
  const prefs = usePrefs();
  const dev = bridgeRoute(hash) ?? playgroundRoute(hash);
  const route = parseRoute(hash);
  useEffect(() => {
    const root = document.getElementById('root');
    if (root) root.dataset.route = dev ? 'dev' : route.kind === 'app' ? 'app' : 'home';
  }, [dev, route.kind]);
  if (dev) return dev;
  if (!prefs) return null;
  const app: AppId = route.kind === 'app' ? route.app : 'hub';
  return (
    <ThemeProvider app={app} defaultTheme={prefs.theme} persist={false}>
      {route.kind === 'app' ? (
        <AppFrameRoute
          key={`${route.app}:${route.doc ?? 'new'}:${route.nonce ?? ''}`}
          app={route.app}
          doc={route.doc}
          prefs={prefs}
        />
      ) : (
        <HomeRoute prefs={prefs} />
      )}
    </ThemeProvider>
  );
}
