/**
 * Kiln Write's features on the UI side (WP-66): the helpers (threads, times, avatars, dragging,
 * goals, the spelling menu), the panes against a fake core, and the ribbon ids mapped to core
 * commands — Format Painter's double-click, comments, arrange, Paste Markdown.
 */
import { connectCore, createCommandRegistry } from '@kiln/commands';
import type {
  Command,
  CommentInfo,
  ExecResult,
  FeatureState,
  FormatState,
  KilnClient,
  ObjectInfo,
  Query,
  QueryResult,
  RevisionsInfo,
  SpellingHit,
} from '@kiln/protocol';
import { act, fireEvent, render, screen } from '@testing-library/react';
import { afterEach, describe, expect, it, vi } from 'vitest';
import { WriteEditorModel } from '../model.ts';
import { CommentsPane } from '../panes/CommentsPane.tsx';
import { ReviewingPane } from '../panes/ReviewingPane.tsx';
import { SelectionPane } from '../panes/SelectionPane.tsx';
import { threadsOf } from './Comments.tsx';
import { registerWriteFeatures } from './commands.ts';
import { draggedRect } from './Layer.tsx';
import { spellingEntries } from './menu.ts';
import { avatarText, featuresOf, whenLabel } from './model.ts';
import { featureStatusItems, goalPercent } from './status.tsx';

const FEATURES: FeatureState = {
  tracking: false,
  author: 'Ana',
  markup: 'All',
  show_comments: true,
  show_insertions: true,
  show_formatting: true,
  revisions: 2,
  comments: 1,
  open_comments: 1,
  comment: null,
  painter: null,
  copied_format: false,
  focus: false,
  word_goal: 0,
  autoformat: { markdown: true, smart_quotes: true, dashes: true, ellipsis: true, fractions: true },
  spelling: true,
  dictionary: true,
  misspellings: 0,
  hyphenation: false,
  object: null,
  in_note: false,
  has_toc: false,
};

const comment = (over: Partial<CommentInfo>): CommentInfo => ({
  id: 10,
  parent: null,
  author: 'Rana Osei',
  initials: 'RO',
  date: '2026-10-17T08:00:00Z',
  text: 'Can we keep the tenmoku pieces off shelf 1?',
  resolved: false,
  anchor_text: 'label anything glazed',
  color: 'B01E2D',
  page: 1,
  balloon: { x: 900, y: 200, width: 240, height: 80 },
  anchor: { x: 300, y: 205 },
  current: false,
  ...over,
});

const COMMENTS: CommentInfo[] = [
  comment({}),
  comment({
    id: 11,
    parent: 10,
    author: 'Jun K',
    initials: 'JK',
    text: 'Agreed.',
    balloon: null,
    anchor: null,
  }),
  comment({ id: 12, author: 'Jun K', initials: 'JK', text: 'Cone 9 shelf for porcelain.', resolved: true }),
];

const REVISIONS: RevisionsInfo = {
  entries: [
    {
      target: { kind: 'Inserted', node: 5, start: 3, end: 8 },
      author: 'Ana',
      date: null,
      text: 'warm ',
      color: '1F5FA8',
      story: 'Main Document',
      current: true,
    },
    {
      target: { kind: 'Deleted', node: 6, start: 0, end: 4 },
      author: 'Bo',
      date: null,
      text: 'cold',
      color: 'B01E2D',
      story: 'Footnote',
      current: false,
    },
  ],
  reviewers: [
    { name: 'Ana', color: '1F5FA8', changes: 1 },
    { name: 'Bo', color: 'B01E2D', changes: 1 },
  ],
  insertions: 1,
  deletions: 1,
  formatting: 0,
  comments: 3,
};

const OBJECTS: ObjectInfo[] = [
  {
    id: 40,
    name: 'Kiln photo',
    kind: 'Picture',
    wrap: 'Square',
    para: 5,
    offset: 0,
    rect: { x: 100, y: 100, width: 200, height: 100 },
    width: 150,
    height: 75,
    rotation: 0,
    z: 2,
    alt_text: null,
    decorative: false,
    selected: true,
  },
  {
    id: 41,
    name: 'Text Box 1',
    kind: 'TextBox',
    wrap: 'InLine',
    para: 6,
    offset: 3,
    rect: null,
    width: 100,
    height: 40,
    rotation: 0,
    z: 0,
    alt_text: null,
    decorative: false,
    selected: false,
  },
];

class FakeCore {
  readonly executed: Command[] = [];
  features: FeatureState = FEATURES;

  client(): KilnClient {
    return {
      bridge: 'wasm',
      subscribe: () => () => undefined,
      query: async (_doc: number | null, q: Query): Promise<QueryResult> => {
        if (typeof q === 'object' && 'Write' in q) {
          const w = q.Write;
          if (w === 'Format')
            return {
              Write: { Format: { features: this.features, collapsed: true } as unknown as FormatState },
            };
          if (w === 'Comments') return { Write: { Comments: COMMENTS } };
          if (w === 'Revisions') return { Write: { Revisions: REVISIONS } };
          if (w === 'Objects') return { Write: { Objects: OBJECTS } };
          if (w === 'Rulers') return { Write: { Rulers: null } };
        }
        return { Text: '' };
      },
      execute: async (_doc: number | null, c: Command): Promise<ExecResult> => {
        this.executed.push(c);
        return { outcome: 'Applied', summary: null, value: 'None' };
      },
    } as unknown as KilnClient;
  }
}

async function settle(): Promise<void> {
  for (let i = 0; i < 3; i++) {
    await act(async () => {
      await new Promise((r) => setTimeout(r, 40));
    });
  }
}

const cleanups: (() => void)[] = [];
afterEach(() => {
  for (const c of cleanups.splice(0)) c();
});

function setup() {
  const core = new FakeCore();
  const registry = createCommandRegistry({ app: 'write' });
  const client = core.client();
  // The core's commands, as the app frame connects them (available on an editable document).
  const connection = connectCore(registry, {
    client,
    document: { doc: 1, read_only: false, can_undo: false, can_redo: false } as never,
  });
  const model = new WriteEditorModel({ client, doc: 1, registry });
  const features = registerWriteFeatures(model);
  cleanups.push(() => {
    features.dispose();
    model.dispose();
    connection.dispose();
  });
  return { core, registry, model };
}

describe('feature helpers', () => {
  it('groups comments into threads and labels times and avatars', () => {
    const threads = threadsOf(COMMENTS);
    expect(threads.map((t) => [t.root.id, t.replies.map((r) => r.id)])).toEqual([
      [10, [11]],
      [12, []],
    ]);
    const now = Date.parse('2026-10-17T10:00:00Z');
    expect(whenLabel('2026-10-17T08:00:00Z', now)).toBe('2 h');
    expect(whenLabel('2026-10-17T09:59:30Z', now)).toBe('Now');
    expect(whenLabel('2026-10-16T09:00:00Z', now)).toBe('Yesterday');
    expect(whenLabel(null, now)).toBe('');
    expect(avatarText('Rana Osei')).toBe('RO');
    expect(avatarText('zoë', 'zå')).toBe('ZÅ');
    expect(avatarText('')).toBe('?');
  });

  it('drags and resizes object rectangles, keeping the aspect from the corners', () => {
    const r = { x: 10, y: 10, width: 200, height: 100 };
    expect(draggedRect(r, 'move', 5, -5, false)).toEqual({ x: 15, y: 5, width: 200, height: 100 });
    expect(draggedRect(r, 'e', 50, 0, false)).toEqual({ x: 10, y: 10, width: 250, height: 100 });
    const corner = draggedRect(r, 'se', 100, 0, true);
    expect(corner.width / corner.height).toBeCloseTo(2);
    const nw = draggedRect(r, 'nw', -20, -20, true);
    expect(nw.x + nw.width).toBeCloseTo(210);
    expect(nw.y + nw.height).toBeCloseTo(110);
    expect(draggedRect(r, 'w', 500, 0, false).width).toBe(8);
  });

  it('measures goals and builds the spelling block of the context menu', () => {
    expect(goalPercent(1284, 1500)).toBe(86);
    expect(goalPercent(3000, 1500)).toBe(100);
    expect(goalPercent(10, 0)).toBe(0);
    const hit: SpellingHit = {
      word: 'teh',
      start: { block: 3, offset: 5 },
      len: 3,
      suggestions: ['the', 'ten', 'tech'],
      context: 'Fire teh kiln',
      context_offset: 5,
    };
    const entries = spellingEntries(hit);
    const commands = entries
      .filter((e) => 'command' in e)
      .map((e) => ('command' in e ? [e.command, e.label] : []));
    expect(commands.slice(0, 3)).toEqual([
      ['write.spell.replace', 'the'],
      ['write.spell.replace', 'ten'],
      ['write.spell.replace', 'tech'],
    ]);
    expect(commands.map((c) => c[1])).toContain('Add to Dictionary');
    expect(spellingEntries(null)).toEqual([]);
  });
});

describe('panes', () => {
  it('lists comment threads with replies, resolves and replies through the core', async () => {
    const { core, model } = setup();
    featuresOf(model).set({ pane: 'comments' });
    await settle();
    render(<CommentsPane model={model} />);
    await settle();
    expect(screen.getByText('1 open · 1 resolved')).toBeTruthy();
    const cards = screen.getAllByTestId('write-comment-card');
    expect(cards).toHaveLength(2);
    expect(screen.getAllByText('label anything glazed').length).toBeGreaterThan(0);
    expect(screen.getByText('Agreed.')).toBeTruthy();
    fireEvent.click(screen.getAllByRole('button', { name: /Resolve/ })[0] as HTMLElement);
    await settle();
    expect(JSON.stringify(core.executed)).toContain('"ResolveComment":{"id":10,"resolved":true}');
    fireEvent.click(screen.getAllByRole('button', { name: 'Reply' })[0] as HTMLElement);
    const box = screen.getByRole('textbox', { name: 'Reply' });
    fireEvent.change(box, { target: { value: 'On it.' } });
    fireEvent.keyDown(box, { key: 'Enter', ctrlKey: true });
    await settle();
    expect(JSON.stringify(core.executed)).toContain('"ReplyComment":{"parent":10,"text":"On it."}');
    fireEvent.click(screen.getByRole('checkbox', { name: 'Show resolved' }));
    expect(screen.getAllByTestId('write-comment-card')).toHaveLength(1);
  });

  it('shows the revisions with reviewers and accepts or rejects one', async () => {
    const { core, model } = setup();
    featuresOf(model).set({ pane: 'review' });
    await settle();
    render(<ReviewingPane model={model} />);
    await settle();
    expect(screen.getAllByTestId('write-revision')).toHaveLength(2);
    expect(screen.getByText('Footnote')).toBeTruthy();
    fireEvent.click(screen.getByRole('button', { name: 'Reject deleted by Bo' }));
    await settle();
    expect(JSON.stringify(core.executed)).toContain(
      '"ResolveRevision":{"target":{"kind":"Deleted","node":6,"start":0,"end":4},"accept":false}',
    );
  });

  it('lists objects front first, selects and renames them', async () => {
    const { core, model } = setup();
    featuresOf(model).set({ pane: 'selection' });
    await settle();
    render(<SelectionPane model={model} />);
    await settle();
    const items = screen.getAllByTestId('write-selection-item');
    expect(items).toHaveLength(2);
    fireEvent.click(screen.getByText('Text Box 1'));
    await settle();
    expect(JSON.stringify(core.executed)).toContain('"SelectObject":{"id":41}');
    fireEvent.doubleClick(screen.getByText('Kiln photo'));
    const input = screen.getByRole('textbox', { name: 'Object name' });
    fireEvent.change(input, { target: { value: 'Kiln at night' } });
    fireEvent.keyDown(input, { key: 'Enter' });
    await settle();
    expect(JSON.stringify(core.executed)).toContain('"name":"Kiln at night"');
    expect(screen.getByRole('button', { name: 'Bring Text Box 1 forward' }).hasAttribute('disabled')).toBe(
      true,
    );
  });
});

describe('ribbon ids', () => {
  it('maps comments, arrange, insert and clipboard ids to core commands', async () => {
    const { core, registry, model } = setup();
    await settle();
    const ran = async (id: string, args?: unknown) => {
      core.executed.length = 0;
      await act(async () => {
        await registry.execute(id, args);
      });
      return JSON.stringify(core.executed);
    };
    expect(await ran('suite.review.next_comment')).toContain(
      '"GoTo":{"target":{"Comment":{"backward":false}}}',
    );
    expect(await ran('suite.arrange.bring_forward')).toContain('"ObjectOrder":{"order":"Forward"}');
    expect(await ran('suite.insert.text_box')).toContain('"InsertTextBox":{"text":null}');
    expect(await ran('suite.format.copy_formatting')).toContain('"CopyFormat"');
    expect(await ran('write.insert.paste_markdown_table', { text: '| a |\n|---|\n| 1 |' })).toContain(
      '"PasteMarkdown"',
    );
    // New Comment opens the composer (nothing reaches the core until it is posted).
    expect(await ran('suite.review.new_comment')).toBe('[]');
    expect(featuresOf(model).state.composer).toEqual({ parent: null });
    // Panes toggle.
    await ran('write.review.reviewing_pane');
    expect(featuresOf(model).state.pane).toBe('review');
    expect(registry.getState('write.review.reviewing_pane').checked).toBe(true);
    await ran('write.review.reviewing_pane');
    expect(featuresOf(model).state.pane).toBe(null);
  });

  it('keeps Format Painter on after a double-click', async () => {
    const { core, registry, model } = setup();
    await settle();
    await act(async () => {
      await registry.execute('suite.edit.format_painter');
    });
    expect(JSON.stringify(core.executed)).toContain('"FormatPainter":{"sticky":false}');
    // The core reports the painter armed; a second click right away makes it sticky.
    core.features = { ...FEATURES, painter: false };
    model.refresh({ content: true });
    await settle();
    core.executed.length = 0;
    await act(async () => {
      await registry.execute('suite.edit.format_painter');
    });
    expect(JSON.stringify(core.executed)).toContain('"FormatPainter":{"sticky":true}');
  });

  it('lists the status items of the features', () => {
    const { model } = setup();
    const none = featureStatusItems(model, null, null);
    expect(none.left.map((i) => i.id)).toEqual(['track']);
    expect(none.right.map((i) => i.id)).toEqual(['focus']);
    const some = featureStatusItems(
      model,
      { ...FEATURES, word_goal: 1500, tracking: true, misspellings: 2 },
      1284,
    );
    expect(some.left.map((i) => i.id)).toEqual(['goal', 'proofing', 'track']);
    expect(some.left.find((i) => i.id === 'track')?.content).toBe('Track Changes: On');
    vi.spyOn(model, 'run').mockResolvedValue(true);
    some.left.find((i) => i.id === 'track')?.onClick?.();
    expect(model.run).toHaveBeenCalledWith('write.review.track_changes');
  });
});
