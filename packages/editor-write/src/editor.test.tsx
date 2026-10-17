import { type CommandRegistry, createCommandRegistry, RIBBONS } from '@kiln/commands';
import type {
  Command,
  Event,
  ExecResult,
  FormatState,
  KilnClient,
  Query,
  QueryResult,
  Selection,
  StatusBar,
  WriteQuery,
} from '@kiln/protocol';
import { ribbonFromData } from '@kiln/ui-kit';
import { act, fireEvent, render, screen } from '@testing-library/react';
import { afterEach, describe, expect, it, vi } from 'vitest';
import { DIALOG_COMMANDS, fitZoom, registerWriteUi } from './commands.ts';
import { afterPaint, contextsOf, WriteEditorModel } from './model.ts';
import { TableGridPicker } from './TableGridPicker.tsx';
import { contextMenuEntries, miniToolbarItems } from './WriteEditor.tsx';
import { writeStatusItems } from './WriteStatusBar.tsx';

type Listener = (e: Event) => void;

const FORMAT: FormatState = {
  font: 'Calibri',
  size: 11,
  bold: false,
  italic: false,
  underline: null,
  strike: false,
  double_strike: false,
  subscript: false,
  superscript: false,
  all_caps: false,
  small_caps: false,
  hidden: false,
  color: null,
  highlight: null,
  spacing: 0,
  mixed: [],
  alignment: 'Left',
  line_multiple: 1.08,
  line_points: null,
  indent_left: 0,
  indent_right: 0,
  first_line: 0,
  space_before: 0,
  space_after: 8,
  keep_next: false,
  keep_lines: false,
  widow_control: true,
  page_break_before: false,
  contextual_spacing: false,
  outline_level: 0,
  style_id: 'Normal',
  style_name: 'Normal',
  char_style_id: null,
  list: null,
  table: null,
  story: 'Body',
  header_footer: null,
  section: {} as FormatState['section'],
  show_marks: false,
  collapsed: true,
  link: null,
  can_repeat: false,
  tabs: [],
  shading: null,
  borders: false,
};

const STATUS: StatusBar = {
  page: { current: 2, total: 12 },
  words: 1284,
  chars: 7000,
  language: 'English (United States)',
  cells: null,
  slide: null,
  zoom: 1,
  view: 'PrintLayout',
} as StatusBar;

const SELECTION: Selection = {
  kind: { Text: { anchor: { block: 1, offset: 0 }, head: { block: 1, offset: 0 } } },
  rects: [],
  caret: null,
  handles: [],
} as unknown as Selection;

/** A core that answers the Write queries the editor asks and records everything. */
class FakeCore {
  readonly listeners = new Set<Listener>();
  readonly queries: WriteQuery[] = [];
  readonly executed: Command[] = [];
  format: FormatState = FORMAT;

  client(): KilnClient {
    return {
      bridge: 'wasm',
      subscribe: (l: Listener) => {
        this.listeners.add(l);
        return () => this.listeners.delete(l);
      },
      query: async (_doc: number | null, q: Query): Promise<QueryResult> => {
        if (q === 'Status') return { Status: STATUS };
        if (typeof q === 'object' && 'Write' in q) {
          this.queries.push(q.Write);
          if (q.Write === 'Format') return { Write: { Format: this.format } };
          if (q.Write === 'Rulers') return { Write: { Rulers: null } };
          if (q.Write === 'Styles') return { Write: { Styles: [] } };
        }
        return { Text: '' };
      },
      execute: async (_doc: number | null, c: Command): Promise<ExecResult> => {
        this.executed.push(c);
        return { outcome: 'Applied', summary: null, value: 'None' };
      },
    } as unknown as KilnClient;
  }

  changed(revision: number): void {
    const e = {
      Changed: {
        summary: { doc: 1, revision } as never,
        selection: SELECTION,
        status: STATUS,
      },
    } as Event;
    for (const l of this.listeners) l(e);
  }
}

/** Lets after-paint work (an animation frame, then a task) and the queries it starts finish. */
async function settle(): Promise<void> {
  for (let i = 0; i < 3; i++) {
    await act(async () => {
      await new Promise((r) => setTimeout(r, 40));
    });
  }
}

function setup(): { core: FakeCore; registry: CommandRegistry; model: WriteEditorModel } {
  const core = new FakeCore();
  const registry = createCommandRegistry({ app: 'write' });
  const model = new WriteEditorModel({ client: core.client(), doc: 1, registry });
  return { core, registry, model };
}

const models: WriteEditorModel[] = [];
afterEach(() => {
  for (const m of models.splice(0)) m.dispose();
});

describe('WriteEditorModel', () => {
  it('asks the core after a change has painted and puts the formatting in the registry context', async () => {
    const { core, registry, model } = setup();
    models.push(model);
    await settle();
    expect(core.queries).toContain('Format');
    expect(model.state.format?.font).toBe('Calibri');
    expect(registry.context.textFormat).toEqual(FORMAT);
    expect(model.state.status?.page).toEqual({ current: 2, total: 12 });
    // A table and a header turn on their contextual tabs.
    core.format = { ...FORMAT, table: {} as FormatState['table'], story: 'Header' };
    core.changed(1);
    await settle();
    expect(registry.context.contexts).toEqual(['table', 'header_footer']);
  });

  it('keeps unchanged answers, so nothing re-renders for them', async () => {
    const { core, model } = setup();
    models.push(model);
    await settle();
    const first = model.state.format;
    core.changed(1);
    await settle();
    expect(model.state.format).toBe(first);
  });

  it('coalesces the changes of one frame into one round of questions', async () => {
    const { core, model } = setup();
    models.push(model);
    await settle();
    core.queries.length = 0;
    core.changed(1);
    core.changed(2);
    core.changed(3);
    await settle();
    expect(core.queries.filter((q) => q === 'Format')).toHaveLength(1);
  });

  it('remembers the ruler and the Navigation pane, and reports a failed command', async () => {
    const { registry, model } = setup();
    models.push(model);
    model.set({ showRuler: false, navOpen: true });
    const again = new WriteEditorModel({ client: new FakeCore().client(), doc: 1, registry });
    models.push(again);
    expect(again.state.showRuler).toBe(false);
    expect(again.state.navOpen).toBe(true);
    registry.register({
      id: 'write.test.fails',
      title: 'Fails',
      run: () => {
        throw new Error('The selection is not in a table.');
      },
    });
    await expect(model.run('write.test.fails')).resolves.toBe(false);
    expect(model.state.notice).toBe('The selection is not in a table.');
  });
});

describe('contexts and helpers', () => {
  it('names the contextual tabs of a selection', () => {
    expect(contextsOf(FORMAT)).toEqual([]);
    expect(contextsOf({ ...FORMAT, story: 'Footer' })).toEqual(['header_footer']);
  });

  it('runs after-paint work once, and not after cancel', async () => {
    const fn = vi.fn();
    afterPaint(fn);
    const cancelled = vi.fn();
    afterPaint(cancelled).cancel();
    await settle();
    expect(fn).toHaveBeenCalledTimes(1);
    expect(cancelled).not.toHaveBeenCalled();
  });

  it('fits the zoom to the page width, one page and two pages', () => {
    const page = { width: 816, height: 1056 };
    expect(fitZoom('page_width', page, { width: 1680, height: 900 })).toBe(2);
    expect(fitZoom('one_page', page, { width: 1680, height: 900 })).toBe(0.81);
    expect(fitZoom('multiple_pages', page, { width: 1680, height: 900 })).toBeLessThan(0.81);
    expect(fitZoom('page_width', page, { width: 10, height: 10 })).toBe(0.1);
  });

  it('lists the status bar items of Word, characters hidden by default', () => {
    const open = vi.fn();
    const items = writeStatusItems(STATUS, false, open);
    expect(items.map((i) => i.id)).toEqual(['page', 'words', 'chars', 'language', 'track']);
    expect(items[0]?.content).toBe('Page 2 of 12');
    expect(items[1]?.content).toBe(`${(1284).toLocaleString()} words`);
    expect(items.find((i) => i.id === 'chars')?.defaultHidden).toBe(true);
    items[0]?.onClick?.();
    expect(open).toHaveBeenCalledWith('find', 'goto');
    expect(writeStatusItems(null, true, open).map((i) => i.id)).toContain('readonly');
  });

  it('shows table commands in the context menu only inside a table, and Remove Link on a link', () => {
    const ids = (entries: ReturnType<typeof contextMenuEntries>) =>
      JSON.stringify(entries).match(/"command":"[a-z._]+"/g) ?? [];
    expect(ids(contextMenuEntries(false, null)).join()).not.toContain('suite.table');
    expect(ids(contextMenuEntries(true, null)).join()).toContain('suite.table.insert_row_above');
    expect(ids(contextMenuEntries(false, 'https://kiln.app')).join()).toContain('write.insert.remove_link');
  });

  it('builds the mini toolbar from the ribbon’s own Home controls', () => {
    const ribbon = RIBBONS.write ? ribbonFromData(RIBBONS.write) : undefined;
    const items = miniToolbarItems(ribbon).map((i) => ('command' in i ? i.command : i.kind));
    expect(items.slice(0, 2)).toEqual(['suite.format.font_family', 'suite.format.font_size']);
    expect(items).toContain('suite.format.bold');
    expect(items).toContain('suite.format.highlight');
    expect(miniToolbarItems(undefined)).toEqual([]);
  });
});

describe('registerWriteUi', () => {
  it('opens the dialogs, the Navigation pane for Find, and toggles the ruler', async () => {
    const { registry, model } = setup();
    models.push(model);
    const ui = registerWriteUi(model);
    await registry.execute('suite.format.font_dialog');
    expect(model.state.dialog).toEqual({ id: 'font' });
    await registry.execute('suite.edit.go_to');
    expect(model.state.dialog).toEqual({ id: 'find', tab: 'goto' });
    await registry.execute('suite.edit.find');
    expect(model.state.navOpen).toBe(true);
    expect(model.state.navTab).toBe('results');
    const ruler = model.state.showRuler;
    await registry.execute('suite.view.ruler');
    expect(model.state.showRuler).toBe(!ruler);
    expect(registry.context.editor).toBe('write');
    ui.dispose();
    expect(registry.context.editor).toBeUndefined();
    expect(DIALOG_COMMANDS.length).toBeGreaterThan(10);
  });
});

describe('TableGridPicker', () => {
  it('sizes with the arrow keys and inserts what it shows', () => {
    const onInsert = vi.fn();
    render(<TableGridPicker onInsert={onInsert} />);
    const first = screen.getByRole('button', { name: 'Insert a table of 1 by 1' });
    act(() => first.focus());
    expect(screen.getByText('1×1 Table')).toBeTruthy();
    fireEvent.keyDown(document.activeElement as Element, { key: 'ArrowRight' });
    fireEvent.keyDown(document.activeElement as Element, { key: 'ArrowRight' });
    fireEvent.keyDown(document.activeElement as Element, { key: 'ArrowDown' });
    expect(screen.getByText('3×2 Table')).toBeTruthy();
    expect(document.activeElement).toBe(screen.getByRole('button', { name: 'Insert a table of 3 by 2' }));
    fireEvent.click(document.activeElement as Element);
    expect(onInsert).toHaveBeenCalledWith(2, 3);
  });
});

describe('dialogs', () => {
  it('Font… applies what the dialog shows as one core command', async () => {
    const { registry, model } = setup();
    models.push(model);
    await settle();
    const ran = vi.fn();
    registry.register({ id: 'write.format.font', title: 'Font', run: (_c, args) => ran(args) });
    const { WriteDialogs } = await import('./WriteDialogs.tsx');
    render(<WriteDialogs model={model} />);
    act(() => model.openDialog('font'));
    fireEvent.click(await screen.findByRole('checkbox', { name: 'All caps' }));
    fireEvent.click(screen.getByRole('button', { name: 'OK' }));
    await settle();
    expect(ran).toHaveBeenCalledTimes(1);
    const call = ran.mock.calls[0] as [{ format: Record<string, unknown> }];
    const { format } = call[0];
    expect(format).toMatchObject({ family: 'Calibri', size: 11, all_caps: true, bold: false, color: 'auto' });
    expect(model.state.dialog).toBeNull();
  });

  it('Go To jumps to a page number through the core', async () => {
    const { registry, model } = setup();
    models.push(model);
    const ran = vi.fn();
    registry.register({ id: 'write.find.go_to', title: 'Go To', run: (_c, args) => ran(args) });
    const { WriteDialogs } = await import('./WriteDialogs.tsx');
    render(<WriteDialogs model={model} />);
    act(() => model.openDialog('find', 'goto'));
    const box = await screen.findByLabelText(/Enter page number/i);
    fireEvent.change(box, { target: { value: '7' } });
    fireEvent.click(screen.getByRole('button', { name: 'Go To' }));
    await settle();
    expect(ran).toHaveBeenCalled();
    expect(JSON.stringify(ran.mock.calls[0]?.[0])).toContain('7');
  });
});
