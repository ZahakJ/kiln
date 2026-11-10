import { createCommandRegistry, DECK_CHROME_COMMANDS, deckRibbon, ribbonCommandRefs } from '@kiln/commands';
import type { DeckState, DocId, Event, KilnClient } from '@kiln/protocol';
import { act, fireEvent, render, screen } from '@testing-library/react';
import { describe, expect, it, vi } from 'vitest';
import { registerDeckUi } from './commands.ts';
import { deckMenuEntries } from './DeckMenus.tsx';
import { DeckStatusBar } from './DeckStatusBar.tsx';
import { DECK_FEATURE_CHROME } from './features/featureCommands.ts';
import { DECK_MOTION_CHROME } from './features/motionCommands.ts';
import { caretMotion, deckKeyAction, railKeyAction } from './keys.ts';
import { DeckEditorModel, scopesOf } from './model.ts';
import { NotesPane } from './NotesPane.tsx';
import { deckRibbonWithLayouts } from './ribbon.ts';
import { clampScroll, cssCursor, FIT_MARGIN, fitView } from './stage.ts';
import { dropIndex, railRows } from './ThumbnailRail.tsx';
import { runs } from './thumbs.ts';

const key = (
  k: string,
  mods: Partial<Record<'shiftKey' | 'ctrlKey' | 'metaKey' | 'altKey', boolean>> = {},
) => ({
  key: k,
  shiftKey: false,
  ctrlKey: false,
  metaKey: false,
  altKey: false,
  ...mods,
});

function deckState(patch: Partial<DeckState> = {}): DeckState {
  return {
    slides: [
      {
        id: 1,
        title: 'Open Studio Weekend',
        hidden: false,
        layout: 'Title Slide',
        notes: false,
        motion: false,
        version: 1,
      },
      {
        id: 2,
        title: 'This year',
        hidden: true,
        layout: 'Title and Content',
        notes: true,
        motion: false,
        version: 1,
      },
      { id: 3, title: '', hidden: false, layout: 'Blank', notes: false, motion: false, version: 1 },
    ],
    sections: [],
    current: 0,
    selected: [0],
    slide_width: 1280,
    slide_height: 720,
    size_kind: 'widescreen',
    shapes: [],
    selection: [],
    editing: false,
    contexts: [],
    format: null,
    selection_box: null,
    alt_text: null,
    theme: 'Kiln',
    themes: ['Kiln'],
    layouts: [
      { id: 10, name: 'Title Slide' },
      { id: 11, name: 'Title and Content' },
    ],
    layout: 10,
    notes: 'Start with the number people remember.',
    guides: false,
    gridlines: false,
    notes_visible: true,
    header_footer: { date: false, slide_number: false, footer: null },
    background: null,
    hide_graphics: false,
    armed: null,
    find_done: false,
    caret: null,
    chart: null,
    comments: [],
    comment_total: 0,
    comment: null,
    spelling: null,
    spell_done: false,
    master_view: null,
    guide_list: [],
    text_area: null,
    cropping: false,
    cells: null,
    motion: {
      transition: null,
      animations: [],
      selected: [],
      options: [],
      option: null,
      painter: false,
      badges: false,
      settings: {
        kind: 'Speaker',
        loop_show: false,
        no_narration: false,
        no_animation: false,
        from: null,
        to: null,
        manual: false,
        presenter_view: false,
      },
      show: {
        running: false,
        mode: 'Show',
        slide: 0,
        position: 0,
        count: 0,
        click: 0,
        clicks: 0,
        screen: 'Slide',
        tool: 'Arrow',
        typed: '',
        next: null,
        on_click: true,
        kind: 'Speaker',
        version: 0,
      },
    },
    ...patch,
  };
}

/** A client that answers State with `state` and records the commands. */
function fakeClient(state: () => DeckState) {
  const executed: unknown[] = [];
  const listeners = new Set<(e: Event) => void>();
  const client = {
    execute: vi.fn(async (_doc: DocId | null, c: unknown) => {
      executed.push(c);
      return { changed: true };
    }),
    query: vi.fn(async (_doc: DocId | null, q: unknown) => {
      if (q === 'Status') return { Status: { zoom: 1, view: 'SlideNormal' } };
      return { Deck: { State: state() } };
    }),
    subscribe: (l: (e: Event) => void) => {
      listeners.add(l);
      return () => listeners.delete(l);
    },
  } as unknown as KilnClient;
  return { client, executed };
}

describe('stage keys', () => {
  it('edit text: caret motions, deletes, paragraphs, tabs and Esc', () => {
    expect(deckKeyAction(key('ArrowLeft'), 'text')).toEqual({
      kind: 'core',
      id: 'deck.text.move',
      args: { motion: 'Backward', extend: false },
    });
    expect(caretMotion(key('ArrowRight', { ctrlKey: true }))).toBe('WordForward');
    expect(caretMotion(key('End', { ctrlKey: true }))).toBe('DocumentEnd');
    expect(deckKeyAction(key('Backspace', { ctrlKey: true }), 'text')).toEqual({
      kind: 'core',
      id: 'deck.text.delete',
      args: { forward: false, word: true },
    });
    expect(deckKeyAction(key('Enter', { shiftKey: true }), 'text')?.kind).toBe('core');
    expect(deckKeyAction(key('Enter'), 'text')).toEqual({ kind: 'core', id: 'deck.text.new_paragraph' });
    expect(deckKeyAction(key('Tab', { shiftKey: true }), 'text')).toEqual({
      kind: 'core',
      id: 'deck.text.tab',
      args: { shift: true },
    });
    expect(deckKeyAction(key('a'), 'text')).toEqual({ kind: 'native' });
    // Ctrl+B is the window's.
    expect(deckKeyAction(key('b', { ctrlKey: true }), 'text')).toBeNull();
  });

  it('objects: arrows nudge, Shift+arrows are the keymap’s, Delete deletes, Tab cycles', () => {
    expect(deckKeyAction(key('ArrowUp'), 'object')).toEqual({
      kind: 'core',
      id: 'deck.drawing.nudge',
      args: { direction: 'up', fine: false },
    });
    expect(deckKeyAction(key('ArrowUp', { shiftKey: true }), 'object')).toBeNull();
    expect(deckKeyAction(key('Delete'), 'object')).toEqual({ kind: 'core', id: 'deck.edit.delete' });
    expect(deckKeyAction(key('Tab'), 'object')?.kind).toBe('core');
    expect(deckKeyAction(key('x'), 'object')).toEqual({ kind: 'native' });
  });

  it('the slide: Page Down and arrows go through the slides', () => {
    expect(deckKeyAction(key('PageDown'), 'slide')).toEqual({ kind: 'core', id: 'deck.slides.next_slide' });
    expect(deckKeyAction(key('ArrowUp'), 'slide')).toEqual({
      kind: 'core',
      id: 'deck.slides.previous_slide',
    });
    expect(deckKeyAction(key('x'), 'slide')).toBeNull();
  });

  it('the rail: arrows select, Delete deletes, Enter adds a slide', () => {
    // Ctrl+Up and Ctrl+Down move the slides, Shift to the ends (PowerPoint).
    expect(railKeyAction(key('ArrowUp', { ctrlKey: true }), 2, 4)).toEqual({
      id: 'deck.slides.move',
      args: { to: 1 },
    });
    expect(railKeyAction(key('ArrowDown', { ctrlKey: true }), 1, 4)).toEqual({
      id: 'deck.slides.move',
      args: { to: 3 },
    });
    expect(railKeyAction(key('ArrowDown', { ctrlKey: true, shiftKey: true }), 1, 4)?.args).toEqual({ to: 4 });
    expect(railKeyAction(key('ArrowDown'), 1, 3)).toEqual({
      id: 'deck.slides.select',
      args: { index: 2, extend: false, toggle: false },
    });
    expect(railKeyAction(key('ArrowDown', { shiftKey: true }), 2, 3)?.args).toEqual({
      index: 2,
      extend: true,
      toggle: false,
    });
    expect(railKeyAction(key('Delete'), 0, 3)).toEqual({ id: 'deck.slides.delete' });
    expect(railKeyAction(key('Enter'), 0, 3)).toEqual({ id: 'deck.slides.new_slide' });
  });

  it('scopes follow the state', () => {
    expect(scopesOf(deckState(), false)).toEqual([]);
    expect(scopesOf(deckState({ selection: [5] }), false)).toEqual(['object']);
    expect(scopesOf(deckState({ selection: [5], editing: true }), false)).toEqual(['text-edit']);
    expect(scopesOf(deckState(), true)).toEqual(['rail']);
  });
});

describe('stage geometry', () => {
  it('fits and centres the slide with a margin', () => {
    const f = fitView({ w: 1280, h: 720 }, { w: 1000, h: 800 });
    expect(f.zoom).toBeCloseTo((1000 - 2 * FIT_MARGIN) / 1280);
    // Centred: the slide's middle is the view's middle.
    expect(-f.x * f.zoom + (1280 * f.zoom) / 2).toBeCloseTo(500);
    expect(-f.y * f.zoom + (720 * f.zoom) / 2).toBeCloseTo(400);
  });

  it('keeps a zoomed-in view on the slide', () => {
    const c = clampScroll(5000, -5000, 2, { w: 1280, h: 720 }, { w: 800, h: 600 });
    expect(c.x).toBeCloseTo(1280 + FIT_MARGIN / 2 - 400);
    expect(c.y).toBeCloseTo(-FIT_MARGIN / 2);
  });

  it('maps the core’s cursors', () => {
    expect(cssCursor('Move')).toBe('move');
    expect(cssCursor({ Resize: 1 })).toBe('ns-resize');
    expect(cssCursor({ Resize: 7 })).toBe('ew-resize');
    expect(cssCursor('Rotate')).toBe('grab');
  });
});

describe('rail', () => {
  it('lists sections and the slides of expanded ones', () => {
    const s = deckState({
      sections: [
        { name: 'Intro', start: 0, count: 1 },
        { name: 'Money', start: 1, count: 2 },
      ],
    });
    const rows = railRows(s.slides, s.sections, new Set([1]));
    expect(rows.map((r) => (r.kind === 'section' ? `§${r.section.name}` : r.index))).toEqual([
      '§Intro',
      0,
      '§Money',
    ]);
  });

  it('drops before the first row whose middle is below the pointer', () => {
    const mids = [
      { index: 0, mid: 50 },
      { index: 1, mid: 150 },
      { index: 2, mid: 250 },
    ];
    expect(dropIndex(10, mids, 3)).toBe(0);
    expect(dropIndex(160, mids, 3)).toBe(2);
    expect(dropIndex(400, mids, 3)).toBe(3);
  });

  it('asks for stale thumbnails in runs', () => {
    expect(runs([5, 1, 2, 3, 9])).toEqual([
      [1, 3],
      [5, 1],
      [9, 1],
    ]);
    expect(runs([0, 1, 2, 3], 2)).toEqual([
      [0, 2],
      [2, 2],
    ]);
  });
});

describe('ribbon', () => {
  it('fills New Slide and Layout with the presentation’s layouts', () => {
    const spec = deckRibbonWithLayouts(deckRibbon, deckState().layouts);
    const refs = ribbonCommandRefs(spec).filter((r) => r.command === 'deck.slides.layout');
    expect(refs.length).toBeGreaterThanOrEqual(2);
    const home = spec.tabs.find((t) => t.id === 'home');
    const slides = home?.groups.find((g) => g.id === 'slides');
    const layout = slides?.items.find((i) => 'id' in i && i.id === 'deck.layout');
    expect(layout && 'menu' in layout ? layout.menu.length : 0).toBe(2);
  });

  it('wires Shape Format and Picture Format to Deck commands', () => {
    const ids = new Set(ribbonCommandRefs(deckRibbon).map((r) => r.command));
    for (const id of [
      'deck.arrange.order',
      'deck.arrange.align',
      'deck.drawing.shape_fill',
      'deck.drawing.shape_outline',
      'deck.drawing.quick_styles',
      'deck.design.themes',
      'deck.design.slide_size',
      'ui.deck.format_background',
      'deck.stage.arm_insert',
      'deck.picture.reset',
      'ui.deck.crop',
    ])
      expect(ids.has(id), id).toBe(true);
  });

  it('builds a context menu for every kind', () => {
    for (const kind of ['rail', 'section', 'shape', 'text', 'slide'] as const)
      expect(deckMenuEntries(kind, deckState(), 0).length).toBeGreaterThan(3);
  });
});

describe('model and chrome', () => {
  it('stores the core’s state and puts it in the registry context', async () => {
    let st = deckState({ selection: [7] });
    const { client } = fakeClient(() => st);
    const registry = createCommandRegistry({ app: 'deck' });
    const model = new DeckEditorModel({ client, doc: 1 as DocId, registry });
    await model.fetch();
    expect(model.state.deck?.slides.length).toBe(3);
    expect(registry.context.scopes).toEqual(['object']);
    st = deckState({ editing: true, selection: [7] });
    await model.fetch();
    expect(registry.context.scopes).toEqual(['text-edit']);
    model.dispose();
  });

  it('sends stage commands straight to the core', async () => {
    const { client, executed } = fakeClient(() => deckState());
    const registry = createCommandRegistry({ app: 'deck' });
    const model = new DeckEditorModel({ client, doc: 1 as DocId, registry });
    await model.core('deck.drawing.nudge', { direction: 'left', fine: true });
    await model.core('deck.stage.pointer_down', { x: 10, y: 20, shift: true, clicks: 2 });
    expect(executed).toEqual([
      { Deck: { Nudge: { direction: 'Left', fine: true } } },
      { Deck: { PointerDown: { x: 10, y: 20, shift: true, ctrl: false, alt: false, clicks: 2 } } },
    ]);
    expect(model.sent).toEqual(['deck.drawing.nudge', 'deck.stage.pointer_down']);
    model.dispose();
  });

  it('registers the chrome commands: Text Box arms a text box, dialogs open', async () => {
    const { client, executed } = fakeClient(() => deckState());
    const registry = createCommandRegistry({ app: 'deck' });
    const model = new DeckEditorModel({ client, doc: 1 as DocId, registry });
    const ui = registerDeckUi(model);
    await registry.execute('suite.insert.text_box');
    expect(executed.at(-1)).toEqual({ Deck: { ArmInsert: { preset: 'textBox' } } });
    await registry.execute('suite.insert.table');
    expect(model.state.dialog?.id).toBe('insertTable');
    await registry.execute('ui.deck.format_background');
    expect(model.state.dialog?.id).toBe('formatBackground');
    await registry.execute('suite.format.copy_formatting');
    expect(executed.at(-1)).toEqual({ Deck: 'CopyFormat' });
    ui.dispose();
    model.dispose();
  });

  it('lists the motion and feature panes’ chrome ids in the Deck gate’s chrome list', () => {
    const missing = [...DECK_MOTION_CHROME, ...DECK_FEATURE_CHROME].filter(
      (id) => !id.startsWith('ui.') && !DECK_CHROME_COMMANDS.includes(id),
    );
    expect(missing).toEqual([]);
  });

  it('implements every id of the Deck gate’s chrome list', () => {
    const { client } = fakeClient(() => deckState());
    const registry = createCommandRegistry({ app: 'deck' });
    const model = new DeckEditorModel({ client, doc: 1 as DocId, registry });
    const ui = registerDeckUi(model);
    const missing = DECK_CHROME_COMMANDS.filter((id) => registry.meta(id)?.pending !== false);
    expect(missing).toEqual([]);
    ui.dispose();
    model.dispose();
  });

  it('Crop toggles the crop handles; Icons and Code Block open their dialogs', async () => {
    let st = deckState({ selection: [7] });
    const { client, executed } = fakeClient(() => st);
    const registry = createCommandRegistry({ app: 'deck' });
    const model = new DeckEditorModel({ client, doc: 1 as DocId, registry });
    const ui = registerDeckUi(model);
    await model.fetch();
    await registry.execute('suite.picture.crop');
    expect(executed.at(-1)).toEqual({ Deck: { CropMode: { on: true } } });
    st = deckState({ selection: [7], cropping: true });
    await model.fetch();
    await registry.execute('suite.picture.crop');
    expect(executed.at(-1)).toEqual({ Deck: { CropMode: { on: false } } });
    await registry.execute('suite.insert.icons');
    expect(model.state.dialog?.id).toBe('icons');
    await registry.execute('suite.insert.code_block');
    expect(model.state.dialog?.id).toBe('codeBlock');
    ui.dispose();
    model.dispose();
  });

  it('the notes pane sends the text after a pause', async () => {
    vi.useFakeTimers();
    const { client, executed } = fakeClient(() => deckState());
    const registry = createCommandRegistry({ app: 'deck' });
    const model = new DeckEditorModel({ client, doc: 1 as DocId, registry });
    await act(async () => {
      await model.fetch();
    });
    render(<NotesPane model={model} />);
    const box = screen.getByRole('textbox', { name: 'Speaker notes' });
    expect(box).toHaveValue('Start with the number people remember.');
    fireEvent.change(box, { target: { value: 'Say why.' } });
    await act(async () => {
      vi.advanceTimersByTime(500);
    });
    expect(executed.at(-1)).toEqual({ Deck: { SetNotes: { text: 'Say why.' } } });
    model.dispose();
  });

  it('the status bar says which slide of how many', async () => {
    const { client } = fakeClient(() => deckState({ current: 1 }));
    const registry = createCommandRegistry({ app: 'deck' });
    const model = new DeckEditorModel({ client, doc: 1 as DocId, registry });
    await act(async () => {
      await model.fetch();
    });
    render(<DeckStatusBar model={model} readOnly={false} />);
    expect(screen.getByText('Slide 2 of 3 (hidden)')).toBeInTheDocument();
    expect(screen.getByTestId('deck-notes-toggle')).toHaveAttribute('aria-pressed', 'true');
    model.dispose();
  });
});
