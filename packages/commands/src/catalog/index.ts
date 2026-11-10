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
import { DECK_SLIDE_MASTER_TAB, deckCommands, deckRibbon } from './deck.ts';
import { DESCRIPTIONS } from './descriptions.ts';
import { gridCommands, gridRibbon } from './grid.ts';
import { gridAnalysisCommands } from './grid-analysis.ts';
import { gridFeatureCommands } from './grid-features.ts';
import { gridSurfaceCommands } from './grid-surface.ts';
import { notesCommands, notesRibbon } from './notes.ts';
import { writeCommands, writeCoreCommands, writeRibbon } from './write.ts';
import { writeFeatureCommands } from './write-features.ts';
import { writeOfficeCommands } from './write-office.ts';

export { DECK_CHROME_COMMANDS } from './deck.ts';
export { c, type DeclOptions, section } from './define.ts';
export { GRID_THEMES } from './grid-analysis.ts';
export { GRID_CHROME_COMMANDS } from './grid-features.ts';
export { COLLAB_CHROME_COMMANDS, FRAME_CHROME_COMMANDS, WRITE_CHROME_COMMANDS } from './write-office.ts';

/**
 * The presentation of every command of the suite: title, Office name, icon, synonyms, Key Tip
 *. Three kinds of ids live here (see `idKind`): the core's commands from
 * `catalog.json` (dispatched by `connectCore`), `ui.*` chrome actions (implemented by the frame),
 * and reservations for commands the core does not implement yet, which stay pending and are hidden
 * in release builds until they are added to the core.
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
    ...writeFeatureCommands,
    ...writeOfficeCommands,
    ...gridCommands,
    ...gridFeatureCommands,
    ...gridAnalysisCommands,
    ...gridSurfaceCommands,
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

export { DECK_SLIDE_MASTER_TAB, deckRibbon, gridRibbon, notesRibbon, writeRibbon };

/** The ribbon of each app (Kiln Home has none). */
export const RIBBONS: Readonly<Partial<Record<AppId, RibbonData>>> = Object.freeze({
  write: writeRibbon,
  grid: gridRibbon,
  deck: deckRibbon,
  notes: notesRibbon,
});
