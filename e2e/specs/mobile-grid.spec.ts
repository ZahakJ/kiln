/**
 * Kiln Grid on a phone and a tablet, run by the `pixel-7` and `ipad`
 * projects with touch: a finger pans and taps, the range handles extend the selection, the
 * floating context toolbar and the long-press menu, pinch and double-tap zoom, the formula bar
 * pinned on the on-screen keyboard with ✕, ✓ and the 123 / fx / A→Z quick keys, the sheet chips
 * with the selection's statistics, and the AutoFilter menu and the pick list as bottom sheets.
 * Screenshots go to e2e/screenshots/mobile/.
 */
import { fileURLToPath } from 'node:url';
import { expect, type Page, test } from '@playwright/test';
import { expectNoSeriousA11yViolations } from '../../packages/testing/src/playwright/a11y.ts';
import { BUDGET, cellPoint, fill, gridState, run, select, value } from './grid-helpers.ts';
import { doubleTap, drag, longPress, menusOffScreen, pinch } from './touch.ts';

const shot = (name: string) => fileURLToPath(new URL(`../screenshots/mobile/${name}`, import.meta.url));
const isPhone = (page: Page) => (page.viewportSize()?.width ?? 1440) < 640;

async function snap(page: Page, name: string): Promise<void> {
  await page.waitForTimeout(400);
  await page.screenshot({ path: shot(name), scale: 'css', animations: 'disabled', caret: 'hide' });
}

/** A new workbook with the budget at A1:C7 (default 64 × 20 px cells). */
async function openBudget(page: Page): Promise<void> {
  await page.goto(`/#/grid?blink=0&new=${Date.now()}`);
  await page.waitForFunction(
    () =>
      !!(window as unknown as { __kilnGrid?: unknown }).__kilnGrid &&
      Number(document.querySelector<HTMLElement>('[data-kiln-grid-surface]')?.dataset.kilnFrames ?? 0) > 0,
    null,
    { timeout: 60_000 },
  );
  await fill(page, BUDGET);
  await select(page, 0, 0);
  await page.evaluate(() => document.fonts.ready);
}

/** The selection's last range as `r0,c0,r1,c1`. */
const selectedRange = (page: Page) =>
  page.evaluate(async () => {
    const g = (
      window as unknown as {
        __kilnGrid: { doc: number; client: { query(d: number, q: unknown): Promise<unknown> } };
      }
    ).__kilnGrid;
    const r = (await g.client.query(g.doc, 'Selection')) as {
      Selection: {
        kind: {
          Cells?: { ranges: { start: { row: number; col: number }; end: { row: number; col: number } }[] };
        };
      };
    };
    const x = r.Selection.kind.Cells?.ranges.at(-1);
    return x ? [x.start.row, x.start.col, x.end.row, x.end.col].join(',') : '';
  });

const zoom = (page: Page) =>
  page.evaluate(
    () =>
      (
        window as unknown as { __kilnGridSurface?: { getZoom(): number } | null }
      ).__kilnGridSurface?.getZoom() ?? 0,
  );

/** The visual viewport replaced by one whose height a test sets (the on-screen keyboard). */
async function emulateKeyboard(page: Page): Promise<void> {
  await page.addInitScript(() => {
    const target = new EventTarget();
    const vv = {
      height: window.innerHeight,
      width: window.innerWidth,
      offsetTop: 0,
      offsetLeft: 0,
      pageTop: 0,
      pageLeft: 0,
      scale: 1,
      onresize: null,
      onscroll: null,
      addEventListener: target.addEventListener.bind(target),
      removeEventListener: target.removeEventListener.bind(target),
      dispatchEvent: target.dispatchEvent.bind(target),
    };
    Object.defineProperty(window, 'visualViewport', { configurable: true, get: () => vv });
    (window as unknown as { __keyboard(h: number): void }).__keyboard = (h: number) => {
      vv.height = window.innerHeight - h;
      target.dispatchEvent(new Event('resize'));
    };
  });
}

test.describe('mobile grid', () => {
  test('mobile grid: a tap selects, the range handles extend it, the toolbar and the long-press menu act on it', async ({
    page,
  }) => {
    await openBudget(page);
    // A finger's tap selects B2, and the two range handles sit at its corners.
    const b2 = await cellPoint(page, 1, 1);
    await page.touchscreen.tap(b2.x, b2.y);
    await expect.poll(async () => (await gridState(page)).name_box).toBe('B2');
    const end = page.getByTestId('grid-range-handle-end');
    const start = page.getByTestId('grid-range-handle-start');
    await expect(end).toBeVisible();
    await expect(start).toBeVisible();
    const hb = await end.boundingBox();
    expect(hb && hb.width >= 44 && hb.height >= 44).toBe(true);
    // The tap gave the keys to the sheet without the on-screen keyboard.
    await expect(page.locator('[data-kiln-grid-proxy]')).toHaveAttribute('inputmode', 'none');
    // Dragging the end handle to C5 selects B2:C5.
    const c5 = await cellPoint(page, 4, 2);
    const from = { x: (hb?.x ?? 0) + 22, y: (hb?.y ?? 0) + 22 };
    await drag(page, from, { x: c5.x + 10, y: c5.y + 12 }, 10);
    await expect.poll(() => selectedRange(page)).toBe('1,1,4,2');
    await expect.poll(async () => (await gridState(page)).name_box).toBe('B2');
    // The floating context toolbar shows over the range: Copy, Paste, More…
    const bar = page.getByTestId('context-toolbar');
    await expect(bar).toBeVisible();
    await expect(bar).toHaveAttribute('data-kind', 'cells');
    for (const id of ['context-cut', 'context-copy', 'context-paste', 'context-more'])
      await expect(bar.getByTestId(id)).toBeVisible();
    await snap(page, `grid-handles-light-${isPhone(page) ? 'phone' : 'tablet'}.png`);
    await expectNoSeriousA11yViolations(page);
    // Its statistics: the chip by the sheet chips on a phone.
    if (isPhone(page)) {
      const stats = page.getByTestId('grid-phone-stats');
      await expect(stats).toContainText('Sum');
      await expect(stats).toContainText('1910');
    }
    // More opens the cell menu (the right-click menu's commands).
    await bar.getByTestId('context-more').tap();
    const menu = page.getByRole('menu');
    await expect(menu).toBeVisible();
    await expect(menu.getByRole('menuitem', { name: /^Insert/ }).first()).toBeVisible();
    await page.keyboard.press('Escape');
    await expect(menu).toBeHidden();
    // A long press on D9 selects it and opens the same menu there.
    const d9 = await cellPoint(page, 8, 3);
    await longPress(page, d9);
    await expect(page.getByRole('menu')).toBeVisible();
    await expect.poll(async () => (await gridState(page)).name_box).toBe('D9');
    // The whole menu is on the screen, near its end edge too.
    await expect.poll(() => menusOffScreen(page)).toEqual([]);
    await snap(page, `grid-longpress-light-${isPhone(page) ? 'phone' : 'tablet'}.png`);
    await page.keyboard.press('Escape');
  });

  test('mobile grid: pinch zooms the sheet, a double tap fits the columns in use to the width', async ({
    page,
  }) => {
    await openBudget(page);
    const before = await zoom(page);
    expect(before).toBeGreaterThan(0);
    const box = await page.locator('[data-kiln-grid-surface]').boundingBox();
    if (!box) throw new Error('no sheet');
    const center = { x: box.x + box.width / 2, y: box.y + box.height / 2 };
    await pinch(page, center, 80, 200);
    await expect.poll(() => zoom(page)).toBeGreaterThan(before * 1.8);
    const zoomed = await zoom(page);
    await pinch(page, center, 220, 110);
    await expect.poll(() => zoom(page)).toBeLessThan(zoomed * 0.7);
    // A double tap: columns A to C fit the width; again: closer.
    await expect.poll(() => zoom(page)).toBeLessThan(zoomed * 0.7);
    const pinched = await zoom(page);
    await doubleTap(page, center);
    await expect.poll(() => zoom(page)).not.toBe(pinched);
    await page.waitForTimeout(600);
    const fit = await zoom(page);
    // Columns A to C (and the row headers) fill the width.
    const right = await page.evaluate(() => {
      const s = document.querySelector('[data-kiln-grid-surface]')?.getBoundingClientRect();
      return s?.width ?? 0;
    });
    expect(fit).toBeGreaterThan(Math.min(2, right / (60 + 3 * 64)) - 0.1);
    // Away from the range handles of the cell the first double tap selected.
    await doubleTap(page, { x: box.x + box.width * 0.85, y: box.y + box.height * 0.85 });
    await expect.poll(() => zoom(page)).toBeCloseTo(Math.min(4, fit * 2), 1);
  });

  test('mobile grid: the formula bar sits on the on-screen keyboard with ✕, ✓ and the quick keys', async ({
    page,
  }) => {
    test.skip(!isPhone(page), 'the phone layout');
    await emulateKeyboard(page);
    await openBudget(page);
    const e2 = await cellPoint(page, 1, 4);
    await page.touchscreen.tap(e2.x, e2.y);
    await expect.poll(async () => (await gridState(page)).name_box).toBe('E2');
    // Tapping the formula bar edits the cell; the keyboard comes up.
    await page.getByRole('textbox', { name: 'Formula Bar' }).tap();
    const inner = await page.evaluate(() => window.innerHeight);
    const keyboard = 320;
    await page.evaluate(
      (h) => (window as unknown as { __keyboard(h: number): void }).__keyboard(h),
      keyboard,
    );
    await expect(page.locator('.kiln-appframe')).toHaveAttribute('data-keyboard', 'open');
    const quick = page.getByTestId('grid-quick-keys');
    await expect(quick).toBeVisible();
    // The quick keys sit on the keyboard's top edge, the formula bar on them.
    await expect
      .poll(async () => {
        const b = await quick.boundingBox();
        return b ? Math.round(b.y + b.height) : -1;
      })
      .toBe(inner - keyboard);
    const bar = page.getByRole('toolbar', { name: 'Formula bar' });
    const barBox = await bar.boundingBox();
    const quickBox = await quick.boundingBox();
    expect(barBox && quickBox && Math.abs(barBox.y + barBox.height - quickBox.y) <= 2).toBe(true);
    // The bottom toolbar and the sheet chips make way.
    await expect(page.getByTestId('bottom-toolbar')).toBeHidden();
    // Typing with the quick keys: =SUM(C2:C7), then ✓.
    await page.getByRole('button', { name: 'Type =' }).tap();
    await page.keyboard.type('SUM');
    await page.getByRole('button', { name: 'Type (' }).tap();
    await page.keyboard.type('C2');
    await page.getByRole('button', { name: 'Type :' }).tap();
    await page.keyboard.type('C7');
    await page.getByRole('button', { name: 'Type )' }).tap();
    await expect(bar.getByRole('button', { name: 'Enter' })).toBeVisible();
    await expect(bar.getByRole('button', { name: 'Cancel' })).toBeVisible();
    // 123 brings the number keys.
    await quick.getByTestId('grid-quick-123').tap();
    await expect(page.getByRole('textbox', { name: 'Formula Bar' })).toHaveAttribute('inputmode', 'decimal');
    await expect(quick.getByTestId('grid-quick-123')).toHaveAttribute('aria-pressed', 'true');
    await snap(page, 'grid-keyboard-light-phone.png');
    await expectNoSeriousA11yViolations(page);
    await bar.getByRole('button', { name: 'Enter' }).tap();
    await expect.poll(() => value(page, 1, 4)).toBe('2930');
    await page.evaluate(() => (window as unknown as { __keyboard(h: number): void }).__keyboard(0));
    await expect(page.getByTestId('bottom-toolbar')).toBeVisible();
  });

  test('mobile grid: sheet chips above the toolbar; the selection statistics in a sheet', async ({
    page,
  }) => {
    test.skip(!isPhone(page), 'the phone layout');
    await openBudget(page);
    const chips = page.locator('.kiln-grid-phone__tabs');
    await expect(chips).toBeVisible();
    // The browser zooms the chrome (low vision); only the sheet zooms itself.
    const touchAction = (sel: string) =>
      page
        .locator(sel)
        .first()
        .evaluate((el) => getComputedStyle(el).touchAction);
    for (const sel of [
      '.kiln-appbar',
      '.kiln-bottombar',
      '.kiln-formulabar',
      '.kiln-grid-phone__tabs',
      '.kiln-appframe',
    ])
      expect(await touchAction(sel), sel).toBe('auto');
    expect(await touchAction('.kiln-grid-surface__scroller')).toBe('pan-x pan-y');
    // The chip strip sits right above the bottom toolbar.
    const strip = await chips.boundingBox();
    const toolbar = await page.getByTestId('bottom-toolbar').boundingBox();
    expect(strip && toolbar && Math.abs(strip.y + strip.height - toolbar.y) <= 2).toBe(true);
    await chips.getByRole('button', { name: 'New Sheet' }).tap();
    await expect.poll(async () => (await gridState(page)).sheets.length).toBe(2);
    await expect(chips.getByRole('tab', { name: 'Sheet2' })).toHaveAttribute('aria-selected', 'true');
    await chips.getByRole('tab', { name: 'Sheet1' }).tap();
    await expect.poll(async () => (await gridState(page)).active_sheet).toBe(0);
    // A range: its sum on a chip; tapped, every figure.
    await select(page, 1, 2, 6, 2);
    const stats = page.getByTestId('grid-phone-stats');
    await expect(stats).toContainText('2930');
    await stats.tap();
    const sheet = page.getByTestId('grid-stats-sheet');
    await expect(sheet).toBeVisible();
    for (const label of ['Sum', 'Average', 'Count', 'Minimum', 'Maximum'])
      await expect(sheet.getByText(label, { exact: true })).toBeVisible();
    await snap(page, 'grid-stats-light-phone.png');
    await expectNoSeriousA11yViolations(page);
    await page.keyboard.press('Escape');
    await expect(sheet).toBeHidden();
  });

  test('mobile grid: the AutoFilter menu and the pick list are bottom sheets on a phone', async ({
    page,
  }) => {
    test.skip(!isPhone(page), 'the phone layout');
    await openBudget(page);
    await select(page, 0, 0, 6, 2);
    expect(await run(page, 'grid.data.toggle_filter')).toBe('ran');
    await expect.poll(async () => (await gridState(page)).features.filter).toBe(true);
    await select(page, 0, 1);
    await page.locator('[data-kiln-grid-proxy]').focus();
    await page.keyboard.press('Alt+ArrowDown');
    const sheet = page.getByTestId('grid-filter-sheet');
    await expect(sheet).toBeVisible();
    await expect(sheet).toHaveAttribute('role', 'dialog');
    await expect(sheet.getByTestId('grid-filter-value').first()).toBeVisible();
    // The sheet stands on the bottom edge, the window's width.
    await page.waitForTimeout(400);
    const box = await sheet.boundingBox();
    const size = page.viewportSize();
    expect(
      box && size && Math.abs(box.y + box.height - size.height) <= 2 && box.width >= size.width - 1,
    ).toBe(true);
    await snap(page, 'grid-filter-light-phone.png');
    await expectNoSeriousA11yViolations(page);
    // Unticking Firing filters it out.
    await sheet.getByTestId('grid-filter-value').filter({ hasText: 'Firing' }).locator('input').tap();
    await sheet.getByTestId('grid-filter-ok').tap();
    await expect(sheet).toBeHidden();
    await expect.poll(async () => (await gridState(page)).features.filtered).toBe(true);

    // The pick list of a column's values (Alt+Down under them).
    await page.goto('about:blank');
    await openBudget(page);
    await select(page, 7, 1);
    await page.locator('[data-kiln-grid-proxy]').focus();
    await page.keyboard.press('Alt+ArrowDown');
    const pick = page.getByTestId('grid-pick-sheet');
    await expect(pick).toBeVisible();
    await expect(pick.getByRole('button', { name: 'Firing' })).toBeVisible();
    await snap(page, 'grid-picklist-light-phone.png');
    await pick.getByRole('button', { name: 'Firing' }).tap();
    await expect(pick).toBeHidden();
    await expect.poll(() => value(page, 7, 1)).toBe('Firing');
    // Opened again after a pick, the list closes without picking: ✕, Escape and Back.
    const reopen = async () => {
      await select(page, 7, 1);
      await page.locator('[data-kiln-grid-proxy]').focus();
      await page.keyboard.press('Alt+ArrowDown');
      await expect(pick).toBeVisible();
    };
    await reopen();
    await pick.getByRole('button', { name: 'Close' }).tap();
    await expect(pick).toBeHidden();
    await expect.poll(() => value(page, 7, 1)).toBe('Firing');
    await reopen();
    await page.keyboard.press('Escape');
    await expect(pick).toBeHidden();
    await expect.poll(() => value(page, 7, 1)).toBe('Firing');
    await reopen();
    await page.evaluate(() => history.back());
    await expect(pick).toBeHidden();
    // Back closed the list, not the workbook.
    await expect(page.getByTestId('grid-editor')).toBeVisible();
    await expect.poll(() => value(page, 7, 1)).toBe('Firing');
  });
});
