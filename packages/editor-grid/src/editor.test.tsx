import { connectCore, createCommandRegistry } from '@kiln/commands';
import type {
  Command,
  Event,
  ExecResult,
  GridLayout,
  GridQuery,
  GridState,
  KilnClient,
  Query,
  QueryResult,
  StatusBar,
} from '@kiln/protocol';
import { act, fireEvent, render, screen, within } from '@testing-library/react';
import { afterEach, describe, expect, it } from 'vitest';
import { hintParts } from './Autocomplete.tsx';
import { GRID_DIALOG_COMMANDS, registerGridUi } from './commands.ts';
import { cursorFor, panes } from './controller.ts';
import { CATEGORIES, dashOf } from './dialogs/FormatCellsDialog.tsx';
import { FindBar } from './FindBar.tsx';
import { colourRuns, FormulaBar } from './FormulaBar.tsx';
import { GridDialogs } from './GridDialogs.tsx';
import { gridMenuEntries } from './GridMenus.tsx';
import { modeItems, statItems } from './GridStatusBar.tsx';
import { gridKeyAction, typesText } from './keys.ts';
import { GridEditorModel, gridResult, messageOf } from './model.ts';
import { SheetTabs } from './SheetTabs.tsx';

const LAYOUT: GridLayout = {
  header_w: 44,
  header_h: 24,
  freeze_rows: 0,
  freeze_cols: 0,
  frozen_h: 0,
  frozen_w: 0,
  content_w: 6400,
  content_h: 20000,
  sheet_zoom: 1,
  split: false,
};

const STATE: GridState = {
  sheets: [
    { name: 'Budget', color: '1D8A60', hidden: false },
    { name: 'Firings', color: null, hidden: false },
    { name: 'Secret', color: null, hidden: true },
  ],
  active_sheet: 0,
  name_box: 'G10',
  formula: '=SUM(G5:G9)',
  mode: 'Ready',
  editor: null,
  format: {
    font_family: 'Calibri',
    font_size: 11,
    bold: true,
    italic: false,
    underline: false,
    strike: false,
    font_color: null,
    fill_color: null,
    h_align: 'General',
    v_align: 'Bottom',
    wrap: false,
    merged: false,
    indent: 0,
    rotation: 0,
    number_format: 'General',
    number_format_name: 'General',
  },
  layout: LAYOUT,
  gridlines: true,
  headings: true,
  show_formulas: false,
  reveal: null,
  copy_marked: false,
  multi: false,
  whole_rows: false,
  whole_columns: false,
  stats: { average: '1,590', count: 5, numerical_count: 5, min: '380', max: '4,380', sum: '7,950' },
  names: ['Budget2026', 'Members'],
} as GridState;

const STATUS = { zoom: 1, view: 'Normal' } as StatusBar;

type Listener = (e: Event) => void;

/** A core that answers the Grid queries and records the commands. */
class FakeCore {
  readonly listeners = new Set<Listener>();
  readonly queries: GridQuery[] = [];
  readonly executed: Command[] = [];
  state: GridState = STATE;

  client(): KilnClient {
    return {
      bridge: 'wasm',
      subscribe: (l: Listener) => {
        this.listeners.add(l);
        return () => this.listeners.delete(l);
      },
      query: async (_doc: number | null, q: Query): Promise<QueryResult> => {
        if (q === 'Status') return { Status: STATUS };
        if (typeof q === 'object' && 'Grid' in q) {
          this.queries.push(q.Grid);
          const g = q.Grid;
          if (g === 'State') return { Grid: { State: this.state } };
          if (typeof g === 'object' && 'FormatPreview' in g)
            return { Grid: { Preview: `[${g.FormatPreview.code}]` } };
          if (typeof g === 'object' && 'ZoomToSelection' in g) return { Grid: { Zoom: 1.5 } };
          if (typeof g === 'object' && 'Functions' in g)
            return {
              Grid: {
                Functions: [
                  {
                    name: 'SUM',
                    category: 'Math & Trig',
                    syntax: 'SUM(number1, [number2], …)',
                    summary: 'Adds.',
                  },
                  {
                    name: 'SUMIF',
                    category: 'Math & Trig',
                    syntax: 'SUMIF(range, criteria)',
                    summary: 'Adds if.',
                  },
                ].filter((f) => f.name.startsWith(g.Functions.prefix)),
              },
            };
        }
        return { Text: '' } as QueryResult;
      },
      execute: async (_doc: number | null, c: Command): Promise<ExecResult> => {
        this.executed.push(c);
        return { outcome: 'Applied', summary: null, value: 'None' } as unknown as ExecResult;
      },
    } as unknown as KilnClient;
  }

  changed(revision: number): void {
    const e = {
      Changed: { summary: { doc: 1, revision } as never, selection: null, status: STATUS },
    } as unknown as Event;
    for (const l of this.listeners) l(e);
  }
}

async function settle(): Promise<void> {
  for (let i = 0; i < 3; i++) {
    await act(async () => {
      await new Promise((r) => setTimeout(r, 30));
    });
  }
}

const models: GridEditorModel[] = [];
afterEach(() => {
  for (const m of models.splice(0)) m.dispose();
});

function setup() {
  const core = new FakeCore();
  const registry = createCommandRegistry({ app: 'grid' });
  const client = core.client();
  connectCore(registry, {
    client,
    document: { doc: 1, title: 'Book1', read_only: false, can_undo: false, can_redo: false } as never,
  });
  const model = new GridEditorModel({ client, doc: 1, registry });
  models.push(model);
  return { core, registry, model };
}

const key = (k: string, mods: Partial<{ shift: boolean; ctrl: boolean; alt: boolean }> = {}) => ({
  key: k,
  shiftKey: !!mods.shift,
  ctrlKey: !!mods.ctrl,
  metaKey: false,
  altKey: !!mods.alt,
});

describe('gridKeyAction', () => {
  it('moves, advances and pages in Ready mode', () => {
    expect(gridKeyAction(key('ArrowDown'), 'Ready', null)).toEqual({
      kind: 'core',
      id: 'grid.nav.move',
      args: { dir: 'Down', extend: false },
    });
    expect(gridKeyAction(key('ArrowRight', { shift: true }), 'Ready', null)).toMatchObject({
      args: { dir: 'Right', extend: true },
    });
    expect(gridKeyAction(key('Enter'), 'Ready', null)).toMatchObject({
      id: 'grid.nav.advance',
      args: { backward: false, across: false },
    });
    expect(gridKeyAction(key('Tab', { shift: true }), 'Ready', null)).toMatchObject({
      args: { backward: true, across: true },
    });
    expect(gridKeyAction(key('PageDown'), 'Ready', null)).toMatchObject({
      id: 'grid.nav.page',
      args: { dir: 'Down' },
    });
    expect(gridKeyAction(key('PageDown', { alt: true }), 'Ready', null)).toMatchObject({
      args: { dir: 'Right' },
    });
    expect(gridKeyAction(key('Home'), 'Ready', null)).toMatchObject({ id: 'grid.nav.row_start' });
    expect(gridKeyAction(key('Delete'), 'Ready', null)).toMatchObject({ id: 'grid.editing.clear_contents' });
    expect(gridKeyAction(key('Backspace'), 'Ready', null)).toMatchObject({
      id: 'grid.editing.begin',
      args: { text: '' },
    });
    expect(gridKeyAction(key('a'), 'Ready', null)).toEqual({ kind: 'native' });
    // Ctrl+arrows and the rest belong to the keymap.
    expect(gridKeyAction(key('ArrowDown', { ctrl: true }), 'Ready', null)).toBeNull();
    expect(gridKeyAction(key('F5'), 'Ready', null)).toBeNull();
  });

  it('commits, cancels, points and completes while editing', () => {
    expect(gridKeyAction(key('Enter'), 'Enter', null)).toMatchObject({
      id: 'grid.editing.commit',
      args: { advance: 'Down' },
    });
    expect(gridKeyAction(key('Enter', { shift: true }), 'Edit', null)).toMatchObject({
      args: { advance: 'Up' },
    });
    expect(gridKeyAction(key('Tab'), 'Enter', null)).toMatchObject({ args: { advance: 'Right' } });
    expect(gridKeyAction(key('Enter', { alt: true }), 'Enter', null)).toEqual({ kind: 'newline' });
    expect(gridKeyAction(key('Enter', { ctrl: true }), 'Enter', null)).toMatchObject({
      id: 'grid.editing.fill_selection',
    });
    expect(gridKeyAction(key('Enter', { ctrl: true, shift: true }), 'Enter', null)).toMatchObject({
      id: 'grid.editing.array_formula',
    });
    expect(gridKeyAction(key('Escape'), 'Edit', null)).toMatchObject({ id: 'grid.editing.cancel_edit' });
    expect(gridKeyAction(key('F4'), 'Edit', null)).toMatchObject({ id: 'grid.editing.toggle_absolute' });
    expect(gridKeyAction(key('ArrowLeft'), 'Point', null)).toMatchObject({ id: 'grid.editing.point_move' });
    // Edit mode: arrows move the caret.
    expect(gridKeyAction(key('ArrowLeft'), 'Edit', null)).toEqual({ kind: 'native' });
    const list = { completions: 3, pickList: false };
    expect(gridKeyAction(key('ArrowDown'), 'Enter', list)).toMatchObject({
      id: 'grid.editing.move_completion',
      args: { down: true },
    });
    expect(gridKeyAction(key('Tab'), 'Enter', list)).toMatchObject({ id: 'grid.editing.accept_completion' });
    expect(gridKeyAction(key('Enter'), 'Enter', { completions: 0, pickList: true })).toMatchObject({
      id: 'grid.editing.accept_completion',
    });
    expect(gridKeyAction(key('c', { ctrl: true }), 'Edit', null)).toEqual({ kind: 'native' });
    expect(gridKeyAction(key('b', { ctrl: true }), 'Edit', null)).toBeNull();
  });

  it('knows which keys type', () => {
    expect(typesText(key('x'))).toBe(true);
    expect(typesText(key('é'))).toBe(true);
    expect(typesText(key('Dead'))).toBe(true);
    expect(typesText(key('ArrowUp'))).toBe(false);
    expect(typesText(key('x', { ctrl: true }))).toBe(false);
  });
});

describe('presentation helpers', () => {
  it('colours the references of a formula', () => {
    const runs = colourRuns('=SUM(G5:G9)+A1', [
      { start: 5, end: 10, color: '1D8A60' },
      { start: 12, end: 14, color: 'C0504D' },
    ]);
    expect(runs).toEqual([
      { text: '=SUM(', color: null },
      { text: 'G5:G9', color: '1D8A60' },
      { text: ')+', color: null },
      { text: 'A1', color: 'C0504D' },
    ]);
    // A stale span past the end is ignored.
    expect(colourRuns('=A', [{ start: 1, end: 4, color: 'x' }])).toEqual([{ text: '=A', color: null }]);
  });

  it('marks the argument at the caret', () => {
    const parts = hintParts({
      function: 'IF',
      syntax: '',
      summary: '',
      since: '2007',
      current: 1,
      args: [
        { name: 'logical_test', description: '', optional: false },
        { name: 'value_if_true', description: '', optional: false },
        { name: 'value_if_false', description: '', optional: true },
      ],
    });
    expect(parts.map((p) => p.text).join('')).toBe('IF(logical_test, value_if_true, [value_if_false])');
    expect(parts.filter((p) => p.current).map((p) => p.text)).toEqual(['value_if_true']);
  });

  it('lays out four panes and the headers for frozen panes', () => {
    const p = panes(
      { ...LAYOUT, freeze_rows: 1, freeze_cols: 1, frozen_h: 20, frozen_w: 64 },
      1,
      800,
      600,
      300,
      400,
    );
    const cells = p.filter((x) => x.kind === 'cells');
    expect(cells).toHaveLength(4);
    expect(cells[0]).toEqual({ kind: 'cells', rect: { x: 44, y: 24, w: 64, h: 20 }, x: 0, y: 0 });
    expect(cells[3]).toEqual({ kind: 'cells', rect: { x: 108, y: 44, w: 692, h: 556 }, x: 300, y: 400 });
    expect(p.filter((x) => x.kind === 'colHeader')).toHaveLength(2);
    expect(p.filter((x) => x.kind === 'rowHeader')).toHaveLength(2);
    expect(p.find((x) => x.kind === 'corner')?.rect).toEqual({ x: 0, y: 0, w: 44, h: 24 });
    // No freeze: one cell pane; no headings: no header panes.
    expect(panes(LAYOUT, 1, 800, 600, 0, 0).filter((x) => x.kind === 'cells')).toHaveLength(1);
    expect(panes({ ...LAYOUT, header_w: 0, header_h: 0 }, 1, 800, 600, 0, 0)).toHaveLength(1);
    // Zoom scales the strips.
    expect(panes(LAYOUT, 2, 800, 600, 0, 0).find((x) => x.kind === 'corner')?.rect).toEqual({
      x: 0,
      y: 0,
      w: 88,
      h: 48,
    });
  });

  it('picks the pointer for what is under it', () => {
    expect(cursorFor({ Cell: { row: 0, col: 0 } } as never)).toBe('cell');
    expect(cursorFor({ ColumnBorder: { col: 1, start: 0, size: 64 } } as never)).toBe('col-resize');
    expect(cursorFor({ RowBorder: { row: 1, start: 0, size: 20 } } as never)).toBe('row-resize');
    expect(cursorFor('FillHandle' as never)).toBe('crosshair');
  });

  it('lists the status bar items Excel shows', () => {
    expect(modeItems('Point', true).map((i) => i.id)).toEqual(['mode', 'readonly']);
    const items = statItems(STATE.stats);
    expect(items.map((i) => i.id)).toEqual(['average', 'count', 'numerical', 'min', 'max', 'sum']);
    expect(items.filter((i) => !i.defaultHidden).map((i) => i.id)).toEqual(['average', 'count', 'sum']);
    expect(statItems(null)).toEqual([]);
    expect(
      statItems({ ...STATE.stats, average: null, sum: null, min: null, max: null } as never).map((i) => i.id),
    ).toEqual(['count', 'numerical']);
  });

  it('builds the context menus of cells, rows and columns', () => {
    const ids = (k: 'cell' | 'row' | 'column') =>
      gridMenuEntries(k).flatMap((e) => ('command' in e ? [e.command] : []));
    expect(ids('cell')).toContain('ui.grid.insert_cells');
    expect(ids('cell')).toContain('grid.cells.format_cells');
    expect(ids('row')).toContain('grid.cells.insert_rows');
    expect(ids('row')).toContain('ui.grid.row_height');
    expect(ids('column')).toContain('grid.cells.hide_columns');
    expect(ids('column')).toContain('ui.grid.column_width');
  });

  it('offers Excel’s number format categories and line styles', () => {
    expect(Object.keys(CATEGORIES)).toEqual([
      'General',
      'Number',
      'Currency',
      'Accounting',
      'Date',
      'Time',
      'Percentage',
      'Fraction',
      'Scientific',
      'Text',
      'Special',
      'Custom',
    ]);
    expect(dashOf('Double')).toEqual({ width: 1, double: true });
    expect(dashOf('Thick').width).toBe(3);
    expect(dashOf('None').width).toBe(0);
  });

  it('unwraps answers and errors', () => {
    expect(gridResult({ Grid: { Zoom: 2 } })).toEqual({ Zoom: 2 });
    expect(gridResult({ Text: '' })).toBeNull();
    expect(messageOf({ message: 'invalid input: no such sheet' })).toBe('no such sheet');
    expect(messageOf(null)).toBe('The command could not run.');
  });
});

describe('GridEditorModel', () => {
  it('asks the core for the state after a change and puts it in the registry context', async () => {
    const { core, registry, model } = setup();
    await settle();
    expect(model.state.grid?.name_box).toBe('G10');
    expect(registry.context.gridState).toEqual(STATE);
    core.state = { ...STATE, mode: 'Enter', name_box: 'A1' };
    core.queries.length = 0;
    core.changed(1);
    core.changed(2);
    await settle();
    expect(core.queries.filter((q) => q === 'State')).toHaveLength(1);
    expect(model.state.grid?.mode).toBe('Enter');
    expect(registry.context.scopes).toEqual(['cell-edit']);
  });

  it('scrolls the surface once per reveal request', async () => {
    const { core, model } = setup();
    const reveals: [number, number][] = [];
    model.surface = {
      focus: () => undefined,
      reveal: (x, y) => reveals.push([x, y]),
      size: () => ({ width: 800, height: 600 }),
    };
    core.state = { ...STATE, reveal: { seq: 3, x: 640, y: 1200 } };
    await settle();
    core.changed(1);
    await settle();
    expect(reveals).toEqual([[640, 1200]]);
  });

  it('sends the surface’s own commands straight to the core', async () => {
    const { core, model } = setup();
    await model.core('grid.nav.move', { dir: 'Down', extend: false });
    expect(core.executed).toEqual([{ Grid: { Move: { dir: 'Down', extend: false } } }]);
  });

  it('remembers the formula bar preferences', () => {
    const { registry, model } = setup();
    model.set({ showFormulaBar: false, formulaBarExpanded: true });
    const again = new GridEditorModel({ client: new FakeCore().client(), doc: 1, registry });
    models.push(again);
    expect(again.state.showFormulaBar).toBe(false);
    expect(again.state.formulaBarExpanded).toBe(true);
    again.set({ showFormulaBar: true, formulaBarExpanded: false });
  });
});

describe('registerGridUi', () => {
  it('opens the dialogs, the find bar and toggles the formula bar', async () => {
    const { registry, model } = setup();
    const ui = registerGridUi(model);
    for (const d of GRID_DIALOG_COMMANDS) expect(registry.getState(d.id).exists).toBe(true);
    await registry.execute('grid.cells.format_cells', { tab: 'border' });
    expect(model.state.dialog).toEqual({ id: 'formatCells', tab: 'border' });
    await registry.execute('grid.formulas.insert_function', { category: 'Financial' });
    expect(model.state.dialog).toEqual({ id: 'insertFunction', tab: 'Financial' });
    await registry.execute('suite.edit.find');
    expect(model.state.find.open).toBe(true);
    const before = model.state.showFormulaBar;
    await registry.execute('grid.view.formula_bar');
    expect(model.state.showFormulaBar).toBe(!before);
    expect(registry.context.editor).toBe('grid');
    ui.dispose();
    expect(registry.context.editor).toBeUndefined();
  });

  it('zooms to the selection with the core’s answer', async () => {
    const { core, registry, model } = setup();
    const ui = registerGridUi(model);
    model.surface = {
      focus: () => undefined,
      reveal: () => undefined,
      size: () => ({ width: 1000, height: 500 }),
    };
    await registry.execute('grid.view.zoom_to_selection');
    expect(core.queries).toContainEqual({ ZoomToSelection: { width: 1000, height: 500 } });
    ui.dispose();
  });
});

describe('components', () => {
  it('shows the name box, formula and Go To through the name box', async () => {
    const { core, model } = setup();
    registerGridUi(model);
    await settle();
    render(<FormulaBar model={model} />);
    const box = screen.getByLabelText('Name Box') as HTMLInputElement;
    expect(box.value).toBe('G10');
    expect((screen.getByLabelText('Formula Bar') as HTMLTextAreaElement).value).toBe('=SUM(G5:G9)');
    fireEvent.focus(box);
    fireEvent.change(box, { target: { value: 'B12' } });
    fireEvent.keyDown(box, { key: 'Enter' });
    await settle();
    expect(core.executed).toContainEqual({ Grid: { GoTo: { reference: 'B12', define: true } } });
  });

  it('lists the visible sheets and activates one', async () => {
    const { core, model } = setup();
    await settle();
    render(<SheetTabs model={model} />);
    const tabs = screen.getAllByRole('tab');
    expect(tabs.map((t) => t.textContent)).toEqual(['Budget', 'Firings']);
    expect(tabs[0]?.getAttribute('aria-selected')).toBe('true');
    fireEvent.pointerDown(tabs[1] as HTMLElement, { button: 0 });
    await settle();
    expect(core.executed).toContainEqual({ Grid: { ActivateSheet: { index: 1 } } });
  });

  it('finds the next match from the find bar', async () => {
    const { core, model } = setup();
    model.set({ find: { ...model.state.find, open: true } });
    render(<FindBar model={model} />);
    const input = screen.getByLabelText('Find what');
    fireEvent.change(input, { target: { value: 'kiln' } });
    fireEvent.keyDown(input, { key: 'Enter' });
    await settle();
    expect(core.executed).toContainEqual({
      Grid: {
        FindNext: { text: 'kiln', match_case: false, whole_cell: false, backward: false, in_formulas: false },
      },
    });
  });

  it('sends Insert Cells with the chosen shift', async () => {
    const { core, model } = setup();
    await settle();
    model.openDialog('insertCells');
    render(<GridDialogs model={model} />);
    fireEvent.click(screen.getByLabelText('Entire row'));
    fireEvent.click(screen.getByRole('button', { name: 'OK' }));
    await settle();
    expect(core.executed).toContainEqual({ Grid: { InsertCells: { shift: 'Rows' } } });
    expect(model.state.dialog).toBeNull();
  });

  it('inserts a function from Insert Function', async () => {
    const { core, model } = setup();
    await settle();
    model.openDialog('insertFunction');
    render(<GridDialogs model={model} />);
    fireEvent.change(screen.getByLabelText('Search for a function'), { target: { value: 'sumi' } });
    await settle();
    expect(
      within(screen.getByRole('listbox', { name: 'Functions' }))
        .getAllByRole('option')
        .map((o) => o.textContent),
    ).toEqual(['SUMIF']);
    fireEvent.click(screen.getByRole('button', { name: 'OK' }));
    await settle();
    expect(core.executed).toContainEqual({ Grid: { BeginEdit: { text: '=SUMIF(' } } });
  });
});
