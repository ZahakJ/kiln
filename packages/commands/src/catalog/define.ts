import type { AppId, CommandDeclaration, Tier } from '../types.ts';

export type DeclOptions = Partial<Omit<CommandDeclaration, 'id' | 'title' | 'icon'>>;

/**
 * Compact declaration helper: `c('suite.format.bold', 'Bold', 'Bold', { keyTip: '1' })`.
 *
 * A declaration gives a command its presentation (title, Office name, icon, synonyms, Key Tip). For
 * a command the core implements (it is in `catalog.json`), that is all the chrome adds; for one the
 * core does not implement yet, the declaration reserves its id so ribbons, keymaps and the palette
 * can refer to it, and the registry keeps it hidden in release builds (PLAN.md §1.4).
 */
export function c(id: string, title: string, icon?: string, options: DeclOptions = {}): CommandDeclaration {
  return { id, title, ...(icon ? { icon } : {}), ...options };
}

/** Applies a category, app list and default tier to a block of declarations (explicit values win). */
export function section(
  category: string,
  defaults: { apps?: readonly AppId[]; tier?: Tier },
  list: readonly CommandDeclaration[],
): CommandDeclaration[] {
  return list.map((d) => ({
    category,
    ...(defaults.apps ? { apps: defaults.apps } : {}),
    ...(defaults.tier ? { tier: defaults.tier } : {}),
    ...d,
  }));
}

export const WRITE: readonly AppId[] = ['write'];
export const GRID: readonly AppId[] = ['grid'];
export const DECK: readonly AppId[] = ['deck'];
export const NOTES: readonly AppId[] = ['notes'];
export const EDITORS: readonly AppId[] = ['write', 'grid', 'deck', 'notes'];
export const TEXT_APPS: readonly AppId[] = ['write', 'deck', 'notes'];
export const WRITE_DECK: readonly AppId[] = ['write', 'deck'];
export const WRITE_GRID: readonly AppId[] = ['write', 'grid'];
export const ALL_APPS: readonly AppId[] = ['write', 'grid', 'deck', 'notes', 'hub'];
