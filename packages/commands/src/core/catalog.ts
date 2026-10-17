/**
 * The core's command catalog: `catalog.json`, generated from `kiln-protocol` by `cargo xtask
 * protocol` (PLAN.md §3.5). It is the single list of commands the Rust core implements; every
 * ribbon item, keymap binding, Key Tip and synonym refers to commands by these ids (or, for chrome
 * actions that never reach a document, by a `ui.*` id; for commands the core does not implement yet,
 * by a reserved id that is hidden in release builds — see `idKind`).
 */
import type { App, Availability, CommandInfo, Tier as CoreTier } from '@kiln/protocol';
import catalogJson from '../../../protocol/src/generated/catalog.json' with { type: 'json' };
import type { AppId, Tier } from '../types.ts';

interface CatalogFile {
  protocol: number;
  commands: CommandInfo[];
}

const FILE = catalogJson as unknown as CatalogFile;

/** Every command the core implements, in catalog order. */
export const CORE_CATALOG: readonly CommandInfo[] = Object.freeze(FILE.commands.map((c) => Object.freeze(c)));

/** The protocol version the catalog was generated for. */
export const CORE_PROTOCOL_VERSION: number = FILE.protocol;

const CORE_BY_ID: ReadonlyMap<string, CommandInfo> = new Map(CORE_CATALOG.map((c) => [c.id, c]));

/** The catalog row of a core command. */
export function coreCommand(id: string): CommandInfo | undefined {
  return CORE_BY_ID.get(id);
}

/** Whether the core implements `id` (it is in `catalog.json`). */
export function isCoreCommand(id: string): boolean {
  return CORE_BY_ID.has(id);
}

/** Prefix of chrome commands: UI state only (palette, ribbon, theme, panes), never a document. */
export const UI_PREFIX = 'ui.';

/**
 * What an id refers to:
 * - `core`: in `catalog.json`; dispatched to the core as a typed `Command`.
 * - `ui`: a chrome action (`ui.*`) handled by the frame itself.
 * - `planned`: reserved for a command the core does not implement yet. Hidden in release builds
 *   (never greyed, PLAN.md §1.4); the app work package that adds it to the core removes the
 *   reservation (a unit test fails while both exist).
 */
export type CommandIdKind = 'core' | 'ui' | 'planned';

export function idKind(id: string): CommandIdKind {
  if (CORE_BY_ID.has(id)) return 'core';
  if (id.startsWith(UI_PREFIX)) return 'ui';
  return 'planned';
}

const APP_IDS: Readonly<Record<App, AppId>> = {
  Write: 'write',
  Grid: 'grid',
  Deck: 'deck',
  Notes: 'notes',
  Home: 'hub',
};

/** The protocol's app names as UI app ids. */
export function appIdOf(app: App): AppId {
  return APP_IDS[app];
}

/** The apps that offer a core command. */
export function coreApps(info: CommandInfo): AppId[] {
  return info.apps.map(appIdOf);
}

/** The core tier as a UI tier (the same three values). */
export function coreTier(tier: CoreTier): Tier {
  return tier;
}

export type { Availability };
