import type { GridState, KilnClient } from '@kiln/protocol';
import { describe, expect, it } from 'vitest';
import { type GridElements, GridSurfaceController, panes, tileX } from './controller';
import type { GridEditorModel } from './model';

/** The controller's private entry points the echo handling runs through. */
interface EntryInternals {
  onInput(): void;
  onState(s: GridState, asked?: number): void;
  editing: boolean;
}

function harness() {
  const sent: { id: string; args: unknown }[] = [];
  const model = {
    asked: 0,
    state: { status: { zoom: 1 } },
    core: (id: string, args: unknown) => {
      sent.push({ id, args });
      return Promise.resolve(true);
    },
    refresh: () => undefined,
  } as unknown as GridEditorModel;
  const el = <K extends keyof HTMLElementTagNameMap>(tag: K) => document.createElement(tag);
  const els: GridElements = {
    root: el('div'),
    canvas: el('canvas'),
    scroller: el('div'),
    spacer: el('div'),
    proxy: el('textarea'),
    a11y: el('div'),
  };
  const client = {} as KilnClient;
  const c = new GridSurfaceController(client, model, els, { theme: 'light', blink: false });
  const inner = c as unknown as EntryInternals & { requestFrame(): void };
  inner.requestFrame = () => undefined;
  const layout = { content_w: 1000, content_h: 1000, header_w: 40, header_h: 20 };
  const state = (text: string) =>
    ({
      layout,
      mode: 'Edit',
      sheets: [{ name: 'Sheet1' }],
      active_sheet: 0,
      editor: {
        text,
        start: text.length,
        end: text.length,
        formula_bar: false,
        rect: { x: 0, y: 0, width: 64, height: 20 },
      },
    }) as unknown as GridState;
  const type = (text: string) => {
    els.proxy.value = text;
    inner.onInput();
  };
  return { inner, proxy: els.proxy, state, type, sent, model, controller: c };
}

describe('input while the core is busy', () => {
  /** A controller whose core answers each hit test only when told to. */
  function busy() {
    const h = harness();
    const answers: (() => void)[] = [];
    const model = h.model as unknown as Record<string, unknown>;
    model.fetch = () => Promise.resolve(null);
    model.hitHooks = [];
    model.hitTest = (x: number, y: number) =>
      new Promise((resolve) =>
        answers.push(() => {
          resolve({ Cell: { row: y, col: x } });
        }),
      );
    const c = h.controller as unknown as {
      press(e: PointerEvent): void;
      onKeyDown(e: KeyboardEvent): void;
      point(e: PointerEvent): { x: number; y: number };
      capture(id: number): void;
    };
    c.point = (e) => ({ x: e.clientX, y: e.clientY });
    c.capture = () => undefined;
    const press = (x: number, y: number) =>
      c.press({
        button: 0,
        clientX: x,
        clientY: y,
        pointerId: 1,
        preventDefault() {},
      } as unknown as PointerEvent);
    const key = (k: string) => {
      const e = new KeyboardEvent('keydown', { key: k, cancelable: true });
      c.onKeyDown(e);
      return e;
    };
    const settle = async () => {
      for (let i = 0; i < 400; i += 1) {
        answers.shift()?.();
        await Promise.resolve();
      }
    };
    return { ...h, press, key, settle, answers };
  }

  it('keeps each key with the cell clicked before it, however late the core answers', async () => {
    const { press, key, sent, settle } = busy();
    press(1, 10);
    key('1');
    key('2');
    key('Enter');
    press(1, 20);
    key('3');
    key('4');
    key('Enter');
    expect(sent.filter((s) => s.id !== 'grid.view.theme')).toEqual([]);
    await settle();
    const seen = sent.filter((s) => s.id !== 'grid.view.theme').map((s) => s.id);
    expect(seen).toEqual([
      'grid.nav.select_cell',
      'grid.editing.begin',
      'grid.editing.set_text',
      'grid.editing.commit',
      'grid.nav.select_cell',
      'grid.editing.begin',
      'grid.editing.set_text',
      'grid.editing.commit',
    ]);
    const typed = sent
      .filter((s) => /begin|set_text/.test(s.id))
      .map((s) => (s.args as { text: string }).text);
    expect(typed).toEqual(['1', '12', '3', '34']);
  });

  it('holds what is typed after the Name Box until the Go To has landed', async () => {
    const { key, sent, controller } = busy();
    let land: () => void = () => undefined;
    const goTo = new Promise<void>((resolve) => {
      land = () => {
        sent.push({ id: 'grid.nav.go_to', args: null });
        resolve();
      };
    });
    (controller as unknown as { holdInput(p: Promise<unknown>): void }).holdInput(goTo);
    for (const k of ['=', 'S', 'U', 'M']) key(k);
    expect(sent.filter((s) => s.id !== 'grid.view.theme')).toEqual([]);
    land();
    for (let i = 0; i < 40; i += 1) await Promise.resolve();
    const seen = sent.filter((s) => s.id !== 'grid.view.theme');
    expect(seen[0]?.id).toBe('grid.nav.go_to');
    const typed = seen
      .filter((s) => /begin|set_text/.test(s.id))
      .map((s) => (s.args as { text: string }).text);
    expect(typed.at(-1)).toBe('=SUM');
  });

  it('keeps keys after a click on the selection border behind the selection it makes', async () => {
    const h = busy();
    const model = h.model as unknown as Record<string, unknown>;
    model.hitTest = () => Promise.resolve({ SelectionBorder: { row: 9, col: 4 } });
    const c = h.controller as unknown as {
      release(e: PointerEvent): void;
      cellUnder(p: { x: number; y: number }): Promise<{ row: number; col: number } | null>;
    };
    c.cellUnder = async () => ({ row: 9, col: 4 });
    const ev = {
      button: 0,
      clientX: 1,
      clientY: 1,
      pointerId: 1,
      preventDefault() {},
    } as unknown as PointerEvent;
    h.press(1, 1);
    c.release(ev);
    h.key('7');
    await h.settle();
    const seen = h.sent.filter((s) => s.id !== 'grid.view.theme').map((s) => s.id);
    expect(seen.indexOf('grid.nav.select_cell')).toBeGreaterThanOrEqual(0);
    expect(seen.indexOf('grid.nav.select_cell')).toBeLessThan(seen.indexOf('grid.editing.begin'));
  });

  it('never lets keys into the proxy while a press is on its way', async () => {
    const { press, key, proxy, settle } = busy();
    press(1, 10);
    expect(key('7').defaultPrevented).toBe(true);
    expect(proxy.value).toBe('');
    await settle();
    expect(proxy.value).toBe('7');
  });

  it('starts anew after an entry that has just ended', async () => {
    const { press, key, proxy, sent, settle } = busy();
    proxy.value = 'committed';
    press(1, 10);
    key('h');
    await settle();
    expect(sent.find((s) => s.id === 'grid.editing.begin')?.args).toEqual({ text: 'h' });
  });
});

describe('the viewport', () => {
  it('asks for the zoom the core reports, not the copy from the last state', () => {
    const { model, controller } = harness();
    const status = (model as unknown as { state: { status: { zoom: number } } }).state.status;
    status.zoom = 1.5;
    const vp = (controller as unknown as { viewport(): { zoom: number } }).viewport();
    expect(vp.zoom).toBe(1.5);
  });
});

describe('the entry’s echoes', () => {
  it('a repeat of an older state does not undo the keys typed since', () => {
    const { inner, proxy, state, type } = harness();
    type('=B');
    type('=B$');
    inner.onState(state('=B'));
    // The same version again (a pushed frame and a fetch's answer): the proxy has newer typing.
    inner.onState(state('=B'));
    expect(proxy.value).toBe('=B$');
    type('=B$1');
    inner.onState(state('=B$'));
    inner.onState(state('=B$1'));
    expect(proxy.value).toBe('=B$1');
  });

  it('a text the core sets on its own still wins, even one typed before', () => {
    const { inner, proxy, state, type } = harness();
    type('=A');
    type('=A1');
    inner.onState(state('=A'));
    inner.onState(state('=A1'));
    // F4 cycles back to a text typed earlier: the core's change must show.
    (inner as unknown as { applyKey(a: unknown): void }).applyKey({
      kind: 'core',
      id: 'grid.editing.cycle_ref',
    });
    inner.onState(state('=$A$1'));
    expect(proxy.value).toBe('=$A$1');
    (inner as unknown as { applyKey(a: unknown): void }).applyKey({
      kind: 'core',
      id: 'grid.editing.cycle_ref',
    });
    inner.onState(state('=A1'));
    expect(proxy.value).toBe('=A1');
  });
});

describe('a right-to-left sheet', () => {
  const layout = {
    header_w: 44,
    header_h: 24,
    freeze_rows: 1,
    freeze_cols: 2,
    frozen_h: 20,
    frozen_w: 128,
    content_w: 4096,
    content_h: 4096,
    sheet_zoom: 1,
    split: false,
    rtl: true,
  };

  it('shows the panes mirrored: the row headers and the frozen columns at the right', () => {
    const all = panes(layout, 1, 800, 600, 400, 300);
    const rows = all.filter((p) => p.kind === 'rowHeader');
    expect(rows.every((p) => p.rect.x === 800 - 44 && p.x === 0)).toBe(true);
    const corner = all.find((p) => p.kind === 'corner');
    expect(corner?.rect).toEqual({ x: 756, y: 0, w: 44, h: 24 });
    const [frozen, scrolled] = all.filter((p) => p.kind === 'cells' && p.y === 0);
    // The frozen columns (sheet 0–128) just left of the headers, in the mirrored sheet.
    expect(frozen?.rect).toEqual({ x: 800 - 44 - 128, y: 24, w: 128, h: 20 });
    expect(frozen?.x).toBe(-128);
    // The scrolled columns from sheet x 400 on, to their left.
    expect(scrolled?.rect.x).toBe(0);
    expect(scrolled?.x).toBe(-(400 + (800 - 44 - 128)));
  });

  it('places tiles at their mirrored sheet x', () => {
    expect(tileX({ x: 256, w: 256 }, true)).toBe(-512);
    expect(tileX({ x: 256, w: 256 }, false)).toBe(256);
  });
});
