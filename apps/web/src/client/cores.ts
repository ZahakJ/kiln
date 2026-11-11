/**
 * The web runs one WebAssembly core per app (budget B12): `cargo xtask wasm` builds
 * `kiln-bridge-wasm` once with each app's feature into `src/wasm/<app>/`, and each core runs in its
 * own Worker, started when the first document of its app is created or opened. {@link CoreRouter}
 * is the one {@link KilnClient} the chrome sees: it sends every call to the core that holds the
 * document, so the chrome does not know how many cores there are. A core asked for another app's
 * document or command answers with its usual error (`Unsupported`, `NotFound`).
 *
 * Document ids tell the cores apart: the Write core numbers its documents 1, 5, 9, …, Grid 2, 6,
 * 10, … and Deck 3, 7, 11, … (`DOC_IDS` in `crates/kiln-bridge-wasm`), so `doc % 4` names the core.
 * Desktop and Android talk to the full native core and do not use this.
 */

import {
  type A11yTree,
  type Command,
  type CoreInfo,
  type DocId,
  type DocKind,
  type DocSummary,
  type EventListener,
  type ExecResult,
  type FileFilter,
  type FileFormat,
  type HostHandler,
  type HostReply,
  type KilnClient,
  KilnError,
  Listeners,
  type OpenHint,
  type Query,
  type QueryResult,
  type RequestId,
  type SceneDelta,
  type Viewport,
} from '@kiln/protocol';

/** The apps that have a web core. */
export type CoreApp = 'write' | 'grid' | 'deck';
export const CORE_APPS: readonly CoreApp[] = ['write', 'grid', 'deck'];

/** `doc % 4` of each core's documents (`DOC_IDS` in `crates/kiln-bridge-wasm/src/lib.rs`). */
const DOC_TAG: Record<CoreApp, number> = { write: 1, grid: 2, deck: 3 };

/** The core that holds a document, from its id. */
export function coreOfDoc(doc: DocId): CoreApp | null {
  const tag = doc % 4;
  return CORE_APPS.find((app) => DOC_TAG[app] === tag) ?? null;
}

/** The core that creates documents of a kind (the developer scratch document is Write's). */
export function coreOfKind(kind: DocKind): CoreApp | null {
  switch (kind) {
    case 'Scratch':
    case 'Write':
      return 'write';
    case 'Grid':
      return 'grid';
    case 'Deck':
      return 'deck';
    case 'Notes':
      return null;
  }
}

/**
 * The core that opens each format on the web (what each app's `AppFactory::opens` accepts in the
 * web build, which has no OpenDocument reader). HTML, OpenDocument and PDF are opened by no web core.
 */
const FORMAT_CORE: Record<FileFormat, CoreApp | null> = {
  Txt: 'write',
  Md: 'write',
  Docx: 'write',
  Dotx: 'write',
  Docm: 'write',
  Dotm: 'write',
  Html: null,
  Odt: null,
  Xlsx: 'grid',
  Xltx: 'grid',
  Xlsm: 'grid',
  Csv: 'grid',
  Ods: null,
  Pptx: 'deck',
  Potx: 'deck',
  Pptm: 'deck',
  Ppsx: 'deck',
  Odp: null,
  Pdf: null,
  // Kiln's own bundles.
  KWrite: 'write',
  KGrid: 'grid',
  KDeck: 'deck',
};

/** The format of a file name or path, from its extension (as `FileFormat::from_path`). */
export function formatOfPath(path: string): FileFormat | null {
  const name = path.split(/[/\\]/).pop() ?? path;
  const dot = name.lastIndexOf('.');
  if (dot < 0) return null;
  const ext = name.slice(dot + 1).toLowerCase();
  if (ext === 'htm') return 'Html';
  return (Object.keys(FORMAT_CORE) as FileFormat[]).find((f) => f.toLowerCase() === ext) ?? null;
}

/** The core that opens a file, from its hint (kind, then format, then name or path). */
export function coreOfHint(hint: OpenHint): CoreApp | null {
  if (hint.kind) return coreOfKind(hint.kind);
  const format = hint.format ?? formatOfPath(hint.name ?? hint.path ?? '');
  return format ? FORMAT_CORE[format] : null;
}

/** What File ▸ Open offers on the web: every format some web core opens. */
export const OPEN_FILTERS: FileFilter[] = [
  {
    name: 'All supported files',
    extensions: (Object.keys(FORMAT_CORE) as FileFormat[])
      .filter((f) => FORMAT_CORE[f] !== null)
      .map((f) => f.toLowerCase()),
  },
];

function unsupported(message: string): KilnError {
  return new KilnError({ kind: 'Unsupported', message: `not supported: ${message}` });
}

function openPath(command: Command): { path: string | null } | null {
  if (typeof command !== 'object' || !('Suite' in command)) return null;
  const suite = command.Suite;
  return typeof suite === 'object' && 'Open' in suite ? suite.Open : null;
}

/** Starts the core of an app and resolves once it is ready. */
export type StartCore = (app: CoreApp) => Promise<KilnClient>;

/** The page's client on the web: one lazily started core per app behind one {@link KilnClient}. */
export class CoreRouter implements KilnClient {
  readonly bridge = 'wasm' as const;
  private readonly cores = new Map<CoreApp, Promise<KilnClient>>();
  private readonly listeners = new Listeners();
  /** Which core sent each pending host request (ids are per core). */
  private readonly requests = new Map<number, CoreApp>();
  /** The core for session calls without a document: the current window's app. */
  private primary: CoreApp = 'write';
  private disposed = false;

  constructor(
    private readonly start: StartCore,
    /** Answers the File ▸ Open dialog that the router shows before choosing a core. */
    private readonly host: HostHandler,
  ) {}

  /** Starts (or reuses) an app's core; a core that failed to start is retried on the next call. */
  core(app: CoreApp): Promise<KilnClient> {
    if (this.disposed)
      return Promise.reject(new KilnError({ kind: 'Closed', message: 'the client was disposed' }));
    let core = this.cores.get(app);
    if (!core) {
      const started = this.start(app).then((client) => {
        client.subscribe((event) => {
          if (typeof event === 'object' && 'HostRequest' in event) this.remember(event.HostRequest.id, app);
          this.listeners.emit(event);
        });
        return client;
      });
      started.catch(() => {
        if (this.cores.get(app) === started) this.cores.delete(app);
      });
      this.cores.set(app, started);
      core = started;
    }
    return core;
  }

  /**
   * Notes which core sent a host request. The cores answer their own requests, so this is only for
   * a caller that answers one itself; the oldest entries are dropped to keep it small.
   */
  private remember(id: number, app: CoreApp): void {
    this.requests.set(id, app);
    if (this.requests.size > 256) {
      const oldest = this.requests.keys().next();
      if (!oldest.done) this.requests.delete(oldest.value);
    }
  }

  /** The apps whose cores have been started (diagnostics and tests). */
  started(): CoreApp[] {
    return [...this.cores.keys()];
  }

  /** Every running core has handled what it was sent so far, and its events have arrived. */
  async caughtUp(): Promise<void> {
    await Promise.allSettled([...this.cores.values()].map(async (core) => (await core).ping()));
  }

  /** Makes `app`'s core answer the session calls that name no document (Ping, Backstage). */
  prefer(app: CoreApp): void {
    this.primary = app;
  }

  private forDoc(doc: DocId): Promise<KilnClient> {
    const app = coreOfDoc(doc);
    if (!app)
      return Promise.reject(new KilnError({ kind: 'NotFound', message: `document ${doc} not found` }));
    return this.core(app);
  }

  /** The core for a call: the document's, else the command's app, else the current window's. */
  private coreFor(doc: DocId | null, command?: Command): Promise<KilnClient> {
    if (doc !== null) return this.forDoc(doc);
    if (command && typeof command === 'object') {
      if ('Write' in command) return this.core('write');
      if ('Grid' in command) return this.core('grid');
      if ('Deck' in command) return this.core('deck');
      if ('Notes' in command) return Promise.reject(unsupported('Kiln Notes is not available in this build'));
      const suite = command.Suite;
      // A new document, and one recovered from its autosave journal, go to its app's core.
      const kind =
        typeof suite === 'object' && 'New' in suite
          ? suite.New.kind
          : typeof suite === 'object' && 'RecoveryOpen' in suite
            ? suite.RecoveryOpen.kind
            : null;
      if (kind) {
        const app = coreOfKind(kind);
        if (!app) return Promise.reject(unsupported(`${kind} documents are not available in this build`));
        return this.core(app);
      }
    }
    return this.core(this.primary);
  }

  async ping(): Promise<CoreInfo> {
    return (await this.core(this.primary)).ping();
  }

  async execute(doc: DocId | null, command: Command): Promise<ExecResult> {
    const open = openPath(command);
    if (open) return this.open_(open.path);
    return (await this.coreFor(doc, command)).execute(doc, command);
  }

  /**
   * File ▸ Open: the router shows the dialog itself (the core that opens the file is chosen by its
   * extension), then the chosen core reads and opens it and emits `Opened` as before.
   */
  private async open_(path: string | null): Promise<ExecResult> {
    let target = path;
    if (target === null) {
      const reply = await this.host.handle({ PickOpen: { filters: OPEN_FILTERS } });
      if (reply === 'Cancelled') return { outcome: 'Unchanged', summary: null, value: 'None' };
      if (typeof reply === 'object' && 'Failed' in reply) throw new KilnError(reply.Failed);
      if (typeof reply !== 'object' || !('Path' in reply))
        throw new KilnError({ kind: 'Internal', message: 'internal error: the file dialog gave no path' });
      target = reply.Path;
    }
    const format = formatOfPath(target);
    if (!format) throw unsupported('a file without a known extension');
    const app = FORMAT_CORE[format];
    if (!app) throw unsupported(`.${format.toLowerCase()} files`);
    return (await this.core(app)).execute(null, { Suite: { Open: { path: target } } });
  }

  async query(doc: DocId | null, query: Query): Promise<QueryResult> {
    // the recent files are in the page's storage, which every core writes; Home reads them
    // without starting a core.
    if (doc === null && query === 'Recents') {
      const reply = await this.host.handle('Recents');
      return { Recents: typeof reply === 'object' && 'Recents' in reply ? reply.Recents : [] };
    }
    // The documents open in this page: every core already started has its own (Kiln Home lists
    // them under Open in this window); no core is started to ask.
    if (doc === null && query === 'Documents') {
      const lists = await Promise.all(
        [...this.cores.values()].map((core) =>
          core.then((c) => c.query(null, 'Documents')).catch(() => ({ Documents: [] }) as QueryResult),
        ),
      );
      return { Documents: lists.flatMap((r) => ('Documents' in r ? r.Documents : [])) };
    }
    return (await this.coreFor(doc)).query(doc, query);
  }

  async open(bytes: Uint8Array, hint: OpenHint): Promise<DocSummary> {
    const app = coreOfHint(hint);
    if (!app) {
      const format = hint.format ?? formatOfPath(hint.name ?? hint.path ?? '');
      throw unsupported(format ? `.${format.toLowerCase()} files` : 'a file without a known extension');
    }
    return (await this.core(app)).open(bytes, hint);
  }

  async new(kind: DocKind): Promise<DocSummary> {
    const app = coreOfKind(kind);
    if (!app) throw unsupported(`${kind} documents are not available in this build`);
    return (await this.core(app)).new(kind);
  }

  async setViewport(doc: DocId, viewport: Viewport): Promise<void> {
    return (await this.forDoc(doc)).setViewport(doc, viewport);
  }

  async frame(doc: DocId): Promise<SceneDelta> {
    return (await this.forDoc(doc)).frame(doc);
  }

  async a11y(doc: DocId): Promise<A11yTree> {
    return (await this.forDoc(doc)).a11y(doc);
  }

  subscribe(listener: EventListener): () => void {
    return this.listeners.add(listener);
  }

  /** loads the collaboration module into the core of `doc` (or of `app`). */
  async loadCollab(doc: DocId | null, app?: string): Promise<void> {
    const core =
      doc !== null ? this.forDoc(doc) : this.core(CORE_APPS.find((a) => a === app) ?? this.primary);
    await (await core).loadCollab?.(doc, app);
  }

  async hostReply(id: RequestId, reply: HostReply): Promise<void> {
    const app = this.requests.get(id) ?? this.primary;
    this.requests.delete(id);
    return (await this.core(app)).hostReply(id, reply);
  }

  dispose(): void {
    this.disposed = true;
    for (const core of this.cores.values())
      void core.then(
        (c) => c.dispose(),
        () => undefined,
      );
    this.cores.clear();
    this.listeners.clear();
  }
}
