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
  type PrintSettings,
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
  downloadPdf,
  ExportPdfPane,
  type FormatChoice,
  type PdfExportOutcome,
  PrintPane,
  ribbonFromData,
  StripButton,
  useTheme,
} from '@kiln/ui-kit';
import { type ReactNode, useCallback, useEffect, useMemo, useRef, useState } from 'react';
import { appHash } from '../../App.tsx';
import { useClient } from '../../client/useClient.ts';
import { CREATABLE } from '../home/index.tsx';
import { useWriteParts } from '../write/index.tsx';
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
  { format: 'Md', label: 'Markdown', note: 'Text and structure, for the web and wikis' },
];

/** Apps whose documents print and export to PDF (Grid and Deck register theirs in their WPs). */
const PRINTABLE: readonly EditorApp[] = ['write'];

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
    // The frame (title, save state, status bar) follows the core after the document has
    // painted: React renders synchronously for these updates, and a keystroke's frame must not
    // wait for them (typing latency, B5). Several changes in one frame make one update.
    let pendingCore: CoreDocState | null = null;
    let raf = 0;
    let timer: ReturnType<typeof setTimeout> | null = null;
    const unsubscribe = conn.subscribe((state) => {
      pendingCore = state;
      if (raf || timer) return;
      raf = requestAnimationFrame(() => {
        raf = 0;
        timer = setTimeout(() => {
          timer = null;
          const next = pendingCore;
          pendingCore = null;
          if (next) setCore(next);
        }, 0);
      });
    });
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
      if (raf) cancelAnimationFrame(raf);
      if (timer) clearTimeout(timer);
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

  // What the frame shows of the document changes far less often than the core reports a change
  // (every keystroke): keep one summary object while those fields stay the same, so the frame
  // below is not rebuilt while typing (B5 typing latency).
  const summary = useStableSummary(core.summary);
  const coreRef = useRef(core);
  coreRef.current = core;
  useEffect(() => {
    document.title = `${summary?.title ?? APP_NAMES[app]} – ${APP_NAMES[app]}`;
  }, [summary?.title, app]);

  const execute = useCallback((id: string, args?: unknown) => void registry.execute(id, args), [registry]);
  const theme = useTheme();
  // The app's editor (WP-64 Write): its surface, status bar and ribbon replace the frame's own.
  const editor = useWriteParts(app === 'write' && !problem ? { client, registry, summary, ribbon } : null);

  const [recent, setRecent] = useState<RecentFile[] | null>(null);
  const [compat, setCompat] = useState<CompatIssue[] | null>(null);
  const [compatError, setCompatError] = useState<string | null>(null);
  const [coreInfo, setCoreInfo] = useState<string | null>(null);
  // Backstage covers the document, so its Info page shows the status of when it opened.
  const [infoStatus, setInfoStatus] = useState(core.status);
  const loadBackstageData = useCallback(async () => {
    setInfoStatus(coreRef.current.status);
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

  // File ▸ Print and Export ▸ PDF (WP-67): everything is rendered by the core.
  const docId = summary?.doc ?? null;
  const requestPreview = useCallback(
    async (settings: PrintSettings, sheet: number, scale: number) => {
      if (!client || docId === null) throw new Error('No document is open.');
      try {
        const r = await client.query(docId, { PrintPreview: { settings, sheet, scale } });
        if (!('PrintPreview' in r)) throw new Error('The core did not return a preview.');
        return r.PrintPreview;
      } catch (error) {
        throw new Error(KilnError.from(error).message);
      }
    },
    [client, docId],
  );
  const printDocument = useCallback(
    async (settings: PrintSettings) => {
      if (!client || docId === null) throw new Error('No document is open.');
      try {
        await client.execute(docId, { Suite: { PrintPdf: { settings } } });
      } catch (error) {
        throw new Error(KilnError.from(error).message);
      }
    },
    [client, docId],
  );
  const exportPdf = useCallback(
    async (settings: PrintSettings): Promise<PdfExportOutcome | null> => {
      if (!client || docId === null) throw new Error('No document is open.');
      try {
        const r = await client.query(docId, { Pdf: { settings } });
        if (!('Pdf' in r)) throw new Error('The core did not return a PDF.');
        const pdf = r.Pdf;
        if (client.bridge === 'tauri') {
          // The native core writes files itself, after the save dialog; `Exported` says where
          // (or that the dialog was cancelled).
          const exported = new Promise<string | null>((resolve, reject) => {
            const stop = client.subscribe((event) => {
              if ('Exported' in event && event.Exported.doc === docId) {
                stop();
                resolve(event.Exported.path);
              } else if ('Error' in event && event.Error.doc === docId) {
                stop();
                reject(new Error(event.Error.error.message));
              }
            });
          });
          await client.execute(docId, { Suite: { SavePdf: { path: null, settings } } });
          const path = await exported;
          if (path === null) return null;
          return { name: path, pdfUa: pdf.pdf_ua, uaIssues: pdf.ua_issues };
        } else {
          downloadPdf(pdf.bytes, pdf.name);
        }
        return { name: pdf.name, pdfUa: pdf.pdf_ua, uaIssues: pdf.ua_issues };
      } catch (error) {
        throw new Error(KilnError.from(error).message);
      }
    },
    [client, docId],
  );
  const printable = PRINTABLE.includes(app) && docId !== null;

  const browse = () => {
    pendingOpen.current = true;
    execute('suite.file.open');
  };
  const backstage: Record<string, ReactNode | undefined> = {
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
        status={infoStatus}
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
    print: printable ? (
      <PrintPane
        requestPreview={requestPreview}
        onPrint={printDocument}
        onSavePdf={async (s) => {
          await exportPdf(s);
        }}
        textOptions={app === 'write'}
      />
    ) : undefined,
    export: (
      <>
        {printable ? <ExportPdfPane onExport={exportPdf} textOptions={app === 'write'} /> : null}
        <BackstageFormatsPage
          formats={EXPORT_FORMATS}
          intro={printable ? 'Other formats' : undefined}
          onChoose={(format) => execute('suite.file.save_as', { format })}
        />
      </>
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

  // The status bar of apps without an editor of their own follows every change of the core.
  const frameCore = editor ? null : core;
  return useMemo(
    () => (
      <AppFrame
        app={app}
        registry={registry}
        keymap={keymap}
        title={summary?.title ?? (problem ? APP_NAMES[app] : 'Opening…')}
        extension={summary?.format ? EXTENSIONS[summary.format] : undefined}
        saveStatus={saveStatusOf(summary, problem)}
        ribbon={editor?.ribbon ?? ribbon}
        ribbonLayout={layout}
        onRibbonLayoutChange={(l) => {
          setLayout(l);
          void storePref('ribbonLayout', l);
        }}
        // Without a document there is nothing for the commands to act on: tabs only (File still
        // opens Backstage for New and Open), never a full slab over the "not available" notice.
        ribbonCollapsed={collapsed || problem !== null}
        onRibbonCollapsedChange={(c) => {
          if (problem !== null) return;
          setCollapsed(c);
          void storePref('ribbonCollapsed', c);
        }}
        backstage={backstage}
        onBackstagePageChange={() => void loadBackstageData()}
        ribbonTrailing={<StripButton command="suite.review.show_comments" label="Comments" />}
        statusBar={
          editor?.statusBar ??
          (frameCore ? <FrameStatusBar app={app} core={frameCore} execute={execute} /> : null)
        }
      >
        {editor?.surface ?? (
          <SurfaceSlot app={app} summary={summary} problem={problem} ready={!!connection} />
        )}
      </AppFrame>
    ),
    [
      app,
      registry,
      keymap,
      summary,
      problem,
      ribbon,
      layout,
      collapsed,
      recent,
      compat,
      compatError,
      coreInfo,
      infoStatus,
      theme.preference,
      theme.setPreference,
      editor,
      frameCore,
      connection,
      execute,
      loadBackstageData,
      checkCompat,
    ],
  );
}

/** The summary, as the same object while the fields the frame shows are unchanged. */
function useStableSummary(summary: DocSummary | null): DocSummary | null {
  const ref = useRef(summary);
  const prev = ref.current;
  if (
    summary === null ||
    prev === null ||
    prev.doc !== summary.doc ||
    prev.title !== summary.title ||
    prev.format !== summary.format ||
    prev.dirty !== summary.dirty ||
    prev.path !== summary.path ||
    prev.read_only !== summary.read_only ||
    prev.app !== summary.app
  ) {
    ref.current = summary;
  }
  return ref.current;
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
