/**
 * File ▸ Share (Office's Share dialog for live collaboration): before sharing, the server, its
 * token and the name the others see; once shared, the link to send (anyone with it can edit),
 * who is in the document, the connection, and Stop sharing. Opening a link asks for the name the
 * others will see ({@link JoinDialog}).
 */
import { t } from '@kiln/i18n';
import type { CollabPeer, CollabState } from '@kiln/protocol';
import { Check, Copy, Link2 } from 'lucide-react';
import { type FormEvent, useEffect, useId, useRef, useState } from 'react';
import { Icon } from '../icons/Icon.tsx';
import { Button } from '../primitives/Button.tsx';
import { Dialog, DialogContent } from '../primitives/Dialog.tsx';
import { peerStyle } from './PresenceAvatars.tsx';
import './collab.css';

/** What the Share dialog starts with (remembered from the last share). */
export interface ShareDefaults {
  server: string;
  token: string;
  name: string;
}

export interface ShareDialogProps {
  open: boolean;
  onOpenChange: (open: boolean) => void;
  /** The document's title. */
  title: string;
  /** The document's collaboration state (`null` until known). */
  state: CollabState | null;
  defaults: ShareDefaults;
  /** The link to the shared document (`…#/write?doc=…&server=…&token=…`). */
  link: string | null;
  /** Starts sharing; rejects with a message to show. */
  onShare: (settings: ShareDefaults) => Promise<void>;
  onStop: () => void;
  /** Copies the link; resolves once it is on the clipboard. */
  onCopy: (link: string) => Promise<void>;
}

function statusText(state: CollabState): string {
  const n = state.peers.length;
  switch (state.status) {
    case 'Online':
      return t('collab.status.online', { count: n });
    case 'Connecting':
      return t('Connecting to the server…');
    case 'Offline':
      return state.queued > 0
        ? t('collab.status.offlineQueued', { count: state.queued })
        : t('Offline. Your changes are kept and sent when the connection is back.');
    case 'Refused':
      return state.problem
        ? t('The server refused the document: {problem}', { problem: state.problem })
        : t('The server refused the document.');
  }
}

export function ShareDialog({
  open,
  onOpenChange,
  title,
  state,
  defaults,
  link,
  onShare,
  onStop,
  onCopy,
}: ShareDialogProps) {
  const id = useId();
  const [server, setServer] = useState(defaults.server);
  const [token, setToken] = useState(defaults.token);
  const [name, setName] = useState(defaults.name);
  const [busy, setBusy] = useState(false);
  const [error, setError] = useState<string | null>(null);
  const [copied, setCopied] = useState(false);
  const linkRef = useRef<HTMLInputElement>(null);
  useEffect(() => {
    if (!open) return;
    setServer(defaults.server);
    setToken(defaults.token);
    setName(defaults.name);
    setError(null);
    setCopied(false);
  }, [open, defaults.server, defaults.token, defaults.name]);
  const shared = !!state?.shared;
  const serverOk = /^wss?:\/\/\S+$/i.test(server.trim());

  const submit = async (e: FormEvent) => {
    e.preventDefault();
    if (!serverOk || busy) return;
    setBusy(true);
    setError(null);
    try {
      await onShare({ server: server.trim(), token, name: name.trim() });
    } catch (err) {
      setError(err instanceof Error ? err.message : String(err));
    } finally {
      setBusy(false);
    }
  };

  const copy = async () => {
    if (!link) return;
    try {
      await onCopy(link);
      setCopied(true);
    } catch {
      linkRef.current?.select();
    }
  };

  return (
    <Dialog open={open} onOpenChange={onOpenChange}>
      <DialogContent
        title={t('Share “{title}”', { title })}
        description={
          shared
            ? t('Anyone with the link can open and edit this document.')
            : t('Edit together in real time: everyone you send the link to can open and edit this document.')
        }
        size="md"
        data-testid="share-dialog"
        data-kiln-shortcuts="off"
        footerStart={
          shared ? (
            <Button variant="danger" onClick={onStop} data-testid="share-stop">
              {t('Stop sharing')}
            </Button>
          ) : null
        }
        footer={
          shared ? (
            <Button variant="primary" onClick={() => onOpenChange(false)}>
              {t('Done')}
            </Button>
          ) : (
            <>
              <Button
                variant="primary"
                type="submit"
                form={`${id}-form`}
                disabled={!serverOk}
                busy={busy}
                data-testid="share-start"
              >
                {t('Share')}
              </Button>
              <Button onClick={() => onOpenChange(false)}>{t('Cancel')}</Button>
            </>
          )
        }
      >
        {shared && state ? (
          <div className="kiln-share">
            <label className="kiln-share__label" htmlFor={`${id}-link`}>
              {t('Link')}
            </label>
            <div className="kiln-share__link">
              <span className="kiln-field kiln-share__link-field">
                <Icon icon={Link2} size={14} />
                <input
                  id={`${id}-link`}
                  ref={linkRef}
                  readOnly
                  value={link ?? ''}
                  onFocus={(e) => e.currentTarget.select()}
                  data-testid="share-link"
                />
              </span>
              <Button
                variant="primary"
                icon={<Icon icon={copied ? Check : Copy} size={14} />}
                onClick={() => void copy()}
                data-testid="share-copy"
              >
                {copied ? t('Copied') : t('Copy link')}
              </Button>
            </div>
            <p
              className={`kiln-share__status kiln-share__status--${state.status.toLowerCase()}`}
              role="status"
              data-testid="share-status"
            >
              {statusText(state)}
            </p>
            {state.peers.length > 0 ? (
              <ul className="kiln-share__people" aria-label={t('People in this document')}>
                {state.peers.map((p: CollabPeer) => (
                  <li key={p.peer}>
                    <span
                      className="kiln-presence__avatar kiln-share__avatar"
                      style={peerStyle(p.colour)}
                      aria-hidden="true"
                    >
                      {p.initials}
                    </span>
                    <span>{p.name}</span>
                  </li>
                ))}
              </ul>
            ) : null}
            <p className="kiln-share__note">
              {t(
                'The token in the link lets anyone who has it into the server; send it only to the people you work with.',
              )}
            </p>
          </div>
        ) : (
          <form id={`${id}-form`} className="kiln-share kiln-share__form" onSubmit={(e) => void submit(e)}>
            <label className="kiln-share__label" htmlFor={`${id}-name`}>
              {t('Your name')}
            </label>
            <input
              id={`${id}-name`}
              className="kiln-field"
              value={name}
              maxLength={64}
              autoComplete="name"
              placeholder={t('Shown to the people you work with')}
              onChange={(e) => setName(e.target.value)}
              data-testid="share-name"
            />
            <label className="kiln-share__label" htmlFor={`${id}-server`}>
              {t('Collaboration server')}
            </label>
            <input
              id={`${id}-server`}
              className={`kiln-field${server && !serverOk ? ' is-invalid' : ''}`}
              value={server}
              spellCheck={false}
              placeholder={t('wss://collab.example.org')}
              aria-invalid={!!server && !serverOk}
              onChange={(e) => setServer(e.target.value)}
              data-testid="share-server"
            />
            <label className="kiln-share__label" htmlFor={`${id}-token`}>
              {t('Access token')}
            </label>
            <input
              id={`${id}-token`}
              className="kiln-field"
              type="password"
              value={token}
              spellCheck={false}
              autoComplete="off"
              onChange={(e) => setToken(e.target.value)}
              data-testid="share-token"
            />
            <p className="kiln-share__note">
              {t(
                "Kiln's collaboration server relays the edits and keeps the document's versions. You can run your own; the manual's Collaboration page says how.",
              )}
            </p>
            {error ? (
              <p className="kiln-share__error" role="alert">
                {error}
              </p>
            ) : null}
          </form>
        )}
      </DialogContent>
    </Dialog>
  );
}

export interface JoinDialogProps {
  open: boolean;
  /** The server of the link. */
  server: string;
  defaultName: string;
  onJoin: (name: string) => void;
  onCancel: () => void;
}

/** Opening a shared document's link: the name the others will see. */
export function JoinDialog({ open, server, defaultName, onJoin, onCancel }: JoinDialogProps) {
  const id = useId();
  const [name, setName] = useState(defaultName);
  useEffect(() => {
    if (open) setName(defaultName);
  }, [open, defaultName]);
  const host = (() => {
    try {
      return new URL(server).host;
    } catch {
      return server;
    }
  })();
  return (
    <Dialog
      open={open}
      onOpenChange={(o) => {
        if (!o) onCancel();
      }}
    >
      <DialogContent
        title={t('Open a shared document')}
        description={t(
          'Someone shared a document with you through {host}. You edit it together, and they see your name and where you are.',
          { host },
        )}
        size="sm"
        data-testid="join-dialog"
        data-kiln-shortcuts="off"
        footer={
          <>
            <Button variant="primary" type="submit" form={`${id}-form`} data-testid="join-open">
              {t('Open')}
            </Button>
            <Button onClick={onCancel}>{t('Cancel')}</Button>
          </>
        }
      >
        <form
          id={`${id}-form`}
          className="kiln-share kiln-share__form"
          onSubmit={(e) => {
            e.preventDefault();
            onJoin(name.trim());
          }}
        >
          <label className="kiln-share__label" htmlFor={`${id}-name`}>
            {t('Your name')}
          </label>
          <input
            id={`${id}-name`}
            className="kiln-field"
            value={name}
            maxLength={64}
            autoComplete="name"
            onChange={(e) => setName(e.target.value)}
            data-testid="join-name"
          />
        </form>
      </DialogContent>
    </Dialog>
  );
}
