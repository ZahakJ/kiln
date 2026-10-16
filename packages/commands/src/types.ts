/**
 * Core types of the command system (PLAN.md §1.3, §1.7, §2.1). Everything a user can do in Kiln
 * (ribbon buttons, menu items, shortcuts, palette entries, Key Tips) is a command with a stable id.
 */

/** The applications of the suite. `hub` is Kiln Home, the start screen. */
export type AppId = 'write' | 'grid' | 'deck' | 'notes' | 'hub';

export const APP_IDS: readonly AppId[] = ['write', 'grid', 'deck', 'notes', 'hub'];

/** Feature tiers (PLAN.md §2): P0 ships first; P1/P2 slots exist from day one but are gated. */
export type Tier = 'P0' | 'P1' | 'P2';

/**
 * What commands see when they run or compute their state. Editors put their own keys here
 * (`selection`, `editor`…); `scopes` lists the active keyboard scopes (e.g. `rail`, `slideshow`,
 * `object`, `cell-edit`) that keymap bindings can be restricted to.
 */
export interface CommandContext {
  app?: AppId;
  scopes?: readonly string[];
  [key: string]: unknown;
}

export type CommandRun<Args = unknown> = (ctx: CommandContext, args: Args) => unknown;

/** Metadata shared by declarations (catalog) and implementations. */
export interface CommandMeta {
  /** Stable, dotted id: `format.bold`, `grid.freezePanes`. */
  id: string;
  /** Title in Kiln's words, shown in menus, tooltips and the palette. */
  title: string;
  /** Microsoft Office's name for the command, when it differs from `title`. */
  officeName?: string;
  /** Other words people search for ("strikeout", "tell me"); Office synonyms are merged in. */
  synonyms?: readonly string[];
  /** One sentence for super tooltips and the palette. */
  description?: string;
  /** Icon name: an Kiln glyph (`FormatPainter`) or a Lucide icon (`Bold`), resolved by ui-kit. */
  icon?: string;
  /** Primary shortcut for display when no keymap is attached (`Mod+B`). Keymaps are authoritative. */
  shortcut?: string;
  /** Apps the command belongs to; omitted = every app. */
  apps?: readonly AppId[];
  /** Feature tier; default `P0`. */
  tier?: Tier;
  /** Palette/Options grouping, e.g. "Home ▸ Font". */
  category?: string;
  /** Preferred Key Tip letters inside its ribbon tab (Office: Bold = 1, Paste = V). */
  keyTip?: string;
  /** `false` keeps it out of the command palette (caret movement, slide-show keys). */
  palette?: boolean;
}

export interface CommandDefinition<Args = unknown> extends CommandMeta {
  /** Whether the command applies at all in this context (hidden from palette/menus when false). */
  when?: (ctx: CommandContext) => boolean;
  run: CommandRun<Args>;
  /** Greyed out when false. Default: enabled. */
  isEnabled?: (ctx: CommandContext) => boolean;
  /** Toggle commands (Bold, Ruler): pressed state. */
  isChecked?: (ctx: CommandContext) => boolean;
  /** Value commands (font name, zoom): the current value shown by comboboxes, number inputs, galleries. */
  value?: (ctx: CommandContext) => unknown;
}

/** A command that exists by id and title but is not implemented yet (resolves to a pending no-op). */
export interface CommandDeclaration extends CommandMeta {}

export interface CommandState {
  id: string;
  /** False when nothing with this id is registered or declared. */
  exists: boolean;
  /** Declared but not implemented: executing it is a no-op. */
  pending: boolean;
  /** Passes `when`, its app filter and the tier gate. */
  visible: boolean;
  enabled: boolean;
  /** `undefined` for commands that are not toggles. */
  checked: boolean | undefined;
  value: unknown;
}

export type ExecuteStatus = 'ran' | 'pending' | 'disabled' | 'hidden' | 'unknown' | 'failed';

export interface ExecuteResult {
  id: string;
  status: ExecuteStatus;
  /** What `run` returned (awaited), when `status === 'ran'`. */
  value?: unknown;
  error?: unknown;
}

export type RegistryEvent =
  | { type: 'registered'; ids: readonly string[] }
  | { type: 'unregistered'; ids: readonly string[] }
  /** State may have changed (context switch, selection change): `ids` undefined means all. */
  | { type: 'state'; ids?: readonly string[] }
  | { type: 'executed'; id: string; result: ExecuteResult };

export interface Disposable {
  dispose(): void;
}
