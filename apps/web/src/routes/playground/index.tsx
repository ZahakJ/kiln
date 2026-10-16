import type { DocId, KilnClient } from '@kiln/protocol';
import { Surface, type SurfaceController } from '@kiln/surface';
import { useCallback, useEffect, useRef, useState } from 'react';
import { useClient } from '../../client/useClient';

/** The playground's hash route (`#/playground`, with optional `?bare` for full-window tests). */
export const PLAYGROUND_ROUTE = '#/playground';

const SAMPLE = [
  'The Kiln surface',
  'Everything on this page is laid out and rasterised by the Rust core and painted on a canvas: the browser never lays out a single line of document text. Type, select with the mouse or Shift and the arrow keys, double-click a word, triple-click a paragraph, or compose text with an input method.',
  'Scrolling paints from cached tiles while the core prepares the next ones a viewport ahead, so it never waits for the bridge. Pinch or Ctrl+wheel to zoom.',
  'A screen reader reads the document through an off-screen mirror of the visible pages, and the caret is announced as it moves.',
].join('\n');

/** What the e2e tests reach through `window.__kilnPlayground`. */
export interface PlaygroundHandle {
  client: KilnClient;
  doc: DocId;
  controller: SurfaceController | null;
  /** Opens `text` as a new scratch document and shows it. */
  open(text: string, name?: string): Promise<DocId>;
}

declare global {
  interface Window {
    __kilnPlayground?: PlaygroundHandle;
  }
}

type Theme = 'system' | 'light' | 'dark';

/**
 * A scratch document on the canvas surface, for trying the painter, input and accessibility by
 * hand and for the `surface` and `painter-parity` e2e tests.
 */
export function PlaygroundRoute() {
  const state = useClient();
  const client = state.status === 'ready' ? state.client : null;
  const [doc, setDoc] = useState<{ id: DocId; title: string } | null>(null);
  const [theme, setTheme] = useState<Theme>('system');
  const [zoom, setZoom] = useState(1);
  const [error, setError] = useState<string | null>(null);
  const controller = useRef<SurfaceController | null>(null);
  const bare = typeof window !== 'undefined' && window.location.hash.includes('bare');

  const open = useCallback(
    async (text: string, name = 'Playground.txt') => {
      if (!client) throw new Error('the core is not ready');
      const summary = await client.open(new TextEncoder().encode(text), {
        name,
        path: null,
        format: 'Txt',
        read_only: false,
      });
      setDoc({ id: summary.doc, title: summary.title });
      return summary.doc;
    },
    [client],
  );

  useEffect(() => {
    if (!client || doc) return;
    open(SAMPLE, 'Playground.txt').catch((e: unknown) => setError(String(e)));
  }, [client, doc, open]);

  useEffect(() => {
    if (!client || !doc) return;
    window.__kilnPlayground = { client, doc: doc.id, controller: controller.current, open };
    return () => {
      delete window.__kilnPlayground;
    };
  }, [client, doc, open]);

  const onController = useCallback((c: SurfaceController | null) => {
    controller.current = c;
    if (window.__kilnPlayground) window.__kilnPlayground.controller = c;
  }, []);

  const zoomBy = (factor: number) => {
    const c = controller.current;
    if (!c) return;
    c.setZoom(factor === 0 ? 1 : c.getZoom() * factor);
  };

  const failure = state.status === 'error' ? state.error.message : error;
  return (
    <div className="flex h-dvh flex-col bg-(--kiln-bg-canvas)" data-testid="playground">
      {!bare && (
        <header className="flex h-11 shrink-0 items-center gap-2 border-b border-(--kiln-border) bg-(--kiln-bg-surface) px-3 text-[13px] sm:px-4">
          <a href="#/" className="font-semibold tracking-tight text-(--kiln-fg) no-underline">
            Kiln
          </a>
          <span aria-hidden="true" className="h-4 w-px bg-(--kiln-border)" />
          <h1 className="m-0 truncate text-[13px] font-medium text-(--kiln-fg-muted)">
            Surface playground{doc ? ` · ${doc.title}` : ''}
          </h1>
          <div className="ml-auto flex items-center gap-1" role="toolbar" aria-label="View">
            <button
              type="button"
              className="rounded px-2 py-1 hover:bg-(--kiln-bg-surface-2)"
              onClick={() => zoomBy(1 / 1.1)}
              aria-label="Zoom out"
            >
              −
            </button>
            <button
              type="button"
              className="min-w-14 rounded px-2 py-1 tabular-nums hover:bg-(--kiln-bg-surface-2)"
              onClick={() => zoomBy(0)}
              aria-label="Zoom to 100%"
              data-testid="playground-zoom"
            >
              {Math.round(zoom * 100)}%
            </button>
            <button
              type="button"
              className="rounded px-2 py-1 hover:bg-(--kiln-bg-surface-2)"
              onClick={() => zoomBy(1.1)}
              aria-label="Zoom in"
            >
              +
            </button>
            <label className="ml-2 flex items-center gap-1 text-(--kiln-fg-muted)">
              <span className="sr-only sm:not-sr-only">Theme</span>
              <select
                value={theme}
                onChange={(e) => setTheme(e.target.value as Theme)}
                className="rounded border border-(--kiln-border) bg-(--kiln-bg-surface) px-1 py-0.5 text-(--kiln-fg)"
              >
                <option value="system">System</option>
                <option value="light">Light</option>
                <option value="dark">Dark</option>
              </select>
            </label>
          </div>
        </header>
      )}
      <main className="relative min-h-0 flex-1">
        {failure && (
          <p
            role="alert"
            className="m-4 rounded border border-(--kiln-border) bg-(--kiln-bg-surface) p-3 text-[13px]"
          >
            {failure}
          </p>
        )}
        {client && doc && (
          <Surface
            key={doc.id}
            client={client}
            doc={doc.id}
            label={doc.title}
            theme={theme}
            onController={onController}
            onZoom={setZoom}
            onError={(e) => setError(String(e))}
          />
        )}
      </main>
    </div>
  );
}

/** The playground route for a location hash, or `null`. */
export function playgroundRoute(hash: string) {
  return hash === PLAYGROUND_ROUTE || hash.startsWith(`${PLAYGROUND_ROUTE}?`) ? <PlaygroundRoute /> : null;
}
