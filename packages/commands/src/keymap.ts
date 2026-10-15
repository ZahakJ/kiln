import {
  chordId,
  detectPlatform,
  eventToChord,
  formatChord,
  isBareModifier,
  type KeyChord,
  type KeyEventLike,
  type Platform,
  parseShortcut,
} from './keys.ts';
import { defaultStorage, readJson, type StorageLike, writeJson } from './storage.ts';
import type { AppId, CommandContext } from './types.ts';

/** One shortcut → command mapping in a keymap file. */
export interface KeyBinding {
  /** Platform-neutral shortcut: `Mod+Shift+L`, `F5`, `Alt+Shift+ArrowUp`. */
  key: string;
  command: string;
  args?: unknown;
  /**
   * Keyboard scope the binding is limited to (`rail`, `slideshow`, `object`, `cell-edit`…). The
   * binding applies only while that scope is in `ctx.scopes`. Unscoped bindings apply everywhere in
   * the app, and scoped bindings win over unscoped ones.
   */
  scope?: string;
  /**
   * The row of docs/SHORTCUTS.md this binding implements (the first column, verbatim). A test parses
   * the document and checks every row against these bindings.
   */
  row?: string;
  /** Fired on key release of a bare modifier (Alt → Key Tips) instead of on key down. */
  onRelease?: boolean;
}

export type KeymapLayerId = AppId | 'common';

export interface ResolvedBinding extends KeyBinding {
  chord: KeyChord;
  layer: KeymapLayerId;
  source: 'default' | 'user';
}

export interface KeymapConflict {
  layer: AppId;
  /** Display label of the chord. */
  shortcut: string;
  scope: string | undefined;
  commands: string[];
}

/** Persisted user remapping: per layer, command id → its complete list of shortcuts. */
export interface KeymapOverrides {
  version: 1;
  layers: Partial<Record<KeymapLayerId, Record<string, string[]>>>;
}

export interface KeymapOptions {
  platform?: Platform;
  /** Default bindings per layer; `common` applies in every app. */
  layers: Partial<Record<KeymapLayerId, readonly KeyBinding[]>>;
  /** Where user overrides persist. Default: `localStorage` (memory fallback). */
  storage?: StorageLike;
  storageKey?: string;
}

const EMPTY_OVERRIDES = (): KeymapOverrides => ({ version: 1, layers: {} });

/**
 * The keymap: resolves key events to commands for the active app and scopes, lists shortcuts per
 * command (tooltips, menus, palette), detects conflicts and stores the user's remapping.
 */
export class Keymap {
  readonly platform: Platform;
  readonly #defaults: Partial<Record<KeymapLayerId, readonly KeyBinding[]>>;
  readonly #storage: StorageLike;
  readonly #storageKey: string;
  readonly #listeners = new Set<() => void>();
  #overrides: KeymapOverrides;
  readonly #cache = new Map<AppId, ResolvedBinding[]>();

  constructor(options: KeymapOptions) {
    this.platform = options.platform ?? detectPlatform();
    this.#defaults = options.layers;
    this.#storage = options.storage ?? defaultStorage();
    this.#storageKey = options.storageKey ?? 'officina.keymap';
    this.#overrides = sanitizeOverrides(readJson<unknown>(this.#storage, this.#storageKey, null));
    // Fail fast on malformed default keymaps (caught by tests, never at a user's desk).
    for (const [layer, list] of Object.entries(this.#defaults)) {
      for (const b of list ?? []) {
        try {
          parseShortcut(b.key, this.platform);
        } catch (e) {
          throw new Error(`Keymap "${layer}": ${(e as Error).message} (command ${b.command})`);
        }
      }
    }
  }

  /** Effective bindings in an app: `common` + the app layer, with user overrides applied. */
  bindings(app: AppId): readonly ResolvedBinding[] {
    const cached = this.#cache.get(app);
    if (cached) return cached;
    const out: ResolvedBinding[] = [];
    const layers: KeymapLayerId[] = ['common', app];
    const overridden = new Set<string>();
    for (const layer of layers) {
      for (const cmd of Object.keys(this.#overrides.layers[layer] ?? {})) overridden.add(cmd);
    }
    for (const layer of layers) {
      for (const b of this.#defaults[layer] ?? []) {
        if (overridden.has(b.command)) continue;
        out.push({ ...b, chord: parseShortcut(b.key, this.platform), layer, source: 'default' });
      }
    }
    // App-layer overrides win over common-layer overrides for the same command.
    const userCommands = new Map<string, { keys: string[]; layer: KeymapLayerId }>();
    for (const layer of layers) {
      for (const [cmd, keys] of Object.entries(this.#overrides.layers[layer] ?? {})) {
        userCommands.set(cmd, { keys, layer });
      }
    }
    for (const [command, { keys, layer }] of userCommands) {
      for (const key of keys) {
        try {
          out.push({ key, command, chord: parseShortcut(key, this.platform), layer, source: 'user' });
        } catch {
          // A stored shortcut that no longer parses is dropped silently.
        }
      }
    }
    this.#cache.set(app, out);
    return out;
  }

  /**
   * The binding for a key event (or chord) in the context, or `undefined`. Precedence: user over
   * default, scoped over unscoped, app layer over common. Bare-modifier bindings (Alt) match only
   * when `release` is true (the caller checks that no other key was pressed in between).
   */
  resolve(
    input: KeyEventLike | KeyChord,
    ctx: Pick<CommandContext, 'app' | 'scopes'>,
    options: { release?: boolean } = {},
  ): ResolvedBinding | undefined {
    if (!ctx.app) return undefined;
    const chord =
      'ctrl' in input && typeof input.ctrl === 'boolean'
        ? (input as KeyChord)
        : eventToChord(input as KeyEventLike);
    if (isBareModifier(chord) !== !!options.release) return undefined;
    const id = chordId(chord);
    const scopes = new Set(ctx.scopes ?? []);
    let best: ResolvedBinding | undefined;
    let bestRank = -1;
    for (const b of this.bindings(ctx.app)) {
      if (chordId(b.chord) !== id) continue;
      if (b.scope && !scopes.has(b.scope)) continue;
      const rank = (b.source === 'user' ? 4 : 0) + (b.scope ? 2 : 0) + (b.layer === 'common' ? 0 : 1);
      if (rank > bestRank) {
        best = b;
        bestRank = rank;
      }
    }
    return best;
  }

  /** Platform-neutral shortcuts of a command in an app (unscoped first), e.g. `['Mod+Y', 'Mod+Shift+Z']`. */
  shortcutsFor(command: string, app: AppId, options: { scope?: string } = {}): string[] {
    return this.bindings(app)
      .filter((b) => b.command === command && (options.scope === undefined || b.scope === options.scope))
      .sort((a, b) => Number(!!a.scope) - Number(!!b.scope))
      .map((b) => b.key);
  }

  /** The first shortcut, for tooltips and menus. */
  primaryShortcut(command: string, app: AppId): string | undefined {
    return this.shortcutsFor(command, app).find((k) => !isBareModifier(parseShortcut(k, this.platform)));
  }

  /** Display label for a command's primary shortcut (`Ctrl+B` / `⌘B`). */
  label(command: string, app: AppId): string | undefined {
    const k = this.primaryShortcut(command, app);
    return k ? formatChord(parseShortcut(k, this.platform), this.platform) : undefined;
  }

  /** Commands bound to the chord in the app and scope (to warn before remapping). */
  commandsFor(key: string | KeyChord, app: AppId, scope?: string): string[] {
    const chord = typeof key === 'string' ? parseShortcut(key, this.platform) : key;
    const id = chordId(chord);
    return [
      ...new Set(
        this.bindings(app)
          .filter((b) => chordId(b.chord) === id && (b.scope ?? undefined) === (scope ?? undefined))
          .map((b) => b.command),
      ),
    ];
  }

  /** Chords bound to two or more different commands in the same app and scope. */
  conflicts(app: AppId): KeymapConflict[] {
    const groups = new Map<string, ResolvedBinding[]>();
    for (const b of this.bindings(app)) {
      const k = `${b.scope ?? ''}|${chordId(b.chord)}`;
      const list = groups.get(k) ?? [];
      list.push(b);
      groups.set(k, list);
    }
    const out: KeymapConflict[] = [];
    for (const list of groups.values()) {
      const commands = [...new Set(list.map((b) => b.command))];
      if (commands.length < 2) continue;
      const first = list[0] as ResolvedBinding;
      out.push({
        layer: app,
        shortcut: formatChord(first.chord, this.platform),
        scope: first.scope,
        commands,
      });
    }
    return out;
  }

  // ── User remapping ───────────────────────────────────────────────────────────────────────────

  get overrides(): KeymapOverrides {
    return this.#overrides;
  }

  /** Replaces all shortcuts of `command` in `layer` (empty list = no shortcut). */
  setUserShortcuts(command: string, keys: readonly string[], layer: KeymapLayerId): void {
    for (const k of keys) parseShortcut(k, this.platform); // validate
    const layers = { ...this.#overrides.layers };
    layers[layer] = { ...(layers[layer] ?? {}), [command]: [...new Set(keys)] };
    this.#commit({ version: 1, layers });
  }

  /**
   * Adds a shortcut to a command, Office-style: if another command in the same app already uses the
   * chord (unscoped), it loses it. Returns the commands that were unassigned.
   */
  assign(command: string, key: string, app: AppId): string[] {
    const chord = parseShortcut(key, this.platform);
    const id = chordId(chord);
    const losers = this.commandsFor(chord, app).filter((c) => c !== command);
    const layers = { ...this.#overrides.layers };
    const layer = { ...(layers[app] ?? {}) };
    for (const loser of losers) {
      layer[loser] = this.shortcutsFor(loser, app).filter(
        (k) => chordId(parseShortcut(k, this.platform)) !== id,
      );
    }
    const current = this.shortcutsFor(command, app).filter((k) => {
      const b = this.bindings(app).find((x) => x.command === command && x.key === k);
      return !b?.scope;
    });
    layer[command] = [...new Set([...current, key])];
    layers[app] = layer;
    this.#commit({ version: 1, layers });
    return losers;
  }

  /** Removes one shortcut from a command in an app. */
  unassign(command: string, key: string, app: AppId): void {
    const id = chordId(parseShortcut(key, this.platform));
    const remaining = this.shortcutsFor(command, app).filter(
      (k) => chordId(parseShortcut(k, this.platform)) !== id,
    );
    this.setUserShortcuts(command, remaining, app);
  }

  /** Restores a command's default shortcuts in a layer. */
  resetCommand(command: string, layer: KeymapLayerId): void {
    const layers = { ...this.#overrides.layers };
    const l = { ...(layers[layer] ?? {}) };
    delete l[command];
    layers[layer] = l;
    this.#commit({ version: 1, layers });
  }

  /** Restores every default (Options ▸ Keyboard ▸ Reset All). */
  resetAll(): void {
    this.#commit(EMPTY_OVERRIDES());
  }

  isCustomized(command: string, app: AppId): boolean {
    return !!(this.#overrides.layers[app]?.[command] ?? this.#overrides.layers.common?.[command]);
  }

  importOverrides(data: unknown): void {
    this.#commit(sanitizeOverrides(data));
  }

  subscribe(listener: () => void): () => void {
    this.#listeners.add(listener);
    return () => this.#listeners.delete(listener);
  }

  #commit(next: KeymapOverrides): void {
    // Drop empty layers so "reset" really leaves no trace.
    for (const k of Object.keys(next.layers) as KeymapLayerId[]) {
      if (Object.keys(next.layers[k] ?? {}).length === 0) delete next.layers[k];
    }
    this.#overrides = next;
    this.#cache.clear();
    if (Object.keys(next.layers).length === 0) {
      try {
        this.#storage.removeItem(this.#storageKey);
      } catch {
        // ignore
      }
    } else writeJson(this.#storage, this.#storageKey, next);
    for (const l of this.#listeners) l();
  }
}

const LAYER_IDS: readonly KeymapLayerId[] = ['common', 'write', 'grid', 'deck', 'notes', 'hub'];

function sanitizeOverrides(raw: unknown): KeymapOverrides {
  const out = EMPTY_OVERRIDES();
  if (!raw || typeof raw !== 'object') return out;
  const r = raw as { version?: unknown; layers?: unknown };
  if (r.version !== 1 || !r.layers || typeof r.layers !== 'object') return out;
  for (const [layer, map] of Object.entries(r.layers as Record<string, unknown>)) {
    if (!(LAYER_IDS as readonly string[]).includes(layer) || !map || typeof map !== 'object') continue;
    const clean: Record<string, string[]> = {};
    for (const [cmd, keys] of Object.entries(map as Record<string, unknown>)) {
      if (Array.isArray(keys)) clean[cmd] = keys.filter((k): k is string => typeof k === 'string');
    }
    out.layers[layer as KeymapLayerId] = clean;
  }
  return out;
}
