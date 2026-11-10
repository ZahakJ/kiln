/**
 * The app frame for `#/write`, `#/grid`, `#/deck` and `#/notes`: the window chrome
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
import { appKind, type HomeApp, type HomeTemplate, TemplateGallery } from '@kiln/home';
import { msg, t, tx } from '@kiln/i18n';
import { useLanguage } from '@kiln/i18n/react';
import {
  CORE_STOPPED_EVENT,
  type DocPropertiesInfo,
  type DocSummary,
  type FileFormat,
  isTauri,
  KilnError,
  type PrintSettings,
  type RecentFile,
} from '@kiln/protocol';
import {
  AccessibleTextView,
  APP_NAMES,
  AppFrame,
  BackstageAccountPage,
  BackstageFormatsPage,
  BackstageInfoPage,
  BackstageNewPage,
  BackstageOpenPage,
  Button,
  type CompatApp,
  CompatibilityDialog,
  coreOptionsOf,
  Dialog,
  DialogContent,
  downloadPdf,
  ExportPdfPane,
  editingLanguage,
  errorText,
  type FormatChoice,
  InspectDialog,
  loadOptions,
  type PdfExportOutcome,
  PrintPane,
  PropertiesDialog,
  ribbonFromData,
  StripButton,
  subscribeOptions,
  type TextViewApp,
  useSelectionAnnouncements,
  useTheme,
} from '@kiln/ui-kit';
import { type ReactNode, useCallback, useEffect, useMemo, useRef, useState } from 'react';
import { appHash } from '../../App.tsx';
import { CORE_APPS, type CoreApp } from '../../client/cores.ts';
import { useClient } from '../../client/useClient.ts';
import { goBack, isAndroid, showRoute, useDesktopFrame, useWebBack } from '../../desktop.ts';
import { classifyFailure } from '../../LoadProblem.tsx';
import { newFromTemplate, TEMPLATES, templateByFile } from '../../templates.ts';
import { useDeckParts } from '../deck/index.tsx';
import { useGridParts } from '../grid/index.tsx';
import { CREATABLE } from '../home/index.tsx';
import { useWriteParts } from '../write/index.tsx';
import { type ShareLink, useCollab } from './collab.tsx';
import { installDebugHandle } from './debug.ts';
import { BackstageFeatured } from './featured.tsx';
import { storePref, type UiPrefs } from './prefs.ts';
import { useShareCopy } from './share.ts';
import { FrameStatusBar } from './statusbar.tsx';
import { loadRecents, type StorageParts, useStorage } from './storage.tsx';
import { type FrameProblem, SurfaceSlot } from './surface.tsx';
import { useTextEncoding } from './TextEncoding.tsx';
import { usePersistTheme } from './usePersistTheme.ts';

export type EditorApp = Exclude<AppId, 'hub'>;

const SAVE_FORMATS: Readonly<Record<EditorApp, readonly FormatChoice[]>> = {
  write: [
    { format: 'Docx', label: msg('Word Document'), note: msg('Opens in Kiln Write and Microsoft Word') },
    {
      format: 'Docm',
      label: msg('Word Macro-Enabled Document'),
      note: msg('Keeps macros; Kiln never runs them'),
    },
    { format: 'Odt', label: msg('OpenDocument Text'), note: msg('Open standard (LibreOffice)') },
    { format: 'Dotx', label: msg('Word Template') },
    { format: 'Dotm', label: msg('Word Macro-Enabled Template') },
    { format: 'Html', label: msg('Web Page'), note: msg('One HTML file with the pictures inside') },
    { format: 'Md', label: msg('Markdown') },
    { format: 'Txt', label: msg('Plain Text'), note: msg('Text only, in the encoding it was opened with') },
    {
      format: 'KWrite',
      label: msg('Kiln Write Document'),
      note: msg('Keeps its version history inside the file'),
    },
  ],
  grid: [
    { format: 'Xlsx', label: msg('Excel Workbook'), note: msg('Opens in Kiln Grid and Microsoft Excel') },
    {
      format: 'Xlsm',
      label: msg('Excel Macro-Enabled Workbook'),
      note: msg('Keeps macros; Kiln never runs them'),
    },
    { format: 'Ods', label: msg('OpenDocument Spreadsheet'), note: msg('Open standard (LibreOffice)') },
    { format: 'Xltx', label: msg('Excel Template') },
    { format: 'Csv', label: msg('CSV (comma-separated values)'), note: msg('The active sheet only') },
    {
      format: 'KGrid',
      label: msg('Kiln Grid Workbook'),
      note: msg('Keeps its version history inside the file'),
    },
  ],
  deck: [
    {
      format: 'Pptx',
      label: msg('PowerPoint Presentation'),
      note: msg('Opens in Kiln Deck and Microsoft PowerPoint'),
    },
    {
      format: 'Pptm',
      label: msg('PowerPoint Macro-Enabled Presentation'),
      note: msg('Keeps macros; Kiln never runs them'),
    },
    { format: 'Odp', label: msg('OpenDocument Presentation'), note: msg('Open standard (LibreOffice)') },
    { format: 'Potx', label: msg('PowerPoint Template') },
    { format: 'Ppsx', label: msg('PowerPoint Show'), note: msg('Opens as a slide show') },
    {
      format: 'KDeck',
      label: msg('Kiln Deck Presentation'),
      note: msg('Keeps its version history inside the file'),
    },
  ],
  // Kiln Notes is not part of v0.1: its strings are not translated.
  notes: [
    { format: 'Docx', label: msg('Word Document'), note: 'This page' }, // i18n-ignore
    { format: 'Md', label: msg('Markdown'), note: 'This page' }, // i18n-ignore
  ],
};

/** The OpenDocument formats (saved by the native core only). */
const ODF_FORMATS: readonly FileFormat[] = ['Odt', 'Ods', 'Odp'];
/** The OpenDocument format of each kind of document (Options ▸ Save ▸ Save new files in). */
const ODF_OF: Readonly<Partial<Record<DocSummary['kind'], FileFormat>>> = {
  Write: 'Odt',
  Grid: 'Ods',
  Deck: 'Odp',
};

const EXPORT_FORMATS: readonly FormatChoice[] = [
  { format: 'Md', label: msg('Markdown'), note: msg('Text and structure, for the web and wikis') },
  {
    format: 'Html',
    label: msg('Web Page (HTML)'),
    note: msg('Semantic HTML in one file, pictures included'),
  },
  { format: 'Txt', label: msg('Plain Text'), note: msg('Text only, UTF-8') },
];

/** Apps whose documents print and export to PDF. */
const PRINTABLE: readonly EditorApp[] = ['write', 'grid', 'deck'];

const EXTENSIONS: Partial<Record<FileFormat, string>> = {
  Docx: '.docx',
  Dotx: '.dotx',
  Docm: '.docm',
  Dotm: '.dotm',
  Html: '.html',
  Odt: '.odt',
  Xlsx: '.xlsx',
  Xltx: '.xltx',
  Xlsm: '.xlsm',
  Csv: '.csv',
  Ods: '.ods',
  Pptx: '.pptx',
  Potx: '.potx',
  Pptm: '.pptm',
  Ppsx: '.ppsx',
  Odp: '.odp',
  Md: '.md',
  Txt: '.txt',
  Pdf: '.pdf',
  KWrite: '.kwrite',
  KGrid: '.kgrid',
  KDeck: '.kdeck',
};

/** One app window. */
export function AppFrameRoute({
  app,
  doc,
  share = null,
  template = null,
  prefs,
}: {
  app: EditorApp;
  doc: number | null;
  /** the link of a shared document to open. */
  share?: ShareLink | null;
  /** the template (its file) a new document starts from. */
  template?: string | null;
  prefs: UiPrefs;
}) {
  // The web starts only this app's core (B12); Notes has none yet.
  const clientState = useClient(
    (CORE_APPS as readonly string[]).includes(app) ? (app as CoreApp) : undefined,
  );
  const client = clientState.status === 'ready' ? clientState.client : null;
  const registry = useMemo(() => createCommandRegistry({ app }), [app]);
  const keymap = useMemo(() => createDefaultKeymap(), []);
  const log = useMemo(() => new CommandLog(), []);
  const [connection, setConnection] = useState<CoreConnection | null>(null);
  const [core, setCore] = useState<CoreDocState>(EMPTY_CORE_STATE);
  const [problem, setProblem] = useState<FrameProblem | null>(null);
  const [layout, setLayout] = useState(prefs.ribbonLayout);
  const [collapsed, setCollapsed] = useState(prefs.ribbonCollapsed);
  const ribbon = useMemo(() => {
    const data = RIBBONS[app];
    return data ? ribbonFromData(data) : undefined;
  }, [app]);
  const pendingOpen = useRef(false);
  const storageGuard = useRef<StorageParts['guard'] | null>(null);
  // whether the route opens a shared document's link (a value, not the parsed object).
  const joining = doc === null && !!share;
  // A core that cannot start says what that means here: an update, an app not on this device
  // yet, or a Reload to try. Never the error itself (`startCore` logs it).
  useEffect(() => {
    if (clientState.status !== 'error') return;
    let live = true;
    void classifyFailure().then((failure) => {
      if (live) setProblem(failure);
    });
    return () => {
      live = false;
    };
  }, [clientState]);
  // A core that stopped after a fault: the window says so in its own words and offers Reload; the
  // editor unmounts, so nothing else calls the stopped core or shows what it answered.
  useEffect(() => {
    // i18n-ignore: a state, not text
    const stopped = () => setProblem('stopped');
    globalThis.addEventListener(CORE_STOPPED_EVENT, stopped);
    return () => globalThis.removeEventListener(CORE_STOPPED_EVENT, stopped);
  }, []);
  usePersistTheme();

  // the options the core applies (AutoSave's wait, recovery, the words the speller skips),
  // sent before the first document opens and again whenever they change.
  useEffect(() => {
    if (!client) return;
    let last = '';
    const send = () => {
      const options = coreOptionsOf(loadOptions());
      const key = JSON.stringify(options);
      if (key === last) return;
      last = key;
      void client.execute(null, { Suite: { SetOptions: { options } } }).catch(() => undefined);
    };
    send();
    return subscribeOptions(send);
  }, [client]);

  // Connect the registry to the core, then show the requested document or create a new one.
  useEffect(() => {
    if (!client) return;
    // Close of a document with unsaved changes asks first (the storage hook's guard).
    const conn = connectCore(registry, {
      client,
      log,
      guard: (id, d) => {
        // a document made with New (Ctrl+N) comes to the front of this window.
        if (id === 'suite.file.new') pendingOpen.current = true;
        // on the desktop a file opened from a document window (Ctrl+O, the menu bar)
        // gets a window of its own.
        if (id === 'suite.file.open' && isTauri()) pendingOpen.current = true;
        // Options ▸ Save ▸ Save new files in OpenDocument: the first Save of a new
        // document asks where to save it in that format.
        const odf = d && !d.path ? ODF_OF[d.kind] : undefined;
        if (
          id === 'suite.file.save' &&
          odf &&
          d?.save_formats.includes(odf) &&
          loadOptions().save.defaultFormat === 'odf'
        )
          return Promise.resolve({ Suite: { SaveAs: { path: null, format: odf } } });
        return storageGuard.current?.(id, d);
      },
      // Options ▸ Language ▸ Default language for new documents.
      language: () => editingLanguage(),
    });
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
        // a shared document's link opens it once the name is chosen (`useCollab`).
        if (joining) return;
        // A document of an earlier page (a reload, or a crash) is not in this core: a new one
        // takes its place, and Document Recovery offers what the earlier page left unsaved.
        const r =
          doc !== null
            ? await client.query(doc, 'Summary').catch((error: unknown) => {
                if (KilnError.from(error).kind === 'NotFound') return null;
                throw error;
              })
            : null;
        const from = doc === null && template ? templateByFile(template) : undefined;
        if (r) {
          if (!('Summary' in r)) throw new Error('the core did not describe the document');
          summary = r.Summary;
        } else if (from) {
          // a new, untitled document with the template's content; the template stays as it is.
          try {
            summary = await newFromTemplate(client, from);
          } catch {
            if (live)
              setProblem({
                title: t('This template could not be opened'),
                text: t('Start from a blank document instead.'),
              });
            return;
          }
          if (live) window.history.replaceState(null, '', appHash(app, summary.doc));
        } else {
          // Options ▸ Language ▸ Default language for new documents: the core makes it the new
          // document's language (and, for a right-to-left language, its paragraph direction).
          const created = await client.execute(null, {
            Suite: { New: { kind: appKind(app as HomeApp), language: editingLanguage() } },
          });
          if (!created.summary) throw new Error('the core did not describe the new document');
          summary = created.summary;
          if (doc !== null && live) {
            window.history.replaceState(null, '', appHash(app, summary.doc));
            // The route follows the address: a recovered document that is given the old number
            // next must count as a new address, or the window stays on this one.
            window.dispatchEvent(new HashChangeEvent('hashchange'));
          }
        }
        if (!live) return;
        conn.setDocument(summary);
        const status = await client.query(summary.doc, 'Status');
        if (live && 'Status' in status) setCore((c) => ({ ...c, status: status.Status }));
      } catch (error) {
        if (!live) return;
        const e = KilnError.from(error);
        console.error('Kiln: the document did not open', error);
        setProblem(
          e.kind === 'Unsupported'
            ? {
                title: t('{app} is not part of this build', { app: APP_NAMES[app] }),
                text: t('{app} documents cannot be created in this build yet.', { app: APP_NAMES[app] }),
              }
            : {
                title: t('This document could not be opened'),
                text: t('Go back to Kiln Home and try again.'),
              },
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
  }, [client, registry, log, app, doc, joining, template]);

  // A document opened from File ▸ Open (or Home) moves the tab to its app; on the desktop it gets
  // a window of its own.
  useEffect(() => {
    if (!client) return;
    return client.subscribe((event) => {
      if ('Opened' in event && event.Opened.summary.doc !== core.summary?.doc && pendingOpen.current) {
        pendingOpen.current = false;
        showRoute(appHash(kindOf(event.Opened.summary), event.Opened.summary.doc));
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
  // The desktop shell: the window's menu bar runs commands here; its close button closes the
  // document first.
  useDesktopFrame(registry, client, summary);
  // the browser's Back closes sheets, dialogs and menus, then the document, as on Android.
  useWebBack(summary?.doc ?? null);
  const extension = summary?.format ? EXTENSIONS[summary.format] : undefined;
  const shareCopy = useShareCopy(client, summary, extension);
  // Save As lists what the core can write for this document (the web core has no OpenDocument
  // writer); before a document is open, OpenDocument only in the native core.
  const savable = summary?.save_formats;
  const saveChoices = useMemo(
    () =>
      SAVE_FORMATS[app].filter((c) =>
        savable && savable.length > 0
          ? savable.includes(c.format)
          : isTauri() || !ODF_FORMATS.includes(c.format),
      ),
    [app, savable],
  );
  // Export lists the structure formats the document can be written as.
  const exportChoices = useMemo(() => EXPORT_FORMATS.filter((c) => !!savable?.includes(c.format)), [savable]);
  const coreRef = useRef(core);
  coreRef.current = core;
  useEffect(() => {
    document.title = `${summary?.title ?? APP_NAMES[app]} – ${APP_NAMES[app]}`;
  }, [summary?.title, app]);

  const execute = useCallback((id: string, args?: unknown) => void registry.execute(id, args), [registry]);
  const theme = useTheme();
  // The app's editor (Write): its surface, status bar and ribbon replace the frame's own.
  const writeEditor = useWriteParts(
    app === 'write' && !problem ? { client, registry, summary, ribbon } : null,
  );
  // Grid: the formula bar, sheet and tabs, and Grid's status bar.
  const gridEditor = useGridParts(app === 'grid' && !problem ? { client, registry, summary, ribbon } : null);
  // Deck: the thumbnail rail, the stage and the notes, and Deck's status bar.
  const deckEditor = useDeckParts(app === 'deck' && !problem ? { client, registry, summary, ribbon } : null);
  const editor = writeEditor ?? gridEditor ?? deckEditor;
  // Document Recovery, AutoSave, a file another program changed.
  const storage = useStorage({ client, summary, appName: APP_NAMES[app] });
  storageGuard.current = storage.guard;
  // Share, presence, the others' names and Version History.
  const collab = useCollab({
    app,
    client,
    registry,
    summary,
    editor: writeEditor ?? deckEditor,
    join: share,
  });

  // View ▸ Accessibility ▸ Text View, and the selection read out as it changes.
  const textViewApp: TextViewApp | null = app === 'write' || app === 'grid' || app === 'deck' ? app : null;
  const [textView, setTextView] = useState(false);
  const textViewOn = textView && !!client && !!summary && textViewApp !== null;
  useEffect(() => {
    const d = registry.register({
      id: 'ui.view.text_view',
      title: t('Text View'),
      when: () => textViewApp !== null,
      isEnabled: () => !!summary,
      isChecked: () => textView,
      run: () => {
        setTextView((on) => !on);
        // The keys go to the view once it has drawn.
        requestAnimationFrame(() =>
          requestAnimationFrame(() =>
            document.querySelector<HTMLElement>('[data-kiln-text-view-focus]')?.focus(),
          ),
        );
      },
    });
    return () => d.dispose();
  }, [registry, textViewApp, summary, textView]);
  useSelectionAnnouncements(client, summary?.doc ?? null);

  const [recent, setRecent] = useState<RecentFile[] | null>(null);
  const [coreInfo, setCoreInfo] = useState<string | null>(null);
  // Backstage covers the document, so its Info page shows the status of when it opened.
  const [infoStatus, setInfoStatus] = useState(core.status);
  const loadBackstageData = useCallback(async () => {
    setInfoStatus(coreRef.current.status);
    if (client) {
      setRecent(await loadRecents(client).catch(() => []));
      try {
        const pong = await client.ping();
        setCoreInfo(
          t('Core {version}, protocol {protocol}, {target}.', {
            version: pong.version,
            protocol: pong.protocol,
            target: pong.target,
          }),
        );
      } catch {
        setCoreInfo(null);
      }
    }
  }, [client]);
  // Word's question before a macro-enabled document is saved as a macro-free one.
  const [macroAsk, setMacroAsk] = useState<FileFormat | null>(null);
  const textEncoding = useTextEncoding(client, summary, app === 'write', execute);
  const saveAsChecked = useCallback(
    async (format: FileFormat) => {
      if (client && summary && MACRO_FREE.includes(format)) {
        try {
          const r = await client.query(summary.doc, { CompatReport: { format } });
          const issues = 'CompatReport' in r ? r.CompatReport : [];
          if (issues.some((i) => i.code === 'macros.removed')) {
            setMacroAsk(format);
            return;
          }
        } catch {
          // Without the report, save as asked.
        }
      }
      execute('suite.file.save_as', { format });
    },
    [summary, client, execute],
  );
  // File ▸ Info's dialogs (Inspect Document, the Compatibility Checker, Properties) and
  // the properties its card shows.
  const [infoDialog, setInfoDialog] = useState<'inspect' | 'compat' | 'properties' | null>(null);
  const [properties, setProperties] = useState<DocPropertiesInfo | null>(null);
  const infoDoc = summary?.doc ?? null;
  const loadProperties = useCallback(async (): Promise<DocPropertiesInfo> => {
    if (!client || infoDoc === null) throw new Error('No document is open.');
    const r = await client.query(infoDoc, 'Properties');
    if (!('Properties' in r)) throw new Error('The core did not return the properties.');
    setProperties(r.Properties);
    return r.Properties;
  }, [client, infoDoc]);
  useEffect(() => {
    if (infoDoc !== null && isInfoApp(app)) void loadProperties().catch(() => setProperties(null));
  }, [loadProperties, infoDoc, app]);
  const compatFormats = useMemo(
    () => saveChoices.map((c) => ({ format: c.format, label: tx(c.label) })),
    [saveChoices],
  );
  const infoDialogs =
    client && summary && isInfoApp(app) ? (
      <>
        <InspectDialog
          open={infoDialog === 'inspect'}
          onOpenChange={(o) => setInfoDialog(o ? 'inspect' : null)}
          app={app}
          readOnly={summary.read_only}
          inspect={async () => {
            const r = await client.query(summary.doc, 'Inspect');
            if (!('Inspect' in r)) throw new Error('The core did not inspect the document.');
            return r.Inspect;
          }}
          remove={async (item) => {
            await registry.execute('suite.file.inspect_remove', { item });
          }}
        />
        <CompatibilityDialog
          open={infoDialog === 'compat'}
          onOpenChange={(o) => setInfoDialog(o ? 'compat' : null)}
          formats={compatFormats}
          initial={
            summary.format && saveChoices.some((c) => c.format === summary.format)
              ? summary.format
              : undefined
          }
          check={async (format) => {
            const r = await client.query(summary.doc, { CompatReport: { format } });
            return 'CompatReport' in r ? r.CompatReport : [];
          }}
        />
        <PropertiesDialog
          open={infoDialog === 'properties'}
          onOpenChange={(o) => setInfoDialog(o ? 'properties' : null)}
          app={app}
          name={summary.title}
          readOnly={summary.read_only}
          load={loadProperties}
          save={async (props) => {
            await registry.execute('suite.file.properties', { props });
            await loadProperties().catch(() => undefined);
          }}
        />
      </>
    ) : null;

  // File ▸ Print and Export ▸ PDF: everything is rendered by the core.
  const docId = summary?.doc ?? null;
  const requestPreview = useCallback(
    async (settings: PrintSettings, sheet: number, scale: number) => {
      if (!client || docId === null) throw new Error('No document is open.');
      try {
        const r = await client.query(docId, { PrintPreview: { settings, sheet, scale } });
        if (!('PrintPreview' in r)) throw new Error('The core did not return a preview.');
        return r.PrintPreview;
      } catch (error) {
        throw new Error(errorText(KilnError.from(error)));
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
        throw new Error(errorText(KilnError.from(error)));
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
        throw new Error(errorText(KilnError.from(error)));
      }
    },
    [client, docId],
  );
  const printable = PRINTABLE.includes(app) && docId !== null;

  const browse = () => {
    pendingOpen.current = true;
    execute('suite.file.open');
  };
  const createBlank = (a: HomeApp) => {
    showRoute(`${appHash(a)}?new=${Date.now()}`);
  };
  // a template opens a new window on a new document made from it.
  const createFrom = (tpl: HomeTemplate) => {
    if (!tpl.file) return;
    const q = new URLSearchParams({ template: tpl.file, new: String(Date.now()) });
    showRoute(`${appHash(tpl.app)}?${q.toString()}`);
  };
  const backstage: Record<string, ReactNode | undefined> = {
    home: (
      <div className="kiln-bs-home">
        <BackstageFeatured app={app} onCreate={createBlank} onOpenTemplate={createFrom} />
        <BackstageOpenPage recent={recent} onOpenRecent={(f) => openRecent(f)} onBrowse={browse} />
      </div>
    ),
    new:
      TEMPLATES.length > 0 ? (
        <TemplateGallery
          templates={TEMPLATES}
          app={app as HomeApp}
          creatable={CREATABLE}
          onCreate={createBlank}
          onOpenTemplate={createFrom}
        />
      ) : (
        <BackstageNewPage app={app} creatable={CREATABLE} onCreate={createBlank} />
      ),
    open: <BackstageOpenPage recent={recent} onOpenRecent={(f) => openRecent(f)} onBrowse={browse} />,
    info: (
      <BackstageInfoPage
        summary={summary}
        status={infoStatus}
        properties={properties}
        onInspect={summary && isInfoApp(app) ? () => setInfoDialog('inspect') : undefined}
        onCheckCompat={summary && isInfoApp(app) ? () => setInfoDialog('compat') : undefined}
        onEditProperties={summary && isInfoApp(app) ? () => setInfoDialog('properties') : undefined}
        onVersionHistory={
          summary && registry.get('suite.file.version_history')
            ? () => execute('suite.file.version_history')
            : undefined
        }
      />
    ),
    saveAs: (
      <BackstageFormatsPage
        formats={saveChoices}
        intro={t('Choose a format. Kiln keeps everything it does not edit exactly as it was.')}
        onChoose={(format) => void saveAsChecked(format)}
      />
    ),
    saveCopy: (
      <BackstageFormatsPage
        formats={saveChoices}
        intro={t('Saves a copy and keeps working on this document.')}
        onChoose={(format) => void saveAsChecked(format)}
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
        slideOptions={app === 'deck'}
      />
    ) : undefined,
    export: (
      <>
        {printable ? (
          <ExportPdfPane onExport={exportPdf} textOptions={app === 'write'} slideOptions={app === 'deck'} />
        ) : null}
        {deckEditor?.exportPane ?? null}
        {exportChoices.length > 0 ? (
          <BackstageFormatsPage
            formats={exportChoices}
            intro={printable ? t('Other formats') : undefined}
            onChoose={(format) => void saveAsChecked(format)}
          />
        ) : null}
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
  const language = useLanguage();
  return useMemo(
    () => (
      <AppFrame
        app={app}
        registry={registry}
        keymap={keymap}
        title={summary?.title ?? (problem ? APP_NAMES[app] : t('Opening…'))}
        extension={extension}
        saveStatus={saveStatusOf(summary, problem)}
        autoSave={storage.autoSave}
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
        presence={collab.presence}
        leftPane={storage.pane}
        messageBar={storage.messageBar}
        rightPane={collab.pane}
        optionsSections={gridEditor?.optionsSections}
        onBackstagePageChange={(page) => {
          void loadBackstageData();
          // File ▸ Info shows the document's properties as the core reports them.
          if (page === 'info' && isInfoApp(app)) void loadProperties().catch(() => setProperties(null));
        }}
        ribbonTrailing={
          writeEditor?.comments ?? <StripButton command="suite.review.show_comments" label={t('Comments')} />
        }
        statusBar={
          editor?.statusBar ??
          (frameCore ? <FrameStatusBar app={app} core={frameCore} execute={execute} /> : null)
        }
        // The desktop app keeps a document in its own window: no Back there.
        onBack={isTauri() && !isAndroid() ? undefined : goBack}
        phoneMenu={writeEditor?.phoneMenu}
        shareCopy={shareCopy}
        textView={
          textViewOn && client && summary && textViewApp ? (
            <AccessibleTextView
              client={client}
              doc={summary.doc}
              app={textViewApp}
              onClose={() => {
                setTextView(false);
                requestAnimationFrame(() => window.dispatchEvent(new CustomEvent('kiln:return-to-document')));
              }}
            />
          ) : undefined
        }
      >
        {editor?.surface ?? (
          <SurfaceSlot app={app} summary={summary} problem={problem} ready={!!connection} />
        )}
        {collab.overlay}
        {collab.dialogs}
        {storage.dialogs}
        {textEncoding}
        {infoDialogs}
        <Dialog
          open={macroAsk !== null}
          onOpenChange={(open) => {
            if (!open) setMacroAsk(null);
          }}
        >
          <DialogContent
            title={APP_NAMES[app]}
            size="md"
            role="alertdialog"
            data-testid="macro-free-question"
            footer={
              <>
                <Button
                  variant="primary"
                  onClick={() => {
                    const format = macroAsk;
                    setMacroAsk(null);
                    if (format) execute('suite.file.save_as', { format });
                  }}
                >
                  {t('Yes')}
                </Button>
                <Button
                  onClick={() => {
                    setMacroAsk(null);
                    // Back to Save As, with the macro-enabled format at hand.
                    requestAnimationFrame(() =>
                      document
                        .querySelector<HTMLElement>(
                          '[data-format="Docm"], [data-format="Xlsm"], [data-format="Pptm"]',
                        )
                        ?.focus(),
                    );
                  }}
                >
                  {t('No')}
                </Button>
              </>
            }
          >
            <p>{t('The following cannot be saved in a macro-free document:')}</p>
            <ul>
              <li>{t('VBA project')}</li>
            </ul>
            <p>
              {t(
                'To save a file with these features, click No to return to Save As, and then choose a macro-enabled file type. Continue saving as a macro-free document?',
              )}
            </p>
          </DialogContent>
        </Dialog>
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
      properties,
      infoDialogs,
      coreInfo,
      infoStatus,
      theme.preference,
      theme.setPreference,
      editor,
      frameCore,
      connection,
      execute,
      loadBackstageData,
      loadProperties,
      saveAsChecked,
      macroAsk,
      textEncoding,
      collab,
      storage,
      textViewOn,
      textViewApp,
      extension,
      shareCopy,
      writeEditor?.phoneMenu,
      // The frame's text follows the interface language.
      language,
    ],
  );
}

/** The apps whose File ▸ Info has Inspect Document, the Compatibility Checker and Properties. */
function isInfoApp(app: AppId): app is CompatApp {
  return app === 'write' || app === 'grid' || app === 'deck';
}

/** Formats that cannot hold a VBA project. */
const MACRO_FREE: readonly FileFormat[] = ['Docx', 'Dotx', 'Xlsx', 'Xltx', 'Pptx', 'Potx', 'Ppsx'];

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
    prev.app !== summary.app ||
    JSON.stringify(prev.autosave) !== JSON.stringify(summary.autosave) ||
    prev.save_formats.length !== summary.save_formats.length
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
function saveStatusOf(summary: DocSummary | null, problem: FrameProblem | null): string | undefined {
  if (problem) return t('Not available');
  if (!summary) return undefined;
  if (summary.read_only) return t('Read-only');
  if (!summary.path) return summary.dirty ? t('Not saved yet') : t('New document');
  return summary.dirty ? t('Unsaved changes') : t('Saved');
}
