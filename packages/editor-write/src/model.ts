/**
 * The Write editor's view model: UI state only (which panes and dialogs are open, the ruler, the
 * mini toolbar) plus the latest answers of the core's `Query::Write` — the selection's formatting,
 * styles, headings, pages, rulers and Find results. Nothing here is computed from the document:
 * after every `Event::Changed` of the window's document the model asks the core again (coalesced
 * to one round trip per animation frame) and stores what it says (PLAN.md §3.7).
 */
import { buildCommand, type CommandRegistry } from '@kiln/commands';
import type {
  DocId,
  FindQuery,
  FindResults,
  FormatState,
  HeadingInfo,
  KilnClient,
  PagesInfo,
  RulerInfo,
  Selection,
  StatusBar,
  StyleInfo,
  WriteQuery,
  WriteQueryResult,
} from '@kiln/protocol';
import type { SurfaceController } from '@kiln/surface';
import { createStore, type StoreApi } from 'zustand/vanilla';

/** The dialogs of the Write editor. */
export type WriteDialog =
  | 'font'
  | 'paragraph'
  | 'tabs'
  | 'pageSetup'
  | 'insertTable'
  | 'find'
  | 'link'
  | 'symbol'
  | 'dateTime'
  | 'wordCount'
  | 'pasteSpecial'
  | 'newStyle'
  | 'modifyStyle'
  | 'bookmark';

export type NavTab = 'headings' | 'pages' | 'results';

/** Scroll position and zoom of the surface (for the rulers). */
export interface SurfaceView {
  scrollX: number;
  scrollY: number;
  zoom: number;
  width: number;
  height: number;
}

export interface WriteUiState {
  /** The selection's formatting (ribbon toggles, boxes, dialogs). */
  format: FormatState | null;
  selection: Selection | null;
  /** The status bar fields of the last change (page, words, view, zoom). */
  status: StatusBar | null;
  styles: StyleInfo[] | null;
  headings: HeadingInfo[] | null;
  pages: PagesInfo | null;
  rulers: RulerInfo | null;
  findQuery: FindQuery;
  findResults: FindResults | null;
  showRuler: boolean;
  navOpen: boolean;
  navTab: NavTab;
  stylesOpen: boolean;
  dialog: { id: WriteDialog; tab?: string; arg?: string } | null;
  view: SurfaceView | null;
  /** Viewport rectangle of the selection for the mini toolbar (`null`: hidden). */
  mini: { left: number; top: number; width: number; height: number } | null;
  /** Surface errors and core rejections worth telling the user about. */
  notice: string | null;
  /** Incremented to ask the Navigation pane to focus its search box (Ctrl+F). */
  searchFocus: number;
}

export const EMPTY_FIND: FindQuery = {
  text: '',
  match_case: false,
  whole_word: false,
  wildcards: false,
  regex: false,
};

/** What the model needs from its host. */
export interface WriteModelDeps {
  client: KilnClient;
  doc: DocId;
  registry: CommandRegistry;
  /** Remembered UI preferences (ruler, Navigation pane). */
  prefs?: { showRuler?: boolean; navOpen?: boolean };
}

const PREF_KEY = 'kiln.write.ui';

function loadPrefs(): Partial<Pick<WriteUiState, 'showRuler' | 'navOpen'>> {
  try {
    const raw = localStorage.getItem(PREF_KEY);
    const v = raw ? (JSON.parse(raw) as Record<string, unknown>) : {};
    return {
      ...(typeof v.showRuler === 'boolean' ? { showRuler: v.showRuler } : {}),
      ...(typeof v.navOpen === 'boolean' ? { navOpen: v.navOpen } : {}),
    };
  } catch {
    return {};
  }
}

function savePrefs(s: WriteUiState): void {
  try {
    localStorage.setItem(PREF_KEY, JSON.stringify({ showRuler: s.showRuler, navOpen: s.navOpen }));
  } catch {
    // Private mode: the preference lasts for this window only.
  }
}

/** Unwraps a `QueryResult::Write`. */
export function writeResult(r: unknown): WriteQueryResult | null {
  return typeof r === 'object' && r !== null && 'Write' in r
    ? (r as { Write: WriteQueryResult }).Write
    : null;
}

export class WriteEditorModel {
  readonly store: StoreApi<WriteUiState>;
  readonly client: KilnClient;
  readonly doc: DocId;
  readonly registry: CommandRegistry;
  controller: SurfaceController | null = null;
  private frame: { cancel(): void } | null = null;
  private wanted = {
    format: false,
    rulers: false,
    headings: false,
    styles: false,
    pages: false,
    find: false,
  };
  private revision = -1;
  /** The latest change not yet shown, and the pending after-paint callback. */
  private changed: { selection: Selection; status: StatusBar; content: boolean } | null = null;
  private afterPaint: { cancel(): void } | null = null;
  private disposed = false;
  private readonly unsubscribe: () => void;

  constructor({ client, doc, registry, prefs }: WriteModelDeps) {
    this.client = client;
    this.doc = doc;
    this.registry = registry;
    const saved = loadPrefs();
    this.store = createStore<WriteUiState>(() => ({
      format: null,
      selection: null,
      status: null,
      styles: null,
      headings: null,
      pages: null,
      rulers: null,
      findQuery: EMPTY_FIND,
      findResults: null,
      showRuler: saved.showRuler ?? prefs?.showRuler ?? true,
      navOpen: saved.navOpen ?? prefs?.navOpen ?? false,
      navTab: 'headings',
      stylesOpen: false,
      dialog: null,
      view: null,
      mini: null,
      notice: null,
      searchFocus: 0,
    }));
    this.unsubscribe = client.subscribe((e) => {
      if (typeof e !== 'object' || !('Changed' in e) || e.Changed.summary.doc !== this.doc) return;
      // The panes, rulers and status bar follow after the document has painted: React renders
      // synchronously for store changes, and a keystroke's frame must not wait for them (B5).
      const revised = e.Changed.summary.revision !== this.revision;
      this.revision = e.Changed.summary.revision;
      this.changed = {
        selection: e.Changed.selection,
        status: e.Changed.status,
        content: revised || (this.changed?.content ?? false),
      };
      if (this.afterPaint) return;
      this.afterPaint = afterPaint(() => {
        this.afterPaint = null;
        const c = this.changed;
        this.changed = null;
        if (!c || this.disposed) return;
        const patch: Partial<WriteUiState> = { selection: c.selection };
        if (JSON.stringify(c.status) !== JSON.stringify(this.state.status)) patch.status = c.status;
        this.store.setState(patch);
        this.refresh({ content: c.content });
      });
    });
    this.refresh({ content: true });
    // The status bar's figures before the first change (page count, words, zoom).
    client.query(doc, 'Status').then(
      (r) => {
        if (!this.disposed && !this.state.status && typeof r === 'object' && r !== null && 'Status' in r)
          this.store.setState({ status: r.Status });
      },
      () => undefined,
    );
  }

  get state(): WriteUiState {
    return this.store.getState();
  }

  set(patch: Partial<WriteUiState>): void {
    this.store.setState(patch);
    if ('showRuler' in patch || 'navOpen' in patch) savePrefs(this.store.getState());
    if (patch.showRuler || patch.navOpen || patch.stylesOpen) this.refresh({ content: true });
  }

  /** Asks the core again (next animation frame; several changes make one round trip). */
  refresh({ content }: { content: boolean }): void {
    const s = this.state;
    this.wanted.format = true;
    this.wanted.rulers ||= s.showRuler;
    this.wanted.headings ||= content && s.navOpen;
    this.wanted.pages ||= content && s.navOpen && s.navTab === 'pages';
    this.wanted.styles ||= s.stylesOpen;
    this.wanted.find ||= content && s.navOpen && s.findQuery.text !== '';
    if (this.frame) return;
    this.frame = afterPaint(() => {
      this.frame = null;
      void this.flush();
    });
  }

  private async ask(q: WriteQuery): Promise<WriteQueryResult | null> {
    try {
      return writeResult(await this.client.query(this.doc, { Write: q }));
    } catch {
      return null;
    }
  }

  private async flush(): Promise<void> {
    const w = this.wanted;
    this.wanted = { format: false, rulers: false, headings: false, styles: false, pages: false, find: false };
    const patch: Partial<WriteUiState> = {};
    const jobs: Promise<void>[] = [];
    if (w.format)
      jobs.push(
        this.ask('Format').then((r) => {
          if (r && 'Format' in r) patch.format = r.Format;
        }),
      );
    if (w.rulers)
      jobs.push(
        this.ask('Rulers').then((r) => {
          if (r && 'Rulers' in r) patch.rulers = r.Rulers;
        }),
      );
    if (w.headings)
      jobs.push(
        this.ask('Headings').then((r) => {
          if (r && 'Headings' in r) patch.headings = r.Headings;
        }),
      );
    if (w.pages)
      jobs.push(
        this.ask('Pages').then((r) => {
          if (r && 'Pages' in r) patch.pages = r.Pages;
        }),
      );
    if (w.styles)
      jobs.push(
        this.ask('Styles').then((r) => {
          if (r && 'Styles' in r) patch.styles = r.Styles;
        }),
      );
    if (w.find)
      jobs.push(
        this.ask({ FindAll: { query: this.state.findQuery } }).then((r) => {
          if (r && 'FindAll' in r) patch.findResults = r.FindAll;
        }),
      );
    await Promise.all(jobs);
    if (this.disposed) return;
    // Unchanged answers keep their old objects, so nothing re-renders for them (typing in one
    // paragraph asks again after every key; the answers rarely change).
    for (const key of Object.keys(patch) as (keyof WriteUiState)[]) {
      const json = JSON.stringify(patch[key]);
      if (this.answers.get(key) === json) delete patch[key];
      else this.answers.set(key, json);
    }
    if (Object.keys(patch).length === 0) return;
    this.store.setState(patch);
    if (patch.format)
      this.registry.setContext({ textFormat: patch.format, contexts: contextsOf(patch.format) });
  }

  /** The last answer of each query, serialised, to drop answers that did not change. */
  private readonly answers = new Map<keyof WriteUiState, string>();

  /** Asks one question now. */
  query(q: WriteQuery): Promise<WriteQueryResult | null> {
    return this.ask(q);
  }

  /** Runs a command through the registry (so the command log and the recent list see it). */
  async run(id: string, args?: unknown): Promise<boolean> {
    const r = await this.registry.execute(id, args);
    if (r.status === 'failed') {
      const e = r.error as { message?: unknown } | undefined;
      this.store.setState({
        notice: typeof e?.message === 'string' ? e.message : 'The command could not run.',
      });
      return false;
    }
    if (r.status !== 'ran') return false;
    this.controller?.reveal();
    return true;
  }

  /**
   * Sends a core command straight to the core, in order with the typing before it, without the
   * registry's availability check (the clipboard events of the document: the selection a key
   * acts on is the core's, which the window may not have heard of yet).
   */
  async runCore(id: string, args?: unknown): Promise<boolean> {
    try {
      await this.client.execute(this.doc, buildCommand(id, args, { app: 'write' }));
      this.controller?.reveal();
      return true;
    } catch (error) {
      const e = error as { message?: unknown; kind?: unknown };
      if (e.kind !== 'Unavailable')
        this.store.setState({
          notice: typeof e.message === 'string' ? e.message : 'The command could not run.',
        });
      return false;
    }
  }

  /** Searches the document for the Navigation pane's results. */
  async find(query: FindQuery): Promise<void> {
    this.store.setState({ findQuery: query });
    if (!query.text) {
      this.store.setState({ findResults: null });
      return;
    }
    const r = await this.ask({ FindAll: { query } });
    if (r && 'FindAll' in r && !this.disposed) this.store.setState({ findResults: r.FindAll });
  }

  /** Asks the Navigation pane to focus its search box. */
  focusSearch(): void {
    this.store.setState((s) => ({ searchFocus: s.searchFocus + 1 }));
  }

  openDialog(id: WriteDialog, tab?: string, arg?: string): void {
    this.store.setState({ dialog: { id, ...(tab ? { tab } : {}), ...(arg ? { arg } : {}) } });
  }

  closeDialog(): void {
    this.store.setState({ dialog: null });
    this.controller?.focus();
  }

  dispose(): void {
    this.disposed = true;
    this.afterPaint?.cancel();
    this.frame?.cancel();
    this.unsubscribe();
    this.registry.setContext({ textFormat: undefined, contexts: undefined });
  }
}

/**
 * The contextual ribbon tabs the selection asks for (Table Design/Layout in a table, Header &
 * Footer while one is edited), from what the core reported.
 */
export function contextsOf(format: FormatState): string[] {
  const out: string[] = [];
  if (format.table) out.push('table');
  if (format.header_footer || format.story === 'Header' || format.story === 'Footer')
    out.push('header_footer');
  return out;
}

/**
 * Runs `fn` once the next frame has painted (a task after the next animation frame), so work
 * that follows a document change never delays the document's own paint.
 */
export function afterPaint(fn: () => void): { cancel(): void } {
  let timer: ReturnType<typeof setTimeout> | null = null;
  let raf = 0;
  const later = () => {
    raf = 0;
    timer = setTimeout(() => {
      timer = null;
      fn();
    }, 0);
  };
  if (typeof requestAnimationFrame === 'function') raf = requestAnimationFrame(later);
  else later();
  return {
    cancel() {
      if (raf && typeof cancelAnimationFrame === 'function') cancelAnimationFrame(raf);
      if (timer) clearTimeout(timer);
    },
  };
}
