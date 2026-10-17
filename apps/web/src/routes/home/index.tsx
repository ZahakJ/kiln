/**
 * Kiln Home wired to the core (PLAN.md §1.6): recent files come from the host's recents store
 * (`HostOp::Recents`, kept by `kiln-storage`), New opens the app frame (which asks the core for a
 * new document), and Open file runs the core's `suite.file.open`, whose `Event::Opened` takes the
 * window to the right app. Only UI preferences are kept here (the dismissed banner).
 */
import { type HomeApp, type HomeStorage, KilnHome, kindApp } from '@kiln/home';
import { type KilnClient, KilnError, type RecentFile, WebHost } from '@kiln/protocol';
import { OptionsDialog, ToastProvider, TooltipProvider, useToast } from '@kiln/ui-kit';
import { useCallback, useEffect, useState } from 'react';
import { appHash } from '../../App.tsx';
import { getClient } from '../../client/index.ts';
import { storePref, type UiPrefs } from '../app-frame/prefs.ts';
import { usePersistTheme } from '../app-frame/usePersistTheme.ts';

/** Apps whose documents the core can create in this build (`Session::new_document`). */
export const CREATABLE: readonly HomeApp[] = ['write', 'grid'];

const recentsHost = new WebHost();

async function loadRecents(): Promise<RecentFile[]> {
  const reply = await recentsHost.handle('Recents');
  return typeof reply === 'object' && 'Recents' in reply ? reply.Recents : [];
}

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
  const { toast } = useToast();
  const [optionsOpen, setOptionsOpen] = useState(false);
  usePersistTheme();

  useEffect(() => {
    document.title = 'Kiln';
    let live = true;
    loadRecents().then(
      (r) => live && setRecent(r),
      () => live && setRecent([]),
    );
    void storageEstimate().then((s) => live && setStorage(s));
    return () => {
      live = false;
    };
  }, []);

  const fail = useCallback(
    (title: string, error: unknown) =>
      toast({ title, description: KilnError.from(error).message, tone: 'danger', duration: 6000 }),
    [toast],
  );

  /** Runs the core's Open and follows the document it opens to its app. */
  const openWith = useCallback(
    async (path: string | null) => {
      let client: KilnClient;
      try {
        client = await getClient();
      } catch (error) {
        fail('The Kiln core did not start', error);
        return;
      }
      const stop = client.subscribe((event) => {
        if ('Opened' in event) {
          stop();
          window.location.hash = appHash(kindApp(event.Opened.summary.kind), event.Opened.summary.doc);
        } else if ('Error' in event && event.Error.doc === null) {
          stop();
          fail('Could not open the file', event.Error.error);
        }
      });
      try {
        const result = await client.execute(null, { Suite: { Open: { path } } });
        if (result.outcome !== 'Applied' && typeof result.outcome === 'string') stop();
      } catch (error) {
        stop();
        if (KilnError.from(error).kind !== 'Cancelled') fail('Could not open the file', error);
      }
    },
    [fail],
  );

  return (
    <>
      <KilnHome
        recent={recent}
        shared={null}
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
        onOpenFile={() => void openWith(null)}
        onOpenRecent={(file) => void openWith(file.path)}
        onOpenSettings={() => setOptionsOpen(true)}
        version={__KILN_VERSION__}
      />
      <OptionsDialog open={optionsOpen} onOpenChange={setOptionsOpen} appName="Kiln" />
    </>
  );
}
