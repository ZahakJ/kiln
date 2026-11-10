/**
 * What a window shows when part of Kiln could not load: a core that did not start, or a
 * chunk that is gone. A tab left open across a deployment asks for files of its version that the
 * server no longer has, and that is an update, not a failure: "Kiln has been updated", with
 * Reload. Offline, it is an app this device has not saved yet. Anything else offers Reload. The
 * reason goes to the console, never to the screen.
 */

import type { AppId } from '@kiln/commands';
import { t } from '@kiln/i18n';
import { APP_NAMES, AppTile, Button } from '@kiln/ui-kit';
import { Component, type ReactNode, useEffect, useState } from 'react';
import { useReloadCost } from './client/unsaved.ts';
import { setPwaState } from './pwa/state.ts';
import './load-problem.css';

/** `stopped`: the core stopped after a fault, mid-session (the document is kept by recovery). */
export type LoadFailure = 'updated' | 'offline' | 'failed' | 'stopped';

/** Whether the server has moved on: its service worker no longer lists this page's own script. */
async function deployedNewer(): Promise<boolean> {
  if (!__KILN_PWA__) return false;
  const src = document.querySelector<HTMLScriptElement>('script[type="module"][src]')?.src;
  const name = src?.split('/').pop();
  if (!name) return false;
  try {
    const response = await fetch(`${import.meta.env.BASE_URL}sw.js`, { cache: 'no-store' });
    return response.ok && !(await response.text()).includes(name);
  } catch {
    return false;
  }
}

/** Why something would not load, as far as the page can tell. */
export async function classifyFailure(): Promise<LoadFailure> {
  if (navigator.onLine === false) return 'offline';
  if (!(await deployedNewer())) return 'failed';
  // The new version installs now, so Reload does not wait for it.
  void navigator.serviceWorker?.getRegistration().then((reg) => reg?.update().catch(() => undefined));
  return 'updated';
}

/** The update notice's sentence for what a reload does to the page's work. */
export function updateMessage(cost: ReturnType<typeof useReloadCost>): string {
  if (cost === 'recovery')
    return t('Reload to start the new version. Your unsaved changes come back in Document Recovery.');
  if (cost === 'asks')
    return t('Reload to start the new version. Kiln asks about your unsaved changes first.');
  return t('Reload to start the new version.');
}

/** Reloads, with the update's care for unsaved work where the page has a service worker. */
async function reloadWindow(): Promise<boolean> {
  if (!__KILN_PWA__) {
    window.location.reload();
    return true;
  }
  try {
    const { reloadIntoUpdate } = await import('./pwa/register.ts');
    return reloadIntoUpdate();
  } catch {
    window.location.reload();
    return true;
  }
}

function ReloadButton() {
  const [busy, setBusy] = useState(false);
  return (
    <Button
      variant="ember"
      size="sm"
      busy={busy}
      data-testid="load-problem-reload"
      onClick={() => {
        setBusy(true);
        void reloadWindow().then(setBusy);
      }}
    >
      {t('Reload')}
    </Button>
  );
}

/** The state itself: the app's tile, what happened, and the way on. */
export function ProblemCard({
  app,
  failure,
  title,
  text,
  reload,
}: {
  app: AppId;
  /** For tests and styles. */
  failure: LoadFailure | 'document';
  title: string;
  text: string;
  reload: boolean;
}) {
  return (
    <div className="kiln-load-problem" role="alert" data-testid="load-problem" data-failure={failure}>
      <AppTile app={app === 'hub' ? 'home' : app} size={48} />
      <div className="kiln-load-problem__title">{title}</div>
      <p className="kiln-load-problem__text">{text}</p>
      <div className="kiln-load-problem__actions">
        {reload ? <ReloadButton /> : null}
        <a href="#/home">{t('Back to Kiln Home')}</a>
      </div>
    </div>
  );
}

/**
 * The core stopped after a fault it could not recover from. What was typed is in the autosave
 * journal: a reload brings it back in Document Recovery (or, with recovery off, says what is lost).
 */
function Stopped({ app }: { app: AppId }) {
  const cost = useReloadCost();
  const name = app === 'hub' ? 'Kiln' : APP_NAMES[app];
  return (
    <ProblemCard
      app={app}
      failure="stopped"
      title={t('{app} stopped working', { app: name })}
      text={
        cost === 'asks'
          ? t('Reload Kiln to go on. Changes made since the document was last saved could not be kept.')
          : t('Your work is kept. Reload Kiln, and it comes back in Document Recovery.')
      }
      reload
    />
  );
}

/** "Kiln has been updated", while the window itself says it (in place of the corner notice). */
function Updated({ app }: { app: AppId }) {
  const cost = useReloadCost();
  useEffect(() => {
    setPwaState({ stale: true });
    return () => setPwaState({ stale: false });
  }, []);
  return (
    <ProblemCard
      app={app}
      failure="updated"
      title={t('Kiln has been updated')}
      text={updateMessage(cost)}
      reload
    />
  );
}

/** The state for a {@link LoadFailure}. */
export function FailureCard({ app, failure }: { app: AppId; failure: LoadFailure }) {
  if (failure === 'updated') return <Updated app={app} />;
  if (failure === 'stopped') return <Stopped app={app} />;
  const name = app === 'hub' ? 'Kiln' : APP_NAMES[app];
  return failure === 'offline' ? (
    <ProblemCard
      app={app}
      failure={failure}
      title={t('{app} is not on this device yet', { app: name })}
      text={t('Connect to the internet once, and it opens offline from then on.')}
      reload={false}
    />
  ) : (
    <ProblemCard
      app={app}
      failure={failure}
      title={t('{app} could not start', { app: name })}
      text={t('Reload Kiln and try again.')}
      reload
    />
  );
}

function FailureScreen({ app }: { app: AppId }) {
  const [failure, setFailure] = useState<LoadFailure | null>(null);
  useEffect(() => {
    void classifyFailure().then(setFailure);
  }, []);
  return (
    <main className="kiln-load-screen">{failure ? <FailureCard app={app} failure={failure} /> : null}</main>
  );
}

/** Catches a screen whose code would not load (a chunk gone after a deployment). */
export class LoadBoundary extends Component<
  { app: AppId; hash: string; children: ReactNode },
  { failedOn: string | null }
> {
  override state = { failedOn: null as string | null };

  static getDerivedStateFromError(): { failedOn: string } {
    return { failedOn: window.location.hash };
  }

  override componentDidCatch(error: unknown): void {
    console.error('Kiln: a screen did not load', error);
  }

  override render(): ReactNode {
    // Another route may load: Kiln Home is always there.
    if (this.state.failedOn === null || this.state.failedOn !== this.props.hash) return this.props.children;
    return <FailureScreen app={this.props.app} />;
  }
}
