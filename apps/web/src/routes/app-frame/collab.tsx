/**
 * Collaboration and Version History in the app frame: File ▸ Share and the title bar's
 * Share, the presence avatars, the others' name flags over the document, the Version History
 * pane with its read-only previews, and opening a shared document's link
 * (`#/write?doc=<room>&server=<url>&token=<token>`). The core does the work (`Suite.Share`,
 * `Query.Collab`, …); on the web the collaboration module is loaded into the core's Worker first.
 */

import type { CommandRegistry } from '@kiln/commands';
import { t } from '@kiln/i18n';
import {
  type CollabPeer,
  type CollabState,
  type DocKind,
  type DocSummary,
  type KilnClient,
  KilnError,
  type Rect,
  type SceneDelta,
  type VersionEntry,
  type VersionParagraph,
  type VersionSlide,
  WebHost,
} from '@kiln/protocol';
import {
  Button,
  errorText,
  JoinDialog,
  type PlacedPeer,
  PresenceAvatars,
  RemoteFlags,
  type ShareDefaults,
  ShareDialog,
  TaskPaneHost,
  useTheme,
  VersionBar,
  VersionHistoryPane,
} from '@kiln/ui-kit';
import { type ReactNode, useCallback, useEffect, useMemo, useRef, useState } from 'react';
import { rememberShared } from '../home/shared.ts';
import type { ShareLink } from '../params.ts';
import type { ClientRect } from '../write/index.tsx';

/** The apps whose documents can be shared. */
const SHARING: readonly string[] = ['write', 'deck', 'notes'];

const KIND: Record<string, DocKind> = { write: 'Write', deck: 'Deck', notes: 'Notes' };

// The link's type and parser live with the other route parameters (`App.tsx` reads them before
// this module loads).
export { type ShareLink, shareLinkOf } from '../params.ts';

/** The link that opens a shared document in this app. */
export function linkFor(app: string, link: { room: string; server: string; token: string }): string {
  const base = typeof window === 'undefined' ? '' : `${window.location.origin}${window.location.pathname}`;
  const q = new URLSearchParams({ doc: link.room, server: link.server, token: link.token });
  return `${base}#/${app}?${q.toString()}`;
}

const prefs = new WebHost();
const PREF = { name: 'collab.name', server: 'collab.server', token: 'collab.token' };

async function loadPref(key: string): Promise<string> {
  const r = await prefs.handle({ LoadPref: { key } });
  return typeof r === 'object' && 'Text' in r ? r.Text : '';
}

async function storePref(key: string, value: string): Promise<void> {
  await prefs.handle({ StorePref: { key, value } });
}

export interface CollabParts {
  /** The title bar's avatars (while the document is shared). */
  presence: ReactNode;
  /** The Version History pane, when open. */
  pane: ReactNode;
  /** Over the document: the others' names and the version bar. */
  overlay: ReactNode;
  /** The Share and Join dialogs. */
  dialogs: ReactNode;
}

/** What the frame's editor tells about its document area. */
export interface CollabEditor {
  locate?: (r: Rect) => ClientRect | null;
  reveal?: (r: Rect) => void;
  showSlide?: (slide: number) => void;
}

export function useCollab(deps: {
  app: string;
  client: KilnClient | null;
  registry: CommandRegistry;
  summary: DocSummary | null;
  editor: CollabEditor | null;
  /** The link of the route, when it opens a shared document. */
  join: ShareLink | null;
}): CollabParts {
  const { app, client, registry, summary, editor, join } = deps;
  const doc = summary?.doc ?? null;
  const theme = useTheme();
  const [state, setState] = useState<CollabState | null>(null);
  const [shareOpen, setShareOpen] = useState(false);
  const [paneOpen, setPaneOpen] = useState(false);
  const [versions, setVersions] = useState<VersionEntry[]>([]);
  const [comparison, setComparison] = useState<VersionParagraph[] | null>(null);
  const [slides, setSlides] = useState<{ slides: VersionSlide[]; scene: SceneDelta } | null>(null);
  const [defaults, setDefaults] = useState<ShareDefaults>({ server: '', token: '', name: '' });
  const [tick, setTick] = useState(0);
  const sharing = SHARING.includes(app);

  useEffect(() => {
    let live = true;
    void Promise.all([loadPref(PREF.server), loadPref(PREF.token), loadPref(PREF.name)]).then(
      ([server, token, name]) => live && setDefaults({ server, token, name }),
    );
    return () => {
      live = false;
    };
  }, []);

  // An edit made while not connected is unsent from the moment it is made, before the next
  // state arrives: the guard on leaving the page must not miss it.
  const unsentEdit = useRef(false);
  // The collaboration state: on opening, on the core's Collab events and, while shared, after
  // changes (the others' carets move with the edits).
  const refresh = useCallback(async () => {
    if (!client || doc === null || !sharing) return;
    try {
      const r = await client.query(doc, 'Collab');
      if ('Collab' in r) {
        unsentEdit.current = false;
        setState(r.Collab);
      }
    } catch {
      // The document closed meanwhile.
    }
  }, [client, doc, sharing]);
  const refreshVersions = useCallback(async () => {
    if (!client || doc === null || !sharing) return;
    try {
      const r = await client.query(doc, 'Versions');
      if ('Versions' in r) setVersions(r.Versions);
    } catch {
      setVersions([]);
    }
  }, [client, doc, sharing]);
  const shared = !!state?.shared;
  const live = useRef({ shared, paneOpen, online: false, queued: 0 });
  live.current = { shared, paneOpen, online: state?.status === 'Online', queued: state?.queued ?? 0 };
  useEffect(() => {
    setState(null);
    void refresh();
  }, [refresh]);
  useEffect(() => {
    if (!client || doc === null) return;
    let raf = 0;
    const later = () => {
      if (raf) return;
      raf = requestAnimationFrame(() => {
        raf = 0;
        void refresh();
        if (live.current.paneOpen) void refreshVersions();
      });
    };
    const stop = client.subscribe((e) => {
      if (typeof e !== 'object') return;
      if ('Collab' in e && e.Collab.doc === doc) later();
      else if ('Changed' in e && e.Changed.summary.doc === doc && live.current.shared) {
        if (!live.current.online) unsentEdit.current = true;
        later();
      }
      // a save of a document that is not shared is a version too.
      else if ('Saved' in e && e.Saved.summary.doc === doc && (live.current.shared || live.current.paneOpen))
        later();
    });
    return () => {
      stop();
      if (raf) cancelAnimationFrame(raf);
    };
  }, [client, doc, refresh, refreshVersions]);

  // Changes not yet sent are kept only while the page is open: leaving it asks first.
  useEffect(() => {
    if (!shared) return;
    const warn = (e: BeforeUnloadEvent) => {
      if (live.current.queued > 0 || unsentEdit.current) e.preventDefault();
    };
    window.addEventListener('beforeunload', warn);
    return () => window.removeEventListener('beforeunload', warn);
  }, [shared]);

  // The name flags follow scrolling and resizing.
  useEffect(() => {
    if (!shared) return;
    let raf = 0;
    const bump = () => {
      if (raf) return;
      raf = requestAnimationFrame(() => {
        raf = 0;
        setTick((t) => t + 1);
      });
    };
    window.addEventListener('scroll', bump, true);
    window.addEventListener('resize', bump);
    return () => {
      window.removeEventListener('scroll', bump, true);
      window.removeEventListener('resize', bump);
      if (raf) cancelAnimationFrame(raf);
    };
  }, [shared]);

  const [notice, setNotice] = useState<string | null>(null);
  const fail = useCallback((error: unknown) => setNotice(errorText(KilnError.from(error))), []);

  // File ▸ Share and Review ▸ Version History.
  useEffect(() => {
    if (!sharing) return;
    const a = registry.register({ id: 'suite.file.share', title: t('Share'), run: () => setShareOpen(true) });
    const b = registry.register({
      id: 'suite.file.version_history',
      title: t('Version History'),
      run: () => setPaneOpen((o) => !o),
      isChecked: () => live.current.paneOpen,
    });
    return () => {
      a.dispose();
      b.dispose();
    };
  }, [registry, sharing]);

  const share = useCallback(
    async (s: ShareDefaults) => {
      if (!client || doc === null) throw new Error('No document is open.');
      try {
        await client.loadCollab?.(doc, app);
        await client.execute(doc, { Suite: { Share: { server: s.server, token: s.token, name: s.name } } });
      } catch (error) {
        throw new Error(errorText(KilnError.from(error)));
      }
      setDefaults(s);
      void storePref(PREF.server, s.server);
      void storePref(PREF.token, s.token);
      void storePref(PREF.name, s.name);
      await refresh();
    },
    [client, doc, app, refresh],
  );

  const stop = useCallback(async () => {
    if (!client || doc === null) return;
    try {
      await client.execute(doc, { Suite: 'StopSharing' });
    } catch (error) {
      fail(error);
    }
    setShareOpen(false);
    setVersions([]);
    await refresh();
  }, [client, doc, refresh, fail]);

  const preview = useCallback(
    async (id: string | null) => {
      if (!client || doc === null) return;
      setComparison(null);
      setSlides(null);
      try {
        await client.execute(doc, { Suite: { VersionPreview: { id } } });
        if (id !== null && app === 'deck') {
          const r = await client.query(doc, {
            VersionSlides: { id, width: 132, dpr: window.devicePixelRatio || 1 },
          });
          if ('VersionSlides' in r) setSlides(r.VersionSlides);
        }
      } catch (error) {
        fail(error);
      }
      await refresh();
    },
    [client, doc, app, refresh, fail],
  );

  const restore = useCallback(
    async (id: string) => {
      if (!client || doc === null) return;
      try {
        await client.execute(doc, { Suite: { VersionRestore: { id } } });
      } catch (error) {
        fail(error);
      }
      setComparison(null);
      setSlides(null);
      await Promise.all([refresh(), refreshVersions()]);
    },
    [client, doc, refresh, refreshVersions, fail],
  );

  const rename = useCallback(
    async (id: string, name: string) => {
      if (!client || doc === null) return;
      try {
        await client.execute(doc, { Suite: { VersionRename: { id, name } } });
      } catch (error) {
        fail(error);
      }
      await refreshVersions();
    },
    [client, doc, refreshVersions, fail],
  );

  const compare = useCallback(
    async (id: string | null) => {
      if (!client || doc === null || id === null) {
        setComparison(null);
        return;
      }
      try {
        const r = await client.query(doc, { VersionCompare: { id } });
        if ('VersionCompare' in r) setComparison(r.VersionCompare);
      } catch (error) {
        fail(error);
      }
    },
    [client, doc, fail],
  );

  useEffect(() => {
    if (paneOpen) void refreshVersions();
  }, [paneOpen, refreshVersions]);

  // Closing the pane goes back to the document.
  const closePane = useCallback(() => {
    setPaneOpen(false);
    if (state?.preview) void preview(null);
  }, [state?.preview, preview]);

  // Opening a link: the name the others see, then the document.
  const [joinOpen, setJoinOpen] = useState(false);
  // The route parses its link afresh on every render: the dialog follows the link, not the object.
  const joinKey = join ? `${join.room}\n${join.server}\n${join.token}` : '';
  useEffect(() => setJoinOpen(joinKey !== ''), [joinKey]);
  // The name is chosen; the document opens as soon as the core is ready (the page may still be
  // starting it).
  const [joinAs, setJoinAs] = useState<string | null>(null);
  const joined = useRef(false);
  useEffect(() => {
    if (!client || !join || joinAs === null || joined.current) return;
    joined.current = true;
    const name = joinAs;
    void (async () => {
      try {
        await client.loadCollab?.(null, app);
        const r = await client.execute(null, {
          Suite: {
            JoinShared: {
              kind: KIND[app] ?? 'Write',
              server: join.server,
              room: join.room,
              token: join.token,
              name,
            },
          },
        });
        if (name) {
          void storePref(PREF.name, name);
          setDefaults((d) => ({ ...d, name }));
        }
        if (r.summary) {
          // Kiln Home lists it under Shared with me.
          const q = new URLSearchParams({ doc: join.room, server: join.server, token: join.token });
          void rememberShared({
            link: `#/${app}?${q.toString()}`,
            name: r.summary.title,
            kind: KIND[app] ?? 'Write',
            at: Date.now(),
          });
          window.location.replace(`#/${app}?doc=${r.summary.doc}`);
        }
      } catch (error) {
        joined.current = false;
        setJoinAs(null);
        setJoinOpen(true);
        fail(error);
      }
    })();
  }, [client, join, joinAs, app, fail]);

  const goTo = useCallback(
    (p: CollabPeer) => {
      if (p.slide !== null && editor?.showSlide) editor.showSlide(p.slide);
      else if (p.at && editor?.reveal) editor.reveal(p.at);
    },
    [editor],
  );

  const placed = useMemo<PlacedPeer[]>(() => {
    void tick;
    // A version shown in place of the document has nobody's caret in it.
    if (!state?.shared || state.preview || !editor?.locate) return [];
    const out: PlacedPeer[] = [];
    for (const peer of state.peers) {
      const r = peer.at ? editor.locate(peer.at) : null;
      if (r) out.push({ peer, rect: r });
    }
    return out;
  }, [state, editor, tick]);

  const previewed = state?.preview ? versions.find((v) => v.id === state.preview) : undefined;
  const link = state?.link ? linkFor(app, state.link) : null;

  const presence =
    shared && state ? (
      <PresenceAvatars
        peers={state.peers}
        me={{ name: state.name, initials: state.initials, colour: state.colour }}
        status={state.status}
        onGoTo={goTo}
        onOpenShare={() => setShareOpen(true)}
      />
    ) : null;

  const pane =
    paneOpen && sharing ? (
      <TaskPaneHost
        panes={[
          {
            id: 'versions',
            title: t('Version History'),
            content: (
              <VersionHistoryPane
                shared={shared}
                local={!shared}
                versions={versions}
                selected={state?.preview ?? null}
                onSelect={(id) => void preview(id)}
                onRestore={(id) => void restore(id)}
                onRename={(id, name) => void rename(id, name)}
                onShare={() => setShareOpen(true)}
                comparison={app === 'write' ? comparison : undefined}
                onCompare={app === 'write' ? (id) => void compare(id) : undefined}
                slides={app === 'deck' ? slides : undefined}
                theme={theme.isDark ? 'dark' : 'light'}
              />
            ),
          },
        ]}
        onClose={closePane}
        defaultWidth={300}
      />
    ) : null;

  const overlay = (
    <>
      {placed.length > 0 ? <RemoteFlags placed={placed} /> : null}
      {notice ? (
        <div className="kiln-version-bar kiln-collab-notice" role="alert">
          <span>{notice}</span>
          <Button size="sm" onClick={() => setNotice(null)}>
            {t('Dismiss')}
          </Button>
        </div>
      ) : null}
      {previewed ? (
        <VersionBar
          version={previewed}
          onRestore={() => void restore(previewed.id)}
          onClose={() => void preview(null)}
        />
      ) : null}
    </>
  );

  const dialogs = sharing ? (
    <>
      <ShareDialog
        open={shareOpen}
        onOpenChange={setShareOpen}
        title={summary?.title ?? t('Document')}
        state={state}
        defaults={defaults}
        link={link}
        onShare={share}
        onStop={() => void stop()}
        onCopy={async (l) => {
          await navigator.clipboard.writeText(l);
        }}
      />
      {join ? (
        <JoinDialog
          open={joinOpen}
          server={join.server}
          defaultName={defaults.name}
          onJoin={(name) => {
            setJoinOpen(false);
            setJoinAs(name);
          }}
          onCancel={() => {
            setJoinOpen(false);
            window.location.replace('#/home');
          }}
        />
      ) : null}
    </>
  ) : null;

  return { presence, pane, overlay, dialogs };
}
