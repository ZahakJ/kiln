import type { CommandDeclaration } from '../types.ts';
import {
  editCommands,
  fileCommands,
  fontCommands,
  insertCommonCommands,
  navigationCommands,
  paragraphCommands,
  reviewCommonCommands,
  viewCommonCommands,
} from './common.ts';
import { deckCommands, notesCommands } from './deck.ts';
import { DESCRIPTIONS } from './descriptions.ts';
import { gridCommands } from './grid.ts';
import { writeCommands } from './write.ts';

export { c, type DeclOptions, section } from './define.ts';

/**
 * Every command of the suite, declared by id, title, Office name, icon and tier (PLAN.md §1.4, §1.7).
 * Editors register implementations for these ids as they land; until then each one is a pending
 * no-op, so the palette, menus, Key Tips and Options ▸ Keyboard are complete from day one.
 */
export const COMMAND_CATALOG: readonly CommandDeclaration[] = Object.freeze(
  [
    ...fileCommands,
    ...editCommands,
    ...fontCommands,
    ...paragraphCommands,
    ...insertCommonCommands,
    ...reviewCommonCommands,
    ...viewCommonCommands,
    ...navigationCommands,
    ...writeCommands,
    ...gridCommands,
    ...deckCommands,
    ...notesCommands,
  ].map((d) => (d.description || !DESCRIPTIONS[d.id] ? d : { ...d, description: DESCRIPTIONS[d.id] })),
);

const BY_ID = new Map(COMMAND_CATALOG.map((d) => [d.id, d]));

export function catalogEntry(id: string): CommandDeclaration | undefined {
  return BY_ID.get(id);
}

export function isDeclared(id: string): boolean {
  return BY_ID.has(id);
}
