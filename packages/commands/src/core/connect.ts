/**
 * Connects a window's command registry to the core (PLAN.md §3.3, §3.7): every core command of the
 * window's app becomes a registry implementation whose `run` builds the typed `Command` and calls
 * `KilnClient.execute`, and whose enabled state comes from the document state the core reports in
 * `Event::Changed` (and `Opened`/`Saved`/`Closed`). Nothing is computed here: availability is a
 * lookup of the flags the core sends (`can_undo`, `read_only`, the selection kind).
 */
import type {
  Availability,
  Command,
  DocId,
  DocSummary,
  Event,
  ExecResult,
  KilnClient,
  Selection,
  StatusBar,
} from '@kiln/protocol';
import { GRID_CORE_STATE } from '../catalog/grid.ts';
import { WRITE_CORE_STATE } from '../catalog/write.ts';
import type { CommandRegistry } from '../registry.ts';
import type { CommandContext, CommandDefinition, Disposable } from '../types.ts';
import { buildCommand } from './build.ts';
import { CORE_CATALOG, coreApps } from './catalog.ts';

/** The state of the window's document, as last reported by the core. */
export interface CoreDocState {
  summary: DocSummary | null;
  selection: Selection | null;
  status: StatusBar | null;
}

export const EMPTY_CORE_STATE: CoreDocState = Object.freeze({ summary: null, selection: null, status: null });

/** Reads the core state a connection put into the registry context. */
export function coreStateOf(ctx: CommandContext): CoreDocState {
  const s = ctx.core as CoreDocState | undefined;
  return s ?? EMPTY_CORE_STATE;
}

function selectionIsEmpty(sel: Selection | null): boolean {
  if (!sel) return true;
  const kind = sel.kind;
  if (kind === 'None') return true;
  if ('Text' in kind) {
    const { anchor, head } = kind.Text;
    return anchor.block === head.block && anchor.offset === head.offset;
  }
  if ('Cells' in kind) return kind.Cells.ranges.length === 0;
  return kind.Shapes.ids.length === 0;
}

/** Whether a command with `availability` can run, from the flags of the core state. */
export function isAvailable(availability: Availability, state: CoreDocState): boolean {
  const doc = state.summary;
  switch (availability) {
    case 'Always':
      return true;
    case 'Document':
      return doc !== null;
    case 'Editable':
      return doc !== null && !doc.read_only;
    case 'CanUndo':
      return doc?.can_undo ?? false;
    case 'CanRedo':
      return doc?.can_redo ?? false;
    case 'Selection':
      return doc !== null && !selectionIsEmpty(state.selection);
  }
}

/**
 * The current value of value commands, read from the status the core reports (the view buttons and
 * zoom controls show it; a ribbon toggle with `checkedValue` is pressed when it matches).
 */
const CORE_VALUES: Readonly<Record<string, (ctx: CommandContext) => unknown>> = {
  'suite.view.mode': (ctx) => coreStateOf(ctx).status?.view,
  'suite.view.zoom': (ctx) => coreStateOf(ctx).status?.zoom,
};

/**
 * Pressed states and values an app's editor reports through the registry context (WP-64: Write's
 * selection formatting). Each entry reads one field the core sent; nothing is computed here.
 */
const APP_STATE: Readonly<
  Partial<
    Record<
      string,
      {
        checked: Readonly<Record<string, (ctx: CommandContext) => boolean | undefined>>;
        values: Readonly<Record<string, (ctx: CommandContext) => unknown>>;
      }
    >
  >
> = { write: WRITE_CORE_STATE, notes: WRITE_CORE_STATE, grid: GRID_CORE_STATE };

/** One command sent to the core, for the session's command log. */
export interface CommandLogEntry {
  /** Monotonic sequence number within the log. */
  seq: number;
  id: string;
  doc: DocId | null;
  command: Command;
  /** `Applied`, `Unchanged`, `Pending`, or `error` when the core rejected it. */
  outcome: 'Applied' | 'Unchanged' | 'Pending' | 'error';
  /** The core's error message when `outcome` is `error`. */
  error?: string;
}

/** A bounded log of the commands a window sent to the core (newest last). */
export class CommandLog {
  readonly #entries: CommandLogEntry[] = [];
  readonly #limit: number;
  #seq = 0;
  readonly #listeners = new Set<(e: CommandLogEntry) => void>();

  constructor(limit = 500) {
    this.#limit = Math.max(1, limit);
  }

  record(entry: Omit<CommandLogEntry, 'seq'>): CommandLogEntry {
    const full = { ...entry, seq: ++this.#seq };
    this.#entries.push(full);
    if (this.#entries.length > this.#limit) this.#entries.splice(0, this.#entries.length - this.#limit);
    for (const l of [...this.#listeners]) l(full);
    return full;
  }

  /** Entries, oldest first. */
  entries(): readonly CommandLogEntry[] {
    return [...this.#entries];
  }

  /** Entries after `seq` (exclusive). */
  since(seq: number): readonly CommandLogEntry[] {
    return this.#entries.filter((e) => e.seq > seq);
  }

  get lastSeq(): number {
    return this.#seq;
  }

  subscribe(listener: (e: CommandLogEntry) => void): () => void {
    this.#listeners.add(listener);
    return () => this.#listeners.delete(listener);
  }
}

export interface ConnectCoreOptions {
  client: KilnClient;
  /** The window's document; `null` until one is open (Home, or an app the core cannot create yet). */
  document?: DocSummary | null;
  /** Receives every command sent to the core. */
  log?: CommandLog;
}

export interface CoreConnection extends Disposable {
  /** The document the window shows (after New/Open, or `null` when it closes). */
  setDocument(doc: DocSummary | null): void;
  readonly state: CoreDocState;
  /** Called after each state change (for status bars and title bars). */
  subscribe(listener: (state: CoreDocState) => void): () => void;
}

function outcomeOf(result: ExecResult): CommandLogEntry['outcome'] {
  return typeof result.outcome === 'string' ? result.outcome : 'Pending';
}

function messageOf(error: unknown): string {
  if (error && typeof error === 'object' && 'message' in error)
    return String((error as { message: unknown }).message);
  return String(error);
}

/**
 * Registers every core command of the registry's app as an implementation that dispatches to the
 * core, and keeps the registry context's `core` state in step with the core's events.
 */
export function connectCore(registry: CommandRegistry, options: ConnectCoreOptions): CoreConnection {
  const { client, log } = options;
  const app = registry.context.app;
  let state: CoreDocState = { summary: options.document ?? null, selection: null, status: null };
  const listeners = new Set<(s: CoreDocState) => void>();

  const publish = (next: CoreDocState) => {
    state = next;
    registry.setContext({ core: state });
    for (const l of [...listeners]) l(state);
  };

  const appState = app ? APP_STATE[app] : undefined;
  const definitions: CommandDefinition<unknown>[] = CORE_CATALOG.filter(
    (info) => !app || coreApps(info).includes(app),
  ).map((info) => ({
    id: info.id,
    // Presentation comes from the declaration; the registry merges it in.
    title: info.id,
    isEnabled: (ctx: CommandContext) => isAvailable(info.availability, coreStateOf(ctx)),
    ...(CORE_VALUES[info.id] ? { value: CORE_VALUES[info.id] } : {}),
    ...(appState?.values[info.id] ? { value: appState.values[info.id] } : {}),
    ...(appState?.checked[info.id]
      ? { isChecked: (ctx: CommandContext) => appState.checked[info.id]?.(ctx) === true }
      : {}),
    run: async (ctx: CommandContext, args: unknown) => {
      const command = buildCommand(info.id, args, { app: ctx.app });
      // Session-level commands (New, Open, Ping) need no document; the others address the window's.
      const doc = info.availability === 'Always' ? null : (state.summary?.doc ?? null);
      try {
        const result = await client.execute(doc, command);
        log?.record({ id: info.id, doc, command, outcome: outcomeOf(result) });
        if (result.summary && (!state.summary || result.summary.doc === state.summary.doc)) {
          publish({ ...state, summary: result.summary });
        }
        return result;
      } catch (error) {
        log?.record({ id: info.id, doc, command, outcome: 'error', error: messageOf(error) });
        throw error;
      }
    },
  }));

  // An implementation keeps the declaration's title, icon and synonyms (registry merges them).
  const registered = registry.registerMany(
    definitions.map((d) => {
      const decl = registry.meta(d.id);
      return { ...d, title: decl?.title ?? d.title } as CommandDefinition<never>;
    }),
  );

  const onEvent = (event: Event) => {
    const current = state.summary?.doc;
    if ('Opened' in event) {
      if (current === undefined) return;
      if (event.Opened.summary.doc === current) publish({ ...state, summary: event.Opened.summary });
    } else if ('Changed' in event) {
      const { summary, selection, status } = event.Changed;
      // A change that changes nothing here (a scroll: the viewport moved) is not published, so
      // the ribbon does not re-render on every frame of a scroll.
      if (summary.doc === current && !sameState(state, { summary, selection, status }))
        publish({ summary, selection, status });
    } else if ('Saved' in event) {
      if (event.Saved.summary.doc === current) publish({ ...state, summary: event.Saved.summary });
    } else if ('Closed' in event) {
      if (event.Closed.doc === current) publish({ summary: null, selection: null, status: null });
    }
  };
  const unsubscribe = client.subscribe(onEvent);
  publish(state);

  return {
    setDocument(doc) {
      publish({ summary: doc, selection: null, status: null });
    },
    get state() {
      return state;
    },
    subscribe(listener) {
      listeners.add(listener);
      return () => listeners.delete(listener);
    },
    dispose() {
      unsubscribe();
      registered.dispose();
      listeners.clear();
      registry.setContext({ core: undefined });
    },
  };
}

/** Whether two reports of the core carry the same state (they are plain JSON data). */
function sameState(a: CoreDocState, b: CoreDocState): boolean {
  return (
    JSON.stringify(a.summary) === JSON.stringify(b.summary) &&
    JSON.stringify(a.selection) === JSON.stringify(b.selection) &&
    JSON.stringify(a.status) === JSON.stringify(b.status)
  );
}
