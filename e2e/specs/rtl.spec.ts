/**
 * Right-to-left documents and sheets: Arabic text in Kiln
 * Write with the caret moving in logical order and drawn where the position shows (two carets
 * where the directions meet), clicks and Shift selection on right-to-left text; and a
 * right-to-left sheet in Kiln Grid, drawn mirrored with column A and the row headers at the
 * right, clicked and moved through with the arrows as in Excel, and switched with Page Layout ▸
 * Sheet Right-to-Left.
 */
import { readFileSync } from 'node:fs';
import { fileURLToPath } from 'node:url';
import { expect, type Page, test } from '@playwright/test';
import {
  ask,
  gridState,
  openFile as openGridFile,
  origin,
  run as runGrid,
  select as selectCells,
  until as untilGrid,
} from './grid-helpers.ts';

const repo = (p: string) => fileURLToPath(new URL(`../../${p}`, import.meta.url));
const shot = (name: string) => repo(`e2e/screenshots/rtl/${name}`);

type Rect = { x: number; y: number; width: number; height: number };
interface WriteHandle {
  doc: number;
  client: {
    query(doc: number, q: unknown): Promise<Record<string, unknown>>;
    execute(doc: number, c: unknown): Promise<unknown>;
  };
  controller: {
    getStats(): { tiles: number };
    clientRectOf(r: Rect): { left: number; top: number; width: number; height: number };
  } | null;
}
type KilnWindow = { __kilnWrite?: WriteHandle };

test.describe('RTL documents and sheets', () => {
  async function openWriteFile(page: Page, name: string): Promise<void> {
    await page.goto('/#/home');
    await expect(
      page.getByText('Your recent files will gather here').or(page.locator('.kiln-home__table')).first(),
    ).toBeVisible({ timeout: 45_000 });
    await page.evaluate(() => {
      // biome-ignore lint/suspicious/noExplicitAny: the API is not in the DOM typings
      const w = window as any;
      delete w.showOpenFilePicker;
      delete w.showSaveFilePicker;
    });
    const [chooser] = await Promise.all([page.waitForEvent('filechooser'), page.keyboard.press('Control+o')]);
    await chooser.setFiles({
      name,
      mimeType: 'application/octet-stream',
      buffer: readFileSync(repo(`fixtures/docx/rtl/${name}`)),
    });
    await page.waitForFunction(
      () => ((window as unknown as KilnWindow).__kilnWrite?.controller?.getStats().tiles ?? 0) > 0,
      null,
      { timeout: 45_000 },
    );
  }

  /** Puts the caret at a paragraph offset and returns its rectangle on the screen. */
  async function caretAt(page: Page, para: number, offset: number): Promise<Rect> {
    return page.evaluate(
      async ([p, o]) => {
        const w = (window as unknown as KilnWindow).__kilnWrite as WriteHandle;
        const r = (await w.client.query(w.doc, 'Paragraphs')) as { Paragraphs: { id: number }[] };
        const block = r.Paragraphs[p as number]?.id ?? 0;
        await w.client.execute(w.doc, { Write: { SelectTo: { pos: { block, offset: o }, extend: false } } });
        const s = (await w.client.query(w.doc, 'Selection')) as { Selection: { caret: { rect: Rect } } };
        const c = w.controller?.clientRectOf(s.Selection.caret.rect);
        return c ? { x: c.left, y: c.top, width: c.width, height: c.height } : s.Selection.caret.rect;
      },
      [para, offset] as const,
    );
  }

  async function head(page: Page): Promise<{ anchor: number; head: number; caret: Rect }> {
    return page.evaluate(async () => {
      const w = (window as unknown as KilnWindow).__kilnWrite as WriteHandle;
      const s = (await w.client.query(w.doc, 'Selection')) as {
        Selection: {
          kind: { Text: { anchor: { offset: number }; head: { offset: number } } };
          caret: { rect: Rect } | null;
        };
      };
      const t = s.Selection.kind.Text;
      const r = s.Selection.caret?.rect ?? { x: 0, y: 0, width: 0, height: 0 };
      const c = w.controller?.clientRectOf(r);
      return {
        anchor: t.anchor.offset,
        head: t.head.offset,
        caret: c ? { x: c.left, y: c.top, width: c.width, height: c.height } : r,
      };
    });
  }

  async function paragraph(page: Page, i: number): Promise<string> {
    return page.evaluate(async (n) => {
      const w = (window as unknown as KilnWindow).__kilnWrite as WriteHandle;
      const r = (await w.client.query(w.doc, 'Paragraphs')) as { Paragraphs: { text: string }[] };
      return r.Paragraphs[n]?.text ?? '';
    }, i);
  }

  test('Write: the caret moves in logical order through mixed Arabic and Latin text', async ({ page }) => {
    await openWriteFile(page, 'mixed-runs.docx');
    // "يستخدم الفريق Rust وTypeScript …" (right to left, "Rust" at 14..18).
    const start = await caretAt(page, 2, 0);
    const boundary = await caretAt(page, 2, 14);
    expect(start.x).toBeGreaterThan(boundary.x);
    await page.getByTestId('kiln-input-proxy').focus();
    // Left goes on in the text; inside "Rust" that is to the right on the screen.
    await page.keyboard.press('ArrowLeft');
    await expect.poll(async () => (await head(page)).head).toBe(15);
    const inRust = (await head(page)).caret;
    await page.keyboard.press('ArrowLeft');
    await expect.poll(async () => (await head(page)).head).toBe(16);
    expect((await head(page)).caret.x).toBeGreaterThan(inRust.x);
    // Shift+Right extends back across the direction change into the Arabic.
    for (let i = 0; i < 4; i++) await page.keyboard.press('Shift+ArrowRight');
    await expect.poll(async () => head(page)).toMatchObject({ anchor: 16, head: 12 });
    // A click on the right half of the paragraph's first letter puts the caret before it.
    const first = await caretAt(page, 2, 0);
    const second = await caretAt(page, 2, 1);
    await page.mouse.click(first.x - (first.x - second.x) * 0.25, first.y + first.height / 2);
    await expect.poll(async () => (await head(page)).head).toBe(0);
    // Backspace at the boundary deletes the space before "Rust" (the logical neighbour).
    await caretAt(page, 2, 14);
    await page.getByTestId('kiln-input-proxy').focus();
    await page.keyboard.press('Backspace');
    await expect.poll(() => paragraph(page, 2)).toContain('الفريقRust');
    await page.keyboard.press('Control+z');
    await expect.poll(() => paragraph(page, 2)).toContain('الفريق Rust');
    await caretAt(page, 2, 14);
    await page.mouse.move(2, 600);
    await page.screenshot({ path: shot('write-mixed-runs-light-desktop.png') });
  });

  test('Write: Right-to-Left Text Direction turns a paragraph round', async ({ page }) => {
    await openWriteFile(page, 'mixed-runs.docx');
    // "The word كيلن means kiln…" runs left to right from the left margin.
    const ltr = await caretAt(page, 1, 0);
    const direction = async (name: string) => {
      await page.getByRole('tab', { name: 'Home', exact: true }).click();
      await page
        .getByRole('button', { name: /Text Direction/ })
        .first()
        .click();
      await page
        .getByRole('menuitem', { name, exact: true })
        .or(page.getByRole('menuitemcheckbox', { name, exact: true }))
        .first()
        .click();
    };
    await direction('Right-to-Left Text Direction');
    await expect.poll(async () => (await caretAt(page, 1, 0)).x).toBeGreaterThan(ltr.x + 300);
    await direction('Left-to-Right Text Direction');
    await expect.poll(async () => (await caretAt(page, 1, 0)).x).toBeLessThan(ltr.x + 2);
  });

  test('Grid: a right-to-left sheet is mirrored, clicked and moved through as in Excel', async ({ page }) => {
    await openGridFile(page, {
      name: 'sales.xlsx',
      buffer: readFileSync(repo('fixtures/xlsx/rtl/sales.xlsx')),
    });
    const s = await gridState(page);
    expect((s.layout as unknown as { rtl: boolean }).rtl).toBe(true);
    const o = await origin(page);
    const box = await page.locator('[data-kiln-grid-surface]').boundingBox();
    if (!box) throw new Error('no surface');
    const right = o.x + box.width;
    const { header_w: hw, header_h: hh } = s.layout;
    // The row numbers are at the right edge: a click there selects the row.
    await page.mouse.click(right - hw / 2, o.y + hh + 10);
    await untilGrid(page, (g) => g.name_box === '1:1' || g.name_box.startsWith('A1'), 'row 1 selected');
    // Just left of them is column A.
    await page.mouse.click(right - hw - 10, o.y + hh + 60);
    await untilGrid(page, (g) => g.name_box.startsWith('A'), 'a cell of column A');
    // The hit test answers through the mirror.
    const hit = await ask<{ Cell?: { col: number } }>(page, {
      HitTest: { x: box.width - hw - 10, y: hh + 60, coarse: false },
    });
    expect(hit.Cell?.col).toBe(0);
    // Right goes towards column A, Left away from it.
    await selectCells(page, 4, 2);
    await page.locator('[data-kiln-grid-proxy]').focus();
    await page.keyboard.press('ArrowRight');
    await untilGrid(page, (g) => g.name_box === 'B5', 'Right moved to B5');
    await page.keyboard.press('ArrowLeft');
    await page.keyboard.press('ArrowLeft');
    await untilGrid(page, (g) => g.name_box === 'D5', 'Left moved to D5');
    await page.waitForTimeout(300);
    await page.mouse.move(2, 2);
    await page.screenshot({ path: shot('grid-sales-rtl-light-desktop.png') });
    // Page Layout ▸ Sheet Right-to-Left turns it off: column A is back at the left.
    await page.getByRole('tab', { name: 'Page Layout', exact: true }).click();
    await page
      .getByRole('button', { name: /Sheet Right-to-Left/ })
      .first()
      .click();
    await untilGrid(page, (g) => !(g.layout as unknown as { rtl: boolean }).rtl, 'left to right');
    const ltr = await ask<{ Cell?: { col: number } }>(page, {
      HitTest: { x: hw + 10, y: hh + 60, coarse: false },
    });
    expect(ltr.Cell?.col).toBe(0);
    expect(await runGrid(page, 'grid.page_layout.sheet_right_to_left')).toBe('ran');
    await untilGrid(page, (g) => (g.layout as unknown as { rtl: boolean }).rtl, 'right to left again');
    await page.emulateMedia({ colorScheme: 'dark' });
    await page.waitForTimeout(500);
    await page.mouse.move(2, 2);
    await page.screenshot({ path: shot('grid-sales-rtl-dark-desktop.png') });
  });
});
