/**
 * Every Deck row of the shortcut table in the Deck editor: with a shape
 * selected on the stage (or the thumbnail rail focused, for the rail's rows), each key press
 * reaches the handler it names — the session's log for the core's commands, the window's
 * registry for the chrome's (dialogs, Key Tips, the palette), the stage's own path for the keys it
 * sends straight to the core. Commands that later work implements (comments, spelling,
 * printing) resolve to their reserved ids, which release builds hide; the test
 * lists them. The stage's own keys (typing, Enter, Tab, Esc, arrows, Delete) are checked by their
 * effect.
 */
import { expect, type Page, test } from '@playwright/test';
import type { KeyBinding } from '../../packages/commands/src/keymap.ts';
import { commonKeymap, deckKeymap } from '../../packages/commands/src/keymaps/index.ts';
import { SHORTCUT_TABLE } from '../../packages/commands/src/keymaps/table.ts';

interface Handle {
  resolved: { key: string; command: string; args?: unknown }[];
  executed: { id: string; status: string }[];
  log(): { id: string; outcome: string; seq: number }[];
  registry: {
    getState(id: string): { enabled: boolean; pending: boolean; visible: boolean; exists: boolean };
  };
}
interface DeckState {
  shapes: { id: number; name: string }[];
  selection: number[];
  editing: boolean;
  slides: { title: string }[];
  current: number;
  selection_box: { x: number; y: number; width: number; height: number } | null;
  format: { subscript: boolean; superscript: boolean; align: string; bold: boolean } | null;
}
interface DeckHandle {
  doc: number;
  sent: string[];
  client: {
    query(doc: number, q: unknown): Promise<Record<string, unknown>>;
    execute(doc: number, c: unknown): Promise<unknown>;
  };
  stage: { focus(): void } | null;
  set(patch: unknown): void;
  fetch(): Promise<unknown>;
}
type W = { __kiln: Handle; __kilnDeck: DeckHandle; __kilnDeckStage?: { stats: { frames: number } } | null };

/** Commands that leave the window (dialogs of the operating system, other documents). */
const LEAVES_WINDOW = new Set([
  'suite.file.new',
  'suite.file.open',
  'suite.file.close',
  'suite.file.save',
  'suite.file.save_as',
  'suite.file.print',
  // The slide show takes the window (deck-show.spec.ts plays it).
  'deck.show.from_beginning',
  'deck.show.from_current',
]);

/** Reservations: commands of later work (hidden in release builds). */
const LATER = ['suite.review.new_comment', 'suite.review.spelling', 'suite.review.thesaurus'];

/** Keys the stage handles itself, sending the core command on the right. */
const STAGE_OWN: Readonly<Record<string, string>> = {
  'ui.edit.cancel': 'deck.select.escape',
};

const NONE = /^[—–-]?$/;

function deckRows(): { row: string; bindings: KeyBinding[] }[] {
  const all = [...commonKeymap, ...deckKeymap];
  return SHORTCUT_TABLE.filter((r) => !NONE.test(r.deck.trim())).map((r) => {
    // Deck's own binding wins over the common one for the same key.
    const plain = all
      .filter((b) => b.row === r.shortcut && !b.scope)
      .filter(
        (b, _, list) =>
          !(
            commonKeymap.includes(b) && list.some((o) => o !== b && o.key === b.key && deckKeymap.includes(o))
          ),
      );
    // Rows that exist only with an object selected or in the rail are pressed there.
    const scoped = deckKeymap.filter(
      (b) => b.row === r.shortcut && (b.scope === 'object' || b.scope === 'rail'),
    );
    return { row: r.shortcut, bindings: [...plain, ...scoped] };
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

async function deckState(page: Page): Promise<DeckState> {
  return page.evaluate(async () => {
    const d = (window as unknown as W).__kilnDeck;
    const r = (await d.client.query(d.doc, { Deck: 'State' })) as { Deck: { State: DeckState } };
    return r.Deck.State;
  });
}

/** Slide 1 with its title placeholder selected (not editing) and the stage focused, or the rail. */
async function reset(page: Page, where: 'object' | 'rail'): Promise<void> {
  await page.evaluate(async (w) => {
    const d = (window as unknown as W).__kilnDeck;
    await d.client.execute(d.doc, { Deck: 'Escape' });
    await d.client.execute(d.doc, { Deck: 'Escape' });
    await d.client.execute(d.doc, { Deck: { SelectSlide: { index: 0, extend: false, toggle: false } } });
    const r = (await d.client.query(d.doc, { Deck: 'State' })) as { Deck: { State: DeckState } };
    // Earlier rows (Cut, Delete) may have taken the title away: any shape will do, else a new one.
    const shape = r.Deck.State.shapes.find((s) => /^Title/.test(s.name)) ?? r.Deck.State.shapes[0];
    if (w === 'object') {
      if (shape) await d.client.execute(d.doc, { Deck: { SelectShapes: { ids: [shape.id] } } });
      else await d.client.execute(d.doc, { Deck: { InsertShape: { preset: 'rect', rect: null } } });
    }
    await d.fetch();
    if (w === 'rail') (document.querySelector('[data-testid="deck-rail"]') as HTMLElement | null)?.focus();
    else d.stage?.focus();
  }, where);
  await page.waitForTimeout(40);
}

async function closeOverlays(page: Page) {
  for (let i = 0; i < 3; i++) {
    const open = await page.locator('[role="dialog"], [data-testid="keytips"], [role="menu"]').count();
    if (open === 0) break;
    await page.keyboard.press('Escape');
  }
}

async function openDeck(page: Page) {
  await page.goto('/#/deck?blink=0');
  await page.waitForFunction(() => ((window as unknown as W).__kilnDeckStage?.stats.frames ?? 0) > 0, null, {
    timeout: 60_000,
  });
}

test.describe('Deck shortcuts', () => {
  test('every Deck row of the shortcut table reaches its handler from the stage', async ({ page }) => {
    test.setTimeout(300_000);
    await openDeck(page);
    // Two slides, the first with a title.
    await page.evaluate(async () => {
      const d = (window as unknown as W).__kilnDeck;
      const r = (await d.client.query(d.doc, { Deck: 'State' })) as { Deck: { State: DeckState } };
      const title = r.Deck.State.shapes.find((s) => /^Title/.test(s.name));
      if (title) {
        await d.client.execute(d.doc, { Deck: { SelectShapes: { ids: [title.id] } } });
        await d.client.execute(d.doc, { Deck: { InsertText: { text: 'Open Studio Weekend' } } });
      }
      await d.client.execute(d.doc, { Deck: { NewSlide: { layout: null } } });
    });

    const rows = deckRows();
    expect(rows.length).toBeGreaterThan(25);
    const reached: string[] = [];
    const reserved: string[] = [];
    for (const { row, bindings } of rows) {
      expect(bindings.length, `"${row}" has no Deck binding`).toBeGreaterThan(0);
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
              ).__kiln.keymap.resolve(i, { app: 'deck', scopes: [] }),
            init,
          );
          expect(r?.command, where).toBe(b.command);
          continue;
        }
        await reset(page, b.scope === 'rail' ? 'rail' : 'object');
        const state = await page.evaluate(
          (id) => (window as unknown as W).__kiln.registry.getState(id),
          b.command,
        );
        const before = await page.evaluate(() => {
          const w = window as unknown as W;
          return {
            session: w.__kiln.log().at(-1)?.seq ?? 0,
            executed: w.__kiln.executed.length,
            resolved: w.__kiln.resolved.length,
            sent: w.__kilnDeck.sent.length,
          };
        });
        await page.evaluate((i) => {
          const target = document.activeElement ?? document.body;
          const opts = { ...i, bubbles: true, cancelable: true, composed: true };
          target.dispatchEvent(new KeyboardEvent('keydown', opts));
          if (i.key === 'Alt') target.dispatchEvent(new KeyboardEvent('keyup', { ...opts, altKey: false }));
        }, init);
        await page.waitForTimeout(80);
        if (state.pending) {
          reserved.push(b.command);
          await closeOverlays(page);
          continue;
        }
        const own = STAGE_OWN[b.command];
        const ctx = await page.evaluate(() => {
          const w = window as unknown as W & { __kiln: { registry: { context: Record<string, unknown> } } };
          return JSON.stringify({
            scopes: w.__kiln.registry.context.scopes,
            active: document.activeElement?.className,
          });
        });
        await expect
          .poll(
            async () =>
              page.evaluate(
                ([id, alt, n]) => {
                  const w = window as unknown as W;
                  const s = w.__kiln
                    .log()
                    .filter((e) => e.seq > n.session)
                    .map((e) => e.id);
                  const ui = w.__kiln.executed
                    .slice(n.executed)
                    .filter((e) => e.status === 'ran')
                    .map((e) => e.id);
                  const sent = w.__kilnDeck.sent.slice(n.sent);
                  const resolved = w.__kiln.resolved.slice(n.resolved).map((r) => r.command);
                  // Undo and Redo with nothing to undo or redo are disabled: the key resolving to
                  // them is what this row checks (their effect is tested in deck.spec.ts).
                  const history =
                    id === 'suite.edit.undo' || id === 'suite.edit.redo' || id === 'suite.edit.repeat';
                  return (
                    s.includes(id) ||
                    ui.includes(id) ||
                    sent.includes(id) ||
                    (alt !== null && (sent.includes(alt) || s.includes(alt))) ||
                    (history && resolved.includes(id))
                  );
                },
                [b.command, own ?? null, before] as const,
              ),
            { message: `${where} did not reach its handler (${ctx})`, timeout: 5_000 },
          )
          .toBe(true);
        reached.push(b.command);
        await closeOverlays(page);
      }
    }
    test.info().annotations.push({ type: 'reached', description: [...new Set(reached)].join(', ') });
    test.info().annotations.push({
      type: 'reserved (later work)',
      description: [...new Set(reserved)].join(', '),
    });
    expect(new Set(reached).size).toBeGreaterThanOrEqual(30);
    for (const id of reserved) expect(LATER, `${id} should be implemented`).toContain(id);
  });

  test('the stage’s own keys: typing, Enter, Tab, Backspace, arrows, Delete and Esc', async ({ page }) => {
    await openDeck(page);
    await reset(page, 'object');
    // Typing on a selected placeholder replaces its text; Enter starts a paragraph.
    await page.keyboard.type('Clay');
    await expect.poll(async () => (await deckState(page)).slides[0]?.title).toBe('Clay');
    expect((await deckState(page)).editing).toBe(true);
    await page.keyboard.press('Backspace');
    await expect.poll(async () => (await deckState(page)).slides[0]?.title).toBe('Cla');
    await page.keyboard.press('ArrowLeft');
    await page.keyboard.type('X');
    await expect.poll(async () => (await deckState(page)).slides[0]?.title).toBe('ClXa');
    // Esc ends the edit (the placeholder stays selected), arrows then nudge it.
    await page.keyboard.press('Escape');
    await expect.poll(async () => (await deckState(page)).editing).toBe(false);
    const x0 = (await deckState(page)).selection_box?.x ?? 0;
    await page.keyboard.press('ArrowRight');
    await expect.poll(async () => (await deckState(page)).selection_box?.x ?? 0).toBeGreaterThan(x0);
    // Ctrl+Arrow nudges by a pixel (0.75 pt).
    const x1 = (await deckState(page)).selection_box?.x ?? 0;
    await page.keyboard.press('Control+ArrowLeft');
    await expect
      .poll(async () => Math.round(((await deckState(page)).selection_box?.x ?? 0) * 100))
      .toBe(Math.round((x1 - 0.75) * 100));
    // Tab selects the next object; Delete deletes the selection.
    const n = (await deckState(page)).shapes.length;
    await page.keyboard.press('Tab');
    await expect.poll(async () => (await deckState(page)).selection.length).toBe(1);
    await page.keyboard.press('Delete');
    await expect.poll(async () => (await deckState(page)).shapes.length).toBe(n - 1);
    // Esc clears the selection; Page Down goes to the next slide.
    await page.keyboard.press('Escape');
    await expect.poll(async () => (await deckState(page)).selection.length).toBe(0);
    await page.keyboard.press('Control+m');
    await expect.poll(async () => (await deckState(page)).slides.length).toBe(2);
    await page.keyboard.press('PageUp');
    await expect.poll(async () => (await deckState(page)).current).toBe(0);
    await page.keyboard.press('PageDown');
    await expect.poll(async () => (await deckState(page)).current).toBe(1);
  });

  test('the shortcuts change the presentation: clipboard, select all, case, scripts, alignment', async ({
    page,
    context,
  }) => {
    await context.grantPermissions(['clipboard-read', 'clipboard-write']);
    await openDeck(page);
    const stage = async () => {
      await page.evaluate(async () => {
        const d = (window as unknown as W).__kilnDeck;
        await d.fetch();
        d.stage?.focus();
      });
    };
    const core = (c: unknown) =>
      page.evaluate(async (cmd) => {
        const d = (window as unknown as W).__kilnDeck;
        await d.client.execute(d.doc, { Deck: cmd });
        await d.fetch();
      }, c);
    await core({ InsertShape: { preset: 'ellipse', rect: { x: 700, y: 400, width: 200, height: 100 } } });
    await core({ InsertText: { text: 'glaze' } });
    await core('Escape');
    await core('Escape');
    await core({ InsertShape: { preset: 'rect', rect: { x: 100, y: 400, width: 200, height: 100 } } });
    await core({ InsertText: { text: 'kiln studio' } });
    await core('Escape');
    const before = (await deckState(page)).shapes.length;
    // Ctrl+C then Ctrl+V: a copy of the shape.
    await stage();
    await page.keyboard.press('Control+c');
    await page.keyboard.press('Control+v');
    await expect.poll(async () => (await deckState(page)).shapes.length).toBe(before + 1);
    // Ctrl+X: the copy goes.
    await stage();
    await page.keyboard.press('Control+x');
    await expect.poll(async () => (await deckState(page)).shapes.length).toBe(before);
    // Ctrl+A selects every object.
    await stage();
    await page.keyboard.press('Control+a');
    await expect.poll(async () => (await deckState(page)).selection.length).toBe(before);
    // Shift+F3 changes the case of the text of the selected shape.
    const st = await deckState(page);
    const box = st.shapes.find((x) => /^Rectangle/.test(x.name));
    if (box) await core({ SelectShapes: { ids: [box.id] } });
    await stage();
    await page.keyboard.press('Shift+F3');
    const outline = async () =>
      page.evaluate(async () => {
        const d = (window as unknown as W).__kilnDeck;
        const r = (await d.client.query(d.doc, { Deck: 'Outline' })) as {
          Deck: { Outline: { lines: [number, string][] }[] };
        };
        return JSON.stringify(r.Deck.Outline);
      });
    await expect.poll(outline).not.toContain('kiln studio');
    // Ctrl+= and Ctrl+Shift+= toggle subscript and superscript; Ctrl+E/R/L/J align.
    await stage();
    await page.keyboard.press('Control+=');
    await expect.poll(async () => (await deckState(page)).format?.subscript).toBe(true);
    await page.keyboard.press('Control+Shift+=');
    await expect.poll(async () => (await deckState(page)).format?.superscript).toBe(true);
    for (const [key, align] of [
      ['Control+e', 'center'],
      ['Control+r', 'right'],
      ['Control+j', 'justify'],
      ['Control+l', 'left'],
    ] as const) {
      await page.keyboard.press(key);
      await expect.poll(async () => (await deckState(page)).format?.align).toBe(align);
    }
    // F4 repeats the last formatting (bold) on another shape.
    await page.keyboard.press('Control+b');
    await expect.poll(async () => (await deckState(page)).format?.bold).toBe(true);
    const other = (await deckState(page)).shapes.find((x) => /^Oval/.test(x.name));
    if (other) await core({ SelectShapes: { ids: [other.id] } });
    await stage();
    const was = (await deckState(page)).format?.bold;
    await page.keyboard.press('F4');
    await expect.poll(async () => (await deckState(page)).format?.bold).toBe(!was);
  });
});
