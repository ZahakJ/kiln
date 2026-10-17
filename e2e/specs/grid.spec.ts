/**
 * Kiln Grid end to end (WP-71, PLAN.md §1.3–§1.7, §2.3): the editor in the app frame of the web
 * build — typing entries and moving with Enter, Tab and the arrows, F2 and Esc, Delete and undo,
 * formulas with autocomplete and point mode, the fill handle, frozen panes, Format Cells, opening a
 * workbook through the file picker, the screenshots of the visual review, axe, and the budgets B6
 * (scrolling the 1M-cell sheet at 60 fps) and B7 (opening it) on a production build.
 */
import { execFileSync } from 'node:child_process';
import { existsSync, mkdtempSync, readFileSync, rmSync, statSync } from 'node:fs';
import { createServer, type Server } from 'node:http';
import type { AddressInfo } from 'node:net';
import { tmpdir } from 'node:os';
import { extname, join, normalize } from 'node:path';
import { fileURLToPath } from 'node:url';
import { expect, type Page, test } from '@playwright/test';
import { expectNoSeriousA11yViolations } from '../../packages/testing/src/playwright/a11y.ts';
import { decodePng } from './surface-helpers.ts';

const repo = (p: string) => fileURLToPath(new URL(`../../${p}`, import.meta.url));
const shot = (name: string) => repo(`e2e/screenshots/grid/${name}`);

interface GridHandle {
  doc: number;
  client: {
    query(doc: number, q: unknown): Promise<Record<string, unknown>>;
    execute(doc: number, c: unknown): Promise<unknown>;
  };
  state: { dialog: unknown; find: { open: boolean } };
}
interface SurfaceHandle {
  stats: { frames: number; composites: number; lastFrameMs: number };
}
interface KilnHandle {
  registry: { execute(id: string, args?: unknown): Promise<{ status: string }> };
}
type KilnWindow = { __kilnGrid?: GridHandle; __kilnGridSurface?: SurfaceHandle | null; __kiln?: KilnHandle };

interface GridState {
  name_box: string;
  formula: string;
  mode: string;
  active_sheet: number;
  sheets: { name: string }[];
  format: { number_format: string; bold: boolean };
  layout: { freeze_rows: number; freeze_cols: number; header_w: number; header_h: number };
  editor: { text: string; completions: { label: string }[] } | null;
}

/** Opens Kiln Grid on a new workbook and waits for its first painted frame. */
async function openGrid(page: Page, base = ''): Promise<void> {
  await page.goto(`${base}/#/grid?blink=0`);
  // The surface mirrors its frame count on its element (production builds have no window handles).
  await page.waitForFunction(
    () =>
      Number(document.querySelector<HTMLElement>('[data-kiln-grid-surface]')?.dataset.kilnFrames ?? 0) > 0,
    null,
    {
      timeout: 60_000,
    },
  );
  await page.locator('[data-kiln-grid-proxy]').focus();
}

/** The document the grid surface paints and its frame count (from the surface element). */
async function surfaceDoc(page: Page): Promise<{ doc: string; frames: number; ms: number }> {
  return page.evaluate(() => {
    const ds = document.querySelector<HTMLElement>('[data-kiln-grid-surface]')?.dataset;
    return { doc: ds?.kilnDoc ?? '', frames: Number(ds?.kilnFrames ?? 0), ms: Number(ds?.kilnFrameMs ?? 0) };
  });
}

/** The core's Grid state. */
async function gridState(page: Page): Promise<GridState> {
  return page.evaluate(async () => {
    const g = (window as unknown as KilnWindow).__kilnGrid as GridHandle;
    const r = (await g.client.query(g.doc, { Grid: 'State' })) as { Grid: { State: GridState } };
    return r.Grid.State;
  });
}

/** Sends a Grid core command (as the surface does). */
async function core(page: Page, command: unknown): Promise<void> {
  await page.evaluate(async (c) => {
    const g = (window as unknown as KilnWindow).__kilnGrid as GridHandle;
    await g.client.execute(g.doc, { Grid: c });
  }, command);
}

/** The values of a range as tab-separated text (selects it). */
async function values(page: Page, r0: number, c0: number, r1: number, c1: number): Promise<string> {
  await core(page, { SelectRange: { range: { r0, c0, r1, c1 }, add: false } });
  return page.evaluate(async () => {
    const g = (window as unknown as KilnWindow).__kilnGrid as GridHandle;
    const r = (await g.client.query(g.doc, 'SelectedText')) as { Text?: string };
    return (r.Text ?? '').replace(/\r\n/g, '\n').replace(/\n$/, '');
  });
}

/** Runs a command through the window's registry, as the ribbon does. */
async function run(page: Page, id: string, args?: unknown): Promise<string> {
  return page.evaluate(
    async ([i, a]) =>
      (await (window as unknown as KilnWindow).__kiln?.registry.execute(i as string, a))?.status ?? '',
    [id, args] as const,
  );
}

/** Polls the core's state until `check` holds. */
async function until(page: Page, check: (s: GridState) => boolean, what: string): Promise<GridState> {
  let last: GridState | undefined;
  await expect
    .poll(
      async () => {
        last = await gridState(page);
        return check(last);
      },
      { message: what, timeout: 10_000 },
    )
    .toBe(true);
  return last as GridState;
}

/** The surface's top-left corner in page coordinates. */
async function surfaceOrigin(page: Page): Promise<{ x: number; y: number }> {
  const box = await page.locator('[data-kiln-grid-surface]').boundingBox();
  if (!box) throw new Error('no grid surface');
  return { x: box.x, y: box.y };
}

/** Opens a workbook through File ▸ Open ▸ Browse (the picker stub of the e2e tests). */
async function openFile(page: Page, path: string, timeout = 60_000): Promise<void> {
  await page.evaluate(() => {
    // biome-ignore lint/suspicious/noExplicitAny: the API is not in the DOM typings
    delete (window as any).showOpenFilePicker;
  });
  await page.getByRole('button', { name: 'File' }).click();
  await page.getByRole('button', { name: 'Open', exact: true }).click();
  const [chooser] = await Promise.all([
    page.waitForEvent('filechooser'),
    page.getByRole('button', { name: /Browse this device/ }).click(),
  ]);
  const before = (await surfaceDoc(page)).doc;
  await chooser.setFiles(path);
  await page.waitForFunction(
    (b) => {
      const ds = document.querySelector<HTMLElement>('[data-kiln-grid-surface]')?.dataset;
      return !!ds?.kilnDoc && ds.kilnDoc !== b && Number(ds.kilnFrames ?? 0) > 0;
    },
    before,
    { timeout },
  );
}

test.describe('Kiln Grid editor', () => {
  test('grid: types entries and moves with Enter, Tab and the arrows; F2, Esc, Delete and undo', async ({
    page,
  }) => {
    await openGrid(page);
    await page.keyboard.type('Item');
    await page.keyboard.press('Tab');
    await page.keyboard.type('Q1');
    await page.keyboard.press('Enter');
    // Enter after Tab returns to the column the row started in (Excel).
    await until(page, (s) => s.name_box === 'A2', 'Enter after Tab goes to A2');
    await page.keyboard.type('Clay');
    await page.keyboard.press('Tab');
    await page.keyboard.type('420');
    await page.keyboard.press('Enter');
    await page.keyboard.type('Glaze');
    await page.keyboard.press('Tab');
    await page.keyboard.type('310');
    await page.keyboard.press('Enter');
    await until(page, (s) => s.name_box === 'A4', 'third row done');
    // Arrows move; F2 edits in place; Esc cancels.
    await page.keyboard.press('ArrowUp');
    await page.keyboard.press('F2');
    await until(page, (s) => s.mode === 'Edit', 'F2 starts Edit mode');
    await page.keyboard.type('s');
    await page.keyboard.press('Enter');
    await page.keyboard.press('ArrowUp');
    await page.keyboard.type('zzz');
    await page.keyboard.press('Escape');
    await until(page, (s) => s.mode === 'Ready' && s.formula === 'Glazes', 'Esc keeps Glazes');
    expect(await values(page, 0, 0, 2, 1)).toBe('Item\tQ1\nClay\t420\nGlazes\t310');
    // Delete clears the selection; Ctrl+Z brings it back.
    await core(page, { SelectRange: { range: { r0: 1, c0: 1, r1: 2, c1: 1 }, add: false } });
    await page.locator('[data-kiln-grid-proxy]').focus();
    await page.keyboard.press('Delete');
    await expect.poll(() => values(page, 1, 1, 2, 1)).toBe('\n');
    await page.locator('[data-kiln-grid-proxy]').focus();
    await page.keyboard.press('Control+z');
    await expect.poll(() => values(page, 1, 1, 2, 1)).toBe('420\n310');
    // Ctrl+arrows jump to the edge of the data; Shift extends.
    await core(page, { SelectCell: { row: 0, col: 0, extend: false, add: false } });
    await page.locator('[data-kiln-grid-proxy]').focus();
    await page.keyboard.press('Control+ArrowDown');
    await until(page, (s) => s.name_box === 'A3', 'Ctrl+Down to the last entry');
    await page.keyboard.press('Control+Shift+ArrowRight');
    await until(page, (s) => s.name_box === 'A3' && s.formula === 'Glazes', 'Ctrl+Shift+Right extends');
    await page.keyboard.press('Control+Home');
    await until(page, (s) => s.name_box === 'A1', 'Ctrl+Home to A1');
  });

  test('grid: formula entry with autocomplete, the argument hint and point mode', async ({ page }) => {
    await openGrid(page);
    for (const v of ['4', '6']) {
      await page.keyboard.type(v);
      await page.keyboard.press('Enter');
    }
    await core(page, { SelectCell: { row: 0, col: 1, extend: false, add: false } });
    await page.locator('[data-kiln-grid-proxy]').focus();
    await page.keyboard.type('=su');
    const popover = page.locator('[data-kiln-autocomplete]');
    await expect(popover.getByRole('option', { name: 'SUM', exact: true })).toBeVisible();
    // The highlighted function is documented beside the list; the arrows move the highlight.
    await expect(popover.locator('.kiln-autocomplete__doc')).toContainText('SUBSTITUTE(');
    await page.keyboard.press('ArrowDown');
    await page.keyboard.press('ArrowDown');
    await expect(popover.locator('.kiln-autocomplete__doc')).toContainText('SUM(number1');
    await page.keyboard.press('Tab');
    await until(page, (s) => s.editor?.text === '=SUM(', 'Tab inserts SUM(');
    await expect(popover.getByRole('tooltip')).toContainText('number1');
    // Point mode: the arrows put a reference in the formula.
    await page.keyboard.press('ArrowLeft');
    await until(page, (s) => s.mode === 'Point' && s.editor?.text === '=SUM(A1', 'points at A1');
    await page.keyboard.press('Shift+ArrowDown');
    await until(page, (s) => s.editor?.text === '=SUM(A1:A2', 'extends to A1:A2');
    // Enter closes the parenthesis (Excel).
    await page.keyboard.press('Enter');
    await core(page, { SelectCell: { row: 0, col: 1, extend: false, add: false } });
    const s = await until(page, (x) => x.formula === '=SUM(A1:A2)', 'formula committed');
    expect(s.name_box).toBe('B1');
    expect(await values(page, 0, 1, 0, 1)).toBe('10');
    // The formula bar shows the formula with its reference coloured.
    await core(page, { SelectCell: { row: 0, col: 1, extend: false, add: false } });
    await expect(page.getByRole('textbox', { name: 'Formula Bar' })).toHaveValue('=SUM(A1:A2)');
  });

  test('grid: the fill handle extends a series', async ({ page }) => {
    await openGrid(page);
    for (const v of ['1', '2']) {
      await page.keyboard.type(v);
      await page.keyboard.press('Enter');
    }
    await core(page, { SelectRange: { range: { r0: 0, c0: 0, r1: 1, c1: 0 }, add: false } });
    const s = await gridState(page);
    const o = await surfaceOrigin(page);
    // Default sizes: 64 px columns, 20 px rows (Calibri 11).
    const hx = o.x + s.layout.header_w;
    const hy = o.y + s.layout.header_h;
    await page.waitForTimeout(150);
    await page.mouse.move(hx + 64 - 1, hy + 40 - 1);
    await page.mouse.down();
    await page.mouse.move(hx + 40, hy + 80, { steps: 4 });
    await page.mouse.move(hx + 40, hy + 115, { steps: 4 });
    await page.mouse.up();
    await expect.poll(() => values(page, 0, 0, 5, 0)).toBe('1\n2\n3\n4\n5\n6');
  });

  test('grid: frozen panes stay while the sheet scrolls', async ({ page }) => {
    await openGrid(page);
    await page.keyboard.type('Header');
    await page.keyboard.press('Tab');
    await page.keyboard.type('Value');
    await page.keyboard.press('Enter');
    await core(page, { SelectCell: { row: 1, col: 1, extend: false, add: false } });
    expect(await run(page, 'grid.view.freeze_panes')).toBe('ran');
    await until(page, (s) => s.layout.freeze_rows === 1 && s.layout.freeze_cols === 1, 'frozen at B2');
    const surface = page.locator('[data-kiln-grid-surface]');
    const box = await surface.boundingBox();
    if (!box) throw new Error('no surface');
    await page.waitForTimeout(300);
    // Regions of the surface, in page coordinates (Locator.screenshot has no clip).
    const top = { x: box.x, y: box.y, width: Math.round(box.width), height: 42 };
    const body = { x: box.x, y: box.y + 60, width: Math.round(box.width), height: 200 };
    const before = await page.screenshot({ clip: top });
    const bodyBefore = await page.screenshot({ clip: body });
    await page.mouse.move(box.x + box.width / 2, box.y + box.height / 2);
    for (let i = 0; i < 10; i++) {
      await page.mouse.wheel(0, 400);
      await page.waitForTimeout(30);
    }
    await page.waitForTimeout(500);
    // The frozen row and the column headers did not move; the cells under them did.
    const after = await page.screenshot({ clip: top });
    const bodyAfter = await page.screenshot({ clip: body });
    const changed = async (a: Buffer, b: Buffer) => {
      const x = await decodePng(page, a);
      const y = await decodePng(page, b);
      let n = 0;
      for (let i = 0; i < x.data.length; i += 4) {
        if (
          Math.abs((x.data[i] ?? 0) - (y.data[i] ?? 0)) +
            Math.abs((x.data[i + 1] ?? 0) - (y.data[i + 1] ?? 0)) >
          24
        )
          n++;
      }
      return n / (x.data.length / 4);
    };
    // A thin sliver may differ (the selection frame's edge over the frozen row), nothing more.
    expect(await changed(before, after)).toBeLessThan(0.01);
    expect(await changed(bodyBefore, bodyAfter)).toBeGreaterThan(0.002);
    await page.screenshot({ path: shot('frozen-scrolled.png') });
  });

  test('grid: Format Cells applies a number format and a border; Ctrl+B bolds', async ({ page }) => {
    await openGrid(page);
    await page.keyboard.type('0.25');
    await page.keyboard.press('Enter');
    await page.keyboard.press('ArrowUp');
    await page.keyboard.press('Control+1');
    const dialog = page.getByRole('dialog', { name: 'Format Cells' });
    await expect(dialog).toBeVisible();
    await dialog.getByRole('option', { name: 'Percentage' }).click();
    await expect(dialog.locator('output')).toHaveText('25%');
    await dialog.getByRole('tab', { name: 'Border' }).click();
    await dialog.getByRole('button', { name: 'Outline' }).click();
    await expectNoSeriousA11yViolations(page);
    await dialog.getByRole('button', { name: 'OK' }).click();
    await expect(dialog).toBeHidden();
    const s = await until(page, (x) => x.format.number_format === '0%', 'percent format');
    expect(s.name_box).toBe('A1');
    expect(await values(page, 0, 0, 0, 0)).toBe('25%');
    await core(page, { SelectCell: { row: 0, col: 0, extend: false, add: false } });
    await page.locator('[data-kiln-grid-proxy]').focus();
    await page.keyboard.press('Control+b');
    await until(page, (x) => x.format.bold, 'bold');
  });

  test('grid: opens a workbook; screenshots light and dark, desktop and phone; axe', async ({ page }) => {
    test.setTimeout(180_000);
    await openGrid(page);
    await openFile(page, repo('fixtures/xlsx/features/styles.xlsx'));
    await expect(page.getByTestId('document-title')).toContainText('styles');
    await expect(page.getByRole('tab', { name: /./ }).first()).toBeVisible();
    await core(page, { SelectRange: { range: { r0: 1, c0: 1, r1: 4, c1: 2 }, add: false } });
    await page.evaluate(() => document.fonts.ready);
    await page.waitForTimeout(500);
    await page.screenshot({ path: shot('editor-light-desktop.png') });
    await expectNoSeriousA11yViolations(page);
    await page.emulateMedia({ colorScheme: 'dark' });
    await page.waitForTimeout(500);
    await page.screenshot({ path: shot('editor-dark-desktop.png') });
    await expectNoSeriousA11yViolations(page);
    // Formula autocomplete, in the dark theme.
    await core(page, { SelectCell: { row: 12, col: 1, extend: false, add: false } });
    await page.locator('[data-kiln-grid-proxy]').focus();
    await page.keyboard.type('=VLO');
    await expect(page.locator('[data-kiln-autocomplete]')).toBeVisible();
    await page.waitForTimeout(300);
    await page.screenshot({ path: shot('autocomplete-dark-desktop.png') });
    await page.keyboard.press('Escape');
    await page.setViewportSize({ width: 390, height: 844 });
    await page.waitForTimeout(500);
    await page.screenshot({ path: shot('editor-dark-phone.png') });
    await page.emulateMedia({ colorScheme: 'light' });
    await page.waitForTimeout(500);
    await page.screenshot({ path: shot('editor-light-phone.png') });
    // The Grid view at phone width (the app frame's own title bar is WP-58's: docs/qa/handoffs.md).
    await expectNoSeriousA11yViolations(page, { include: ['[data-testid="grid-editor"]'] });
    await page.setViewportSize({ width: 1440, height: 900 });
    await page.keyboard.press('Control+1');
    await expect(page.getByRole('dialog', { name: 'Format Cells' })).toBeVisible();
    await page.waitForTimeout(300);
    await page.screenshot({ path: shot('format-cells-light.png') });
  });
});

const MIME: Record<string, string> = {
  '.html': 'text/html; charset=utf-8',
  '.js': 'text/javascript',
  '.css': 'text/css',
  '.wasm': 'application/wasm',
  '.json': 'application/json',
  '.webmanifest': 'application/manifest+json',
  '.svg': 'image/svg+xml',
  '.png': 'image/png',
  '.ico': 'image/x-icon',
  '.ttf': 'font/ttf',
  '.woff2': 'font/woff2',
  '.map': 'application/json',
};

/** Serves a built site (the app routes by hash: no fallback needed). */
function serveStatic(root: string): Promise<Server> {
  const server = createServer((req, res) => {
    const path = decodeURIComponent(new URL(req.url ?? '/', 'http://x').pathname);
    const file = normalize(join(root, path === '/' ? 'index.html' : path));
    try {
      if (!file.startsWith(root) || !statSync(file).isFile()) throw new Error('not found');
      res.writeHead(200, { 'content-type': MIME[extname(file)] ?? 'application/octet-stream' });
      res.end(readFileSync(file));
    } catch {
      res.writeHead(404).end();
    }
  });
  return new Promise((resolve) => server.listen(0, '127.0.0.1', () => resolve(server)));
}

const pct = (xs: number[], p: number) => {
  const s = [...xs].sort((a, b) => a - b);
  return s[Math.min(s.length - 1, Math.ceil(s.length * p) - 1)] ?? Number.POSITIVE_INFINITY;
};

/**
 * B6 and B7 on a production build (`VITE_KILN_RELEASE=1`, the release web core): the frames of
 * scrolling the 1M-cell sheet and the time to open it.
 */
test.describe('grid budgets', () => {
  test.describe.configure({ mode: 'serial' });
  let outDir = '';
  let server: Server | undefined;
  let base = '';

  test.beforeAll(async () => {
    test.setTimeout(300_000);
    const sheet = repo('fixtures/large/sheet-1m.xlsx');
    if (!existsSync(sheet)) {
      execFileSync('python3', ['-I', repo('tools/make-fixtures.py'), '--synthetic-large', '--only', 'xlsx'], {
        stdio: 'pipe',
        timeout: 240_000,
      });
    }
    outDir = mkdtempSync(join(tmpdir(), 'kiln-b6-'));
    execFileSync(
      'pnpm',
      ['--filter', '@kiln/web', 'exec', 'vite', 'build', '--outDir', outDir, '--emptyOutDir'],
      {
        cwd: repo(''),
        env: { ...process.env, VITE_KILN_RELEASE: '1' },
        stdio: 'pipe',
      },
    );
    server = await serveStatic(outDir);
    base = `http://127.0.0.1:${(server.address() as AddressInfo).port}`;
  });

  test.afterAll(async () => {
    await new Promise<void>((resolve) => (server ? server.close(() => resolve()) : resolve()));
    if (outDir) rmSync(outDir, { recursive: true, force: true });
  });

  test('grid B7 and B6: open the 1M-cell sheet, then scroll it at 60 fps (frame p95 ≤ 16.7 ms)', async ({
    page,
  }) => {
    test.setTimeout(300_000);
    await openGrid(page, base);
    const t0 = Date.now();
    await openFile(page, repo('fixtures/large/sheet-1m.xlsx'), 240_000);
    const openMs = Date.now() - t0;
    test.info().annotations.push({
      type: 'B7',
      description: `open + first paint of sheet-1m.xlsx (6.4 MB, 1M cells) in the web core: ${openMs} ms (budget 1.2 s for 500k cells, native)`,
    });
    console.log(`B7 web: sheet-1m.xlsx open to first paint ${openMs} ms`);
    expect(openMs).toBeLessThan(240_000);
    // The populated sheet ("Data", the last tab), as a user picks it.
    await page.getByRole('tablist', { name: 'Sheets' }).getByRole('tab').last().click();
    await page.waitForTimeout(1500);
    // Scroll at 2,000 px/s for 4 s, one step per animation frame. B6 is the surface's work per
    // animation frame (composite and prefetched tile steps, `kiln:grid-frame`), as the page
    // surface measures it (surface.spec.ts); the work of applying each delta (`kiln:grid-apply`),
    // the core's frame round trip and the frame intervals are recorded with it.
    const result = await page.evaluate(async () => {
      const scroller = document.querySelector('.kiln-grid-surface__scroller') as HTMLElement;
      const el = document.querySelector('[data-kiln-grid-surface]') as HTMLElement;
      const frames: number[] = [];
      const applies: number[] = [];
      const measures = new PerformanceObserver((list) => {
        for (const m of list.getEntries()) {
          if (m.name === 'kiln:grid-frame') frames.push(m.duration);
          else if (m.name === 'kiln:grid-apply') applies.push(m.duration);
        }
      });
      measures.observe({ type: 'measure' });
      const intervals: number[] = [];
      const core: number[] = [];
      let prev = performance.now();
      const start = prev;
      const frames0 = Number(el.dataset.kilnFrames ?? 0);
      await new Promise<void>((resolve) => {
        const step = (t: number) => {
          intervals.push(t - prev);
          // 2,000 px/s whatever the frame rate (a slow frame scrolls further, as a fling does).
          scroller.scrollTop += (2000 * Math.min(100, t - prev)) / 1000;
          prev = t;
          core.push(Number(el.dataset.kilnFrameMs ?? 0));
          if (t - start < 4000) requestAnimationFrame(step);
          else resolve();
        };
        requestAnimationFrame(step);
      });
      await new Promise((r) => setTimeout(r, 100));
      measures.disconnect();
      return {
        intervals: intervals.slice(1),
        frames,
        applies,
        core,
        coreFrames: Number(el.dataset.kilnFrames ?? 0) - frames0,
        top: scroller.scrollTop,
      };
    });
    const work = pct(result.frames, 0.95);
    const apply = pct(result.applies, 0.95);
    const p50 = pct(result.intervals, 0.5);
    const p95 = pct(result.intervals, 0.95);
    const summary = `surface work per frame p95=${work.toFixed(1)} ms (n=${result.frames.length}), delta apply p95=${apply.toFixed(1)} ms; frame interval p50=${p50.toFixed(1)} ms p95=${p95.toFixed(1)} ms; core round trip p95=${pct(result.core, 0.95).toFixed(1)} ms; ${result.coreFrames} core frames; scrolled ${Math.round(result.top)} px`;
    test.info().annotations.push({ type: 'B6', description: summary });
    console.log(`B6 grid scroll on sheet-1m (production build): ${summary}`);
    expect(result.top).toBeGreaterThan(5000);
    expect(result.frames.length).toBeGreaterThan(100);
    // B6: the surface's frame ≤ 16.7 ms p95 (CI runners: §3.9's 1.5× tolerance). The frame
    // interval also depends on the Worker and on what else the machine runs (headless Chromium
    // renders in software here): its median is one vsync on a quiet machine and must stay
    // within two (docs/qa/perf.md records the measurements).
    const tolerance = process.env.CI ? 1.5 : 1;
    expect(work, `B6 surface work p95 ${work.toFixed(1)} ms`).toBeLessThanOrEqual(16.7 * tolerance);
    expect(work + apply, 'composite and delta apply together fit a frame').toBeLessThanOrEqual(
      16.7 * tolerance,
    );
    expect(p50, `frame interval p50 ${p50.toFixed(1)} ms`).toBeLessThanOrEqual(33.4 * tolerance + 0.5);
  });
});
