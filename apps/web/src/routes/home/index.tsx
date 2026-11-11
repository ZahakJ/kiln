/**
 * Kiln Home wired to the core: recent and pinned files come from `Query::Recents`
 * (kept by `kiln-storage`; the web reads them from the page's storage without starting a core),
 * New opens the app frame (which asks the core for a new document), and Open file runs the core's
 * `suite.file.open`, whose `Event::Opened` takes the window to the right app. A template opens the
 * app frame on a new document from it; Shared with me lists the shared documents joined
 * from links; the welcome tour shows once. Only UI preferences are kept here (the dismissed
 * banner, the tour, the shared documents).
 */

import { type HomeApp, type HomeOpenDocument, type HomeStorage, KilnHome, kindApp } from '@kiln/home';
import { t } from '@kiln/i18n';
import { isTauri, type KilnClient, KilnError, type RecentFile } from '@kiln/protocol';
import {
  errorText,
  OptionsDialog,
  ToastProvider,
  TooltipProvider,
  useToast,
  WelcomeTour,
} from '@kiln/ui-kit';
import { useCallback, useEffect, useState } from 'react';
import { appHash } from '../../App.tsx';
import { getClient } from '../../client/index.ts';
import { desktopHomePainted, installDesktopDrop, useDesktopHome } from '../../desktop.ts';
import { TEMPLATES } from '../../templates.ts';
import { storePref, type UiPrefs } from '../app-frame/prefs.ts';
import { loadRecents } from '../app-frame/storage.tsx';
import { usePersistTheme } from '../app-frame/usePersistTheme.ts';
import { loadShared, type SharedRoom, sharedRows } from './shared.ts';

/** Apps whose documents the core can create in this build (`Session::new_document`). */
export const CREATABLE: readonly HomeApp[] = ['write', 'grid', 'deck'];

async function storageEstimate(): Promise<HomeStorage | null> {
  try {
    const est = await navigator.storage?.estimate?.();
    return est?.quota ? { used: est.usage ?? 0, quota: est.quota } : null;
  } catch {
    return null;
  }
}

export function HomeRoute({ prefs }: { prefs: UiPrefs }) {
  return (
    <TooltipProvider>
      <ToastProvider>
        <HomeScreen prefs={prefs} />
      </ToastProvider>
    </TooltipProvider>
  );
}

function HomeScreen({ prefs }: { prefs: UiPrefs }) {
  const [recent, setRecent] = useState<RecentFile[] | null>(null);
  const [storage, setStorage] = useState<HomeStorage | null>(null);
  const [dismissed, setDismissed] = useState(prefs.switcherDismissed);
  const [shared, setShared] = useState<SharedRoom[]>([]);
  const [openDocs, setOpenDocs] = useState<HomeOpenDocument[]>([]);
  // The welcome tour shows once, on the first visit (or again from the banner's button).
  const [tour, setTour] = useState(!prefs.tourSeen);
  const [view, setView] = useState<'home' | 'templates'>('home');
  const [homeKey, setHomeKey] = useState(0);
  const { toast } = useToast();
  const [optionsOpen, setOptionsOpen] = useState(false);
  usePersistTheme();

  useEffect(() => {
    let live = true;
    void loadShared().then((rooms) => live && setShared(rooms));
    return () => {
      live = false;
    };
  }, []);

  // `--startup-probe` ends at Kiln Home's first paint (budget B1).
  useEffect(() => desktopHomePainted(), []);

  useEffect(() => {
    document.title = 'Kiln';
    let live = true;
    let stop = () => {};
    getClient()
      .then((client) => {
        if (!live) return;
        // Pins and files opened elsewhere arrive from the core as they change.
        stop = client.subscribe((e) => {
          if (typeof e === 'object' && 'Recents' in e) setRecent(e.Recents.files);
        });
        // The documents still open in this tab (the browser's Back and the title bar's app mark
        // come here without closing them).
        if (!isTauri())
          void client
            .query(null, 'Documents')
            .then((r) => {
              if (!live || !('Documents' in r)) return;
              setOpenDocs(
                r.Documents.map((d) => ({
                  app: kindApp(d.kind),
                  doc: d.doc,
                  title: d.title,
                  unsaved: d.dirty,
                })).filter((d) => d.app !== 'notes'),
              );
            })
            .catch(() => undefined);
        return loadRecents(client).then((r) => live && setRecent(r));
      })
      .catch(() => live && setRecent([]));
    void storageEstimate().then((s) => live && setStorage(s));
    return () => {
      live = false;
      stop();
    };
  }, []);

  const fail = useCallback(
    (title: string, error: unknown) =>
      toast({ title, description: errorText(KilnError.from(error)), tone: 'danger', duration: 6000 }),
    [toast],
  );

  /** Runs the core's Open and follows the document it opens to its app. */
  const openWith = useCallback(
    async (path: string | null) => {
      let client: KilnClient;
      try {
        client = await getClient();
      } catch (error) {
        fail(t('The Kiln core did not start'), error);
        return;
      }
      const stop = client.subscribe((event) => {
        if ('Opened' in event) {
          stop();
          window.location.hash = appHash(kindApp(event.Opened.summary.kind), event.Opened.summary.doc);
        } else if ('Error' in event && event.Error.doc === null) {
          stop();
          fail(t('Could not open the file'), event.Error.error);
        }
      });
      try {
        const result = await client.execute(null, { Suite: { Open: { path } } });
        if (result.outcome !== 'Applied' && typeof result.outcome === 'string') stop();
      } catch (error) {
        stop();
        if (KilnError.from(error).kind !== 'Cancelled') fail(t('Could not open the file'), error);
      }
    },
    [fail],
  );

  // The desktop's File ▸ Open, and documents dropped on the window where the page gets them.
  useDesktopHome(() => void openWith(null));
  useEffect(
    () =>
      installDesktopDrop((file) => {
        void (async () => {
          const client = await getClient();
          const bytes = new Uint8Array(await file.arrayBuffer());
          const s = await client.open(bytes, { name: file.name, path: null, format: null, read_only: false });
          window.location.hash = appHash(kindApp(s.kind), s.doc);
        })().catch((error: unknown) => fail(t('Could not open the file'), error));
      }),
    [fail],
  );

  return (
    <>
      {/* First in the tab order: the tour is the first thing a newcomer reaches. */}
      <WelcomeTour
        open={tour}
        finishLabel={t('Browse templates')}
        onDone={() => {
          setTour(false);
          void storePref('tourSeen', true);
        }}
        onFinishAction={() => {
          // The last card's action: the template gallery.
          setView('templates');
          setHomeKey((k) => k + 1);
        }}
      />
      <KilnHome
        key={homeKey}
        initialView={view}
        openDocument={isTauri()}
        recent={recent}
        openDocuments={openDocs}
        onOpenDocument={(d) => {
          window.location.hash = appHash(d.app, d.doc);
        }}
        shared={shared.length > 0 ? sharedRows(shared) : null}
        templates={TEMPLATES}
        storage={storage}
        creatable={CREATABLE}
        switcherDismissed={dismissed}
        onDismissSwitcher={() => {
          setDismissed(true);
          void storePref('switcherDismissed', true);
        }}
        onCreate={(app) => {
          window.location.hash = appHash(app);
        }}
        onOpenTemplate={(template) => {
          if (!template.file) return;
          const q = new URLSearchParams({ template: template.file, new: String(Date.now()) });
          window.location.hash = `${appHash(template.app)}?${q.toString()}`;
        }}
        onOpenShared={(file) => {
          window.location.hash = file.path;
        }}
        onTour={() => setTour(true)}
        onOpenFile={() => void openWith(null)}
        onOpenRecent={(file) => void openWith(file.path)}
        onTogglePin={(file) => {
          setRecent((list) =>
            (list ?? []).map((f) => (f.path === file.path ? { ...f, pinned: !file.pinned } : f)),
          );
          void getClient()
            .then((client) =>
              client.execute(null, { Suite: { RecentPin: { path: file.path, pinned: !file.pinned } } }),
            )
            .catch((error: unknown) => fail(t('Could not pin the file'), error));
        }}
        onRemoveRecent={(file) => {
          setRecent((list) => (list ?? []).filter((f) => f.path !== file.path));
          void getClient()
            .then((client) => client.execute(null, { Suite: { RecentRemove: { path: file.path } } }))
            .catch((error: unknown) => fail(t('Could not remove the file from the list'), error));
        }}
        onOpenSettings={() => setOptionsOpen(true)}
        version={__KILN_VERSION__}
      />
      <OptionsDialog open={optionsOpen} onOpenChange={setOptionsOpen} appName="Kiln" />
    </>
  );
}
