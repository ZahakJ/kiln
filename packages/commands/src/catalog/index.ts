import type { RibbonData } from '../ribbon.ts';
import type { AppId, CommandDeclaration } from '../types.ts';
import {
  chartCommands,
  contextualCommands,
  coreViewCommands,
  devCommands,
  editCommands,
  fileCommands,
  fontCommands,
  insertCommonCommands,
  navigationCommands,
  paragraphCommands,
  reviewCommonCommands,
  viewCommonCommands,
} from './common.ts';
import { deckCommands, deckRibbon } from './deck.ts';
import { DESCRIPTIONS } from './descriptions.ts';
import { gridCommands, gridRibbon } from './grid.ts';
import { notesCommands, notesRibbon } from './notes.ts';
import { writeCommands, writeCoreCommands, writeRibbon } from './write.ts';

export { c, type DeclOptions, section } from './define.ts';

/**
 * The presentation of every command of the suite: title, Office name, icon, synonyms, Key Tip
 * (PLAN.md §1.4, §1.7). Three kinds of ids live here (see `idKind`): the core's commands from
 * `catalog.json` (dispatched by `connectCore`), `ui.*` chrome actions (implemented by the frame),
 * and reservations for commands the core does not implement yet, which stay pending and are hidden
 * in release builds until their app work package adds them to the core.
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
    ...coreViewCommands,
    ...devCommands,
    ...chartCommands,
    ...contextualCommands,
    ...navigationCommands,
    ...writeCommands,
    ...writeCoreCommands,
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

export { deckRibbon, gridRibbon, notesRibbon, writeRibbon };

/** The ribbon of each app (Kiln Home has none). */
export const RIBBONS: Readonly<Partial<Record<AppId, RibbonData>>> = Object.freeze({
  write: writeRibbon,
  grid: gridRibbon,
  deck: deckRibbon,
  notes: notesRibbon,
});
