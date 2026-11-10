import type { A11yTree, KilnClient } from '@kiln/protocol';
import { act, render, screen } from '@testing-library/react';
import { describe, expect, it, vi } from 'vitest';
import { menuKeyTips } from '../keytips/KeyTipsLayer.tsx';
import { announce, lastAnnouncement, prepareAnnouncer } from './announce.ts';
import { cycleRegions, regionsOf } from './regions.ts';
import { selectionKey, textAnnouncement } from './selection.ts';
import { AccessibleTextView, offsetIn, pointIn } from './TextView.tsx';

describe('announce', () => {
  it('writes to one polite region, and a repeated message is read again', () => {
    prepareAnnouncer();
    expect(document.querySelectorAll('[data-kiln-announcer]')).toHaveLength(2);
    announce('Edit mode');
    expect(lastAnnouncement()).toBe('Edit mode');
    const region = document.querySelector('[data-kiln-announcer="polite"]');
    const first = region?.textContent;
    vi.useFakeTimers();
    vi.advanceTimersByTime(200);
    announce('Edit mode');
    vi.useRealTimers();
    expect(region?.textContent).not.toBe(first);
    expect(lastAnnouncement()).toBe('Edit mode');
    announce('Stop', 'assertive');
    expect(lastAnnouncement('assertive')).toBe('Stop');
    announce('   ');
    expect(lastAnnouncement()).toBe('Edit mode');
  });
});

describe('F6 regions', () => {
  it('go document, panes, status bar, title bar, ribbon, and Shift+F6 back', () => {
    document.body.innerHTML = `
      <div id="frame">
        <header class="kiln-titlebar"><button>Save</button></header>
        <div class="kiln-ribbon"><button role="tab" aria-selected="false">File</button><button role="tab" aria-selected="true">Home</button></div>
        <div class="kiln-appframe__body">
          <main class="kiln-appframe__canvas"><div tabindex="0" id="caret">doc</div></main>
          <aside><button>Pane button</button></aside>
        </div>
        <section class="kiln-statusbar"><button>Words</button></section>
      </div>`;
    const root = document.getElementById('frame') as HTMLElement;
    // jsdom has no layout: everything counts as shown.
    for (const el of root.querySelectorAll<HTMLElement>('*'))
      el.getBoundingClientRect = () => ({ width: 10, height: 10 }) as DOMRect;
    expect(regionsOf(root).map((r) => r.id)).toEqual([
      'document',
      'panes',
      'statusbar',
      'titlebar',
      'ribbon',
    ]);
    const onDoc = vi.fn(() => (document.getElementById('caret') as HTMLElement).focus());
    window.addEventListener('kiln:return-to-document', onDoc);
    const order: string[] = [];
    for (let i = 0; i < 5; i++) {
      cycleRegions(root, false);
      order.push(document.activeElement?.textContent ?? '');
    }
    expect(order).toEqual(['doc', 'Pane button', 'Words', 'Save', 'Home']);
    cycleRegions(root, true);
    expect(document.activeElement?.textContent).toBe('Save');
    window.removeEventListener('kiln:return-to-document', onDoc);
  });
});

describe('selection announcements', () => {
  it('say selected text shortened and change with the selection', () => {
    const one = {
      Cells: {
        ranges: [{ start: { sheet: 0, row: 1, col: 1 }, end: { sheet: 0, row: 1, col: 1 } }],
        active: { sheet: 0, row: 1, col: 1 },
      },
    };

    const range = {
      Cells: {
        ranges: [{ start: { sheet: 0, row: 0, col: 0 }, end: { sheet: 0, row: 3, col: 2 } }],
        active: { sheet: 0, row: 0, col: 0 },
      },
    };

    expect(textAnnouncement('  Hello\n world ')).toBe('Hello world, selected');
    expect(textAnnouncement('x'.repeat(400))).toMatch(/…, selected$/);
    expect(selectionKey('None')).toBe('none');
    expect(selectionKey(range)).not.toBe(selectionKey(one));
  });
});

describe('text view offsets', () => {
  it('count model characters: labels none, an object one, astral characters one', () => {
    const p = document.createElement('p');
    p.innerHTML = '<span data-kiln-label="">1. </span>ab<span data-kiln-object="">Picture</span>c😀d';
    const text = [...p.childNodes];
    expect(offsetIn(p, text[1] as Node, 1)).toBe(1);
    expect(offsetIn(p, text[3] as Node, 0)).toBe(3);
    // After the emoji (two UTF-16 units, one character).
    expect(offsetIn(p, text[3] as Node, 3)).toBe(5);
    const at = pointIn(p, 5);
    expect(at.node).toBe(text[3]);
    expect(at.offset).toBe(3);
    expect(pointIn(p, 2)).toEqual({ node: text[1], offset: 2 });
    // Past the object: the next text.
    expect(pointIn(p, 3)).toEqual({ node: text[3], offset: 0 });
  });
});

describe('Accessible text view', () => {
  function client(tree: A11yTree, execute = vi.fn(async () => ({ outcome: 'Applied' }))): KilnClient {
    return {
      bridge: 'wasm',
      query: vi.fn(async () => ({ A11y: tree })),
      execute,
      subscribe: () => () => undefined,
    } as unknown as KilnClient;
  }
  const node = (id: number, role: string, extra: object = {}) => ({
    id,
    role,
    name: null,
    text: null,
    bounds: null,
    level: null,
    children: [],
    objects: null,
    ...extra,
  });

  it('renders Write as headings, lists and tables', async () => {
    const tree = {
      nodes: [
        node(1, 'Document', { name: 'Report', children: [2, 3, 6] }),
        node(2, 'Heading', { level: 1, text: 'Kiln', objects: [] }),
        node(3 + 2 ** 51, 'List', { level: 1, children: [3] }),
        node(3, 'ListItem', { name: '1.', text: 'First', objects: [] }),
        node(6, 'Table', { children: [7] }),
        node(7, 'Row', { children: [8] }),
        node(8, 'ColumnHeader', { children: [9] }),
        node(9, 'Paragraph', { text: 'Head', objects: [] }),
      ],
      active: 2,
      announce: null,
      caret: { block: 2, offset: 0 },
    } as unknown as A11yTree;
    tree.nodes[0].children = [2, 3 + 2 ** 51, 6];
    render(<AccessibleTextView client={client(tree)} doc={1} app="write" onClose={() => undefined} />);
    await act(async () => {
      await new Promise((r) => setTimeout(r, 40));
    });
    expect(screen.getByRole('heading', { name: 'Kiln' })).toBeTruthy();
    expect(screen.getByRole('listitem').textContent).toBe('1. First');
    expect(screen.getByRole('columnheader').textContent).toBe('Head');
    expect(screen.getByRole('article', { name: 'Report' })).toBeTruthy();
  });

  it('renders Grid as a table with row and column headers', async () => {
    const C = 2 ** 40;
    const tree = {
      nodes: [
        node(2 ** 37, 'Grid', { name: 'Book – Sheet1', children: [2 ** 38, 2 ** 39] }),
        node(2 ** 38, 'Row', { children: [2 ** 38 + 1] }),
        node(2 ** 38 + 1, 'ColumnHeader', { name: 'A' }),
        node(2 ** 39, 'Row', { name: '1', children: [2 ** 36, C] }),
        node(2 ** 36, 'RowHeader', { name: '1' }),
        node(C, 'GridCell', { name: 'A1', text: '42' }),
      ],
      active: C,
      announce: 'A1, 42',
      caret: null,
    } as unknown as A11yTree;
    render(<AccessibleTextView client={client(tree)} doc={1} app="grid" onClose={() => undefined} />);
    await act(async () => {
      await new Promise((r) => setTimeout(r, 40));
    });
    expect(screen.getByRole('columnheader', { name: 'A' })).toBeTruthy();
    expect(screen.getByRole('rowheader', { name: '1' })).toBeTruthy();
    expect(screen.getByRole('cell', { name: 'A1, 42' }).getAttribute('tabindex')).toBe('0');
  });
});

describe('Key Tips in menus', () => {
  it('give every enabled item of a menu a unique letter from its name', () => {
    document.body.innerHTML = `
      <div role="menu">
        <div role="menuitem"><span class="kiln-menu__label">Bottom Border</span></div>
        <div role="menuitem"><span class="kiln-menu__label">Top Border</span></div>
        <div role="menuitem" data-disabled=""><span class="kiln-menu__label">Left Border</span></div>
        <div role="menuitem" data-keytip="N"><span class="kiln-menu__label">No Border</span></div>
      </div>`;
    const tips = menuKeyTips(document.querySelector('[role="menu"]') as HTMLElement);
    expect(tips.map((t) => t.label)).toEqual(['Bottom Border', 'Top Border', 'No Border']);
    const letters = tips.map((t) => t.tip);
    expect(new Set(letters).size).toBe(3);
    expect(letters[2]).toBe('N');
  });
});
