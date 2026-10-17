/**
 * PLAN.md §1.7 end to end (WP-58): every row of the shortcut table, in every app that uses it,
 * resolves in the running app to the catalog id its keymap names. Commands the core implements are
 * dispatched and checked in the session's command log (a fresh document per app); the others are
 * checked through the registry (they resolve, and release builds hide them).
 */
import { expect, type Page, test } from '@playwright/test';
import type { KeyBinding } from '../../packages/commands/src/keymap.ts';
import {
  commonKeymap,
  deckKeymap,
  gridKeymap,
  SLIDE_SHOW_ROW,
  writeKeymap,
} from '../../packages/commands/src/keymaps/index.ts';
import { SHORTCUT_TABLE, SLIDE_SHOW_TABLE } from '../../packages/commands/src/keymaps/table.ts';

type Resolved = { key: string; command: string; args?: unknown };

declare global {
  interface Window {
    /** The app frame's development handle (apps/web/src/routes/app-frame/debug.ts). */
    __kiln?: {
      app: string;
      resolved: { key: string; command: string; args?: unknown }[];
      executed: { id: string; status: string }[];
      log(): { seq: number; id: string; outcome: string }[];
      registry: {
        setContext(patch: Record<string, unknown>): void;
        getState(id: string): { enabled: boolean; pending: boolean; visible: boolean };
      };
      keymap: {
        resolve(
          e: { key: string; ctrlKey: boolean; shiftKey: boolean; altKey: boolean },
          ctx: Record<string, unknown>,
          o?: { release?: boolean },
        ): { command: string; args?: unknown } | undefined;
      };
    };
  }
}

const APPS = { write: writeKeymap, grid: gridKeymap, deck: deckKeymap } as const;
type App = keyof typeof APPS;
const NONE = /^[—–-]?$/;

/** Commands that leave the window (New, Open, Close, Save dialogs): resolved only, dispatched in their own test. */
const LEAVES_WINDOW = new Set([
  'suite.file.new',
  'suite.file.open',
  'suite.file.close',
  'suite.file.save',
  'suite.file.save_as',
  'suite.file.print',
]);

function rowsFor(app: App): { row: string; bindings: KeyBinding[] }[] {
  return SHORTCUT_TABLE.filter((r) => !NONE.test(r[app].trim())).map((r) => ({
    row: r.shortcut,
    bindings: [...commonKeymap, ...APPS[app]].filter((b) => b.row === r.shortcut),
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

async function openApp(page: Page, app: App) {
  await page.goto(`/#/${app}`);
  await expect(page.getByTestId('document-title')).not.toHaveText('Opening…', { timeout: 30_000 });
  await expect.poll(() => page.evaluate(() => window.__kiln?.app)).toBe(app);
}

/** Dispatches a key press as the browser would and returns what it resolved to. */
async function press(page: Page, b: KeyBinding): Promise<Resolved | undefined> {
  const init = eventInit(b.key);
  return page.evaluate(
    ({ init, scope }) => {
      const k = window.__kiln;
      if (!k) throw new Error('window.__kiln is missing (development build only)');
      k.registry.setContext({ scopes: scope ? [scope] : [] });
      const before = k.resolved.length;
      const target = document.activeElement ?? document.body;
      const opts = { ...init, bubbles: true, cancelable: true, composed: true };
      target.dispatchEvent(new KeyboardEvent('keydown', opts));
      if (init.key === 'Alt') target.dispatchEvent(new KeyboardEvent('keyup', { ...opts, altKey: false }));
      k.registry.setContext({ scopes: [] });
      return k.resolved.length > before ? k.resolved[k.resolved.length - 1] : undefined;
    },
    { init, scope: b.scope },
  );
}

/** Resolves without dispatching (for the commands that would leave this window). */
function resolveOnly(page: Page, b: KeyBinding) {
  const init = eventInit(b.key);
  return page.evaluate(
    ({ init, scope }) =>
      window.__kiln?.keymap.resolve(
        init,
        { app: window.__kiln.app, scopes: scope ? [scope] : [] },
        {
          release: init.key === 'Alt',
        },
      ),
    { init, scope: b.scope },
  );
}

async function closeOverlays(page: Page) {
  for (let i = 0; i < 3; i++) {
    const open = await page.locator('[role="dialog"], [data-testid="keytips"], [role="menu"]').count();
    if (open === 0) return;
    await page.keyboard.press('Escape');
  }
}

test.describe('shortcuts', () => {
  test('the §1.7 table has all 57 rows and the 7 slide-show keys', () => {
    expect(SHORTCUT_TABLE).toHaveLength(57);
    expect(SLIDE_SHOW_TABLE).toHaveLength(7);
  });

  for (const app of Object.keys(APPS) as App[]) {
    test(`${app}: every row of §1.7 resolves to its catalog id`, async ({ page }) => {
      test.setTimeout(180_000);
      await openApp(page, app);
      const rows = rowsFor(app);
      expect(rows.length).toBeGreaterThan(app === 'write' ? 40 : 30);
      let dispatchedToCore = 0;
      for (const { row, bindings } of rows) {
        expect(bindings.length, `${app}: "${row}" has no binding`).toBeGreaterThan(0);
        for (const b of bindings) {
          const where = `${app}: ${b.key} (${row})`;
          if (LEAVES_WINDOW.has(b.command)) {
            const r = await resolveOnly(page, b);
            expect(r?.command, where).toBe(b.command);
            continue;
          }
          const state = await page.evaluate((id) => window.__kiln?.registry.getState(id), b.command);
          const logBefore = await page.evaluate(() => window.__kiln?.log().length ?? 0);
          const r = await press(page, b);
          expect(r?.command, where).toBe(b.command);
          expect(r?.args ?? null, where).toEqual(b.args ?? null);
          if (state && !state.pending && state.enabled) {
            // A core command: it reached the session.
            const isCore = await page.evaluate(
              ({ id, n }) => (window.__kiln?.log() ?? []).slice(n).some((e) => e.id === id),
              { id: b.command, n: logBefore },
            );
            if (!b.command.startsWith('ui.')) {
              expect(isCore, `${where} did not reach the core`).toBe(true);
              dispatchedToCore++;
            }
          }
          await closeOverlays(page);
        }
      }
      if (app === 'write') expect(dispatchedToCore).toBeGreaterThanOrEqual(10);
    });
  }

  test('deck: the slide-show keys resolve in the slideshow scope', async ({ page }) => {
    await openApp(page, 'deck');
    for (const row of SLIDE_SHOW_TABLE) {
      const bindings = deckKeymap.filter((b) => b.row === `${SLIDE_SHOW_ROW}${row.key}`);
      expect(bindings.length).toBeGreaterThan(0);
      for (const b of bindings) {
        const r = await press(page, b);
        expect(r?.command, `${b.key} (${row.key})`).toBe(b.command);
        await closeOverlays(page);
      }
    }
  });

  test('write: Ctrl+N, Ctrl+W and Ctrl+S reach the session (command log)', async ({ page }) => {
    await openApp(page, 'write');
    const fire = (key: string, scope?: string) =>
      press(page, { key, command: '', row: '', ...(scope ? { scope } : {}) });
    const lastLog = () => page.evaluate(() => window.__kiln?.log().at(-1));
    await fire('Mod+N');
    await expect.poll(lastLog).toMatchObject({ id: 'suite.file.new', outcome: 'Applied' });
    await fire('Mod+S');
    await expect.poll(lastLog).toMatchObject({ id: 'suite.file.save' });
    await closeOverlays(page);
    await fire('Mod+W');
    await expect.poll(lastLog).toMatchObject({ id: 'suite.file.close', outcome: 'Applied' });
    // The window's document is gone: Save is no longer available.
    await expect
      .poll(() => page.evaluate(() => window.__kiln?.registry.getState('suite.file.save').enabled))
      .toBe(false);
  });

  test('Undo is unavailable until the core reports something to undo', async ({ page }) => {
    await openApp(page, 'write');
    const state = await page.evaluate(() => window.__kiln?.registry.getState('suite.edit.undo'));
    expect(state?.enabled).toBe(false);
    expect(state?.pending).toBe(false);
  });
});
