import type { AppId, CommandDeclaration, Tier } from '../types.ts';

export type DeclOptions = Partial<Omit<CommandDeclaration, 'id' | 'title' | 'icon'>>;

/** Compact declaration helper: `c('format.bold', 'Bold', 'Bold', { keyTip: '1' })`. */
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
