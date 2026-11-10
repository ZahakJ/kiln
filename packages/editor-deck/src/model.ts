/**
 * The Deck editor's view model: UI state only (dialogs, menus, which pane has the keyboard, the
 * zoom-to-fit choice) plus the latest answer of the core's `Query::Deck(State)` — the slides and
 * sections, the selection, the text being edited, the formatting the ribbon shows, the notes.
 * Nothing is computed from the presentation: after every `Event::Changed` of the window's document
 * the model asks the core again (one round trip per animation frame) and stores what it says.
 */

import { buildCommand, type CommandRegistry } from '@kiln/commands';
import type {
  DeckCursor,
  DeckQuery,
  DeckQueryResult,
  DeckShapeIcon,
  DeckState,
  DocId,
  KilnClient,
  SceneDelta,
  StatusBar,
  SuiteCommand,
} from '@kiln/protocol';
import { errorText, showRibbonTab } from '@kiln/ui-kit';
import { createStore, type StoreApi } from 'zustand/vanilla';
import { rulerPreference } from './views/viewCommands.ts';

/** The dialogs of the Deck editor. */
export type DeckDialog =
  | 'slideSize'
  | 'formatBackground'
  | 'insertTable'
  | 'headerFooter'
  | 'sizePosition'
  | 'altText'
  | 'link'
  | 'find'
  | 'replace'
  | 'replaceFonts'
  | 'renameSection'
  | 'shapes'
  | 'changeShape'
  | 'crop'
  | 'selectionPane'
  | 'pasteSpecial'
  | 'font'
  | 'paragraph'
  | 'symbol'
  | 'setUpShow'
  | 'equation'
  | 'insertChart'
  | 'chartData'
  | 'renameMaster'
  | 'icons'
  | 'codeBlock'
  | 'formatShape'
  | 'splitCells'
  | 'cellMargins';

/** What starting a slide show asks the window for. */
export interface PresentRequest {
  /** The slide to start at (index), or the first of the show. */
  from: number | null;
  /** The show, or Rehearse Timings. */
  mode: 'Show' | 'Rehearse';
  /** In this browser tab, not full screen (Present in Browser Tab). */
  tab: boolean;
  /** Presenter View in this window, the show in a second one. */
  presenter: boolean;
  /** A pointer tool to pick up at once (the Audience buttons). */
  tool?: 'laser' | 'pen' | 'highlighter' | 'black';
}

/** A context menu: where (client px) and for what. */
export interface DeckMenu {
  x: number;
  y: number;
  kind: 'rail' | 'section' | 'shape' | 'slide' | 'text';
  /** The section of a section menu. */
  section?: number;
}

export interface DeckUiState {
  /** The core's answer to `Query::Deck(State)`. */
  deck: DeckState | null;
  /** The status bar fields of the last change (zoom, view). */
  status: StatusBar | null;
  dialog: { id: DeckDialog; arg?: number } | null;
  menu: DeckMenu | null;
  /** A message for the user (a refused command). */
  notice: string | null;
  /** The stage zooms to fit the window (Fit to Window; off once a zoom is chosen). */
  fit: boolean;
  /** The thumbnail rail has the keyboard. */
  rail: boolean;
  /** The Format Painter holds formatting for the next click on the slide. */
  painter: boolean;
  /** The find text, kept between Find and Replace. */
  find: { text: string; with: string; matchCase: boolean; wholeWord: boolean };
  /** The Animation Pane is open. */
  animPane: boolean;
  /** The Animation Painter holds effects for the next click on a shape. */
  animPainter: boolean;
  /** A preview plays on the stage. */
  preview: 'transition' | 'animations' | null;
  /** The ribbon tab shown (the stage numbers the effects on the Animations tab). */
  tab: string;
  /** The Design Ideas pane is open. */
  designPane: boolean;
  /** Outline View: the outline instead of the thumbnail rail. */
  outline: boolean;
  /** The Reuse Slides pane is open. */
  reusePane: boolean;
  /** The Format Chart pane is open. */
  chartPane: boolean;
  /** The Comments pane is open. */
  commentsPane: boolean;
  /** The Comments pane shows a box for a new comment. */
  composer: boolean;
  /** The Spelling pane is open. */
  spellingPane: boolean;
  /** View ▸ Ruler. */
  ruler: boolean;
  /** Reading View's show runs. */
  reading: boolean;
}

export interface DeckModelDeps {
  client: KilnClient;
  doc: DocId;
  registry: CommandRegistry;
}

/** Unwraps a `QueryResult::Deck`. */
export function deckResult(r: unknown): DeckQueryResult | null {
  return typeof r === 'object' && r !== null && 'Deck' in r ? (r as { Deck: DeckQueryResult }).Deck : null;
}

/** The message of a rejected command. */
export function messageOf(error: unknown): string {
  return errorText(error);
}

/** What a state was asked after: every command sent before `sentBefore` was answered when `settled`. */
export interface StateFreshness {
  sentBefore: number;
  settled: boolean;
}

/** The stage the model drives (so it can focus it and fit the zoom). */
export interface DeckStageHandle {
  focus(): void;
  /** Zooms to fit the window. */
  fitToWindow(): void;
  /** The stage's size in CSS pixels. */
  size(): { width: number; height: number };
  /** Where the slide is on the stage (CSS px from its top-left) and the zoom. */
  view?(): { zoom: number; left: number; top: number; width: number; height: number };
}

/** The keyboard scopes of the keymap for a state (`object`: shapes selected, not editing). */
export function scopesOf(s: DeckState | null, rail: boolean): string[] {
  if (rail) return ['rail'];
  if (!s) return [];
  if (s.editing) return ['text-edit'];
  return s.selection.length > 0 ? ['object'] : [];
}

export class DeckEditorModel {
  readonly store: StoreApi<DeckUiState>;
  readonly client: KilnClient;
  readonly doc: DocId;
  readonly registry: CommandRegistry;
  stage: DeckStageHandle | null = null;
  /**
   * Starts a slide show in the window (the web app's route goes to `#/present`); set by the app.
   * Without one the show runs in the core and nothing shows it.
   */
  present: ((req: PresentRequest) => void) | null = null;
  /** The ids the editor sent straight to the core, newest last (the last 200; e2e tests). */
  readonly sent: string[] = [];
  private pending: { cancel(): void } | null = null;
  private stateKey = '';
  private disposed = false;
  private readonly unsubscribe: () => void;
  private readonly listeners = new Set<(s: DeckState, fresh: StateFreshness) => void>();
  /** Commands sent straight to the core, and those not answered yet. */
  sentCount = 0;
  private inflight = 0;
  /** The commands sent and not answered (the stage waits for them before asking for a frame). */
  private queue: Promise<unknown> = Promise.resolve();
  private icons: Promise<DeckShapeIcon[]> | null = null;

  constructor({ client, doc, registry }: DeckModelDeps) {
    this.client = client;
    this.doc = doc;
    this.registry = registry;
    this.store = createStore<DeckUiState>(() => ({
      deck: null,
      status: null,
      dialog: null,
      menu: null,
      notice: null,
      fit: true,
      rail: false,
      painter: false,
      find: { text: '', with: '', matchCase: false, wholeWord: false },
      animPane: false,
      animPainter: false,
      preview: null,
      tab: '',
      designPane: false,
      outline: false,
      reusePane: false,
      chartPane: false,
      commentsPane: false,
      composer: false,
      spellingPane: false,
      ruler: rulerPreference(),
      reading: false,
    }));
    this.unsubscribe = client.subscribe((e) => {
      if (typeof e !== 'object' || !('Changed' in e) || e.Changed.summary.doc !== this.doc) return;
      const status = e.Changed.status;
      if (JSON.stringify(status) !== JSON.stringify(this.state.status)) this.store.setState({ status });
      this.refresh();
    });
    this.refresh();
    client.query(doc, 'Status').then(
      (r) => {
        if (!this.disposed && typeof r === 'object' && r !== null && 'Status' in r)
          this.store.setState({ status: r.Status });
      },
      () => undefined,
    );
  }

  get state(): DeckUiState {
    return this.store.getState();
  }

  set(patch: Partial<DeckUiState>): void {
    this.store.setState(patch);
    if ('rail' in patch) this.registry.setContext({ scopes: scopesOf(this.state.deck, this.state.rail) });
    // Toggles of the chrome (Format Painter, Fit to Window) re-read their state from the context.
    if (
      'painter' in patch ||
      'fit' in patch ||
      'animPane' in patch ||
      'animPainter' in patch ||
      'designPane' in patch ||
      'outline' in patch ||
      'reusePane' in patch ||
      'commentsPane' in patch ||
      'spellingPane' in patch ||
      'ruler' in patch
    )
      this.registry.setContext({
        deckUi: {
          painter: this.state.painter,
          fit: this.state.fit,
          animPane: this.state.animPane,
          animPainter: this.state.animPainter,
          designPane: this.state.designPane,
          outline: this.state.outline,
          reusePane: this.state.reusePane,
          commentsPane: this.state.commentsPane,
          spellingPane: this.state.spellingPane,
          ruler: this.state.ruler,
        },
      });
  }

  /** Follows every new state (returns the unsubscribe function). */
  onState(listener: (s: DeckState, fresh: StateFreshness) => void): () => void {
    this.listeners.add(listener);
    return () => this.listeners.delete(listener);
  }

  /** Asks the core for the state (coalesced to one round trip per animation frame). */
  refresh(): void {
    if (this.pending) return;
    this.pending = nextFrame(() => {
      this.pending = null;
      void this.fetch();
    });
  }

  /** Asks for the state now. */
  async fetch(): Promise<DeckState | null> {
    const fresh: StateFreshness = { sentBefore: this.sentCount, settled: this.inflight === 0 };
    const r = await this.query('State');
    if (this.disposed || !r || !('State' in r)) return null;
    const deck = r.State;
    const key = JSON.stringify(deck);
    if (key !== this.stateKey) {
      this.stateKey = key;
      this.store.setState({ deck });
      this.registry.setContext({
        deckState: deck,
        contexts: deck.editing ? [...deck.contexts.filter((c) => c !== 'picture')] : deck.contexts,
        scopes: scopesOf(deck, this.state.rail),
      });
    } else {
      // Another part of the window (Key Tips, a menu) may have set the scopes meanwhile.
      const scopes = scopesOf(deck, this.state.rail);
      if (JSON.stringify(this.registry.context.scopes ?? []) !== JSON.stringify(scopes))
        this.registry.setContext({ scopes });
    }
    for (const l of [...this.listeners]) l(deck, fresh);
    return deck;
  }

  async query(q: DeckQuery): Promise<DeckQueryResult | null> {
    try {
      return deckResult(await this.client.query(this.doc, { Deck: q }));
    } catch {
      return null;
    }
  }

  /** Thumbnails of `count` slides from `from`, `width` CSS px wide. */
  async thumbnails(from: number, count: number, width: number, dpr: number): Promise<SceneDelta | null> {
    const r = await this.query({ Thumbnails: { from, count, width, dpr } });
    return r && 'Scene' in r ? r.Scene : null;
  }

  async layoutThumbnails(width: number, dpr: number): Promise<SceneDelta | null> {
    const r = await this.query({ LayoutThumbnails: { width, dpr } });
    return r && 'Scene' in r ? r.Scene : null;
  }

  async themeThumbnails(width: number, dpr: number): Promise<SceneDelta | null> {
    const r = await this.query({ ThemeThumbnails: { width, dpr } });
    return r && 'Scene' in r ? r.Scene : null;
  }

  /** The cursor over a point of the slide. */
  async cursorAt(x: number, y: number): Promise<DeckCursor> {
    const r = await this.query({ HitTest: { x, y } });
    return r && 'Cursor' in r ? r.Cursor : 'Default';
  }

  /** Every preset of the Shapes gallery as an outline (asked once). */
  shapeIcons(): Promise<DeckShapeIcon[]> {
    this.icons ??= this.query({ ShapeIcons: { size: 20 } }).then((r) => (r && 'Icons' in r ? r.Icons : []));
    return this.icons;
  }

  /** The outline of every slide's text. */
  async outline() {
    const r = await this.query('Outline');
    return r && 'Outline' in r ? r.Outline : [];
  }

  /** Runs a command through the registry (the ribbon's path; the command log sees it). */
  async run(id: string, args?: unknown): Promise<boolean> {
    const r = await this.registry.execute(id, args);
    if (r.status === 'failed') {
      this.store.setState({ notice: messageOf(r.error) });
      return false;
    }
    return r.status === 'ran';
  }

  /**
   * Sends a core command straight to the core, in order with everything sent before (pointer
   * gestures, typing): the stage's own input path, which must not wait for the registry's
   * availability check.
   */
  core(id: string, args?: unknown): Promise<boolean> {
    this.sent.push(id);
    if (this.sent.length > 200) this.sent.splice(0, this.sent.length - 200);
    let command: ReturnType<typeof buildCommand>;
    try {
      command = buildCommand(id, args, { app: 'deck' });
    } catch (error) {
      this.store.setState({ notice: messageOf(error) });
      return Promise.resolve(false);
    }
    this.sentCount += 1;
    this.inflight += 1;
    const done = this.client.execute(this.doc, command).then(
      () => {
        this.inflight -= 1;
        return true;
      },
      (error: unknown) => {
        this.inflight -= 1;
        const e = error as { kind?: unknown };
        if (e.kind !== 'Unavailable') this.store.setState({ notice: messageOf(error) });
        return false;
      },
    );
    this.queue = done;
    return done;
  }

  /**
   * Runs a chart command (`SuiteCommand::Chart*`) on the selected chart, in order with the stage's
   * commands. A chart just inserted shows Chart Design, as in PowerPoint.
   */
  async chart(command: SuiteCommand): Promise<boolean> {
    this.sentCount += 1;
    this.inflight += 1;
    const done = this.client.execute(this.doc, { Suite: command }).then(
      () => true,
      (error: unknown) => {
        this.store.setState({ notice: messageOf(error) });
        return false;
      },
    );
    this.queue = done;
    const ok = await done;
    this.inflight -= 1;
    await this.fetch();
    if (ok && typeof command === 'object' && 'ChartInsert' in command && this.state.deck?.chart)
      showRibbonTab('chart-design');
    return ok;
  }

  /** Resolves when every command sent so far is answered. */
  settled(): Promise<unknown> {
    return this.queue;
  }

  openDialog(id: DeckDialog, arg?: number): void {
    this.store.setState({ dialog: { id, ...(arg !== undefined ? { arg } : {}) }, menu: null });
  }

  closeDialog(): void {
    this.store.setState({ dialog: null });
    this.stage?.focus();
  }

  dispose(): void {
    this.disposed = true;
    this.pending?.cancel();
    this.unsubscribe();
    this.listeners.clear();
    this.registry.setContext({ deckState: undefined, contexts: undefined, scopes: [] });
  }
}

/** Runs `fn` on the next animation frame (or a task where there is none). */
export function nextFrame(fn: () => void): { cancel(): void } {
  if (typeof requestAnimationFrame === 'function') {
    const id = requestAnimationFrame(fn);
    return { cancel: () => cancelAnimationFrame(id) };
  }
  const t = setTimeout(fn, 0);
  return { cancel: () => clearTimeout(t) };
}
