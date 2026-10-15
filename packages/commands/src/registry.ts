import type { RecentCommands } from './recent.ts';
import { DEV_TIERS, isShippable, type TierConfig } from './tiers.ts';
import type {
  CommandContext,
  CommandDeclaration,
  CommandDefinition,
  CommandMeta,
  CommandState,
  Disposable,
  ExecuteResult,
  RegistryEvent,
} from './types.ts';

/** A registered entry: an implementation, or a declaration that resolves to a pending no-op. */
export interface RegisteredCommand extends CommandDefinition<unknown> {
  pending: boolean;
}

export interface CommandRegistryOptions {
  /** Initial context (merged with `setContext`). */
  context?: CommandContext;
  /** Tier gate; default: development (all tiers, pending shown). */
  tiers?: TierConfig;
  /** Successful executions are recorded here (local only, never sent anywhere). */
  recent?: RecentCommands;
  /** Errors thrown by `run`; default `console.error`. */
  onError?: (id: string, error: unknown) => void;
}

const ID_RE = /^[a-z][a-zA-Z0-9]*(\.[a-zA-Z0-9]+)+$/;

const noop = () => undefined;

/**
 * The command registry: one per document window. Apps declare their commands up front (the catalog)
 * and editors register implementations when they mount; ribbons, menus, the palette, Key Tips and the
 * keymap all read from here, so a command's enabled/checked state is consistent everywhere.
 */
export class CommandRegistry {
  readonly #commands = new Map<string, RegisteredCommand>();
  /** Declarations shadowed by an implementation; restored when the implementation is disposed. */
  readonly #declarations = new Map<string, CommandDeclaration>();
  readonly #listeners = new Set<(e: RegistryEvent) => void>();
  readonly #idListeners = new Map<string, Set<() => void>>();
  #context: CommandContext;
  #tiers: TierConfig;
  #recent: RecentCommands | undefined;
  #onError: (id: string, error: unknown) => void;
  #batchDepth = 0;
  #batched: RegistryEvent[] = [];

  constructor(options: CommandRegistryOptions = {}) {
    this.#context = { ...(options.context ?? {}) };
    this.#tiers = options.tiers ?? DEV_TIERS;
    this.#recent = options.recent;
    this.#onError = options.onError ?? ((id, e) => console.error(`Command "${id}" failed`, e));
  }

  // ── Registration ─────────────────────────────────────────────────────────────────────────────

  /**
   * Declares commands that exist by id and title but are implemented later (by an editor work
   * package). They show up in the palette, menus and Options ▸ Keyboard, flagged `pending`, and
   * executing them is a no-op. Re-declaring an id updates its metadata.
   */
  declare(decls: CommandDeclaration | readonly CommandDeclaration[]): void {
    const list = Array.isArray(decls) ? decls : [decls as CommandDeclaration];
    const ids: string[] = [];
    for (const d of list) {
      assertMeta(d);
      this.#declarations.set(d.id, d);
      const existing = this.#commands.get(d.id);
      if (existing && !existing.pending) continue; // an implementation wins
      this.#commands.set(d.id, { ...d, run: noop, pending: true });
      ids.push(d.id);
    }
    if (ids.length) this.#emit({ type: 'registered', ids });
  }

  /**
   * Registers an implementation. Metadata missing from `def` (officeName, synonyms, icon…) is taken
   * from the declaration with the same id. Throws if another implementation is already registered.
   * Disposing restores the declaration (if any).
   */
  register<Args = unknown>(def: CommandDefinition<Args>): Disposable {
    assertMeta(def);
    const existing = this.#commands.get(def.id);
    if (existing && !existing.pending) {
      throw new Error(`Command "${def.id}" is already registered`);
    }
    const decl = this.#declarations.get(def.id);
    const merged: RegisteredCommand = {
      ...(decl ?? {}),
      ...stripUndefined(def as CommandDefinition<unknown>),
      synonyms: unique([...(decl?.synonyms ?? []), ...(def.synonyms ?? [])]),
      pending: false,
    };
    this.#commands.set(def.id, merged);
    this.#emit({ type: 'registered', ids: [def.id] });
    let disposed = false;
    return {
      dispose: () => {
        if (disposed) return;
        disposed = true;
        if (this.#commands.get(def.id) !== merged) return;
        const d = this.#declarations.get(def.id);
        if (d) this.#commands.set(def.id, { ...d, run: noop, pending: true });
        else this.#commands.delete(def.id);
        this.#emit({ type: 'unregistered', ids: [def.id] });
      },
    };
  }

  /** Registers several implementations; one disposable removes them all. */
  registerMany(defs: readonly CommandDefinition<never>[]): Disposable {
    const disposables: Disposable[] = [];
    this.batch(() => {
      for (const d of defs) disposables.push(this.register(d as CommandDefinition<unknown>));
    });
    return {
      dispose: () =>
        this.batch(() => {
          for (const d of disposables) d.dispose();
        }),
    };
  }

  /** Adds search synonyms to existing or future commands (e.g. `createOfficeSynonyms()`). */
  addSynonyms(map: Readonly<Record<string, readonly string[]>>): void {
    const ids: string[] = [];
    for (const [id, words] of Object.entries(map)) {
      const decl = this.#declarations.get(id);
      if (decl)
        this.#declarations.set(id, { ...decl, synonyms: unique([...(decl.synonyms ?? []), ...words]) });
      const cmd = this.#commands.get(id);
      if (cmd) {
        this.#commands.set(id, { ...cmd, synonyms: unique([...(cmd.synonyms ?? []), ...words]) });
        ids.push(id);
      }
    }
    if (ids.length) this.#emit({ type: 'state', ids });
  }

  // ── Lookup ───────────────────────────────────────────────────────────────────────────────────

  has(id: string): boolean {
    return this.#commands.has(id);
  }

  get(id: string): RegisteredCommand | undefined {
    return this.#commands.get(id);
  }

  /** Metadata only (safe to hand to UI). */
  meta(id: string): (CommandMeta & { pending: boolean }) | undefined {
    const c = this.#commands.get(id);
    if (!c) return undefined;
    const { run: _r, when: _w, isEnabled: _e, isChecked: _c, value: _v, ...meta } = c;
    return meta;
  }

  /** Every command, optionally only those visible in the current (or given) context. */
  list(options: { visibleOnly?: boolean; context?: CommandContext } = {}): RegisteredCommand[] {
    const all = [...this.#commands.values()];
    if (!options.visibleOnly) return all;
    const ctx = options.context ?? this.#context;
    return all.filter((c) => this.#isVisible(c, ctx));
  }

  get size(): number {
    return this.#commands.size;
  }

  // ── Context and state ────────────────────────────────────────────────────────────────────────

  get context(): CommandContext {
    return this.#context;
  }

  /** Merges `patch` into the context and notifies state listeners. */
  setContext(patch: CommandContext): void {
    this.#context = { ...this.#context, ...patch };
    this.#emit({ type: 'state' });
  }

  /** The recent-commands store, if one was given. */
  get recent(): RecentCommands | undefined {
    return this.#recent;
  }

  get tiers(): TierConfig {
    return this.#tiers;
  }

  setTiers(config: TierConfig): void {
    this.#tiers = config;
    this.#emit({ type: 'state' });
  }

  /** Tells listeners that state may have changed (selection moved, document edited). */
  invalidate(ids?: readonly string[]): void {
    this.#emit(ids ? { type: 'state', ids } : { type: 'state' });
  }

  getState(id: string, context?: CommandContext): CommandState {
    const ctx = context ?? this.#context;
    const c = this.#commands.get(id);
    if (!c) {
      return {
        id,
        exists: false,
        pending: false,
        visible: false,
        enabled: false,
        checked: undefined,
        value: undefined,
      };
    }
    const visible = this.#isVisible(c, ctx);
    return {
      id,
      exists: true,
      pending: c.pending,
      visible,
      enabled: visible && !c.pending && safeCall(c.isEnabled, ctx, true),
      checked: c.isChecked ? safeCall(c.isChecked, ctx, false) : undefined,
      value: c.value ? safeCall(c.value, ctx, undefined) : undefined,
    };
  }

  // ── Execution ────────────────────────────────────────────────────────────────────────────────

  /**
   * Runs a command. Never throws: the result says what happened. Pending commands resolve to a
   * no-op with status `pending`. Successful runs are recorded in the recent-commands store.
   */
  async execute(id: string, args?: unknown, context?: CommandContext): Promise<ExecuteResult> {
    const ctx = context ?? this.#context;
    const c = this.#commands.get(id);
    let result: ExecuteResult;
    if (!c) result = { id, status: 'unknown' };
    else if (c.pending) result = { id, status: 'pending' };
    else if (!this.#isVisible(c, ctx)) result = { id, status: 'hidden' };
    else if (!safeCall(c.isEnabled, ctx, true)) result = { id, status: 'disabled' };
    else {
      try {
        const value = await c.run(ctx, args);
        result = { id, status: 'ran', value };
        this.#recent?.record(id);
      } catch (error) {
        this.#onError(id, error);
        result = { id, status: 'failed', error };
      }
    }
    this.#emit({ type: 'executed', id, result });
    // Running a command usually changes what other commands show (Bold pressed, Undo enabled).
    if (result.status === 'ran') this.#emit({ type: 'state' });
    return result;
  }

  // ── Subscriptions ────────────────────────────────────────────────────────────────────────────

  /** Every registry event. Returns an unsubscribe function. */
  subscribe(listener: (event: RegistryEvent) => void): () => void {
    this.#listeners.add(listener);
    return () => this.#listeners.delete(listener);
  }

  /** Called when the given command's state may have changed (for `useSyncExternalStore`). */
  subscribeTo(id: string, listener: () => void): () => void {
    let set = this.#idListeners.get(id);
    if (!set) {
      set = new Set();
      this.#idListeners.set(id, set);
    }
    set.add(listener);
    return () => {
      set.delete(listener);
      if (set.size === 0) this.#idListeners.delete(id);
    };
  }

  /** Coalesces notifications of many changes into one per listener. */
  batch(fn: () => void): void {
    this.#batchDepth++;
    try {
      fn();
    } finally {
      this.#batchDepth--;
      if (this.#batchDepth === 0 && this.#batched.length) {
        const events = this.#batched;
        this.#batched = [];
        this.#dispatch(coalesce(events));
      }
    }
  }

  // ── Internals ────────────────────────────────────────────────────────────────────────────────

  #isVisible(c: RegisteredCommand, ctx: CommandContext): boolean {
    if (!isShippable(c, this.#tiers)) return false;
    if (c.apps && ctx.app && !c.apps.includes(ctx.app)) return false;
    return safeCall(c.when, ctx, true);
  }

  #emit(event: RegistryEvent): void {
    if (this.#batchDepth > 0) {
      this.#batched.push(event);
      return;
    }
    this.#dispatch([event]);
  }

  #dispatch(events: RegistryEvent[]): void {
    for (const e of events) {
      for (const l of [...this.#listeners]) l(e);
      if (e.type === 'executed') continue;
      const ids = 'ids' in e && e.ids ? e.ids : undefined;
      if (ids) {
        for (const id of ids) for (const l of [...(this.#idListeners.get(id) ?? [])]) l();
      } else {
        for (const set of [...this.#idListeners.values()]) for (const l of [...set]) l();
      }
    }
  }
}

function coalesce(events: RegistryEvent[]): RegistryEvent[] {
  const out: RegistryEvent[] = [];
  const merged = new Map<string, Set<string> | null>();
  for (const e of events) {
    if (e.type === 'executed') {
      out.push(e);
      continue;
    }
    const prev = merged.get(e.type);
    if (prev === null) continue;
    if (!('ids' in e) || !e.ids) merged.set(e.type, null);
    else merged.set(e.type, new Set([...(prev ?? []), ...e.ids]));
  }
  for (const [type, ids] of merged) {
    out.push((ids ? { type, ids: [...ids] } : { type }) as RegistryEvent);
  }
  return out;
}

function safeCall<T>(fn: ((ctx: CommandContext) => T) | undefined, ctx: CommandContext, fallback: T): T {
  if (!fn) return fallback;
  try {
    return fn(ctx);
  } catch {
    return fallback;
  }
}

function assertMeta(m: CommandMeta): void {
  if (!ID_RE.test(m.id)) throw new Error(`Invalid command id "${m.id}" (expected e.g. "format.bold")`);
  if (!m.title?.trim()) throw new Error(`Command "${m.id}" needs a title`);
}

function unique(list: readonly string[]): string[] {
  return [...new Set(list)];
}

function stripUndefined<T extends object>(o: T): T {
  return Object.fromEntries(Object.entries(o).filter(([, v]) => v !== undefined)) as T;
}
