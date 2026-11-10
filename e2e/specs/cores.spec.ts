/**
 * The web cores, one per app (budget B12): Home starts none, each app window starts only its
 * own core, documents of several apps stay open side by side in one page, File ▸ Open in one app
 * opens another app's file in that app's core, and the clipboard carries text and HTML between the
 * apps through the system clipboard.
 *
 * Needs the wasm build: `cargo xtask wasm` (or `--release`) before `pnpm e2e`.
 */
import { readFileSync } from 'node:fs';
import { expect, type Page, test } from '@playwright/test';
import { fill, focusSheet, openGrid, repo, value } from './grid-helpers';

const START = 60_000;

/** The per-app wasm modules the page has fetched (`write`, `grid`, `deck`). */
function wasmFetched(page: Page): string[] {
  const seen: string[] = [];
  page.on('request', (r) => {
    const m = /\/wasm\/(\w+)\/[^/]*\.wasm/.exec(r.url()) ?? /kiln_bridge_wasm_bg[^/]*\.wasm/.exec(r.url());
    if (m) seen.push(m[1] ?? 'single');
  });
  return seen;
}

type Handle = { client: { query(doc: number, q: unknown): Promise<unknown> }; doc: number };
type W = { __kilnWrite?: Handle; __kilnGrid?: Handle; __kilnDeck?: Handle };

async function docOf(page: Page, app: 'Write' | 'Grid' | 'Deck'): Promise<number> {
  return page.evaluate(
    (key) => (window as unknown as Record<string, Handle | undefined>)[key]?.doc ?? 0,
    `__kiln${app}`,
  );
}

async function writeText(page: Page): Promise<string> {
  return page.evaluate(async () => {
    const w = (window as unknown as W).__kilnWrite as Handle;
    const r = (await w.client.query(w.doc, 'Paragraphs')) as { Paragraphs?: { text: string }[] };
    return (r.Paragraphs ?? []).map((p) => p.text).join('\n');
  });
}

async function openWriteHere(page: Page): Promise<void> {
  await page.evaluate(() => {
    window.location.hash = `#/write?new=${Date.now()}`;
  });
  await page.waitForFunction(
    () =>
      ((
        window as unknown as { __kilnWrite?: { controller?: { getStats(): { tiles: number } } } }
      ).__kilnWrite?.controller?.getStats().tiles ?? 0) > 0,
    null,
    { timeout: START },
  );
  await page.getByTestId('kiln-input-proxy').focus();
}

test.describe('web cores', () => {
  test('Home starts no core, and each app starts only its own', async ({ page }) => {
    test.setTimeout(150_000);
    const fetched = wasmFetched(page);
    await page.goto('/#/home');
    await expect(page.getByText(/Your recent files will gather here|Recent/).first()).toBeVisible();
    await page.waitForTimeout(500);
    expect(fetched).toEqual([]);
    expect(page.workers()).toHaveLength(0);

    await openWriteHere(page);
    expect(fetched).toEqual(['write']);
    const write = await docOf(page, 'Write');

    await page.evaluate(() => {
      window.location.hash = '#/grid?blink=0';
    });
    await page.waitForFunction(
      () =>
        Number(document.querySelector<HTMLElement>('[data-kiln-grid-surface]')?.dataset.kilnFrames ?? 0) > 0,
      null,
      { timeout: START },
    );
    expect(fetched).toEqual(['write', 'grid']);
    const grid = await docOf(page, 'Grid');
    // Each core numbers its documents in its own residue class (`DOC_IDS`).
    expect([write % 4, grid % 4]).toEqual([1, 2]);

    // The Write document is still open in its core.
    await page.evaluate((doc) => {
      window.location.hash = `#/write?doc=${doc}`;
    }, write);
    await expect(page.getByTestId('document-title')).toContainText('Document', { timeout: START });
    expect(fetched).toEqual(['write', 'grid']);
    expect(page.workers()).toHaveLength(2);
  });

  test('File ▸ Open in Write opens a presentation in the Deck core', async ({ page }) => {
    test.setTimeout(150_000);
    const fetched = wasmFetched(page);
    await page.goto('/#/write');
    await expect(page.getByTestId('document-title')).not.toHaveText('Opening…', { timeout: START });
    await page.evaluate(() => {
      delete (window as { showOpenFilePicker?: unknown }).showOpenFilePicker;
    });
    await page.getByRole('button', { name: 'File' }).click();
    const backstage = page.getByRole('dialog', { name: /File menu/ });
    await backstage.getByRole('button', { name: 'Open', exact: true }).click();
    const [chooser] = await Promise.all([
      page.waitForEvent('filechooser'),
      backstage.getByRole('button', { name: /Browse this device/ }).click(),
    ]);
    await chooser.setFiles({
      name: 'all-layouts.pptx',
      mimeType: 'application/octet-stream',
      buffer: readFileSync(repo('fixtures/pptx/features/all-layouts.pptx')),
    });
    await page.waitForFunction(() => /^#\/deck\?doc=\d+/.test(window.location.hash), null, {
      timeout: START,
    });
    await expect(page.getByTestId('document-title')).toContainText('all-layouts', { timeout: START });
    expect(Number(/doc=(\d+)/.exec(await page.evaluate(() => window.location.hash))?.[1]) % 4).toBe(3);
    expect(fetched).toEqual(['write', 'deck']);
  });

  test('the clipboard carries text between Grid and Write', async ({ page, context }) => {
    test.setTimeout(150_000);
    await context.grantPermissions(['clipboard-read', 'clipboard-write']);
    await openGrid(page);
    await fill(page, [['glaze']]);
    await page.keyboard.press('Control+Home');
    await page.keyboard.press('Control+c');
    await expect.poll(() => page.evaluate(() => navigator.clipboard.readText())).toMatch(/^glaze\r?\n?$/);

    await openWriteHere(page);
    await page.keyboard.press('Control+v');
    await expect.poll(() => writeText(page), { timeout: 20_000 }).toContain('glaze');

    // And back: text copied in Write pastes into a cell.
    await page.keyboard.press('Control+a');
    await page.keyboard.press('Delete');
    await page.keyboard.type('Fired clay');
    await page.keyboard.press('Control+a');
    await page.keyboard.press('Control+c');
    await expect.poll(() => page.evaluate(() => navigator.clipboard.readText())).toBe('Fired clay');
    await page.evaluate(() => {
      window.location.hash = `#/grid?new=${Date.now()}&blink=0`;
    });
    await page.waitForFunction(
      () =>
        Number(document.querySelector<HTMLElement>('[data-kiln-grid-surface]')?.dataset.kilnFrames ?? 0) > 0,
      null,
      { timeout: START },
    );
    await focusSheet(page);
    await page.keyboard.press('Control+v');
    await expect.poll(() => value(page, 0, 0), { timeout: 20_000 }).toBe('Fired clay');
  });
});
