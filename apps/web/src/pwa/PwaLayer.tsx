/**
 * The installable web app's own notices, over every screen of the Pages build:
 *
 * - "Kiln has been updated", with Reload (and Later) when a new version is waiting, and the
 *   save-changes question Reload asks when unsaved work has no recovery copy;
 * - the offline badge while the network is gone, and "Back online" when it returns;
 * - Kiln's install card in place of the browser's bar, on Kiln Home, until it is installed or
 *   declined;
 * - a file from a launch or a share that no web core opens.
 */
import { t } from '@kiln/i18n';
import type { KilnClient } from '@kiln/protocol';
import { AppTile, Button, Icon, SaveChangesDialog } from '@kiln/ui-kit';
import { CheckCircle2, CloudOff, Download, FileWarning, RefreshCw, Wifi, X } from 'lucide-react';
import { useEffect, useState, useSyncExternalStore } from 'react';
import { createPortal } from 'react-dom';
import { getClient } from '../client/index.ts';
import { type UnsavedDocument, useReloadCost } from '../client/unsaved.ts';
import { updateMessage } from '../LoadProblem.tsx';
import { applyUpdate, reloadIntoUpdate, warmOfflineFiles } from './register.ts';
import { pwaState, remember, setPwaState, subscribePwa } from './state.ts';
import './pwa.css';

/** How long Kiln Home shows before the install card slides in. */
const INSTALL_DELAY_MS = 2500;

/** Saves one document; false when it did not save (a failure, or Save As cancelled). */
function saved(client: KilnClient, doc: number): Promise<boolean> {
  return new Promise((resolve) => {
    let stop: () => void = () => undefined;
    const end = (ok: boolean) => {
      stop();
      window.removeEventListener('keydown', resumed, true);
      window.removeEventListener('pointerdown', resumed, true);
      resolve(ok);
    };
    // A cancelled Save As leaves no event: the user going on with the document ends the wait.
    const resumed = () => end(false);
    stop = client.subscribe((event) => {
      if ('Saved' in event && event.Saved.summary.doc === doc) end(true);
      else if ('Error' in event && event.Error.doc === doc) end(false);
    });
    setTimeout(() => {
      window.addEventListener('keydown', resumed, true);
      window.addEventListener('pointerdown', resumed, true);
    }, 0);
    client.execute(doc, { Suite: 'Save' }).catch(() => end(false));
  });
}

/** Save, Don't Save or Cancel for each unsaved document, then the reload. */
function ReloadQuestion({ documents }: { documents: readonly UnsavedDocument[] }) {
  const [first, ...rest] = documents;
  if (!first) return null;
  const next = () => {
    setPwaState({ asking: rest });
    if (rest.length === 0) void applyUpdate();
  };
  const cancel = () => setPwaState({ asking: [] });
  return (
    <SaveChangesDialog
      open
      name={first.name}
      appName="Kiln"
      onSave={() => {
        setPwaState({ asking: [] });
        void getClient()
          .then((client) => saved(client, first.doc))
          .then(
            (ok) => {
              if (!ok) return;
              if (rest.length > 0) setPwaState({ asking: rest });
              else void applyUpdate();
            },
            () => undefined,
          );
      }}
      onDontSave={next}
      onCancel={cancel}
    />
  );
}

function UpdateNotice() {
  const [later, setLater] = useState(false);
  const [busy, setBusy] = useState(false);
  const cost = useReloadCost();
  if (later) return null;
  return (
    <div
      className="kiln-pwa-card kiln-pwa-card--update"
      role="alertdialog"
      aria-labelledby="kiln-pwa-update"
      data-testid="pwa-update"
    >
      <div className="kiln-pwa-card__art" aria-hidden="true">
        <AppTile app="home" size={40} />
        <span className="kiln-pwa-card__spin">
          <Icon icon={RefreshCw} size={14} />
        </span>
      </div>
      <div className="kiln-pwa-card__body">
        <div className="kiln-pwa-card__title" id="kiln-pwa-update">
          {t('Kiln has been updated')}
        </div>
        <div className="kiln-pwa-card__text">{updateMessage(cost)}</div>
        <div className="kiln-pwa-card__actions">
          <Button
            variant="ember"
            size="sm"
            busy={busy}
            data-testid="pwa-reload"
            onClick={() => setBusy(reloadIntoUpdate())}
          >
            {t('Reload')}
          </Button>
          <Button variant="ghost" size="sm" onClick={() => setLater(true)}>
            {t('Later')}
          </Button>
        </div>
      </div>
    </div>
  );
}

function InstallCard({ onHome }: { onHome: boolean }) {
  const { install } = useSyncExternalStore(subscribePwa, pwaState);
  const [shown, setShown] = useState(false);
  useEffect(() => {
    if (!install || !onHome) return;
    const timer = setTimeout(() => setShown(true), INSTALL_DELAY_MS);
    return () => clearTimeout(timer);
  }, [install, onHome]);
  if (!install || !onHome || !shown) return null;
  const decline = () => {
    remember('install-declined', '1');
    setPwaState({ install: null });
  };
  return (
    <div
      className="kiln-pwa-card kiln-pwa-card--install"
      role="dialog"
      aria-labelledby="kiln-pwa-install"
      data-testid="pwa-install"
    >
      <div className="kiln-pwa-card__art" aria-hidden="true">
        <AppTile app="home" size={40} />
      </div>
      <div className="kiln-pwa-card__body">
        <div className="kiln-pwa-card__title" id="kiln-pwa-install">
          {t('Install Kiln')}
        </div>
        <div className="kiln-pwa-card__text">
          {t(
            'Open Kiln from your desktop or home screen, work offline, and open Word, Excel and PowerPoint files with it.',
          )}
        </div>
        <div className="kiln-pwa-card__actions">
          <Button
            variant="ember"
            size="sm"
            icon={<Icon icon={Download} size={14} />}
            onClick={() => {
              void install
                .prompt()
                .then(() => install.userChoice)
                .then((choice) => {
                  setPwaState({ install: null });
                  if (choice.outcome === 'accepted') warmOfflineFiles();
                })
                .catch(() => setPwaState({ install: null }));
            }}
          >
            {t('Install')}
          </Button>
          <Button variant="ghost" size="sm" onClick={decline}>
            {t('Not now')}
          </Button>
        </div>
      </div>
      <button
        type="button"
        className="kiln-btn kiln-btn--ghost kiln-btn--sm kiln-btn--icon-only kiln-pwa-card__close"
        aria-label={t('Not now')}
        onClick={decline}
      >
        <Icon icon={X} size={14} />
      </button>
    </div>
  );
}

function NetworkBadge() {
  const { offline, backOnline, offlineReady } = useSyncExternalStore(subscribePwa, pwaState);
  if (offline)
    return (
      <div className="kiln-pwa-badge kiln-pwa-badge--offline" role="status" data-testid="pwa-network">
        <Icon icon={CloudOff} size={14} />
        <span className="kiln-pwa-badge__label">{t('Offline')}</span>
        <span className="kiln-pwa-badge__detail">{t('Your work is kept on this device')}</span>
      </div>
    );
  if (backOnline)
    return (
      <div className="kiln-pwa-badge" role="status" data-testid="pwa-network">
        <Icon icon={Wifi} size={14} />
        <span className="kiln-pwa-badge__label">{t('Back online')}</span>
      </div>
    );
  if (offlineReady)
    return (
      <div className="kiln-pwa-badge" role="status" data-testid="pwa-offline-ready">
        <Icon icon={CheckCircle2} size={14} />
        <span className="kiln-pwa-badge__label">{t('Kiln works offline now')}</span>
        <span className="kiln-pwa-badge__detail">{t('Write, Grid and Deck are saved on this device')}</span>
      </div>
    );
  return null;
}

function UnopenedNotice({ names }: { names: string[] }) {
  const name = names[names.length - 1] ?? '';
  return (
    <div className="kiln-pwa-card kiln-pwa-card--warning" role="alert" data-testid="pwa-unopened">
      <div className="kiln-pwa-card__icon" aria-hidden="true">
        <Icon icon={FileWarning} size={20} />
      </div>
      <div className="kiln-pwa-card__body">
        <div className="kiln-pwa-card__title">{t('Kiln could not open {name}', { name })}</div>
        <div className="kiln-pwa-card__text">
          {t(
            'In the browser Kiln opens Word, Excel and PowerPoint files, Kiln files, CSV, text and Markdown. The desktop app opens OpenDocument files too.',
          )}
        </div>
      </div>
      <button
        type="button"
        className="kiln-btn kiln-btn--ghost kiln-btn--sm kiln-btn--icon-only kiln-pwa-card__close"
        aria-label={t('Dismiss notification')}
        onClick={() => setPwaState({ unopened: [] })}
      >
        <Icon icon={X} size={14} />
      </button>
    </div>
  );
}

/** The notices, in one region pinned to the window's end corner (the bottom of a phone screen). */
export default function PwaLayer({ onHome }: { onHome: boolean }) {
  const state = useSyncExternalStore(subscribePwa, pwaState);
  return createPortal(
    <section className="kiln-pwa-region" aria-label={t('Notifications')}>
      <NetworkBadge />
      {state.unopened.length > 0 ? <UnopenedNotice names={state.unopened} /> : null}
      {state.update && !state.stale ? <UpdateNotice /> : <InstallCard onHome={onHome} />}
      <ReloadQuestion documents={state.asking} />
    </section>,
    document.body,
  );
}
