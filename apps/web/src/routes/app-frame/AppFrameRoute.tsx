/**
 * The app frame for `#/write`, `#/grid`, `#/deck` and `#/notes` (PLAN.md §1.3): the window chrome
 * of `@kiln/ui-kit` over the command registry of `@kiln/commands`, connected to the core. The
 * frame asks the core for a new document (or shows the one in `?doc=`), renders the app's ribbon
 * from its data definition, takes title, save state, status bar and command availability from the
 * core's events, and leaves the document surface to the app's editor (`surface` slot).
 */
import {
  type AppId,
  CommandLog,
  type CoreConnection,
  type CoreDocState,
  connectCore,
  createCommandRegistry,
  createDefaultKeymap,
  EMPTY_CORE_STATE,
  RIBBONS,
} from '@kiln/commands';
import { appKind, type HomeApp } from '@kiln/home';
import {
  type CompatIssue,
  type DocSummary,
  type FileFormat,
  KilnError,
  type RecentFile,
  WebHost,
} from '@kiln/protocol';
import {
  APP_NAMES,
  AppFrame,
  BackstageAccountPage,
  BackstageFormatsPage,
  BackstageInfoPage,
  BackstageNewPage,
  BackstageOpenPage,
  type FormatChoice,
  ribbonFromData,
  StripButton,
  useTheme,
} from '@kiln/ui-kit';
import { type ReactNode, useCallback, useEffect, useMemo, useRef, useState } from 'react';
import { appHash } from '../../App.tsx';
import { useClient } from '../../client/useClient.ts';
import { CREATABLE } from '../home/index.tsx';
import { installDebugHandle } from './debug.ts';
import { storePref, type UiPrefs } from './prefs.ts';
import { FrameStatusBar } from './statusbar.tsx';
import { SurfaceSlot } from './surface.tsx';
import { usePersistTheme } from './usePersistTheme.ts';

export type EditorApp = Exclude<AppId, 'hub'>;

const SAVE_FORMATS: Readonly<Record<EditorApp, readonly FormatChoice[]>> = {
  write: [
    { format: 'Docx', label: 'Word Document', note: 'Opens in Kiln Write and Microsoft Word' },
    { format: 'Odt', label: 'OpenDocument Text', note: 'Open standard (LibreOffice)' },
    { format: 'Dotx', label: 'Word Template' },
    { format: 'Md', label: 'Markdown' },
    { format: 'Txt', label: 'Plain Text' },
  ],
  grid: [
    { format: 'Xlsx', label: 'Excel Workbook', note: 'Opens in Kiln Grid and Microsoft Excel' },
    { format: 'Ods', label: 'OpenDocument Spreadsheet', note: 'Open standard (LibreOffice)' },
    { format: 'Xltx', label: 'Excel Template' },
    { format: 'Csv', label: 'CSV (comma-separated values)', note: 'The active sheet only' },
  ],
  deck: [
    { format: 'Pptx', label: 'PowerPoint Presentation', note: 'Opens in Kiln Deck and Microsoft PowerPoint' },
    { format: 'Odp', label: 'OpenDocument Presentation', note: 'Open standard (LibreOffice)' },
    { format: 'Potx', label: 'PowerPoint Template' },
  ],
  notes: [
    { format: 'Docx', label: 'Word Document', note: 'This page' },
    { format: 'Md', label: 'Markdown', note: 'This page' },
  ],
};

const EXPORT_FORMATS: readonly FormatChoice[] = [
  { format: 'Pdf', label: 'PDF', note: 'Exactly as it looks on screen and in print' },
  { format: 'Md', label: 'Markdown', note: 'Text and structure, for the web and wikis' },
];

const EXTENSIONS: Partial<Record<FileFormat, string>> = {
  Docx: '.docx',
  Dotx: '.dotx',
  Docm: '.docm',
  Odt: '.odt',
  Xlsx: '.xlsx',
  Xltx: '.xltx',
  Xlsm: '.xlsm',
  Csv: '.csv',
  Ods: '.ods',
  Pptx: '.pptx',
  Potx: '.potx',
  Pptm: '.pptm',
  Odp: '.odp',
  Md: '.md',
  Txt: '.txt',
  Pdf: '.pdf',
};

const prefsHost = new WebHost();

/** One app window. */
export function AppFrameRoute({ app, doc, prefs }: { app: EditorApp; doc: number | null; prefs: UiPrefs }) {
  const clientState = useClient();
  const client = clientState.status === 'ready' ? clientState.client : null;
  const registry = useMemo(() => createCommandRegistry({ app }), [app]);
  const keymap = useMemo(() => createDefaultKeymap(), []);
  const log = useMemo(() => new CommandLog(), []);
  const [connection, setConnection] = useState<CoreConnection | null>(null);
  const [core, setCore] = useState<CoreDocState>(EMPTY_CORE_STATE);
  const [problem, setProblem] = useState<string | null>(null);
  const [layout, setLayout] = useState(prefs.ribbonLayout);
  const [collapsed, setCollapsed] = useState(prefs.ribbonCollapsed);
  const ribbon = useMemo(() => {
    const data = RIBBONS[app];
    return data ? ribbonFromData(data) : undefined;
  }, [app]);
  const pendingOpen = useRef(false);
  usePersistTheme();

  // Connect the registry to the core, then show the requested document or create a new one.
  useEffect(() => {
    if (!client) return;
    const conn = connectCore(registry, { client, log });
    const unsubscribe = conn.subscribe(setCore);
    setConnection(conn);
    let live = true;
    const show = async () => {
      try {
        let summary: DocSummary;
        if (doc !== null) {
          const r = await client.query(doc, 'Summary');
          if (!('Summary' in r)) throw new Error('the core did not describe the document');
          summary = r.Summary;
        } else {
          summary = await client.new(appKind(app as HomeApp));
        }
        if (!live) return;
        conn.setDocument(summary);
        const status = await client.query(summary.doc, 'Status');
        if (live && 'Status' in status) setCore((c) => ({ ...c, status: status.Status }));
      } catch (error) {
        if (!live) return;
        const e = KilnError.from(error);
        setProblem(
          e.kind === 'Unsupported'
            ? `${APP_NAMES[app]} documents cannot be created in this build yet.`
            : `The core could not open the document: ${e.message}`,
        );
      }
    };
    void show();
    return () => {
      live = false;
      unsubscribe();
      conn.dispose();
      setConnection(null);
    };
  }, [client, registry, log, app, doc]);

  // A document opened from File ▸ Open (or Home) moves the window to its app.
  useEffect(() => {
    if (!client) return;
    return client.subscribe((event) => {
      if ('Opened' in event && event.Opened.summary.doc !== core.summary?.doc && pendingOpen.current) {
        pendingOpen.current = false;
        window.location.hash = appHash(kindOf(event.Opened.summary), event.Opened.summary.doc);
      }
    });
  }, [client, core.summary?.doc]);

  useEffect(() => {
    if (client) void loadBackstageData();
  }, [client]);

  useEffect(
    () => installDebugHandle({ app, registry, keymap, log, client }),
    [app, registry, keymap, log, client],
  );

  const summary = core.summary;
  useEffect(() => {
    document.title = `${summary?.title ?? APP_NAMES[app]} – ${APP_NAMES[app]}`;
  }, [summary?.title, app]);

  const execute = useCallback((id: string, args?: unknown) => void registry.execute(id, args), [registry]);
  const theme = useTheme();

  const [recent, setRecent] = useState<RecentFile[] | null>(null);
  const [compat, setCompat] = useState<CompatIssue[] | null>(null);
  const [compatError, setCompatError] = useState<string | null>(null);
  const [coreInfo, setCoreInfo] = useState<string | null>(null);
  const loadBackstageData = useCallback(async () => {
    const reply = await prefsHost.handle('Recents');
    setRecent(typeof reply === 'object' && 'Recents' in reply ? reply.Recents : []);
    if (client) {
      try {
        const pong = await client.ping();
        setCoreInfo(`Core ${pong.version}, protocol ${pong.protocol}, ${pong.target}.`);
      } catch {
        setCoreInfo(null);
      }
    }
  }, [client]);
  const checkCompat = useCallback(async () => {
    if (!client || !summary) return;
    try {
      setCompatError(null);
      const r = await client.query(summary.doc, { CompatReport: { format: null } });
      setCompat('CompatReport' in r ? r.CompatReport : []);
    } catch (error) {
      setCompat(null);
      setCompatError(KilnError.from(error).message);
    }
  }, [client, summary]);

  const browse = () => {
    pendingOpen.current = true;
    execute('suite.file.open');
  };
  const backstage: Record<string, ReactNode> = {
    home: <BackstageOpenPage recent={recent} onOpenRecent={(f) => openRecent(f)} onBrowse={browse} />,
    new: (
      <BackstageNewPage
        app={app}
        creatable={CREATABLE}
        onCreate={(a) => {
          window.location.hash = `${appHash(a)}?new=${Date.now()}`;
        }}
      />
    ),
    open: <BackstageOpenPage recent={recent} onOpenRecent={(f) => openRecent(f)} onBrowse={browse} />,
    info: (
      <BackstageInfoPage
        summary={summary}
        status={core.status}
        compat={compat}
        compatError={compatError}
        onCheckCompat={summary ? () => void checkCompat() : undefined}
      />
    ),
    saveAs: (
      <BackstageFormatsPage
        formats={SAVE_FORMATS[app]}
        intro="Choose a format. Kiln keeps everything it does not edit exactly as it was."
        onChoose={(format) => execute('suite.file.save_as', { format })}
      />
    ),
    saveCopy: (
      <BackstageFormatsPage
        formats={SAVE_FORMATS[app]}
        intro="Saves a copy and keeps working on this document."
        onChoose={(format) => execute('suite.file.save_as', { format })}
      />
    ),
    export: (
      <BackstageFormatsPage
        formats={EXPORT_FORMATS}
        onChoose={(format) => execute('suite.file.save_as', { format })}
      />
    ),
    account: (
      <BackstageAccountPage
        theme={theme.preference}
        onTheme={theme.setPreference}
        version={__KILN_VERSION__}
        core={coreInfo}
      />
    ),
  };
  function openRecent(file: RecentFile) {
    pendingOpen.current = true;
    execute('suite.file.open', { path: file.path });
  }

  return (
    <AppFrame
      app={app}
      registry={registry}
      keymap={keymap}
      title={summary?.title ?? (problem ? APP_NAMES[app] : 'Opening…')}
      extension={summary?.format ? EXTENSIONS[summary.format] : undefined}
      saveStatus={saveStatusOf(summary, problem)}
      ribbon={ribbon}
      ribbonLayout={layout}
      onRibbonLayoutChange={(l) => {
        setLayout(l);
        void storePref('ribbonLayout', l);
      }}
      ribbonCollapsed={collapsed}
      onRibbonCollapsedChange={(c) => {
        setCollapsed(c);
        void storePref('ribbonCollapsed', c);
      }}
      backstage={backstage}
      onBackstagePageChange={() => void loadBackstageData()}
      ribbonTrailing={<StripButton command="suite.review.show_comments" label="Comments" />}
      statusBar={<FrameStatusBar app={app} core={core} execute={execute} />}
    >
      <SurfaceSlot app={app} summary={summary} problem={problem} ready={!!connection} />
    </AppFrame>
  );
}

function kindOf(summary: DocSummary): EditorApp {
  return summary.app === 'Grid'
    ? 'grid'
    : summary.app === 'Deck'
      ? 'deck'
      : summary.app === 'Notes'
        ? 'notes'
        : 'write';
}

/** The chip after the title, from the core's summary. */
function saveStatusOf(summary: DocSummary | null, problem: string | null): string | undefined {
  if (problem) return 'Not available';
  if (!summary) return undefined;
  if (summary.read_only) return 'Read-only';
  if (!summary.path) return summary.dirty ? 'Not saved yet' : 'New document';
  return summary.dirty ? 'Unsaved changes' : 'Saved';
}
