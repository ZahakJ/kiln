/**
 * Helpers of the Kiln Grid feature specs: open the editor, put data in through the core,
 * read the core's state and answers, click ribbon items and menus, find cells on the surface,
 * check that the keys came back to the sheet, and save screenshots.
 */
import { fileURLToPath } from 'node:url';
import { expect, type Page } from '@playwright/test';

export const repo = (p: string) => fileURLToPath(new URL(`../../${p}`, import.meta.url));
export const shot = (name: string) => repo(`e2e/screenshots/grid/${name}`);

interface GridHandle {
  doc: number;
  client: {
    query(doc: number, q: unknown): Promise<Record<string, unknown>>;
    execute(doc: number, c: unknown): Promise<unknown>;
  };
}
interface KilnHandle {
  registry: { execute(id: string, args?: unknown): Promise<{ status: string }> };
}
type W = { __kilnGrid?: GridHandle; __kiln?: KilnHandle };

export interface GridState {
  name_box: string;
  formula: string;
  mode: string;
  active_sheet: number;
  sheets: { name: string }[];
  layout: { header_w: number; header_h: number };
  format: {
    bold: boolean;
    italic: boolean;
    fill_color: string | null;
    font_color: string | null;
    merged: boolean;
    h_align: string;
  };
  drawing: {
    kind: string;
    name: string;
    x: number;
    y: number;
    width: number;
    height: number;
    object: { preset: string; fill: string; text: string; crop: number[]; alt_text: string } | null;
  } | null;
  features: {
    filter: boolean;
    filtered: boolean;
    table: {
      name: string;
      style: string;
      options: Record<string, boolean>;
      range: { r0: number; r1: number; c0: number; c1: number };
    } | null;
    link: { target: string; location: string } | null;
    note: boolean;
    comment: boolean;
    any_comments: boolean;
    cards: unknown[];
    prompt: [string, string] | null;
    dropdown: boolean;
    arrows: boolean;
    pages: number;
    calc: string | null;
    iterative: [boolean, number, number];
    circular: string[];
    watches: { cell: string; value: string }[];
    message: string | null;
    print_gridlines: boolean;
    landscape: boolean;
  };
}

/** Opens Kiln Grid on a new workbook and waits for its first painted frame. */
export async function openGrid(page: Page): Promise<void> {
  await page.goto('/#/grid?blink=0');
  await page.waitForFunction(
    () =>
      Number(document.querySelector<HTMLElement>('[data-kiln-grid-surface]')?.dataset.kilnFrames ?? 0) > 0,
    null,
    { timeout: 60_000 },
  );
  await focusSheet(page);
}

export async function focusSheet(page: Page): Promise<void> {
  await page.locator('[data-kiln-grid-proxy]').focus();
}

/** The core's Grid state. */
export async function gridState(page: Page): Promise<GridState> {
  return page.evaluate(async () => {
    const g = (window as unknown as W).__kilnGrid as GridHandle;
    const r = (await g.client.query(g.doc, { Grid: 'State' })) as { Grid: { State: GridState } };
    return r.Grid.State;
  });
}

/** Asks the core a Grid question. */
export async function ask<T>(page: Page, q: unknown): Promise<T> {
  return page.evaluate(async (query) => {
    const g = (window as unknown as W).__kilnGrid as GridHandle;
    const r = (await g.client.query(g.doc, { Grid: query })) as { Grid: Record<string, unknown> };
    return Object.values(r.Grid)[0] as T;
  }, q);
}

/** Sends a Grid core command (as the surface does). */
export async function core(page: Page, command: unknown): Promise<void> {
  await page.evaluate(async (c) => {
    const g = (window as unknown as W).__kilnGrid as GridHandle;
    await g.client.execute(g.doc, { Grid: c });
  }, command);
}

/** Runs a command through the window's registry, as the ribbon does. */
export async function run(page: Page, id: string, args?: unknown): Promise<string> {
  return page.evaluate(
    async ([i, a]) => (await (window as unknown as W).__kiln?.registry.execute(i as string, a))?.status ?? '',
    [id, args] as const,
  );
}

export async function select(page: Page, r0: number, c0: number, r1 = r0, c1 = c0): Promise<void> {
  await core(page, { SelectRange: { range: { r0, c0, r1, c1 }, add: false } });
}

/** Types rows of entries from `(r0, c0)` through the core (each one committed as typed). */
export async function fill(page: Page, rows: readonly (readonly string[])[], r0 = 0, c0 = 0): Promise<void> {
  await page.evaluate(
    async ({ rows, r0, c0 }) => {
      const g = (window as unknown as W).__kilnGrid as GridHandle;
      for (let r = 0; r < rows.length; r++) {
        const row = rows[r] ?? [];
        for (let c = 0; c < row.length; c++) {
          const text = row[c] ?? '';
          if (!text) continue;
          await g.client.execute(g.doc, {
            Grid: { SelectCell: { row: r0 + r, col: c0 + c, extend: false, add: false } },
          });
          await g.client.execute(g.doc, { Grid: { BeginEdit: { text } } });
          await g.client.execute(g.doc, { Grid: { CommitEdit: { advance: null } } });
        }
      }
    },
    { rows, r0, c0 },
  );
}

/** The displayed values of a range as tab-separated text. */
export async function values(page: Page, r0: number, c0: number, r1: number, c1: number): Promise<string> {
  await select(page, r0, c0, r1, c1);
  return page.evaluate(async () => {
    const g = (window as unknown as W).__kilnGrid as GridHandle;
    const r = (await g.client.query(g.doc, 'SelectedText')) as { Text?: string };
    return (r.Text ?? '').replace(/\r\n/g, '\n').replace(/\n$/, '');
  });
}

/** One cell's displayed value. */
export async function value(page: Page, row: number, col: number): Promise<string> {
  return values(page, row, col, row, col);
}

/** Polls the core's state until `check` holds. */
export async function until(page: Page, check: (s: GridState) => boolean, what: string): Promise<GridState> {
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
export async function origin(page: Page): Promise<{ x: number; y: number }> {
  const box = await page.locator('[data-kiln-grid-surface]').boundingBox();
  if (!box) throw new Error('no grid surface');
  return { x: box.x, y: box.y };
}

/** A point of a cell in page coordinates (default 64 × 20 px cells, no scroll). */
export async function cellPoint(
  page: Page,
  row: number,
  col: number,
  dx = 0.5,
  dy = 0.5,
): Promise<{ x: number; y: number }> {
  const o = await origin(page);
  const s = await gridState(page);
  return { x: o.x + s.layout.header_w + 64 * (col + dx), y: o.y + s.layout.header_h + 20 * (row + dy) };
}

/** Clicks a ribbon tab. */
export async function tab(page: Page, name: string): Promise<void> {
  await page.getByRole('tab', { name, exact: true }).click();
}

/** Clicks a menu path from a ribbon button (`['Data Bars', 'Blue Data Bar']`). */
export async function menu(
  page: Page,
  button: string | RegExp,
  path: readonly (string | RegExp)[],
): Promise<void> {
  await page.getByRole('button', { name: button }).first().click();
  for (const item of path) {
    const exact = typeof item === 'string';
    const m = page
      .getByRole('menuitem', { name: item, exact })
      .or(page.getByRole('menuitemcheckbox', { name: item, exact }))
      .first();
    await m.hover();
    await m.click();
  }
}

/** The keys are back on the sheet (its input proxy has the focus). */
export async function expectSheetFocused(page: Page): Promise<void> {
  await expect
    .poll(() => page.evaluate(() => document.activeElement?.hasAttribute('data-kiln-grid-proxy') ?? false), {
      message: 'the keys are back on the sheet',
      timeout: 5_000,
    })
    .toBe(true);
}

/** Item, Category, Amount and six rows, at A1:C7. */
export const BUDGET: readonly (readonly string[])[] = [
  ['Item', 'Category', 'Amount'],
  ['Clay', 'Materials', '420'],
  ['Glaze', 'Materials', '310'],
  ['Gas', 'Firing', '1180'],
  ['Shelves', 'Equipment', '0'],
  ['Open day', 'Events', '380'],
  ['Kiln hire', 'Firing', '640'],
];

/** Counts pixels of a screenshot region close to an RGB colour. */
export async function countColor(
  page: Page,
  clip: { x: number; y: number; width: number; height: number },
  rgb: [number, number, number],
  tolerance = 40,
): Promise<number> {
  const png = await page.screenshot({ clip });
  return page.evaluate(
    async ({ b64, rgb, tolerance }) => {
      const img = new Image();
      img.src = `data:image/png;base64,${b64}`;
      await img.decode();
      const c = document.createElement('canvas');
      c.width = img.width;
      c.height = img.height;
      const ctx = c.getContext('2d');
      if (!ctx) return 0;
      ctx.drawImage(img, 0, 0);
      const d = ctx.getImageData(0, 0, c.width, c.height).data;
      let n = 0;
      for (let i = 0; i < d.length; i += 4) {
        if (
          Math.abs((d[i] ?? 0) - rgb[0]) +
            Math.abs((d[i + 1] ?? 0) - rgb[1]) +
            Math.abs((d[i + 2] ?? 0) - rgb[2]) <
          tolerance
        )
          n++;
      }
      return n;
    },
    { b64: png.toString('base64'), rgb, tolerance },
  );
}

// ── files ───────────────────────────────────────────────────────────────────────────────────

/** Opens a file through File ▸ Open ▸ Browse (the file input the picker falls back to). */
export async function openFile(page: Page, file: { name: string; buffer: Buffer }): Promise<void> {
  await page.goto('/#/home');
  await expect(
    page.getByText('Your recent files will gather here').or(page.locator('.kiln-home__table')).first(),
  ).toBeVisible({ timeout: 45_000 });
  await noFilePickers(page);
  const [chooser] = await Promise.all([page.waitForEvent('filechooser'), page.keyboard.press('Control+o')]);
  await chooser.setFiles({ mimeType: 'application/octet-stream', ...file });
  await page.waitForFunction(
    () =>
      !!(window as unknown as W).__kilnGrid &&
      Number(document.querySelector<HTMLElement>('[data-kiln-grid-surface]')?.dataset.kilnFrames ?? 0) > 0,
    null,
    { timeout: 60_000 },
  );
  await noFilePickers(page);
}

export async function noFilePickers(page: Page): Promise<void> {
  await page.evaluate(() => {
    // biome-ignore lint/suspicious/noExplicitAny: the API is not in the DOM typings
    const w = window as any;
    delete w.showOpenFilePicker;
    delete w.showSaveFilePicker;
  });
}

export async function clearSaved(page: Page): Promise<void> {
  await page.evaluate(async () => {
    const root = await navigator.storage.getDirectory();
    await root.removeEntry('documents', { recursive: true }).catch(() => undefined);
  });
}

/** The bytes of a saved file (`documents/<name>` in the origin's private file system), complete. */
export async function savedBytes(page: Page, name: string): Promise<Buffer> {
  let bytes: number[] | null = null;
  await expect
    .poll(
      async () => {
        bytes = await page.evaluate(async (n) => {
          try {
            const root = await navigator.storage.getDirectory();
            const dir = await root.getDirectoryHandle('documents');
            const file = await (await dir.getFileHandle(n)).getFile();
            return Array.from(new Uint8Array(await file.arrayBuffer()));
          } catch {
            return null;
          }
        }, name);
        return bytes !== null && Buffer.from(bytes).lastIndexOf(Buffer.from([0x50, 0x4b, 0x05, 0x06])) > 0;
      },
      { message: `${name} is saved`, timeout: 30_000 },
    )
    .toBe(true);
  return Buffer.from(bytes ?? []);
}
