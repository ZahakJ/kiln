/**
 * Storage in the app frame: the Document Recovery pane (docked on the left, as Word
 * shows it, when the core finds documents a crash or a closed tab left unsaved), the title bar's
 * AutoSave switch, the question when another program changed the open file, the question before
 * a document with unsaved changes closes, the recent files for Backstage, and a notice for every
 * failure the core reports for the document or for the app storage (with the action that fixes
 * it, where there is one: the browser's permission to write a picked file again). The core does
 * the work (`suite.file.recovery_*`, `suite.file.autosave`, `suite.file.reload`,
 * `suite.file.keep_mine`, `suite.file.close_without_saving`, `Query::Recents`).
 *
 * AutoSave writes a file from one window at a time: turning it on takes a Web Lock on the file
 * (`kiln-autosave:<path>`), and a second tab with the same file open is told AutoSave is on in
 * the other window (its own saves are still checked against the file's stamp by the core, so
 * neither window writes over the other's changes without asking).
 */
import { t } from '@kiln/i18n';
import {
  type Command,
  type DocSummary,
  FONTS_PENDING,
  type KilnClient,
  KilnError,
  type RecentFile,
  type RecoveryEntry,
  type RecoveryNote,
} from '@kiln/protocol';
import {
  Button,
  DocumentRecoveryPane,
  errorText,
  FileChangedDialog,
  Icon,
  loadOptions,
  SaveChangesDialog,
  TaskPaneHost,
} from '@kiln/ui-kit';
import { Check, Copy, Info } from 'lucide-react';
import {
  type ReactNode,
  useCallback,
  useEffect,
  useMemo,
  useRef,
  useState,
  useSyncExternalStore,
} from 'react';
import { appHash } from '../../App.tsx';
import { showRoute } from '../../desktop.ts';
import { FILE_PERMISSION, requestFilePermission, TAIL_TROUBLE } from '../../host/index.ts';

/**
 * What a recovered document did not get back, by document: the core says so while it opens the
 * document, before its window shows it.
 */
const recoveredNotes = new Map<number, RecoveryNote>();

/** The name of a change a recovery stopped before. */
function changeName(command: string | null): string {
  if (command === 'collab.remote') return t('the edits of others');
  if (command === 'journal.unrepeatable') return t('a change Kiln could not repeat');
  if (!command) return t('a change that was not written');
  const label = t(`cmd.${command}`);
  return label === `cmd.${command}` ? command : label;
}

/**
 * What the recovery scan found, for the whole page: the frame of a recovered document (another
 * route) shows the rest of the list, and the scan runs once per page.
 */
const recovery = {
  entries: [] as RecoveryEntry[],
  dismissed: false,
  scanned: false,
  listeners: new Set<() => void>(),
  set(entries: RecoveryEntry[]) {
    this.entries = entries;
    for (const l of this.listeners) l();
  },
  dismiss() {
    this.dismissed = true;
    for (const l of this.listeners) l();
  },
};

function useRecoveryEntries(): { entries: RecoveryEntry[]; dismissed: boolean } {
  const subscribe = useCallback((l: () => void) => {
    recovery.listeners.add(l);
    return () => recovery.listeners.delete(l);
  }, []);
  const entries = useSyncExternalStore(subscribe, () => recovery.entries);
  const dismissed = useSyncExternalStore(subscribe, () => recovery.dismissed);
  return { entries, dismissed };
}

/** The recent files: the page's storage on the web, the core's list on the desktop. */
export async function loadRecents(client: KilnClient): Promise<RecentFile[]> {
  if (client.bridge === 'tauri') await client.execute(null, { Suite: 'RecentsLoad' });
  const r = await client.query(null, 'Recents');
  return typeof r === 'object' && 'Recents' in r ? r.Recents : [];
}

const APP_OF_KIND: Record<string, string> = { Write: 'write', Grid: 'grid', Deck: 'deck', Notes: 'notes' };

/** The core's errors that concern the app storage rather than one document. */
const STORAGE_ERRORS = ['Kiln could not write its app storage', 'Document Recovery:'];

/** A message over the document, with the action that answers it. */
interface Notice {
  /** One notice per kind of failure (a later one of the same kind replaces it). */
  kind: string;
  message: string;
  action?: { label: string; run: () => void };
  /** Text the notice keeps for the user to copy (an entry that could not go back). */
  copy?: string;
  /** The notice takes the keys as it appears (Put it back, so the button is a key away). */
  focus?: boolean;
}

/**
 * A reference or a typed text inside a sentence, isolated (U+2068 … U+2069) so a right-to-left
 * sentence keeps `ورقة2!F8` and `=SUM(B2:B9)` in their own order.
 */
export const isolate = (s: string): string => `\u2068${s}\u2069`;

/** One notice of the message bar: its words, the text it keeps to copy, its answers. */
function NoticeRow({ notice: n, onDismiss }: { notice: Notice; onDismiss: () => void }) {
  const [copied, setCopied] = useState<'done' | 'failed' | null>(null);
  useEffect(() => {
    if (copied !== 'done') return;
    const timer = setTimeout(() => setCopied(null), 2500);
    return () => clearTimeout(timer);
  }, [copied]);
  const copy = () => {
    const text = n.copy ?? '';
    const write = navigator.clipboard?.writeText(text);
    if (!write) {
      setCopied('failed');
      return;
    }
    write.then(
      () => setCopied('done'),
      () => setCopied('failed'),
    );
  };
  return (
    <div
      className="kiln-messagebar__row"
      role="alert"
      aria-label={t('Notice')}
      data-testid="storage-notice"
      data-notice={n.kind}
    >
      <Icon icon={Info} size={16} className="kiln-messagebar__icon" />
      <div className="kiln-messagebar__body">
        <span className="kiln-messagebar__message">{n.message}</span>
        {n.copy !== undefined ? (
          <output className="kiln-messagebar__text" dir="auto" data-testid="storage-notice-text">
            {n.copy}
          </output>
        ) : null}
        {copied ? (
          <span className="kiln-messagebar__status" role="status" data-testid="storage-notice-copy-status">
            {copied === 'done'
              ? t('Copied.')
              : t('Kiln could not copy the text. Select it and press Ctrl+C.')}
          </span>
        ) : null}
      </div>
      <div className="kiln-messagebar__actions">
        {n.copy !== undefined ? (
          <Button size="sm" icon={<Icon icon={copied === 'done' ? Check : Copy} size={14} />} onClick={copy}>
            {copied === 'done' ? t('Copied') : t('Copy')}
          </Button>
        ) : null}
        {n.action ? (
          <Button size="sm" variant="primary" onClick={n.action.run} data-notice-action="">
            {n.action.label}
          </Button>
        ) : null}
        <Button size="sm" variant="ghost" onClick={onDismiss}>
          {t('Dismiss')}
        </Button>
      </div>
    </div>
  );
}

/**
 * What a Document Recovery failure says: written words for what happened, from the kind of the
 * core's error, never the core's own text (which names files, keys and host errors).
 */
export function recoveryFailure(
  what: 'open' | 'delete' | 'scan' | 'journal' | 'storage',
  kind: string | undefined,
  name?: string,
): string {
  if (what === 'journal')
    return t('Kiln could not keep a recovery copy of {name}. Save the document to keep your work.', {
      name: isolate(name ?? ''),
    });
  if (what === 'storage')
    return t('Kiln could not write its recovery copies. Save your documents to keep your work.');
  if (kind === 'NotFound')
    return t('That recovery copy is no longer here: another Kiln window may have opened or deleted it.');
  if (kind === 'Format') return t('That recovery copy is damaged and cannot be opened. You can delete it.');
  if (what === 'delete')
    return t('Kiln could not delete that recovery copy. Close Kiln’s other windows and try again.');
  return t('Kiln could not open that recovery copy. Close Kiln’s other windows and try again.');
}

/** What Put it back says when the text cannot go back: in words, the text kept to copy. */
export function putBackFailure(kind: string | undefined, cell: string): string {
  const at = isolate(cell);
  return kind === 'NotFound'
    ? t('{cell} is no longer in this workbook: its sheet was deleted. Here is the text, to copy.', {
        cell: at,
      })
    : t('Kiln could not put the text back into {cell}. Here it is, to copy.', { cell: at });
}

/** The core's "… is not available: why" without the command id. */
function reason(message: string): string {
  return message.replace(/^.*?is not available: /, '');
}

/** An error's words for a notice: a fault inside Kiln in written words, else the core's reason. */
function written(error: { kind: string; message: string }): string {
  return error.kind === 'Internal' ? errorText(error) : reason(error.message);
}

/** The kind of a failure the core reported, for one notice per kind. */
export function kindOf(message: string): string {
  if (message.includes('AutoSave')) return 'autosave';
  if (message.includes('recovery copy')) return 'journal';
  if (message.startsWith('Document Recovery')) return 'recovery';
  if (message.includes('app storage')) return 'storage';
  return message;
}

/** Calls `release` when the AutoSave lock of a file is held by this window. */
type Release = () => void;

/** Takes the AutoSave lock of a file unless another window holds it. */
function lockFile(path: string): Promise<Release | null> {
  const locks = (navigator as Navigator & { locks?: LockManager }).locks;
  if (!locks) return Promise.resolve(() => undefined);
  return new Promise((resolve) => {
    let release: Release = () => undefined;
    const held = new Promise<void>((done) => {
      release = done;
    });
    locks
      .request(`kiln-autosave:${path}`, { ifAvailable: true }, (lock) => {
        if (!lock) {
          resolve(null);
          return undefined;
        }
        resolve(release);
        return held;
      })
      .catch(() => resolve(null));
  });
}

export interface StorageParts {
  /** The Document Recovery pane, while there is something to recover. */
  pane: ReactNode;
  /** The title bar's AutoSave switch. */
  autoSave: { enabled: boolean; onChange: (enabled: boolean) => void; disabledReason?: string } | undefined;
  /** The message bar's notices (failures, AutoSave refused or turned off, what recovery did). */
  messageBar: ReactNode;
  /** The questions: another program changed the file; save before closing. */
  dialogs: ReactNode;
  /**
   * Asked before a command runs (`connectCore`'s guard): Close of a document with unsaved
   * changes asks whether to save them first.
   */
  guard: (id: string, doc: DocSummary | null) => Promise<Command | null | undefined> | undefined;
}

export function useStorage(deps: {
  client: KilnClient | null;
  summary: DocSummary | null;
  appName: string;
}): StorageParts {
  const { client, summary, appName } = deps;
  const doc = summary?.doc ?? null;
  const path = summary?.path ?? null;
  const name = path?.split(/[/\\]/).pop() ?? summary?.title ?? '';
  const { entries, dismissed } = useRecoveryEntries();
  const [notices, setNotices] = useState<Notice[]>([]);
  const [changed, setChanged] = useState(false);
  const opening = useRef<{ saveAs: boolean } | null>(null);
  // A write of the document failed since it was last saved (Close then asks even with AutoSave on).
  const failed = useRef(false);

  const [notesFor, setNotesFor] = useState(0);
  const notify = useCallback((n: Notice) => {
    setNotices((all) => [...all.filter((o) => o.kind !== n.kind), n]);
  }, []);
  const dismiss = useCallback((kind: string) => setNotices((all) => all.filter((n) => n.kind !== kind)), []);
  // Fonts a document needs could not be fetched (offline): one quiet notice until they arrive.
  useEffect(() => {
    const pending = () =>
      notify({
        kind: 'fonts',
        message: t('Some characters need a download and appear once you are back online.'),
      });
    const added = () => dismiss('fonts');
    globalThis.addEventListener(FONTS_PENDING, pending);
    globalThis.addEventListener('kiln:fonts-added', added);
    return () => {
      globalThis.removeEventListener(FONTS_PENDING, pending);
      globalThis.removeEventListener('kiln:fonts-added', added);
    };
  }, [notify, dismiss]);

  // The list from the core, the recovered document's window once it opens, and every failure.
  useEffect(() => {
    if (!client) return;
    const stop = client.subscribe((e) => {
      if (typeof e !== 'object') return;
      if ('Recovery' in e) recovery.set(e.Recovery.entries);
      else if ('Recovered' in e) {
        recoveredNotes.set(e.Recovered.doc, e.Recovered.note);
        if (e.Recovered.doc === doc) setNotesFor((n) => n + 1);
      } else if ('Opened' in e && opening.current) {
        const { saveAs } = opening.current;
        opening.current = null;
        const s = e.Opened.summary;
        if (saveAs)
          void client
            .execute(s.doc, { Suite: { SaveAs: { path: null, format: null } } })
            .catch(() => undefined);
        showRoute(appHash((APP_OF_KIND[s.kind] ?? 'write') as Parameters<typeof appHash>[0], s.doc));
      } else if ('FileChanged' in e && e.FileChanged.doc === doc) setChanged(true);
      else if ('Saved' in e && e.Saved.summary.doc === doc) {
        failed.current = false;
        setNotices((all) => all.filter((n) => n.kind !== 'permission' && n.kind !== 'save'));
      } else if ('Error' in e) {
        const message = e.Error.error.message;
        const ours = e.Error.doc !== null && e.Error.doc === doc;
        const storage = e.Error.doc === null && STORAGE_ERRORS.some((p) => message.startsWith(p));
        if (!ours && !storage) return;
        if (ours) failed.current = true;
        // A write the browser has not allowed has its own notice, with the button that asks.
        if (message.includes('needs your permission')) return;
        const kind = kindOf(message);
        // Recovery speaks in written words: the core's text never shows.
        if (kind === 'recovery') notify({ kind, message: recoveryFailure('open', e.Error.error.kind) });
        else if (kind === 'journal') notify({ kind, message: recoveryFailure('journal', undefined, name) });
        else if (kind === 'storage') notify({ kind, message: recoveryFailure('storage', undefined) });
        else notify({ kind, message: written(e.Error.error) });
      }
    });
    if (!recovery.scanned) {
      recovery.scanned = true;
      void client.execute(null, { Suite: 'RecoveryScan' }).catch(() => undefined);
    }
    return stop;
  }, [client, doc, notify, name]);

  // The browser's storage for the site is full, or holds work of a later Kiln.
  useEffect(() => {
    const onTrouble = (event: Event) => {
      const reason = (event as CustomEvent<{ reason: 'full' | 'newer' }>).detail?.reason;
      notify({
        kind: 'recovery',
        message:
          reason === 'full'
            ? t(
                'The browser has no more room for this site, so your latest changes may not survive a crash. Save your work.',
              )
            : t('A newer version of Kiln left a recovery copy here. Update Kiln to get it back.'),
      });
    };
    window.addEventListener(TAIL_TROUBLE, onTrouble);
    return () => window.removeEventListener(TAIL_TROUBLE, onTrouble);
  }, [notify]);

  // What a recovered document did not get back: said once, and the text that was being typed
  // offered, never put back on its own.
  useEffect(() => {
    if (!client || doc === null || notesFor < 0) return;
    const note = recoveredNotes.get(doc);
    if (!note) return;
    recoveredNotes.delete(doc);
    if (note.stopped) {
      const { command, after } = note.stopped;
      notify({
        kind: 'recovered-stop',
        message:
          after > 0
            ? t(
                'This document is back as it was before {change}. That change and {count, plural, one {# change} other {# changes}} after it could not be kept.',
                { change: changeName(command), count: after },
              )
            : t('This document is back as it was before {change}. That change could not be kept.', {
                change: changeName(command),
              }),
      });
    }
    if (note.draft) {
      const { cell, text, sheet, row, col } = note.draft;
      notify({
        kind: 'recovered-draft',
        message: t('“{text}” was being typed in {cell} and was not entered.', {
          text: isolate(text),
          cell: isolate(cell),
        }),
        focus: true,
        action: {
          label: t('Put it back'),
          run: () => {
            dismiss('recovered-draft');
            // By the sheet's id: a rename since does not lose it.
            void client
              .execute(doc, { Grid: { PutBackDraft: { sheet, row, col, text } } })
              .then(() => window.dispatchEvent(new CustomEvent('kiln:return-to-document')))
              .catch((error: unknown) =>
                notify({
                  kind: 'recovered-draft',
                  message: putBackFailure(KilnError.from(error).kind, cell),
                  copy: text,
                }),
              );
          },
        },
      });
    }
  }, [client, doc, notesFor, notify, dismiss]);

  // A picked file the browser no longer lets the page write: one click asks again, then saves.
  useEffect(() => {
    if (!client || doc === null || !path) return;
    const onPermission = (event: Event) => {
      if ((event as CustomEvent<{ path: string }>).detail?.path !== path) return;
      failed.current = true;
      notify({
        kind: 'permission',
        message: t('Kiln needs your permission to save to {name}.', { name }),
        action: {
          label: t('Allow Kiln to save to {name}', { name }),
          run: () => {
            void requestFilePermission(path).then((granted) => {
              if (!granted) return;
              dismiss('permission');
              void client.execute(doc, { Suite: 'Save' }).catch((error: unknown) => {
                notify({ kind: 'save', message: errorText(KilnError.from(error)) });
              });
            });
          },
        },
      });
    };
    window.addEventListener(FILE_PERMISSION, onPermission);
    return () => window.removeEventListener(FILE_PERMISSION, onPermission);
  }, [client, doc, path, name, notify, dismiss]);

  const open = useCallback(
    async (e: RecoveryEntry, saveAs: boolean) => {
      if (!client) return;
      opening.current = { saveAs };
      try {
        await client.execute(null, { Suite: { RecoveryOpen: { key: e.key, kind: e.kind } } });
      } catch (error) {
        opening.current = null;
        notify({ kind: 'recovery', message: recoveryFailure('open', KilnError.from(error).kind) });
      }
    },
    [client, notify],
  );

  const pane =
    entries.length > 0 && !dismissed ? (
      <TaskPaneHost
        side="left"
        panes={[
          {
            id: 'recovery',
            title: t('Document Recovery'),
            content: (
              <DocumentRecoveryPane
                entries={entries}
                onOpen={(e) => void open(e, false)}
                onSaveAs={(e) => void open(e, true)}
                onDelete={(e) => {
                  if (!client) return;
                  void client
                    .execute(null, { Suite: { RecoveryDelete: { key: e.key } } })
                    .catch((error: unknown) =>
                      notify({
                        kind: 'recovery',
                        message: recoveryFailure('delete', KilnError.from(error).kind),
                      }),
                    );
                }}
                onClose={() => recovery.dismiss()}
              />
            ),
          },
        ]}
        onClose={() => recovery.dismiss()}
        defaultWidth={300}
      />
    ) : null;

  // The AutoSave lock of the file, while AutoSave is on in this window.
  const release = useRef<Release | null>(null);
  useEffect(
    () => () => {
      release.current?.();
      release.current = null;
    },
    [path],
  );

  const state = summary?.autosave;
  const autoSave = useMemo(() => {
    // A document without a file shows its save state only (as Office's title bar does before the
    // first save); the switch appears once there is a file to save to.
    if (!summary?.path || !client || state === undefined) return undefined;
    const enabled = state === 'On';
    const disabledReason = typeof state === 'object' ? state.Unavailable : undefined;
    const filePath = summary.path;
    return {
      enabled,
      disabledReason,
      onChange: (on: boolean) => {
        dismiss('autosave');
        const set = async () => {
          if (on && !release.current) {
            const got = await lockFile(filePath);
            if (!got) {
              notify({
                kind: 'autosave',
                message: t(
                  'AutoSave is on for {name} in another window: only one window saves it automatically.',
                  { name },
                ),
              });
              return;
            }
            release.current = got;
          }
          try {
            await client.execute(summary.doc, { Suite: { SetAutoSave: { on } } });
            if (!on) {
              release.current?.();
              release.current = null;
            }
          } catch (error) {
            if (on) {
              release.current?.();
              release.current = null;
            }
            notify({ kind: 'autosave', message: written(KilnError.from(error).toInfo()) });
          }
        };
        void set();
      },
    };
  }, [summary, client, state, name, notify, dismiss]);

  // Options ▸ Save ▸ Turn on AutoSave when a file opens: once per file, when its switch
  // can be turned on (the user may turn it off again).
  const autoStarted = useRef(new Set<string>());
  useEffect(() => {
    if (!autoSave || autoSave.enabled || autoSave.disabledReason || !path) return;
    if (!loadOptions().save.autoSave || autoStarted.current.has(path)) return;
    autoStarted.current.add(path);
    autoSave.onChange(true);
  }, [autoSave, path]);

  const noticesRef = useRef<HTMLDivElement | null>(null);

  // A notice that asks for an answer takes the keys as it appears: the button is focused and
  // the notice read out; F6 comes back to it (it is a pane region of the frame).
  const focusWanted = notices.find((n) => n.focus)?.kind;
  useEffect(() => {
    if (!focusWanted) return;
    const box = noticesRef.current;
    // After the document's own first focus, which a window that just opened gives its editor.
    const frame = requestAnimationFrame(() =>
      requestAnimationFrame(() =>
        box
          ?.querySelector<HTMLButtonElement>(
            `[data-notice="${CSS.escape(focusWanted)}"] [data-notice-action]`,
          )
          ?.focus(),
      ),
    );
    return () => cancelAnimationFrame(frame);
  }, [focusWanted]);

  // Docked under the ribbon, full width, pushing the document down: a notice never covers the
  // grid's headings, its cells or the active cell, on a desktop or a phone.
  const messageBar =
    notices.length > 0 ? (
      <div className="kiln-messagebar" ref={noticesRef}>
        {notices.map((n) => (
          <NoticeRow key={n.kind} notice={n} onDismiss={() => dismiss(n.kind)} />
        ))}
      </div>
    ) : null;

  // Close of a document with unsaved changes: the answer of the question, once given.
  const [closing, setClosing] = useState(false);
  const answer = useRef<((c: Command | null | undefined) => void) | null>(null);
  const settle = useCallback((c: Command | null | undefined) => {
    setClosing(false);
    const resolve = answer.current;
    answer.current = null;
    resolve?.(c);
  }, []);
  useEffect(() => () => settle(null), [settle]);
  const guard = useCallback(
    (id: string, s: DocSummary | null): Promise<Command | null | undefined> | undefined => {
      if (id !== 'suite.file.close' || !s?.dirty) return undefined;
      // AutoSave writes the changes before the document closes (the core waits for it).
      if (s.autosave === 'On' && !failed.current) return undefined;
      answer.current?.(null);
      setClosing(true);
      return new Promise((resolve) => {
        answer.current = resolve;
      });
    },
    [],
  );
  const saveThenClose = useCallback(() => {
    if (!client || doc === null) return settle(null);
    setClosing(false);
    const resolve = answer.current;
    answer.current = null;
    // Close once the file is written; a failed save leaves the document open, and so does a
    // cancelled Save As dialog (the user goes on working: the next key or click ends the wait).
    let unsubscribe: () => void = () => undefined;
    let ended = false;
    const end = (c: Command | null | undefined) => {
      ended = true;
      unsubscribe();
      window.removeEventListener('keydown', resumed, true);
      window.removeEventListener('pointerdown', resumed, true);
      resolve?.(c);
    };
    const resumed = () => end(null);
    unsubscribe = client.subscribe((e) => {
      if (typeof e !== 'object') return;
      if ('Saved' in e && e.Saved.summary.doc === doc) end(undefined);
      else if ('Error' in e && e.Error.doc === doc) end(null);
    });
    setTimeout(() => {
      if (ended) return;
      window.addEventListener('keydown', resumed, true);
      window.addEventListener('pointerdown', resumed, true);
    }, 0);
    void client.execute(doc, { Suite: 'Save' }).catch((error: unknown) => {
      end(null);
      if (KilnError.from(error).kind !== 'Cancelled')
        notify({ kind: 'save', message: errorText(KilnError.from(error)) });
    });
  }, [client, doc, settle, notify]);

  const dialogs = (
    <>
      <FileChangedDialog
        open={changed}
        name={name}
        appName={appName}
        dirty={!!summary?.dirty}
        onReload={() => {
          setChanged(false);
          if (client && doc !== null)
            void client.execute(doc, { Suite: 'Reload' }).catch((error: unknown) => {
              notify({ kind: 'reload', message: errorText(KilnError.from(error)) });
            });
        }}
        onKeep={() => {
          setChanged(false);
          if (client && doc !== null)
            void client.execute(doc, { Suite: 'KeepMine' }).catch((error: unknown) => {
              notify({ kind: 'save', message: errorText(KilnError.from(error)) });
            });
        }}
        onDismiss={() => setChanged(false)}
      />
      <SaveChangesDialog
        open={closing}
        name={name}
        appName={appName}
        onSave={saveThenClose}
        onDontSave={() => settle({ Suite: 'CloseWithoutSaving' })}
        onCancel={() => settle(null)}
      />
    </>
  );

  return { pane, autoSave, messageBar, dialogs, guard };
}
