/**
 * Kiln Grid's surface gestures: the declarations of the core's F8 / Shift+F8 selection
 * modes, drag and drop of cells, the context menu's Filter and Sort by the active cell, and the
 * error indicator's Ignore Error and Convert to Number, with how the UI builds each typed
 * `Command` from its arguments.
 */
import type { Command, GridCommand, GridDragPhase, GridSelectionKey, GridSelectMode } from '@kiln/protocol';
import type { CommandDeclaration } from '../types.ts';
import { c, GRID, section } from './define.ts';

const G = { apps: GRID };
const hidden = { palette: false } as const;

export const gridSurfaceCommands: readonly CommandDeclaration[] = [
  ...section('Selection', G, [
    c('grid.nav.selection_mode', 'Extend or Add to Selection', 'LassoSelect', {
      ...hidden,
      synonyms: ['extend selection', 'add or remove selection', 'F8'],
    }),
  ]),
  ...section('Editing', G, [c('grid.editing.drag_cells', 'Move or Copy Cells', 'Move', hidden)]),
  ...section('Data', G, [
    c('grid.data.filter_by_selection', "Filter by Selected Cell's Value", 'FilterToggle', hidden),
    c('grid.data.sort_by_selection_color', 'Put Selected Cell Color On Top', 'SortAscending', hidden),
  ]),
  ...section('Formulas', G, [
    c('grid.formulas.ignore_error', 'Ignore Error', 'ErrorChecking', hidden),
    c('grid.formulas.convert_to_number', 'Convert to Number', 'Hash', hidden),
  ]),
];

type Args = Readonly<Record<string, unknown>>;

class SurfaceArgsError extends Error {
  override readonly name = 'CommandArgsError';
}

const uint = (id: string, a: Args, key: string): number => {
  const v = a[key];
  if (typeof v !== 'number' || !Number.isInteger(v) || v < 0)
    throw new SurfaceArgsError(`${id}: "${key}" must be a row or column`);
  return v;
};

function oneOf<T extends string>(id: string, v: unknown, all: readonly T[], fallback: T): T {
  if (v === undefined) return fallback;
  if (!all.includes(v as T)) throw new SurfaceArgsError(`${id}: must be one of ${all.join(', ')}`);
  return v as T;
}

const MODES: readonly GridSelectMode[] = ['Normal', 'Extend', 'Add'];
const PHASES: readonly GridDragPhase[] = ['Preview', 'Drop', 'Cancel'];
const KEYS: readonly GridSelectionKey[] = ['Value', 'FillColor', 'FontColor'];

const TABLE: Readonly<Record<string, (a: Args, id: string) => GridCommand>> = {
  'grid.nav.selection_mode': (a, id) => ({
    SelectionMode: { mode: oneOf(id, a.mode, MODES, 'Extend') },
  }),
  'grid.editing.drag_cells': (a, id) => ({
    DragCells: {
      phase: oneOf(id, a.phase, PHASES, 'Drop'),
      row: uint(id, a, 'row'),
      col: uint(id, a, 'col'),
      grab_row: uint(id, a, 'grab_row'),
      grab_col: uint(id, a, 'grab_col'),
      copy: a.copy === true,
      insert: a.insert === true,
      replace: a.replace === true,
    },
  }),
  'grid.data.filter_by_selection': (a, id) => ({
    FilterBySelection: { by: oneOf(id, a.by, KEYS, 'Value') },
  }),
  'grid.data.sort_by_selection_color': (a) => ({ SortBySelectionColor: { font: a.font === true } }),
  'grid.formulas.ignore_error': () => 'IgnoreError',
  'grid.formulas.convert_to_number': () => 'ConvertToNumber',
};

/** The builders of the core commands (spread into `GRID_BUILDERS`). */
export const GRID_SURFACE_BUILDERS: Readonly<Record<string, (a: Args) => Command>> = Object.freeze(
  Object.fromEntries(
    Object.entries(TABLE).map(([id, build]) => [id, (a: Args) => ({ Grid: build(a, id) })] as const),
  ),
);
