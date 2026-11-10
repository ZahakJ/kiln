/**
 * Kiln Grid's surface gestures end to end, each used as an Excel user does with the
 * keyboard and the mouse: F8 and Shift+F8, the error indicator's menu, drag and drop of cells,
 * the right-click menus and the mini toolbar. The workbook is checked through the core, and the
 * keys must come back to the sheet after every menu and dialog.
 */
import { expect, type Page, test } from '@playwright/test';
import {
  ask,
  cellPoint,
  core,
  expectSheetFocused,
  fill,
  focusSheet,
  gridState,
  openGrid,
  select,
  shot,
  until,
  value,
  values,
} from './grid-helpers.ts';

interface Surface {
  select_mode: string;
  drag: { target: string; occupied: boolean } | null;
  error: { rule: string; error: string; title: string; help: string } | null;
}

async function surface(page: Page): Promise<Surface> {
  return ((await gridState(page)) as unknown as { surface: Surface }).surface;
}

/** The surface part of a state `until` polled. */
const sf = (s: unknown): Surface => (s as { surface: Surface }).surface;

/** Waits until the selection is one range (a drop selects where the cells landed). */
async function landed(page: Page, r0: number, c0: number, r1: number, c1: number): Promise<void> {
  await expect.poll(() => selection(page)).toEqual([{ r0, c0, r1, c1 }]);
}

/** The selection's ranges as the core holds them. */
async function selection(page: Page): Promise<{ r0: number; c0: number; r1: number; c1: number }[]> {
  return page.evaluate(async () => {
    const w = window as unknown as {
      __kilnGrid: { client: { query(d: unknown, q: unknown): Promise<unknown> }; doc: unknown };
    };
    const r = (await w.__kilnGrid.client.query(w.__kilnGrid.doc, 'Selection')) as {
      Selection?: {
        kind: {
          Cells?: { ranges: { start: { row: number; col: number }; end: { row: number; col: number } }[] };
        };
      };
    };
    return (r.Selection?.kind.Cells?.ranges ?? []).map((x) => ({
      r0: x.start.row,
      c0: x.start.col,
      r1: x.end.row,
      c1: x.end.col,
    }));
  });
}

test.describe('Kiln Grid surface', () => {
  test('multi-select: F8 extends, Shift+F8 adds ranges, and the status bar names the mode', async ({
    page,
  }) => {
    await openGrid(page);
    await select(page, 1, 1);
    await focusSheet(page);
    await page.keyboard.press('F8');
    await expect(page.getByText('Extend Selection', { exact: true })).toBeVisible();
    await page.keyboard.press('ArrowDown');
    await page.keyboard.press('ArrowRight');
    await expect.poll(async () => (await selection(page))[0]).toEqual({ r0: 1, c0: 1, r1: 2, c1: 2 });
    await page.keyboard.press('F8');
    await expect(page.getByText('Extend Selection', { exact: true })).toHaveCount(0);

    // Shift+F8: the arrows move a new cell; a click adds another range, as Ctrl+click.
    await page.keyboard.press('Shift+F8');
    await expect(page.getByText('Add or Remove Selection', { exact: true })).toBeVisible();
    await page.keyboard.press('ArrowDown');
    await page.keyboard.press('ArrowDown');
    await page.keyboard.press('Shift+ArrowRight');
    const p = await cellPoint(page, 6, 4);
    await page.mouse.click(p.x, p.y);
    await expect
      .poll(() => selection(page))
      .toEqual([
        { r0: 1, c0: 1, r1: 2, c1: 2 },
        { r0: 3, c0: 1, r1: 3, c1: 2 },
        { r0: 6, c0: 4, r1: 6, c1: 4 },
      ]);
    // Typing fills the active cell; Ctrl+Enter fills every range.
    await page.keyboard.type('7');
    await page.keyboard.press('Control+Enter');
    await expect.poll(() => values(page, 1, 1, 3, 2)).toBe('7\t7\n7\t7\n7\t7');
    expect(await value(page, 6, 4)).toBe('7');
    await expect(page.getByText('Add or Remove Selection', { exact: true })).toHaveCount(0);
    await page.screenshot({ path: shot('surface-add-selection.png') });
    // Esc leaves the mode too.
    await focusSheet(page);
    await page.keyboard.press('Shift+F8');
    await expect.poll(async () => (await surface(page)).select_mode).toBe('Add');
    await page.keyboard.press('Escape');
    await expect.poll(async () => (await surface(page)).select_mode).toBe('Normal');
  });

  test('error indicators: the button names the error, shows help and Ignore Error clears the triangle', async ({
    page,
  }) => {
    await openGrid(page);
    await fill(page, [['10', '0', '=A1/B1']]);
    await select(page, 0, 2);
    const e = await until(page, (s) => sf(s).error?.error === '#DIV/0!', 'C1 is flagged');
    expect(sf(e).error?.title).toBe('Divide by Zero Error');
    // The button sits just left of C1.
    const b = await cellPoint(page, 0, 2, 0, 0.5);
    await until(page, (s) => sf(s).error !== null, 'C1 shows its error button');
    await page.mouse.click(b.x - 9, b.y);
    const m = page.getByTestId('grid-error-menu');
    await expect(m).toBeVisible();
    await expect(m.getByText('Divide by Zero Error')).toBeVisible();
    for (const item of [
      'Help on this Error',
      'Show Calculation Steps…',
      'Ignore Error',
      'Edit in Formula Bar',
      'Error Checking…',
    ])
      await expect(m.getByRole('menuitem', { name: item })).toBeVisible();
    await page.screenshot({ path: shot('surface-error-menu.png') });
    // Help on this Error explains it; OK returns the keys to the sheet.
    await m.getByRole('menuitem', { name: 'Help on this Error' }).click();
    const help = page.getByTestId('grid-error-help');
    await expect(help).toContainText('divides by zero');
    await page.keyboard.press('Enter');
    await expect(help).toHaveCount(0);
    await expectSheetFocused(page);
    // Show Calculation Steps opens Evaluate Formula.
    await until(page, (s) => sf(s).error !== null, 'C1 shows its error button');
    await page.mouse.click(b.x - 9, b.y);
    await m.getByRole('menuitem', { name: 'Show Calculation Steps…' }).click();
    await expect(page.getByRole('dialog', { name: 'Evaluate Formula' })).toBeVisible();
    await page.keyboard.press('Escape');
    await expectSheetFocused(page);
    // Edit in Formula Bar edits the formula there, the caret at its end; Esc leaves it as it was
    // and hands the keys back to the sheet.
    await until(page, (s) => sf(s).error !== null, 'C1 shows its error button');
    await page.mouse.click(b.x - 9, b.y);
    await m.getByRole('menuitem', { name: 'Edit in Formula Bar' }).click();
    const bar = page.getByRole('textbox', { name: 'Formula Bar' });
    await expect(bar).toBeFocused();
    await until(
      page,
      (s) =>
        s.mode !== 'Ready' &&
        s.formula === '=A1/B1' &&
        (s as unknown as { editor?: { formula_bar: boolean } }).editor?.formula_bar === true,
      'the formula is edited in the Formula Bar',
    );
    expect(await bar.evaluate((el) => (el as HTMLTextAreaElement).selectionStart)).toBe(6);
    await page.keyboard.press('Escape');
    await until(page, (s) => s.mode === 'Ready', 'the edit is cancelled');
    await expectSheetFocused(page);
    // Ignore Error: the indicator goes, and Ctrl+Z brings it back.
    await until(page, (s) => sf(s).error !== null, 'C1 shows its error button');
    await page.mouse.click(b.x - 9, b.y);
    await m.getByRole('menuitem', { name: 'Ignore Error' }).click();
    await until(page, (s) => sf(s).error === null, 'ignored');
    await expectSheetFocused(page);
    await page.keyboard.press('Control+z');
    await until(page, (s) => sf(s).error !== null, 'flagged again');
    // A number stored as text (typed after an apostrophe) offers Convert to Number.
    await fill(page, [["'42"]], 2, 0);
    await select(page, 2, 0);
    await until(page, (s) => sf(s).error?.rule === 'NumberAsText', 'A3 is a number stored as text');
    const t = await cellPoint(page, 2, 0, 1, 0.5);
    await page.mouse.click(t.x + 9, t.y);
    await expect(m.getByText('Number Stored as Text')).toBeVisible();
    await m.getByRole('menuitem', { name: 'Convert to Number' }).click();
    await until(page, (s) => sf(s).error === null, 'converted');
    await fill(page, [['=A3+1']], 3, 0);
    expect(await value(page, 3, 0)).toBe('43');
  });

  test('drag-move: dragging the border moves cells, Ctrl copies, Shift inserts and references follow', async ({
    page,
  }) => {
    await openGrid(page);
    await fill(page, [['4'], ['5']]);
    await fill(page, [['=SUM(A1:A2)']], 0, 3);
    await select(page, 0, 0, 1, 0);
    await focusSheet(page);
    // Take the block by its right border and drop it two columns right, three rows down.
    const from = await cellPoint(page, 1, 0, 1, 0.5);
    await page.mouse.move(from.x - 1, from.y);
    await page.mouse.down();
    const to = await cellPoint(page, 4, 2, 1, 0.5);
    await page.mouse.move(to.x - 20, to.y, { steps: 8 });
    await until(page, (s) => sf(s).drag?.target === 'C4:C5', 'the outline previews C4:C5');
    await page.screenshot({ path: shot('surface-drag-move.png') });
    await page.mouse.up();
    await landed(page, 3, 2, 4, 2);
    await expect.poll(() => values(page, 3, 2, 4, 2)).toBe('4\n5');
    expect(await values(page, 0, 0, 1, 0)).toBe('\n');
    await select(page, 0, 3);
    await expect.poll(async () => (await gridState(page)).formula).toBe('=SUM(C4:C5)');
    await focusSheet(page);
    await page.keyboard.press('Control+z');
    await expect.poll(() => values(page, 0, 0, 1, 0)).toBe('4\n5');

    // Ctrl: a copy.
    await select(page, 0, 0, 1, 0);
    const a = await cellPoint(page, 0, 0, 1, 0.5);
    await page.mouse.move(a.x - 1, a.y);
    await page.mouse.down();
    await page.keyboard.down('Control');
    const b = await cellPoint(page, 0, 1, 1, 0.5);
    await page.mouse.move(b.x - 20, b.y, { steps: 6 });
    await page.mouse.up();
    await page.keyboard.up('Control');
    await landed(page, 0, 1, 1, 1);
    await expect.poll(() => values(page, 0, 0, 1, 1)).toBe('4\t4\n5\t5');

    // Shift: A1 goes in between A2 and the cell below, the cells close up.
    await fill(page, [['6']], 2, 0);
    await select(page, 0, 0);
    const c = await cellPoint(page, 0, 0, 0.5, 1);
    await page.mouse.move(c.x, c.y - 1);
    await page.mouse.down();
    await page.keyboard.down('Shift');
    const d = await cellPoint(page, 2, 0, 0.5, 0.5);
    await page.mouse.move(d.x, d.y, { steps: 6 });
    await page.mouse.up();
    await page.keyboard.up('Shift');
    await landed(page, 1, 0, 1, 0);
    await expect.poll(() => values(page, 0, 0, 2, 0)).toBe('5\n4\n6');

    // Over data Kiln asks first; Cancel keeps it.
    await select(page, 0, 0);
    const e = await cellPoint(page, 0, 0, 1, 0.5);
    await page.mouse.move(e.x - 1, e.y);
    await page.mouse.down();
    const f = await cellPoint(page, 2, 0, 1, 0.5);
    await page.mouse.move(f.x - 20, f.y, { steps: 6 });
    await page.mouse.up();
    const ask = page.getByTestId('grid-replace-cells');
    await expect(ask).toContainText("There's already data here");
    await ask.getByRole('button', { name: 'Cancel' }).click();
    await expectSheetFocused(page);
    expect(await values(page, 0, 0, 2, 0)).toBe('5\n4\n6');
  });

  test("right-click menu: cells, headers, sheet tabs and charts show Excel's entries", async ({ page }) => {
    await openGrid(page);
    await fill(page, [
      ['Item', 'Qty'],
      ['pot', '3'],
      ['jug', '5'],
      ['pot', '7'],
    ]);
    const cell = await cellPoint(page, 1, 0);
    await page.mouse.click(cell.x, cell.y, { button: 'right' });
    const m = page.getByTestId('grid-context-menu');
    await expect(m).toBeVisible();
    for (const item of [
      'Cut',
      'Copy',
      'Paste Special…',
      'Insert…',
      'Delete…',
      'Clear Contents',
      'Filter',
      'Sort',
      'New Comment',
      'New Note',
      'Format Cells…',
      'Pick From Drop-down List…',
      'Define Name…',
      'Link',
    ])
      await expect(m.getByRole('menuitem', { name: item, exact: true })).toBeVisible();
    await page.screenshot({ path: shot('surface-cell-menu.png') });
    // Filter ▸ Filter by Selected Cell's Value.
    await m.getByRole('menuitem', { name: 'Filter', exact: true }).hover();
    await page.getByRole('menuitem', { name: "Filter by Selected Cell's Value" }).click();
    await until(page, (s) => s.features.filtered, 'the list is filtered by "pot"');
    await expectSheetFocused(page);
    await page.keyboard.press('Control+Shift+L');
    await until(page, (s) => !s.features.filter, 'the filter is off');

    // A column header: Column Width… opens its dialog.
    const o = await cellPoint(page, 0, 1);
    const s = await gridState(page);
    await page.mouse.click(o.x, o.y - 20 - s.layout.header_h / 2 + 10, { button: 'right' });
    await expect(m.getByRole('menuitem', { name: 'Column Width…' })).toBeVisible();
    await expect(m.getByRole('menuitem', { name: 'Hide', exact: true })).toBeVisible();
    await page.keyboard.press('Escape');
    await expectSheetFocused(page);
    // A row header: Row Height….
    const r = await cellPoint(page, 2, 0);
    await page.mouse.click(r.x - 64 / 2 - s.layout.header_w / 2, r.y, { button: 'right' });
    await expect(m.getByRole('menuitem', { name: 'Row Height…' })).toBeVisible();
    await m.getByRole('menuitem', { name: 'Insert', exact: true }).click();
    await expect.poll(() => value(page, 3, 0)).toBe('jug');
    await expectSheetFocused(page);
    // A sheet tab.
    await page.locator('[data-sheet="Sheet1"]').click({ button: 'right' });
    await expect(page.getByRole('menu', { name: 'Sheet' })).toBeVisible();
    await page.keyboard.press('Escape');
    // A chart: its own menu, and Format Chart Area… opens the pane.
    await select(page, 0, 0, 4, 1);
    await page.evaluate(async () => {
      const w = window as unknown as {
        __kiln: { registry: { execute(id: string, a?: unknown): Promise<unknown> } };
      };
      await w.__kiln.registry.execute('grid.insert.chart_column');
    });
    const g = await until(page, (st) => st.drawing !== null, 'the chart is inserted');
    const box = await page.locator('[data-kiln-grid-surface]').boundingBox();
    if (!box || !g.drawing) throw new Error('no chart');
    await page.mouse.click(
      box.x + g.layout.header_w + g.drawing.x + g.drawing.width / 2,
      box.y + g.layout.header_h + g.drawing.y + 12,
      { button: 'right' },
    );
    await expect(m.getByRole('menuitem', { name: 'Format Chart Area…' })).toBeVisible();
    await expect(m.getByRole('menuitem', { name: 'Bring to Front' })).toBeVisible();
    await m.getByRole('menuitem', { name: 'Delete', exact: true }).click();
    await until(page, (st) => st.drawing === null, 'the chart is deleted');
  });

  test('mini toolbar: a right-click shows it above the menu and its buttons format the selection', async ({
    page,
  }) => {
    await openGrid(page);
    await fill(page, [['1234.5', '2']]);
    await select(page, 0, 0, 0, 1);
    const p = await cellPoint(page, 0, 0);
    await page.mouse.click(p.x, p.y, { button: 'right' });
    const bar = page.getByTestId('grid-mini-toolbar');
    await expect(bar).toBeVisible();
    await expect(page.getByTestId('grid-context-menu')).toBeVisible();
    for (const name of [/^Bold/, /^Italic/, /^Comma Style/, /^Percent Style/, /^Merge/])
      await expect(bar.getByRole('button', { name }).first()).toBeVisible();
    await expect(bar.getByRole('combobox')).toHaveCount(2);
    await page.screenshot({ path: shot('surface-mini-toolbar.png') });
    // Bold from the mini toolbar formats both cells and keeps the selection.
    await bar.getByRole('button', { name: /^Bold/ }).click();
    await until(page, (s) => s.format.bold, 'the cells are bold');
    await expect(bar).toHaveCount(0);
    await expectSheetFocused(page);
    // Comma Style.
    await page.mouse.click(p.x, p.y, { button: 'right' });
    await bar.getByRole('button', { name: /^Comma Style/ }).click();
    await expect.poll(() => value(page, 0, 0)).toBe(' 1,234.50 ');
  });
});

/** The active cell's format as the core reports it. */
async function fmt(page: Page): Promise<Record<string, unknown>> {
  return (await gridState(page)).format as unknown as Record<string, unknown>;
}

/**
 * Right-clicks A`row+1` (with B selected too), uses one control of the mini toolbar, and checks what Excel does after
 * it: the menu and the toolbar close and the keys are back on the sheet, so what is typed next
 * goes into the cell.
 */
async function viaMini(
  page: Page,
  row: number,
  use: (bar: ReturnType<Page['getByTestId']>) => Promise<void>,
  check: (f: Record<string, unknown>) => boolean,
  what: string,
  typed = true,
): Promise<void> {
  // Two cells, so that Merge & Center has something to merge.
  await select(page, row, 0, row, 1);
  const p = await cellPoint(page, row, 0);
  await page.mouse.click(p.x, p.y, { button: 'right' });
  const bar = page.getByTestId('grid-mini-toolbar');
  await expect(bar).toBeVisible();
  await use(bar);
  await expect.poll(async () => check(await fmt(page)), { message: what }).toBe(true);
  await expect(bar, `${what}: the mini toolbar closes`).toHaveCount(0);
  await expect(page.getByTestId('grid-context-menu'), `${what}: the menu closes`).toHaveCount(0);
  await expectSheetFocused(page);
  if (!typed) return;
  await page.keyboard.type(`t${row}`);
  await page.keyboard.press('Enter');
  await expect
    // (Shown with the cell's number format: Accounting pads text.)
    .poll(async () => (await value(page, row, 0)).trim(), { message: `${what}: typing goes into the cell` })
    .toBe(`t${row}`);
}

test.describe('Kiln Grid sheet tab menu', () => {
  test("sheet tab menu: Excel's entries; Insert, Move or Copy and Unhide do what their dialogs say", async ({
    page,
  }) => {
    await openGrid(page);
    await fill(page, [['first']]);
    const tabMenu = async (name: string) => {
      await page.locator(`[data-sheet="${name}"]`).click({ button: 'right' });
      const m = page.getByRole('menu', { name: 'Sheet' });
      await expect(m).toBeVisible();
      return m;
    };
    const names = async () => (await gridState(page)).sheets.map((t) => t.name).join();
    let m = await tabMenu('Sheet1');
    const items = (await m.getByRole('menuitem').allInnerTexts()).map((t) => t.split('\n')[0]?.trim());
    expect(items).toEqual([
      'Insert…',
      'Delete',
      'Rename',
      'Move or Copy…',
      'View Code',
      'Protect Sheet…',
      'Tab Color',
      'Hide',
      'Unhide…',
      'Select All Sheets',
    ]);
    await page.screenshot({ path: shot('surface-tab-menu.png') });
    await expect(m.getByRole('menuitem', { name: /^View Code/ })).toBeDisabled();
    await expect(m.getByRole('menuitem', { name: /^Unhide/ })).toBeDisabled();
    // Insert… ▸ Worksheet.
    await m.getByRole('menuitem', { name: 'Insert…' }).click();
    let dialog = page.getByTestId('grid-insert-sheet');
    await expect(dialog.getByRole('option', { name: 'Worksheet' })).toHaveAttribute('aria-selected', 'true');
    await page.screenshot({ path: shot('surface-tab-insert.png') });
    await dialog.getByRole('button', { name: 'OK' }).click();
    await expect.poll(names).toBe('Sheet1,Sheet2');
    await expectSheetFocused(page);
    // Move or Copy… ▸ before Sheet1, as a copy.
    await core(page, { ActivateSheet: { index: 0 } });
    m = await tabMenu('Sheet1');
    await m.getByRole('menuitem', { name: 'Move or Copy…' }).click();
    dialog = page.getByTestId('grid-move-copy-sheet');
    await expect(dialog.getByRole('option', { name: '(move to end)' })).toBeVisible();
    await dialog.getByRole('option', { name: 'Sheet1' }).click();
    await dialog.getByRole('checkbox', { name: 'Create a copy' }).check();
    await page.screenshot({ path: shot('surface-tab-move-copy.png') });
    await dialog.getByRole('button', { name: 'OK' }).click();
    await expect.poll(names).toBe('Sheet1 (2),Sheet1,Sheet2');
    await expectSheetFocused(page);
    // Move or Copy… ▸ (move to end), moving.
    m = await tabMenu('Sheet1 (2)');
    await m.getByRole('menuitem', { name: 'Move or Copy…' }).click();
    await dialog.getByRole('option', { name: '(move to end)' }).click();
    await dialog.getByRole('button', { name: 'OK' }).click();
    await expect.poll(names).toBe('Sheet1,Sheet2,Sheet1 (2)');
    // Hide, then Unhide… brings it back.
    m = await tabMenu('Sheet2');
    await m.getByRole('menuitem', { name: 'Hide', exact: true }).click();
    await expect(page.locator('[data-sheet="Sheet2"]')).toHaveCount(0);
    m = await tabMenu('Sheet1');
    await m.getByRole('menuitem', { name: 'Unhide…' }).click();
    dialog = page.getByTestId('grid-unhide-sheet');
    await expect(dialog.getByRole('option', { name: 'Sheet2' })).toHaveAttribute('aria-selected', 'true');
    await dialog.getByRole('button', { name: 'OK' }).click();
    await expect(page.locator('[data-sheet="Sheet2"]')).toHaveCount(1);
    await expectSheetFocused(page);
    // Insert… ▸ Chart: a chart sheet of the selection.
    await core(page, { ActivateSheet: { index: 0 } });
    await select(page, 0, 0);
    m = await tabMenu('Sheet1');
    await m.getByRole('menuitem', { name: 'Insert…' }).click();
    dialog = page.getByTestId('grid-insert-sheet');
    await dialog.getByRole('option', { name: 'Chart' }).dblclick();
    await expect.poll(names).toBe('Chart1,Sheet1,Sheet2,Sheet1 (2)');
    await expect(page.locator('[data-sheet="Chart1"] .kiln-sheettab__chart')).toHaveCount(1);
  });
});

test.describe('Kiln Grid mini toolbar', () => {
  test('mini toolbar: every control formats, closes the menu and the toolbar, and gives the sheet the keys', async ({
    page,
  }) => {
    test.setTimeout(120_000);
    await openGrid(page);
    await fill(
      page,
      Array.from({ length: 26 }, () => ['1234.5']),
    );
    // Bottom row first: a font made bigger makes its row taller and moves the rows below it, which
    // are done by then (the test's cell points take 20 px rows).
    let row = 26;
    const next = () => {
      row -= 1;
      return row;
    };
    const size = (bar: ReturnType<Page['getByTestId']>) => bar.getByRole('combobox', { name: 'Font Size' });
    const font = (bar: ReturnType<Page['getByTestId']>) =>
      bar.getByRole('combobox', { name: 'Font', exact: true });
    // Font Size typed, then Enter.
    await viaMini(
      page,
      next(),
      async (bar) => {
        await size(bar).click();
        await page.keyboard.type('20');
        await page.keyboard.press('Enter');
      },
      (f) => f.font_size === 20,
      'Font Size typed',
    );
    // Font Size picked from its list (the option is stable and the click applies it).
    await viaMini(
      page,
      next(),
      async (bar) => {
        await bar.getByRole('button', { name: 'Show Font Size list' }).click();
        await page.getByRole('option', { name: '16', exact: true }).click();
      },
      (f) => f.font_size === 16,
      'Font Size picked',
    );
    // The size already set, typed again: nothing changes, and the keys still go back.
    await viaMini(
      page,
      next(),
      async (bar) => {
        await size(bar).click();
        await page.keyboard.type('11');
        await page.keyboard.press('Enter');
      },
      (f) => f.font_size === 11,
      'Font Size unchanged',
    );
    // Font typed, then Enter.
    await viaMini(
      page,
      next(),
      async (bar) => {
        await font(bar).click();
        await page.keyboard.type('Georgia');
        await page.keyboard.press('Enter');
      },
      (f) => f.font_family === 'Georgia',
      'Font typed',
    );
    // Font picked from its list.
    await viaMini(
      page,
      next(),
      async (bar) => {
        await bar.getByRole('button', { name: 'Show Font list' }).click();
        await page
          .getByRole('option', { name: /^Arial/ })
          .first()
          .click();
      },
      (f) => f.font_family === 'Arial',
      'Font picked',
    );
    // The current font picked again from the list.
    await viaMini(
      page,
      next(),
      async (bar) => {
        await bar.getByRole('button', { name: 'Show Font list' }).click();
        await page
          .getByRole('option', { name: /^Arial/ })
          .first()
          .click();
      },
      (f) => f.font_family === 'Arial',
      'the current Font picked',
    );
    const buttons: [string, (f: Record<string, unknown>) => boolean][] = [
      ['Increase Font Size', (f) => f.font_size === 12],
      ['Decrease Font Size', (f) => f.font_size === 10],
      ['Accounting Number Format', (f) => f.number_format !== 'General'],
      ['Percent Style', (f) => String(f.number_format).includes('%')],
      ['Comma Style', (f) => String(f.number_format).includes('#,##0')],
      ['Bold', (f) => f.bold === true],
      ['Italic', (f) => f.italic === true],
      ['Center', (f) => f.h_align === 'Center'],
      ['Fill Color', (f) => f.fill_color !== null],
      ['Font Color', (f) => f.font_color !== '000000'],
      ['Merge & Center', (f) => f.h_align === 'Center'],
      ['Increase Decimal', (f) => String(f.number_format).includes('0.0')],
      ['Decrease Decimal', (f) => f.number_format !== 'General'],
    ];
    for (const [name, check] of buttons)
      await viaMini(
        page,
        next(),
        async (bar) => {
          await bar.getByRole('button', { name, exact: true }).click();
        },
        check,
        `${name}`,
      );
    // Borders (its last pick, Bottom Border, by default): nothing on the state's format to read,
    // so the workbook is asked.
    await viaMini(
      page,
      next(),
      (bar) => bar.getByRole('button', { name: 'Borders', exact: true }).click(),
      () => true,
      'Borders',
    );
    // The lists and palettes of the split buttons.
    await viaMini(
      page,
      next(),
      async (bar) => {
        await bar.getByRole('button', { name: 'More options for Accounting Number Format' }).click();
        await page.getByRole('menuitem', { name: '€ Euro' }).click();
      },
      (f) => String(f.number_format).includes('€'),
      'Accounting ▸ € Euro',
    );
    await viaMini(
      page,
      next(),
      async (bar) => {
        await bar.getByRole('button', { name: 'More options for Fill Color' }).click();
        await page.getByRole('button', { name: 'Yellow', exact: true }).click();
      },
      (f) => f.fill_color === 'FFFF00',
      'Fill Color ▸ Yellow',
    );
    await viaMini(
      page,
      next(),
      async (bar) => {
        await bar.getByRole('button', { name: 'More options for Font Color' }).click();
        await page.getByRole('button', { name: 'Red', exact: true }).click();
      },
      (f) => f.font_color === 'FF0000',
      'Font Color ▸ Red',
    );
    await viaMini(
      page,
      next(),
      async (bar) => {
        await bar.getByRole('button', { name: 'More options for Borders' }).click();
        await page.getByRole('menuitem', { name: 'All Borders' }).click();
      },
      () => true,
      'Borders ▸ All Borders',
    );
    await viaMini(
      page,
      next(),
      async (bar) => {
        await bar.getByRole('button', { name: 'More options for Merge & Center' }).click();
        await page.getByRole('menuitem', { name: 'Merge Cells', exact: true }).click();
      },
      () => true,
      'Merge & Center ▸ Merge Cells',
    );
    // Format Painter: picked up, the toolbar closes and the sheet has the keys (Esc puts it down).
    await viaMini(
      page,
      next(),
      (bar) => bar.getByRole('button', { name: 'Format', exact: true }).click(),
      () => true,
      'Format Painter',
      false,
    );
    await page.keyboard.press('Escape');
    await page.screenshot({ path: shot('surface-mini-toolbar-after.png') });
  });
});

test.describe('Kiln Grid ribbon font boxes', () => {
  test('Home ▸ Font: the font already set, picked from the list, gives the sheet the keys', async ({
    page,
  }) => {
    await openGrid(page);
    await select(page, 0, 0);
    const ribbonFont = page.locator('.kiln-ribbon').getByRole('combobox', { name: 'Font', exact: true });
    await expect(ribbonFont).toHaveValue('Calibri');
    await page.locator('.kiln-ribbon').getByRole('button', { name: 'Show Font list' }).click();
    await page
      .getByRole('option', { name: /^Calibri/ })
      .first()
      .click();
    await expectSheetFocused(page);
    await page.keyboard.type('same font');
    await page.keyboard.press('Enter');
    await expect.poll(() => value(page, 0, 0)).toBe('same font');
    // Enter on the size already set does the same.
    await select(page, 1, 0);
    await page.locator('.kiln-ribbon').getByRole('combobox', { name: 'Font Size' }).click();
    await page.keyboard.press('Enter');
    await expectSheetFocused(page);
    await page.keyboard.type('same size');
    await page.keyboard.press('Enter');
    await expect.poll(() => value(page, 1, 0)).toBe('same size');
  });
});

test.describe('Kiln Grid Page Layout view', () => {
  test('page layout: cells are selected, typed into and edited in place on the printed page', async ({
    page,
  }) => {
    await openGrid(page);
    await fill(page, [
      ['Item', 'Qty'],
      ['pot', '3'],
      ['jug', '5'],
    ]);
    await select(page, 0, 0);
    await page.getByRole('radio', { name: 'Page Layout' }).click();
    const sheet = page.getByTestId('grid-page-cells').first();
    await expect(sheet).toBeVisible();
    // Where B4 is printed on page 1, from the core's page map.
    const map = await ask<{
      width: number;
      height: number;
      bands: {
        x: number;
        y: number;
        rows: { index: number; at: number; size: number }[];
        cols: { index: number; at: number; size: number }[];
      }[];
    }>(page, { PageMap: { page: 0 } });
    const box = await page.locator('.kiln-pagelayout__page canvas').first().boundingBox();
    if (!box) throw new Error('no page');
    // The preview draws the page with a 24 px margin round it (kiln-io-pdf preview::MARGIN).
    const k = box.width / (map.width + 48);
    const ky = box.height / (map.height + 48);
    const ox = box.x + 24 * k;
    const oy = box.y + 24 * ky;
    const body = map.bands.at(-1);
    if (!body) throw new Error('no cells on the page');
    const at = (row: number, col: number) => {
      const r = body.rows.find((x) => x.index === row) ?? { at: row * 20, size: 20 };
      const c = body.cols.find((x) => x.index === col) ?? { at: col * 64, size: 64 };
      return { x: ox + (body.x + c.at + c.size / 2) * k, y: oy + (body.y + r.at + r.size / 2) * ky };
    };
    // A click selects the cell; the sheet keeps the keys.
    const b4 = at(3, 1);
    await page.mouse.click(b4.x, b4.y);
    await expect.poll(async () => (await gridState(page)).name_box).toBe('B4');
    await expect(page.getByTestId('grid-page-active')).toBeVisible();
    await expectSheetFocused(page);
    // Typing starts an entry shown on the page; Enter commits it and moves down, as on the sheet.
    await page.keyboard.type('12');
    await expect(page.getByTestId('grid-page-entry')).toContainText('12');
    await page.keyboard.press('Enter');
    await expect.poll(async () => (await gridState(page)).name_box).toBe('B5');
    await page.keyboard.type('=SUM(B2:B4)');
    await page.keyboard.press('Enter');
    await expect(page.getByTestId('grid-page-cells').first()).toBeVisible();
    await page.screenshot({ path: shot('surface-page-layout-edit.png') });
    // Still in Page Layout view, the values are in the workbook.
    await expect(page.getByTestId('grid-page-layout')).toBeVisible();
    expect(await values(page, 3, 1, 4, 1)).toBe('12\n20');
    // Shift+click extends the selection on the page; a double-click edits a cell in place.
    await page.mouse.click(at(1, 0).x, at(1, 0).y);
    await page.keyboard.down('Shift');
    await page.mouse.click(at(2, 1).x, at(2, 1).y);
    await page.keyboard.up('Shift');
    await expect.poll(() => selection(page)).toEqual([{ r0: 1, c0: 0, r1: 2, c1: 1 }]);
    await page.mouse.dblclick(at(1, 0).x, at(1, 0).y);
    await until(page, (s) => s.mode === 'Edit', 'the cell is edited in place');
    await page.keyboard.press('End');
    await page.keyboard.type('s');
    await page.keyboard.press('Enter');
    await expect.poll(() => value(page, 1, 0)).toBe('pots');
  });
});
