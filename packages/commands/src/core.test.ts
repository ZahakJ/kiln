import type {
  Command,
  DocSummary,
  Event,
  EventListener,
  ExecResult,
  KilnClient,
  Selection,
} from '@kiln/protocol';
import { describe, expect, it } from 'vitest';
import { createCommandRegistry } from './bootstrap.ts';
import { buildCommand, CommandArgsError } from './core/build.ts';
import { CommandLog, connectCore, isAvailable } from './core/connect.ts';
import { RELEASE_TIERS } from './tiers.ts';

function summary(patch: Partial<DocSummary> = {}): DocSummary {
  return {
    doc: 1,
    kind: 'Write',
    app: 'Write',
    title: 'Document1',
    path: null,
    format: null,
    revision: 0,
    dirty: false,
    read_only: false,
    can_undo: false,
    can_redo: false,
    undo_label: null,
    redo_label: null,
    ...patch,
  };
}

const NO_SELECTION: Selection = { kind: 'None', rects: [], caret: null, handles: [] };
const STATUS = {
  page: null,
  words: 3,
  chars: 12,
  language: 'en-US',
  cells: null,
  slide: null,
  zoom: 1,
  view: 'PrintLayout',
} as const;

/** A client that records commands and lets the test emit events. */
class FakeClient implements Partial<KilnClient> {
  readonly bridge = 'wasm' as const;
  readonly sent: { doc: number | null; command: Command }[] = [];
  readonly listeners = new Set<EventListener>();
  next: ExecResult | Error = { outcome: 'Applied', summary: null, value: 'None' };

  async execute(doc: number | null, command: Command): Promise<ExecResult> {
    this.sent.push({ doc, command });
    if (this.next instanceof Error) throw this.next;
    return this.next;
  }

  subscribe(listener: EventListener): () => void {
    this.listeners.add(listener);
    return () => this.listeners.delete(listener);
  }

  emit(event: Event): void {
    for (const l of this.listeners) l(event);
  }
}

describe('buildCommand', () => {
  it('builds the typed Command of every shape', () => {
    expect(buildCommand('suite.file.save')).toEqual({ Suite: 'Save' });
    expect(buildCommand('suite.edit.undo')).toEqual({ Suite: 'Undo' });
    expect(buildCommand('suite.view.zoom', { zoom: 1.5 })).toEqual({ Suite: { SetZoom: { zoom: 1.5 } } });
    expect(buildCommand('suite.view.mode', { mode: 'Draft' })).toEqual({
      Suite: { SetViewMode: { mode: 'Draft' } },
    });
    expect(buildCommand('suite.file.new', {}, { app: 'grid' })).toEqual({ Suite: { New: { kind: 'Grid' } } });
    expect(buildCommand('suite.file.save_as', { format: 'Docx' })).toEqual({
      Suite: { SaveAs: { path: null, format: 'Docx' } },
    });
    expect(buildCommand('write.caret.move', { motion: 'WordForward' })).toEqual({
      Write: { MoveCaret: { motion: 'WordForward', extend: false } },
    });
    expect(buildCommand('write.edit.insert_text', { text: 'a' })).toEqual({
      Write: { InsertText: { text: 'a' } },
    });
    expect(buildCommand('suite.chart.move', { to: { NewSheet: { name: 'Chart1' } } })).toEqual({
      Suite: { ChartMove: { to: { NewSheet: { name: 'Chart1' } } } },
    });
  });

  it('rejects arguments of the wrong shape and unknown ids with a typed error', () => {
    expect(() => buildCommand('suite.view.zoom', { zoom: 'big' })).toThrow(CommandArgsError);
    expect(() => buildCommand('suite.view.zoom', {})).toThrow(/"zoom" must be a number/);
    expect(() => buildCommand('write.caret.move', { motion: 'Up', extend: 'yes' })).toThrow(/extend/);
    expect(() => buildCommand('suite.file.new', {})).toThrow(/kind/);
    expect(() => buildCommand('suite.format.text_effects')).toThrow(/not a core command/);
    expect(() => buildCommand('suite.format.font_size', { value: 'big' })).toThrow(/size/);
    expect(buildCommand('suite.format.font_color', { color: { type: 'rgb', hex: '#c8352b' } })).toEqual({
      Suite: { Edit: { FontColor: { color: 'C8352B' } } },
    });
    expect(() => buildCommand('suite.view.zoom', { zoom: Number.NaN })).toThrow(CommandArgsError);
  });
});

describe('availability comes from the core state', () => {
  it('reads the flags of DocSummary and the selection kind', () => {
    const empty = { summary: null, selection: null, status: null };
    expect(isAvailable('Always', empty)).toBe(true);
    expect(isAvailable('Document', empty)).toBe(false);
    const doc = { summary: summary(), selection: NO_SELECTION, status: null };
    expect(isAvailable('Document', doc)).toBe(true);
    expect(isAvailable('Editable', doc)).toBe(true);
    expect(isAvailable('Editable', { ...doc, summary: summary({ read_only: true }) })).toBe(false);
    expect(isAvailable('CanUndo', doc)).toBe(false);
    expect(isAvailable('CanUndo', { ...doc, summary: summary({ can_undo: true }) })).toBe(true);
    expect(isAvailable('Selection', doc)).toBe(false);
    const text = (a: number, b: number): Selection => ({
      kind: { Text: { anchor: { block: 1, offset: a }, head: { block: 1, offset: b } } },
      rects: [],
      caret: null,
      handles: [],
    });
    expect(isAvailable('Selection', { ...doc, selection: text(2, 2) })).toBe(false);
    expect(isAvailable('Selection', { ...doc, selection: text(2, 5) })).toBe(true);
  });
});

describe('connectCore', () => {
  it('dispatches core commands to KilnClient.execute and logs them', async () => {
    const client = new FakeClient();
    const registry = createCommandRegistry({ app: 'write' });
    const log = new CommandLog();
    const conn = connectCore(registry, { client: client as unknown as KilnClient, document: summary(), log });
    expect(registry.getState('suite.file.save').pending).toBe(false);
    const result = await registry.execute('suite.file.save');
    expect(result.status).toBe('ran');
    expect(client.sent).toEqual([{ doc: 1, command: { Suite: 'Save' } }]);
    expect(log.entries()).toMatchObject([{ seq: 1, id: 'suite.file.save', doc: 1, outcome: 'Applied' }]);
    // Session-level commands go without a document.
    await registry.execute('suite.file.new');
    expect(client.sent[1]).toEqual({ doc: null, command: { Suite: { New: { kind: 'Write' } } } });
    // Commands keep the declaration's presentation.
    expect(registry.meta('suite.file.save')?.title).toBe('Save');
    conn.dispose();
    expect(registry.getState('suite.file.save').pending).toBe(true);
  });

  it('records rejected commands with the core error', async () => {
    const client = new FakeClient();
    client.next = Object.assign(new Error('there is nothing to undo'), { kind: 'Unavailable' });
    const registry = createCommandRegistry({ app: 'write', onError: () => {} });
    const log = new CommandLog();
    connectCore(registry, {
      client: client as unknown as KilnClient,
      document: summary({ can_undo: true }),
      log,
    });
    const result = await registry.execute('suite.edit.undo');
    expect(result.status).toBe('failed');
    expect(log.entries()[0]).toMatchObject({
      id: 'suite.edit.undo',
      outcome: 'error',
      error: 'there is nothing to undo',
    });
  });

  it('takes enabled state from Event::Changed, never from its own logic', () => {
    const client = new FakeClient();
    const registry = createCommandRegistry({ app: 'write' });
    const conn = connectCore(registry, { client: client as unknown as KilnClient, document: summary() });
    expect(registry.getState('suite.edit.undo').enabled).toBe(false);
    const seen: boolean[] = [];
    conn.subscribe((s) => seen.push(s.summary?.can_undo ?? false));
    client.emit({
      Changed: {
        summary: summary({ can_undo: true, revision: 1 }),
        selection: NO_SELECTION,
        status: STATUS,
      },
    });
    expect(registry.getState('suite.edit.undo').enabled).toBe(true);
    expect(conn.state.status?.words).toBe(3);
    expect(registry.getState('suite.view.mode').value).toBe('PrintLayout');
    expect(registry.getState('suite.view.zoom').value).toBe(1);
    expect(seen).toEqual([true]);
    // Events of other documents are ignored.
    client.emit({
      Changed: {
        summary: summary({ doc: 9, can_redo: true }),
        selection: NO_SELECTION,
        status: STATUS,
      },
    });
    expect(registry.getState('suite.edit.redo').enabled).toBe(false);
    client.emit({ Closed: { doc: 1 } });
    expect(registry.getState('suite.file.save').enabled).toBe(false);
  });

  it('offers only the commands of the window app and hides planned ones in release builds', () => {
    const client = new FakeClient();
    const registry = createCommandRegistry({ app: 'grid', tiers: RELEASE_TIERS });
    connectCore(registry, {
      client: client as unknown as KilnClient,
      document: summary({ kind: 'Grid', app: 'Grid' }),
    });
    expect(registry.getState('suite.chart.move').visible).toBe(true);
    expect(registry.getState('write.caret.move').visible).toBe(false);
    expect(registry.getState('suite.format.bold').visible).toBe(false); // planned: hidden, never greyed
    expect(registry.getState('suite.file.save').visible).toBe(true);
  });
});
