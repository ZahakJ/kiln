/**
 * Every Grid row of the shortcut table (PLAN.md §1.7) in the Grid editor (WP-71): with the sheet
 * focused and a range selected, each key press reaches the handler it names — the session's log for
 * the core's commands, the window's registry for the chrome's (dialogs, the find bar, Key Tips), the
 * surface's own path for the keys it sends straight to the core. Commands that later work packages
 * implement (AutoFilter, Flash Fill, tables, outline, comments, links, spelling) resolve to their
 * reserved ids, which release builds hide; the test lists them. The editing keys of the cell
 * (Enter, Tab, Esc, F2, F4, Alt+Enter, Ctrl+Enter) are checked by their effect.
 */
import { expect, type Page, test } from '@playwright/test';
import type { KeyBinding } from '../../packages/commands/src/keymap.ts';
import { commonKeymap, gridKeymap } from '../../packages/commands/src/keymaps/index.ts';
import { SHORTCUT_TABLE } from '../../packages/commands/src/keymaps/table.ts';

interface Handle {
  resolved: { key: string; command: string; args?: unknown }[];
  executed: { id: string; status: string }[];
  log(): { id: string; outcome: string }[];
  registry: {
    getState(id: string): { enabled: boolean; pending: boolean; visible: boolean; exists: boolean };
  };
}
interface GridHandle {
  doc: number;
  sent: string[];
  client: {
    query(doc: number, q: unknown): Promise<Record<string, unknown>>;
    execute(doc: number, c: unknown): Promise<unknown>;
  };
  surface: { focus(): void } | null;
  set(patch: unknown): void;
}
type W = { __kiln: Handle; __kilnGrid: GridHandle; __kilnGridSurface?: { stats: { frames: number } } | null };

/** Commands that leave the window (dialogs of the operating system, other documents). */
const LEAVES_WINDOW = new Set([
  'suite.file.new',
  'suite.file.open',
  'suite.file.close',
  'suite.file.save',
  'suite.file.save_as',
  'suite.file.print',
]);

/** Reservations: commands of later work packages (hidden in release builds). */
const LATER = [
  'grid.data.toggle_filter',
  'grid.editing.flash_fill',
  'grid.styles.format_as_table',
  'grid.data.show_outline_symbols',
  'grid.data.group_rows',
  'grid.data.ungroup_rows',
  'grid.review.new_note',
  'suite.review.new_comment',
  'suite.review.spelling',
  'suite.review.thesaurus',
  'suite.insert.link',
  'suite.edit.repeat',
  'suite.edit.replace',
  'suite.format.copy_formatting',
  'suite.format.paste_formatting',
];

const NONE = /^[—–-]?$/;

function gridRows(): { row: string; bindings: KeyBinding[] }[] {
  const all = [...commonKeymap, ...gridKeymap];
  return SHORTCUT_TABLE.filter((r) => !NONE.test(r.grid.trim())).map((r) => {
    // Grid's own binding wins over the common one for the same key.
    const sheet = all
      .filter((b) => b.row === r.shortcut && !b.scope)
      .filter(
        (b, _, list) =>
          !(
            commonKeymap.includes(b) && list.some((o) => o !== b && o.key === b.key && gridKeymap.includes(o))
          ),
      );
    // Rows that only exist while a cell is being edited (Ctrl+Shift+Enter, Alt+Enter, Esc) are
    // pressed during an entry.
    const editing = gridKeymap.filter((b) => b.row === r.shortcut && b.scope === 'cell-edit');
    return { row: r.shortcut, bindings: sheet.length > 0 ? sheet : editing };
  });
}

/** Keymap key (`Mod+Shift+ArrowUp`) → the KeyboardEvent a Windows/Linux keyboard sends. */
function eventInit(key: string): { key: string; ctrlKey: boolean; shiftKey: boolean; altKey: boolean } {
  const parts = key.endsWith('++') ? [...key.slice(0, -2).split('+'), '+'] : key.split('+');
  const main = parts.pop() ?? '';
  const mods = new Set(parts.map((p) => p.toLowerCase()));
  const named: Record<string, string> = { Esc: 'Escape', Space: ' ' };
  return {
    key: named[main] ?? (main.length === 1 ? main.toLowerCase() : main),
    ctrlKey: mods.has('mod') || mods.has('ctrl'),
    shiftKey: mods.has('shift'),
    altKey: mods.has('alt'),
  };
}

/** Back to Ready on Sheet1 with A1:B2 selected and the sheet focused. */
async function reset(page: Page): Promise<void> {
  await page.evaluate(async () => {
    const g = (window as unknown as W).__kilnGrid;
    await g.client.execute(g.doc, { Grid: 'CancelEdit' });
    await g.client.execute(g.doc, { Grid: { ActivateSheet: { index: 0 } } });
    await g.client.execute(g.doc, {
      Grid: { SelectRange: { range: { r0: 0, c0: 0, r1: 1, c1: 1 }, add: false } },
    });
    g.set({ find: { open: false, text: '', matchCase: false, wholeCell: false, inFormulas: false } });
    g.surface?.focus();
  });
}

async function closeOverlays(page: Page) {
  for (let i = 0; i < 3; i++) {
    const open = await page
      .locator('[role="dialog"], [data-testid="keytips"], [role="menu"], [data-testid="grid-find"]')
      .count();
    if (open === 0) break;
    await page.keyboard.press('Escape');
  }
}

async function gridState(page: Page): Promise<{ mode: string; name_box: string; formula: string }> {
  return page.evaluate(async () => {
    const g = (window as unknown as W).__kilnGrid;
    const r = (await g.client.query(g.doc, { Grid: 'State' })) as {
      Grid: { State: { mode: string; name_box: string; formula: string } };
    };
    return r.Grid.State;
  });
}

async function editorText(page: Page): Promise<string | undefined> {
  return page.evaluate(async () => {
    const g = (window as unknown as W).__kilnGrid;
    const r = (await g.client.query(g.doc, { Grid: 'State' })) as {
      Grid: { State: { editor: { text: string } | null } };
    };
    return r.Grid.State.editor?.text;
  });
}

test.describe('Grid shortcuts', () => {
  test('every Grid row of §1.7 reaches its handler from the sheet', async ({ page }) => {
    test.setTimeout(300_000);
    await page.goto('/#/grid?blink=0');
    await page.waitForFunction(
      () => ((window as unknown as W).__kilnGridSurface?.stats.frames ?? 0) > 0,
      null,
      {
        timeout: 60_000,
      },
    );
    await page.locator('[data-kiln-grid-proxy]').focus();
    for (const row of [
      ['Kiln', '4', '7'],
      ['Clay', '6', '9'],
      ['Glaze', '8', '11'],
    ]) {
      for (const [i, v] of row.entries()) {
        await page.keyboard.type(v);
        await page.keyboard.press(i === row.length - 1 ? 'Enter' : 'Tab');
      }
    }

    const rows = gridRows();
    expect(rows.length).toBeGreaterThan(40);
    const reached: string[] = [];
    const reserved: string[] = [];
    for (const { row, bindings } of rows) {
      expect(bindings.length, `"${row}" has no Grid binding`).toBeGreaterThan(0);
      for (const b of bindings) {
        const where = `${b.key} (${row}) → ${b.command}`;
        const init = eventInit(b.key);
        if (LEAVES_WINDOW.has(b.command)) {
          const r = await page.evaluate(
            (i) =>
              (
                window as unknown as {
                  __kiln: { keymap: { resolve(e: unknown, c: unknown): { command: string } | undefined } };
                }
              ).__kiln.keymap.resolve(i, { app: 'grid', scopes: [] }),
            init,
          );
          expect(r?.command, where).toBe(b.command);
          continue;
        }
        await reset(page);
        if (b.scope === 'cell-edit') {
          // An entry in A1 ("x"), as typed.
          await page.keyboard.type('x');
          await expect.poll(async () => (await gridState(page)).mode).toBe('Enter');
        }
        const state = await page.evaluate(
          (id) => (window as unknown as W).__kiln.registry.getState(id),
          b.command,
        );
        const before = await page.evaluate(() => {
          const w = window as unknown as W;
          return {
            session: w.__kiln.log().length,
            executed: w.__kiln.executed.length,
            resolved: w.__kiln.resolved.length,
            sent: w.__kilnGrid.sent.length,
          };
        });
        // The key goes to the focused sheet, as a real key would.
        await page.evaluate((i) => {
          const target = document.activeElement ?? document.body;
          const opts = { ...i, bubbles: true, cancelable: true, composed: true };
          target.dispatchEvent(new KeyboardEvent('keydown', opts));
          if (i.key === 'Alt') target.dispatchEvent(new KeyboardEvent('keyup', { ...opts, altKey: false }));
        }, init);
        await page.waitForTimeout(80);
        if (b.command === 'grid.editing.new_line_in_cell') {
          // The line break goes into the entry the proxy holds, then to the core with the text.
          await expect
            .poll(async () => (await editorText(page)) ?? '', { message: `${where} did not break the line` })
            .toBe('x\n');
          reached.push(b.command);
          await page.keyboard.press('Escape');
          continue;
        }
        if (state.pending) {
          reserved.push(b.command);
          await closeOverlays(page);
          continue;
        }
        await expect
          .poll(
            async () =>
              page.evaluate(
                ([id, n]) => {
                  const w = window as unknown as W;
                  const s = w.__kiln
                    .log()
                    .slice(n.session)
                    .map((e) => e.id);
                  const ui = w.__kiln.executed
                    .slice(n.executed)
                    .filter((e) => e.status === 'ran')
                    .map((e) => e.id);
                  const sent = w.__kilnGrid.sent.slice(n.sent);
                  const resolved = w.__kiln.resolved.slice(n.resolved).map((r) => r.command);
                  // Undo and Redo with nothing to undo or redo are disabled: the key resolving to
                  // them is what this row checks (their effect is tested in grid.spec.ts).
                  const history = id === 'suite.edit.undo' || id === 'suite.edit.redo';
                  return (
                    s.includes(id) ||
                    ui.includes(id) ||
                    sent.includes(id) ||
                    (history && resolved.includes(id))
                  );
                },
                [b.command, before] as const,
              ),
            { message: `${where} did not reach its handler`, timeout: 5_000 },
          )
          .toBe(true);
        reached.push(b.command);
        await closeOverlays(page);
      }
    }
    test.info().annotations.push({ type: 'reached', description: [...new Set(reached)].join(', ') });
    test.info().annotations.push({
      type: 'reserved (later work packages)',
      description: [...new Set(reserved)].join(', '),
    });
    expect(new Set(reached).size).toBeGreaterThanOrEqual(45);
    for (const id of reserved) expect(LATER, `${id} should be implemented by WP-71`).toContain(id);
  });

  test('the cell editing keys: Enter, Tab, Esc, F2, F4, Alt+Enter, Ctrl+Enter', async ({ page }) => {
    await page.goto('/#/grid?blink=0');
    await page.waitForFunction(
      () => ((window as unknown as W).__kilnGridSurface?.stats.frames ?? 0) > 0,
      null,
      {
        timeout: 60_000,
      },
    );
    const proxy = page.locator('[data-kiln-grid-proxy]');
    await proxy.focus();
    // Tab commits and moves right; Enter commits and moves down.
    await page.keyboard.type('a');
    await page.keyboard.press('Tab');
    await expect.poll(async () => (await gridState(page)).name_box).toBe('B1');
    await page.keyboard.type('b');
    await page.keyboard.press('Enter');
    await expect.poll(async () => (await gridState(page)).name_box).toBe('A2');
    // Esc cancels the entry.
    await page.keyboard.type('nope');
    await page.keyboard.press('Escape');
    await expect.poll(async () => (await gridState(page)).formula).toBe('');
    // F2 edits in place; Alt+Enter breaks the line.
    await page.keyboard.type('one');
    await page.keyboard.press('Enter');
    await page.keyboard.press('ArrowUp');
    await page.keyboard.press('F2');
    await expect.poll(async () => (await gridState(page)).mode).toBe('Edit');
    await page.keyboard.press('Alt+Enter');
    await page.keyboard.type('two');
    await page.keyboard.press('Enter');
    await page.keyboard.press('ArrowUp');
    await expect.poll(async () => (await gridState(page)).formula).toBe('one\ntwo');
    // F4 cycles the reference at the caret while typing a formula.
    await page.keyboard.press('ArrowDown');
    await page.keyboard.type('=A1');
    await page.keyboard.press('F4');
    await expect
      .poll(async () =>
        page.evaluate(async () => {
          const g = (window as unknown as W).__kilnGrid;
          const r = (await g.client.query(g.doc, { Grid: 'State' })) as {
            Grid: { State: { editor: { text: string } | null } };
          };
          return r.Grid.State.editor?.text;
        }),
      )
      .toBe('=$A$1');
    await page.keyboard.press('Escape');
    // Ctrl+Enter fills the selection with the entry.
    await page.evaluate(async () => {
      const g = (window as unknown as W).__kilnGrid;
      await g.client.execute(g.doc, {
        Grid: { SelectRange: { range: { r0: 4, c0: 0, r1: 6, c1: 1 }, add: false } },
      });
      g.surface?.focus();
    });
    await page.keyboard.type('x');
    await page.keyboard.press('Control+Enter');
    await expect
      .poll(async () =>
        page.evaluate(async () => {
          const g = (window as unknown as W).__kilnGrid;
          const r = (await g.client.query(g.doc, 'SelectedText')) as { Text?: string };
          return (r.Text ?? '').replace(/\r\n/g, '\n').trim();
        }),
      )
      .toBe('x\tx\nx\tx\nx\tx');
  });
});
