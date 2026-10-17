/**
 * Every Write row of the shortcut table (PLAN.md §1.7) in the Write editor (WP-64): with the caret
 * in a document, each key press reaches the handler it names — the Write model's command log
 * (`Query::Write(CommandLog)`) for the editor's commands, the session's log for the suite's
 * (save, undo, zoom), the window's registry for the chrome's (dialogs, panes, Key Tips). Commands
 * that later work packages implement (Format Painter, spelling, Track Changes, comments, Print)
 * resolve to their reserved ids, which release builds hide; the test lists them.
 */
import { expect, type Page, test } from '@playwright/test';
import type { KeyBinding } from '../../packages/commands/src/keymap.ts';
import { commonKeymap, writeKeymap } from '../../packages/commands/src/keymaps/index.ts';
import { SHORTCUT_TABLE } from '../../packages/commands/src/keymaps/table.ts';

interface Handle {
  resolved: { key: string; command: string; args?: unknown }[];
  executed: { id: string; status: string }[];
  log(): { id: string; outcome: string }[];
  registry: {
    getState(id: string): { enabled: boolean; pending: boolean; visible: boolean; exists: boolean };
  };
}

/** Commands that leave the window (dialogs of the operating system, other documents). */
const LEAVES_WINDOW = new Set([
  'suite.file.new',
  'suite.file.open',
  'suite.file.close',
  'suite.file.save',
  'suite.file.save_as',
  'suite.file.print',
]);

const NONE = /^[—–-]?$/;

function writeRows(): { row: string; bindings: KeyBinding[] }[] {
  return SHORTCUT_TABLE.filter((r) => !NONE.test(r.write.trim())).map((r) => ({
    row: r.shortcut,
    bindings: [...commonKeymap, ...writeKeymap].filter((b) => b.row === r.shortcut && !b.scope),
  }));
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

async function writeLog(page: Page): Promise<string[]> {
  return page.evaluate(async () => {
    const w = (
      window as unknown as {
        __kilnWrite: { doc: number; client: { query(d: number, q: unknown): Promise<unknown> } };
      }
    ).__kilnWrite;
    const r = (await w.client.query(w.doc, { Write: 'CommandLog' })) as {
      Write: { CommandLog: { id: string }[] };
    };
    return r.Write.CommandLog.map((e) => e.id);
  });
}

/** Puts the caret in the document with its first word selected (so Copy, Bold… apply). */
async function selectFirstWord(page: Page): Promise<void> {
  await page.evaluate(async () => {
    const w = (
      window as unknown as {
        __kilnWrite: {
          doc: number;
          client: { execute(d: number, c: unknown): Promise<unknown> };
          controller: { focus(): void };
        };
      }
    ).__kilnWrite;
    await w.client.execute(w.doc, { Write: { MoveCaret: { motion: 'DocumentStart', extend: false } } });
    await w.client.execute(w.doc, { Write: { MoveCaret: { motion: 'WordForward', extend: true } } });
    w.controller.focus();
  });
}

async function closeOverlays(page: Page) {
  for (let i = 0; i < 3; i++) {
    const open = await page
      .locator('[role="dialog"], [data-testid="keytips"], [role="menu"], [role="listbox"]')
      .count();
    if (open === 0) break;
    await page.keyboard.press('Escape');
  }
  await page.evaluate(() =>
    (window as unknown as { __kilnWrite: { controller: { focus(): void } } }).__kilnWrite.controller.focus(),
  );
}

test.describe('Write shortcuts', () => {
  test('every Write row of §1.7 reaches its handler from the document', async ({ page }) => {
    test.setTimeout(240_000);
    await page.goto('/#/write');
    await page.waitForFunction(
      () =>
        ((window as unknown as { __kilnWrite?: { controller?: unknown } }).__kilnWrite?.controller ??
          null) !== null,
      null,
      { timeout: 45_000 },
    );
    await page.getByTestId('kiln-input-proxy').focus();
    await page.keyboard.type('Kiln fires clay at cone ten.');
    await page.keyboard.press('Enter');
    await page.keyboard.type('Second paragraph of the test.');

    const rows = writeRows();
    expect(rows.length).toBeGreaterThan(40);
    const reached: string[] = [];
    const reserved: string[] = [];
    for (const { row, bindings } of rows) {
      expect(bindings.length, `"${row}" has no Write binding`).toBeGreaterThan(0);
      for (const b of bindings) {
        const where = `${b.key} (${row}) → ${b.command}`;
        if (LEAVES_WINDOW.has(b.command)) {
          const init = eventInit(b.key);
          const r = await page.evaluate(
            (i) =>
              (
                window as unknown as {
                  __kiln: { keymap: { resolve(e: unknown, c: unknown): { command: string } | undefined } };
                }
              ).__kiln.keymap.resolve(i, { app: 'write', scopes: [] }),
            init,
          );
          expect(r?.command, where).toBe(b.command);
          continue;
        }
        await selectFirstWord(page);
        const state = await page.evaluate(
          (id) => (window as unknown as { __kiln: Handle }).__kiln.registry.getState(id),
          b.command,
        );
        const before = {
          write: (await writeLog(page)).length,
          session: await page.evaluate(() => (window as unknown as { __kiln: Handle }).__kiln.log().length),
          executed: await page.evaluate(
            () => (window as unknown as { __kiln: Handle }).__kiln.executed.length,
          ),
          resolved: await page.evaluate(
            () => (window as unknown as { __kiln: Handle }).__kiln.resolved.length,
          ),
        };
        const init = eventInit(b.key);
        // The key goes to the focused input proxy of the document, as a real key would.
        await page.evaluate((i) => {
          const target = document.activeElement ?? document.body;
          const opts = { ...i, bubbles: true, cancelable: true, composed: true };
          target.dispatchEvent(new KeyboardEvent('keydown', opts));
          if (i.key === 'Alt') target.dispatchEvent(new KeyboardEvent('keyup', { ...opts, altKey: false }));
        }, init);
        await page.waitForTimeout(60);
        const resolved = await page.evaluate(
          (n) => (window as unknown as { __kiln: Handle }).__kiln.resolved.slice(n),
          before.resolved,
        );
        if (!resolved.some((r) => r.command === b.command)) {
          // The surface handles the editing keys itself (arrows, Ctrl+A, Ctrl+Z/Y): the core's
          // log shows them; anything else must resolve to the keymap's id.
          expect(
            ['write.caret.move', 'write.caret.select_all', 'suite.edit.undo', 'suite.edit.redo'],
            where,
          ).toContain(b.command);
        }
        if (state.pending) {
          reserved.push(b.command);
          await closeOverlays(page);
          continue;
        }
        if (b.command === 'suite.edit.undo' || b.command === 'suite.edit.redo') {
          // From the document, the surface sends them to the session's history directly (their
          // effect is the next test).
          reached.push(b.command);
          continue;
        }
        await expect
          .poll(
            async () => {
              const w = (await writeLog(page)).slice(before.write);
              const s = await page.evaluate(
                (n) =>
                  (window as unknown as { __kiln: Handle }).__kiln
                    .log()
                    .slice(n)
                    .map((e) => e.id),
                before.session,
              );
              const ui = await page.evaluate(
                (n) =>
                  (window as unknown as { __kiln: Handle }).__kiln.executed
                    .slice(n)
                    .filter((e) => e.status === 'ran')
                    .map((e) => e.id),
                before.executed,
              );
              return w.includes(b.command) || s.includes(b.command) || ui.includes(b.command);
            },
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
    // Only the commands that belong to later work packages are reservations.
    for (const id of reserved) {
      expect(
        [
          'suite.format.copy_formatting',
          'suite.format.paste_formatting',
          'suite.review.thesaurus',
          'suite.review.spelling',
          'write.review.track_changes',
          'suite.review.new_comment',
        ],
        `${id} should be implemented by WP-64`,
      ).toContain(id);
    }
  });

  test('Undo and Redo from the document undo typing and redo it', async ({ page }) => {
    await page.goto('/#/write');
    await page.getByTestId('kiln-input-proxy').waitFor();
    await page.getByTestId('kiln-input-proxy').focus();
    await page.keyboard.type('fire');
    const text = () =>
      page.evaluate(async () => {
        const w = (
          window as unknown as {
            __kilnWrite: { doc: number; client: { query(d: number, q: unknown): Promise<unknown> } };
          }
        ).__kilnWrite;
        const r = (await w.client.query(w.doc, 'Paragraphs')) as { Paragraphs: { text: string }[] };
        return r.Paragraphs.map((p) => p.text).join('\n');
      });
    await expect.poll(text).toBe('fire');
    await page.keyboard.press('Control+z');
    await expect.poll(text).toBe('');
    await page.keyboard.press('Control+y');
    await expect.poll(text).toBe('fire');
    await page.keyboard.press('Control+z');
    await page.keyboard.press('Control+Shift+z');
    await expect.poll(text).toBe('fire');
  });
});
