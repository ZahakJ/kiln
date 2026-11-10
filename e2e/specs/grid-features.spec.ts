/**
 * Kiln Grid features end to end: each test uses the feature as an Excel user does — the
 * ribbon, its menus, the dialogs, the AutoFilter dropdown, the panes — in the web build, checks
 * the workbook through the core, and checks that the keys come back to the sheet afterwards.
 * Screenshots for the visual review go to e2e/screenshots/grid/.
 */
import { execFileSync } from 'node:child_process';
import { mkdtempSync, rmSync, writeFileSync } from 'node:fs';
import { tmpdir } from 'node:os';
import { join } from 'node:path';
import { expect, type Page, test } from '@playwright/test';
import {
  ask,
  BUDGET,
  cellPoint,
  core,
  countColor,
  expectSheetFocused,
  fill,
  focusSheet,
  gridState,
  menu,
  openGrid,
  origin,
  repo,
  run,
  select,
  shot,
  tab,
  until,
  value,
  values,
} from './grid-helpers.ts';

async function budget(page: Page): Promise<void> {
  await openGrid(page);
  await fill(page, BUDGET);
  await select(page, 1, 0);
  await focusSheet(page);
}

/** The dialog with a title. */
const dialog = (page: Page, name: string | RegExp) => page.getByRole('dialog', { name });

test.describe('Kiln Grid features', () => {
  test('autofilter: filter by value from the dropdown, then clear', async ({ page }) => {
    await budget(page);
    // A visible-row count that follows the filter.
    await fill(page, [['=SUBTOTAL(103,A2:A7)']], 0, 5);
    await select(page, 1, 0);
    await tab(page, 'Data');
    await page.getByRole('button', { name: 'Filter', exact: true }).click();
    await until(page, (s) => s.features.filter, 'the AutoFilter is on');
    await expectSheetFocused(page);
    // The dropdown button of Category (B1).
    const p = await cellPoint(page, 0, 1, 0.86, 0.5);
    await page.mouse.click(p.x, p.y);
    const m = page.getByTestId('grid-filter-menu');
    await expect(m).toBeVisible();
    await expect(m.getByTestId('grid-filter-value')).toHaveCount(4);
    await m.getByText('(Select All)').click();
    await m.getByTestId('grid-filter-value').filter({ hasText: 'Firing' }).click();
    await page.screenshot({ path: shot('features-filter-menu.png') });
    await m.getByTestId('grid-filter-ok').click();
    await expect(m).toBeHidden();
    await until(page, (s) => s.features.filtered, 'rows are filtered');
    await expect.poll(() => value(page, 0, 5)).toBe('2');
    await expectSheetFocused(page);
    await page.screenshot({ path: shot('features-filtered.png') });
    // Search in the list, then Clear Filter From "Category".
    await page.mouse.click(p.x, p.y);
    await expect(m).toBeVisible();
    await m.getByRole('button', { name: /Clear Filter From/ }).click();
    await expect.poll(() => value(page, 0, 5)).toBe('6');
  });

  test('sort: multi-key sort from the Sort dialog', async ({ page }) => {
    await budget(page);
    await tab(page, 'Data');
    await page.getByRole('button', { name: 'Sort…', exact: true }).click();
    const d = dialog(page, 'Sort');
    await expect(d).toBeVisible();
    await expect(d.getByTestId('grid-sort-header')).toBeChecked();
    await d.getByRole('combobox', { name: 'Sort by' }).click();
    await page.getByRole('option', { name: 'Category' }).click();
    await d.getByRole('button', { name: 'Add Level' }).click();
    await d.getByRole('combobox', { name: 'Then by' }).click();
    await page.getByRole('option', { name: 'Amount' }).click();
    await d.getByRole('combobox', { name: 'Order' }).nth(1).click();
    await page.getByRole('option', { name: /Z to A/ }).click();
    await page.waitForTimeout(400);
    await page.screenshot({ path: shot('features-sort-dialog.png') });
    await d.getByRole('button', { name: 'OK' }).click();
    await expect(d).toBeHidden();
    await expect
      .poll(() => values(page, 1, 0, 6, 0))
      .toBe(['Shelves', 'Open day', 'Gas', 'Kiln hire', 'Clay', 'Glaze'].join('\n'));
    await expectSheetFocused(page);
  });

  test('conditional formatting: a data bar appears in the cells', async ({ page }) => {
    await budget(page);
    await select(page, 1, 2, 6, 2);
    await focusSheet(page);
    await tab(page, 'Home');
    await menu(page, 'Conditional Formatting', ['Data Bars', 'Blue Data Bar']);
    const rules = await ask<{ kind: Record<string, unknown> }[]>(page, {
      ConditionalFormats: { sheet: true },
    });
    expect(rules).toHaveLength(1);
    expect(Object.keys(rules[0]?.kind ?? {})).toEqual(['DataBar']);
    await expectSheetFocused(page);
    await select(page, 9, 5);
    await page.waitForTimeout(300);
    // The bar of Gas (1180, the largest) fills most of C4 in the bar's blue.
    const a = await cellPoint(page, 3, 2, 0, 0);
    const bluish = await countColor(
      page,
      { x: a.x + 2, y: a.y + 3, width: 58, height: 14 },
      [0x63, 0x8e, 0xc6],
      90,
    );
    expect(bluish).toBeGreaterThan(200);
    await page.screenshot({ path: shot('features-data-bars.png') });
  });

  test('validation: a list gives an in-cell dropdown with an input message', async ({ page }) => {
    await openGrid(page);
    await select(page, 9, 0);
    await focusSheet(page);
    await tab(page, 'Data');
    await page.locator('[data-command="grid.data.data_validation"]').first().click();
    const d = dialog(page, 'Data Validation');
    await expect(d).toBeVisible();
    await d.getByRole('combobox', { name: 'Allow' }).click();
    await page.getByRole('option', { name: 'List' }).click();
    await d.getByTestId('grid-validation-source').fill('Low,Medium,High');
    await d.getByRole('tab', { name: 'Input Message' }).click();
    await d.getByRole('textbox', { name: 'Title' }).fill('Priority');
    await d.getByRole('textbox', { name: 'Input message' }).fill('Pick one');
    await d.getByRole('button', { name: 'OK' }).click();
    await expect(d).toBeHidden();
    await expectSheetFocused(page);
    await until(page, (s) => s.features.dropdown, 'the cell has a dropdown');
    await expect(page.getByTestId('grid-input-message')).toContainText('Pick one');
    // The button beside the cell opens the list; a click enters the value.
    const p = await cellPoint(page, 9, 1, 0.12, 0.5);
    await page.mouse.click(p.x, p.y);
    const list = page.locator('[data-kiln-autocomplete]');
    await expect(list.getByRole('option', { name: 'High' })).toBeVisible();
    await page.screenshot({ path: shot('features-validation-dropdown.png') });
    await list.getByRole('option', { name: 'High' }).click();
    await expect.poll(() => value(page, 9, 0)).toBe('High');
    // A value outside the list is refused with the alert.
    await select(page, 9, 0);
    await focusSheet(page);
    await page.keyboard.type('Urgent');
    await page.keyboard.press('Enter');
    await expect(page.getByTestId('grid-validation-alert')).toBeVisible();
  });

  test('tables: Format as Table with a total row and structured references', async ({ page }) => {
    await budget(page);
    await page.keyboard.press('Control+t');
    const d = dialog(page, /Create Table|Format As Table/);
    await expect(d).toBeVisible();
    await expect(d.getByTestId('grid-table-headers')).toBeChecked();
    await d.getByRole('button', { name: 'OK' }).click();
    await until(page, (s) => s.features.table?.name === 'Table1', 'Table1 exists');
    await expectSheetFocused(page);
    await expect(page.getByRole('tab', { name: 'Table Design' })).toBeVisible();
    await page.getByRole('tab', { name: /Table Design/ }).click();
    await page.locator('[data-command="ui.grid.table_total_row"]').click();
    await until(page, (s) => !!s.features.table?.options.total_row, 'the total row is on');
    await expect.poll(() => values(page, 7, 0, 7, 2)).toBe('Total\t\t2930');
    await fill(page, [['=SUM(Table1[Amount])']], 9, 0);
    await expect.poll(() => value(page, 9, 0)).toBe('2930');
    await select(page, 2, 1);
    await page.getByRole('tab', { name: /Table Design/ }).click();
    await page.screenshot({ path: shot('features-table.png') });
  });

  test('find and replace: Replace All with match case', async ({ page }) => {
    await budget(page);
    await page.keyboard.press('Control+h');
    const d = dialog(page, 'Find and Replace');
    await expect(d).toBeVisible();
    await d.getByTestId('grid-find-what').fill('Firing');
    await d.getByTestId('grid-find-with').fill('Kiln');
    await d.getByRole('button', { name: 'Options >>' }).click();
    await d.getByRole('checkbox', { name: 'Match case' }).check();
    await d.getByTestId('grid-find-all').click();
    await expect(d.getByText('2 cell(s) found')).toBeVisible();
    await d.getByTestId('grid-replace-all').click();
    await expect(d.getByText('All done. We made 2 replacements.')).toBeVisible();
    await page.screenshot({ path: shot('features-find-replace.png') });
    await d.getByRole('button', { name: 'Close', exact: true }).last().click();
    await expect.poll(() => values(page, 3, 1, 6, 1)).toBe('Kiln\nEquipment\nEvents\nKiln');
    await expectSheetFocused(page);
  });

  test('auditing: trace precedents draws arrows', async ({ page }) => {
    await openGrid(page);
    await fill(page, [['1'], ['2'], ['3'], ['=SUM(A1:A3)*B1']]);
    await fill(page, [['4']], 0, 1);
    await select(page, 3, 0);
    await focusSheet(page);
    await tab(page, 'Formulas');
    await page.getByRole('button', { name: 'Trace Precedents' }).click();
    await until(page, (s) => s.features.arrows, 'arrows are shown');
    await expectSheetFocused(page);
    await page.waitForTimeout(300);
    const a = await cellPoint(page, 0, 0, 0, 0);
    const blue = await countColor(page, { x: a.x, y: a.y, width: 130, height: 80 }, [0x31, 0x59, 0xc6], 60);
    expect(blue).toBeGreaterThan(40);
    await page.screenshot({ path: shot('features-trace-precedents.png') });
    await page.locator('[data-command="grid.formulas.remove_arrows"]').first().click();
    await until(page, (s) => !s.features.arrows, 'arrows removed');
  });

  test('formula lab: shows the steps of =IF(SUM(A1:A3)>5,"x","y")', async ({ page }) => {
    await openGrid(page);
    await fill(page, [['1'], ['2'], ['3']]);
    await fill(page, [['=IF(SUM(A1:A3)>5,"x","y")']], 0, 1);
    await select(page, 0, 1);
    await focusSheet(page);
    await tab(page, 'Formulas');
    await page.getByRole('button', { name: 'Formula Lab' }).last().click();
    const lab = page.getByTestId('grid-formula-lab');
    await expect(lab).toBeVisible();
    const steps = lab.getByTestId('grid-lab-steps');
    await expect(steps).toContainText('=IF(SUM({1;2;3})>5,"x","y")');
    await expect(steps).toContainText('=IF(6>5,"x","y")');
    await expect(steps).toContainText('=IF(TRUE,"x","y")');
    await expect(lab).toContainText('not evaluated');
    await page.screenshot({ path: shot('features-formula-lab.png') });
    // Evaluate Formula steps through the same reduction.
    await page.getByRole('button', { name: 'Evaluate Formula' }).click();
    const ev = dialog(page, 'Evaluate Formula');
    await expect(ev.getByTestId('grid-evaluate-text')).toHaveText('=IF(SUM(A1:A3)>5,"x","y")');
    await ev.getByTestId('grid-evaluate-step').click();
    await expect(ev.getByTestId('grid-evaluate-text')).toHaveText('=IF(SUM({1;2;3})>5,"x","y")');
    await ev.getByRole('button', { name: 'Close', exact: true }).last().click();
    await expectSheetFocused(page);
  });

  test('print: preview of a sheet with print titles', async ({ page }) => {
    await openGrid(page);
    const rows = [['Month', 'Sales']];
    for (let i = 1; i <= 120; i++) rows.push([`M${i}`, String(i * 10)]);
    await page.evaluate(
      async ({ rows }) => {
        type G = {
          doc: number;
          client: { execute(d: number, c: unknown): Promise<unknown> };
        };
        const g = (window as unknown as { __kilnGrid: G }).__kilnGrid;
        // One paste of the whole block (tab-separated), as typing 240 cells would be slow.
        await g.client.execute(g.doc, {
          Grid: { SelectCell: { row: 0, col: 0, extend: false, add: false } },
        });
        await g.client.execute(g.doc, {
          Suite: {
            Edit: {
              Paste: { data: { text: rows.map((r) => r.join('\t')).join('\n'), html: null, kiln: null } },
            },
          },
        });
      },
      { rows },
    );
    await expect.poll(() => value(page, 120, 1)).toBe('1200');
    await tab(page, 'Page Layout');
    await page.getByRole('button', { name: 'Print Titles' }).click();
    const d = dialog(page, 'Page Setup');
    await expect(d.getByRole('tab', { name: 'Sheet', selected: true })).toBeVisible();
    await d.getByTestId('grid-title-rows').fill('$1:$1');
    await d.getByRole('button', { name: 'OK' }).click();
    await expect(d).toBeHidden();
    await expectSheetFocused(page);
    const setup = await ask<{ title_rows: string; pages: number }>(page, 'PageSetup');
    expect(setup.title_rows).toBe('$1:$1');
    expect(setup.pages).toBeGreaterThanOrEqual(3);
    // File ▸ Print: the preview shows the pages.
    await page.keyboard.press('Control+p');
    await expect(page.getByText(new RegExp(`of ${setup.pages}`)).first()).toBeVisible({ timeout: 20_000 });
    await page.screenshot({ path: shot('features-print-preview.png') });
    // The PDF repeats the title row on every page.
    const pdf = await page.evaluate(async () => {
      type G = { doc: number; client: { query(d: number, q: unknown): Promise<unknown> } };
      const g = (window as unknown as { __kilnGrid: G }).__kilnGrid;
      const r = (await g.client.query(g.doc, {
        Pdf: {
          settings: {
            pages: '',
            parity: 'All',
            copies: 1,
            paper: null,
            scale: 1,
            per_sheet: 1,
            orientation: null,
            margins: null,
            grayscale: false,
            markup: false,
            hidden_text: false,
            tagged: true,
            bookmarks: true,
          },
        },
      })) as { Pdf: { bytes: number[] } };
      return r.Pdf.bytes;
    });
    const dir = mkdtempSync(join(tmpdir(), 'kiln-grid-print-'));
    try {
      const file = join(dir, 'p.pdf');
      writeFileSync(file, Buffer.from(pdf));
      const info = execFileSync('pdfinfo', [file], { encoding: 'utf8' });
      expect(info).toMatch(new RegExp(`Pages:\\s+${setup.pages}`));
      const page2 = execFileSync('pdftotext', ['-f', '2', '-l', '2', '-layout', file, '-'], {
        encoding: 'utf8',
      });
      expect(page2).toContain('Month');
    } finally {
      rmSync(dir, { recursive: true, force: true });
    }
  });

  test('pictures: insert, crop, alt text and move with the mouse', async ({ page }) => {
    await budget(page);
    await select(page, 2, 4);
    await focusSheet(page);
    await tab(page, 'Insert');
    // The web host's file input (the File System Access picker is not scriptable).
    await page.evaluate(() => {
      // biome-ignore lint/suspicious/noExplicitAny: the API is not in the DOM typings
      delete (window as any).showOpenFilePicker;
    });
    const chooser = page.waitForEvent('filechooser');
    await page.locator('[data-command="suite.insert.pictures"]').first().click();
    await (await chooser).setFiles(repo('fixtures/golden/scene/composition.png'));
    const s = await until(page, (s) => s.drawing?.kind === 'Picture', 'the picture is selected');
    expect(s.drawing?.width).toBe(560);
    await expect(page.getByRole('tab', { name: /Picture Format/ })).toBeVisible();
    await expectSheetFocused(page);
    await page.getByRole('tab', { name: /Picture Format/ }).click();
    await page.locator('[data-command="suite.picture.crop"]').first().click();
    const crop = dialog(page, 'Crop');
    await crop.getByTestId('grid-crop-left').fill('25');
    await crop.getByRole('button', { name: 'OK' }).click();
    await expect(crop).toBeHidden();
    const c = await until(page, (s) => (s.drawing?.object?.crop?.[0] ?? 0) === 25, 'cropped');
    expect(c.drawing?.width).toBeCloseTo(420, 0);
    await expectSheetFocused(page);
    await page.locator('[data-command="suite.picture.alt_text"]').first().click();
    const alt = dialog(page, 'Alt Text');
    await alt.getByTestId('grid-alt-text-input').fill('Kiln shapes and colours');
    await alt.getByRole('button', { name: 'OK' }).click();
    await until(page, (s) => s.drawing?.object?.alt_text === 'Kiln shapes and colours', 'described');
    await page.waitForTimeout(300);
    await page.screenshot({ path: shot('features-picture.png') });
    // Drag it by 64 px to the right.
    const o = await origin(page);
    const x = o.x + c.layout.header_w + (c.drawing?.x ?? 0) + 60;
    const y = o.y + c.layout.header_h + (c.drawing?.y ?? 0) + 60;
    await page.mouse.move(x, y);
    await page.mouse.down();
    await page.mouse.move(x + 32, y, { steps: 4 });
    await page.mouse.move(x + 64, y, { steps: 4 });
    await page.mouse.up();
    await until(page, (s) => Math.abs((s.drawing?.x ?? 0) - (c.drawing?.x ?? 0) - 64) < 2, 'moved');
    // Delete removes it; undo brings it back.
    await focusSheet(page);
    await page.keyboard.press('Delete');
    await until(page, (s) => !s.drawing, 'deleted');
    await page.keyboard.press('Control+z');
    await expect.poll(async () => (await ask<{ length: number }>(page, 'State')) !== null).toBe(true);
  });

  test('shapes: insert from the gallery, type their text and fill them', async ({ page }) => {
    await openGrid(page);
    await select(page, 1, 1);
    await focusSheet(page);
    await tab(page, 'Insert');
    await menu(page, 'Shapes', ['Oval']);
    await until(
      page,
      (s) => s.drawing?.kind === 'Shape' && s.drawing?.object?.preset === 'ellipse',
      'an oval',
    );
    await expectSheetFocused(page);
    await expect(page.getByRole('tab', { name: /Shape Format/ })).toBeVisible();
    // Typing writes the shape's text.
    await page.keyboard.type('Kiln');
    const t = dialog(page, 'Edit Text');
    await expect(t.getByTestId('grid-shape-text-input')).toHaveValue('Kiln');
    await t.getByTestId('grid-shape-text-input').fill('Kiln');
    await t.getByRole('button', { name: 'OK' }).click();
    await until(page, (s) => s.drawing?.object?.text === 'Kiln', 'the text is in the shape');
    await expectSheetFocused(page);
    await page.getByRole('tab', { name: /Shape Format/ }).click();
    await run(page, 'ui.grid.shape_fill', { color: '#C2481C' });
    await until(page, (s) => s.drawing?.object?.fill === 'C2481C', 'filled');
    await page.waitForTimeout(300);
    const a = await cellPoint(page, 1, 1, 0, 0);
    const ember = await countColor(
      page,
      { x: a.x + 20, y: a.y + 20, width: 70, height: 70 },
      [0xc2, 0x48, 0x1c],
      40,
    );
    expect(ember).toBeGreaterThan(1500);
    await page.screenshot({ path: shot('features-shape.png') });
    // A click on a cell leaves the shape; the tab goes away.
    await page.mouse.click((await cellPoint(page, 12, 6)).x, (await cellPoint(page, 12, 6)).y);
    await until(page, (s) => !s.drawing, 'deselected');
    await expect(page.getByRole('tab', { name: /Shape Format/ })).toBeHidden();
  });

  test('conditional formatting: highlight cells greater than a value', async ({ page }) => {
    await budget(page);
    await select(page, 1, 2, 6, 2);
    await focusSheet(page);
    await tab(page, 'Home');
    await menu(page, 'Conditional Formatting', ['Highlight Cells Rules', 'Greater Than…']);
    const d = page.getByTestId('grid-cf-quick');
    await d.getByTestId('grid-cf-value').fill('500');
    await d.getByRole('button', { name: 'OK' }).click();
    await expect(d).toBeHidden();
    await expectSheetFocused(page);
    const rules = await ask<
      { kind: Record<string, { op?: string }>; style: { fill: string | null } | null }[]
    >(page, { ConditionalFormats: { sheet: true } });
    expect(rules).toHaveLength(1);
    expect(Object.keys(rules[0]?.kind ?? {})).toEqual(['CellValue']);
    await select(page, 9, 5);
    await page.waitForTimeout(300);
    // Gas (1180) and Kiln hire (640) are light red; Clay (420) is not.
    const gas = await cellPoint(page, 3, 2, 0, 0);
    const clay = await cellPoint(page, 1, 2, 0, 0);
    const pink = [0xff, 0xc7, 0xce] as [number, number, number];
    expect(
      await countColor(page, { x: gas.x + 4, y: gas.y + 4, width: 20, height: 12 }, pink, 30),
    ).toBeGreaterThan(150);
    expect(await countColor(page, { x: clay.x + 4, y: clay.y + 4, width: 20, height: 12 }, pink, 30)).toBe(0);
  });

  test('conditional formatting: top and bottom rules', async ({ page }) => {
    await budget(page);
    await select(page, 1, 2, 6, 2);
    await focusSheet(page);
    await tab(page, 'Home');
    await menu(page, 'Conditional Formatting', ['Top/Bottom Rules', 'Top 10 Items…']);
    const d = page.getByTestId('grid-cf-quick');
    await d.getByTestId('grid-cf-value').fill('2');
    await d.getByRole('button', { name: 'OK' }).click();
    const rules = await ask<{ kind: { TopBottom?: { rank: number; bottom: boolean } } }[]>(page, {
      ConditionalFormats: { sheet: true },
    });
    expect(rules[0]?.kind.TopBottom).toMatchObject({ rank: 2, bottom: false });
    await expectSheetFocused(page);
  });

  test('conditional formatting: colour scales', async ({ page }) => {
    await budget(page);
    await select(page, 1, 2, 6, 2);
    await focusSheet(page);
    await tab(page, 'Home');
    await menu(page, 'Conditional Formatting', ['Color Scales', 'Green - Yellow - Red Color Scale']);
    const rules = await ask<{ kind: Record<string, unknown> }[]>(page, {
      ConditionalFormats: { sheet: true },
    });
    expect(Object.keys(rules[0]?.kind ?? {})).toEqual(['ColorScale']);
    await select(page, 9, 5);
    await page.waitForTimeout(300);
    // The largest value (Gas) is green, the smallest (Shelves, 0) red.
    const gas = await cellPoint(page, 3, 2, 0, 0);
    const shelves = await cellPoint(page, 4, 2, 0, 0);
    expect(
      await countColor(page, { x: gas.x + 4, y: gas.y + 4, width: 20, height: 12 }, [0x63, 0xbe, 0x7b], 40),
    ).toBeGreaterThan(150);
    expect(
      await countColor(
        page,
        { x: shelves.x + 4, y: shelves.y + 4, width: 20, height: 12 },
        [0xf8, 0x69, 0x6b],
        40,
      ),
    ).toBeGreaterThan(150);
    await page.screenshot({ path: shot('features-colour-scale.png') });
  });

  test('conditional formatting: icon sets', async ({ page }) => {
    await budget(page);
    await select(page, 1, 2, 6, 2);
    await focusSheet(page);
    await tab(page, 'Home');
    await menu(page, 'Conditional Formatting', ['Icon Sets', '3 Traffic Lights']);
    const rules = await ask<{ kind: { IconSet?: { set: string } } }[]>(page, {
      ConditionalFormats: { sheet: true },
    });
    expect(rules[0]?.kind.IconSet?.set).toBe('3TrafficLights1');
    await select(page, 9, 5);
    await page.waitForTimeout(300);
    // A green light beside the largest value.
    const gas = await cellPoint(page, 3, 2, 0, 0);
    expect(
      await countColor(page, { x: gas.x, y: gas.y, width: 22, height: 20 }, [0x00, 0xb0, 0x50], 80),
    ).toBeGreaterThan(20);
    await page.screenshot({ path: shot('features-icon-set.png') });
  });

  test('conditional formatting: a rule from a formula', async ({ page }) => {
    await budget(page);
    await select(page, 1, 0, 6, 2);
    await focusSheet(page);
    await tab(page, 'Home');
    await menu(page, 'Conditional Formatting', ['New Rule…']);
    const d = page.getByTestId('grid-cf-rule');
    await d.getByText('Use a formula to determine which cells to format').click();
    await d.getByTestId('grid-cf-formula').fill('=$B2="Firing"');
    await d.getByRole('button', { name: 'OK' }).click();
    await expect(d).toBeHidden();
    const rules = await ask<{ kind: { Formula?: { formula: string } }; applies_to: string }[]>(page, {
      ConditionalFormats: { sheet: true },
    });
    expect(rules[0]?.kind.Formula?.formula).toBe('=$B2="Firing"');
    expect(rules[0]?.applies_to).toBe('$A$2:$C$7');
    await expectSheetFocused(page);
  });

  test('conditional formatting: manage rules edits and deletes', async ({ page }) => {
    await budget(page);
    await select(page, 1, 2, 6, 2);
    await focusSheet(page);
    await tab(page, 'Home');
    await menu(page, 'Conditional Formatting', ['Data Bars', 'Blue Data Bar']);
    await menu(page, 'Conditional Formatting', ['Color Scales', 'Green - Yellow - Red Color Scale']);
    await menu(page, 'Conditional Formatting', ['Manage Rules…']);
    const d = page.getByTestId('grid-cf-manage');
    await expect(d.getByTestId('grid-cf-row')).toHaveCount(2);
    await d.getByTestId('grid-cf-row').first().click();
    await d.getByRole('button', { name: 'Delete Rule' }).click();
    await expect(d.getByTestId('grid-cf-row')).toHaveCount(1);
    await page.waitForTimeout(400);
    await page.screenshot({ path: shot('features-cf-manage.png') });
    await d.getByRole('button', { name: 'OK' }).click();
    await expect(d).toBeHidden();
    const rules = await ask<unknown[]>(page, { ConditionalFormats: { sheet: true } });
    expect(rules).toHaveLength(1);
    await expectSheetFocused(page);
  });

  test('home: format painter copies a format', async ({ page }) => {
    await budget(page);
    await select(page, 0, 0);
    await focusSheet(page);
    await page.keyboard.press('Control+b');
    await until(page, (s) => s.format.bold, 'A1 is bold');
    await tab(page, 'Home');
    await page.locator('[data-command="suite.edit.format_painter"]').first().click();
    await expectSheetFocused(page);
    // The brush paints the next cells clicked.
    const p = await cellPoint(page, 3, 1);
    await page.mouse.click(p.x, p.y);
    await select(page, 3, 1);
    await until(page, (s) => s.format.bold, 'B4 is bold');
    await select(page, 4, 1);
    expect((await gridState(page)).format.bold).toBe(false);
  });

  test('home: merge and center', async ({ page }) => {
    await openGrid(page);
    await fill(page, [['Firing log']]);
    await select(page, 0, 0, 0, 3);
    await focusSheet(page);
    await tab(page, 'Home');
    await page.locator('[data-command="grid.alignment.merge_center"]').first().click();
    await select(page, 0, 0);
    const s = await until(page, (s) => s.format.merged, 'merged');
    expect(s.format.h_align).toBe('Center');
    await expectSheetFocused(page);
  });

  test('home: cell styles', async ({ page }) => {
    await budget(page);
    await select(page, 3, 2);
    await focusSheet(page);
    await tab(page, 'Home');
    await menu(page, 'Cell Styles', ['Good, Bad and Neutral', 'Good']);
    const s = await until(page, (s) => !!s.format.fill_color, 'C4 has the Good fill');
    expect(s.format.fill_color?.toUpperCase()).toContain('C6EFCE');
    await expectSheetFocused(page);
  });

  test('home: sort and filter menu', async ({ page }) => {
    await budget(page);
    await select(page, 2, 0);
    await focusSheet(page);
    await tab(page, 'Home');
    await menu(page, 'Sort & Filter', [/Sort A to Z/]);
    await expect.poll(() => values(page, 1, 0, 6, 0)).toBe('Clay\nGas\nGlaze\nKiln hire\nOpen day\nShelves');
    await page.getByRole('button', { name: 'Sort & Filter' }).first().click();
    await page.getByRole('menuitemcheckbox', { name: /^Filter/ }).click();
    await until(page, (s) => s.features.filter, 'filter on');
    await expectSheetFocused(page);
  });

  test('home: go to special selects the blanks', async ({ page }) => {
    await budget(page);
    await select(page, 2, 1);
    await focusSheet(page);
    await page.keyboard.press('Delete');
    await select(page, 0, 0, 6, 2);
    await focusSheet(page);
    await tab(page, 'Home');
    await menu(page, 'Find', ['Go To Special…']);
    const d = dialog(page, 'Go To Special');
    await d.getByText('Blanks', { exact: true }).click();
    await d.getByRole('button', { name: 'OK' }).click();
    await expect(d).toBeHidden();
    const s = await until(page, (s) => s.name_box === 'B3', 'the blank is selected');
    expect(s.name_box).toBe('B3');
    await expectSheetFocused(page);
  });

  test('insert: a hyperlink to a web page and to a place', async ({ page }) => {
    await budget(page);
    await select(page, 9, 0);
    await focusSheet(page);
    await page.keyboard.press('Control+k');
    const d = dialog(page, 'Insert Hyperlink');
    await d.getByRole('textbox', { name: 'Text to display' }).fill('Kiln');
    await d.getByTestId('grid-link-address').fill('kiln.example');
    await d.getByRole('button', { name: 'OK' }).click();
    await expect(d).toBeHidden();
    const s = await until(page, (s) => !!s.features.link, 'A10 links');
    expect(s.features.link?.target).toBe('https://kiln.example');
    expect(await value(page, 9, 0)).toBe('Kiln');
    await expectSheetFocused(page);
    await select(page, 10, 0);
    await focusSheet(page);
    await page.keyboard.press('Control+k');
    await d.getByText('Place in This Document').click();
    await d.getByRole('textbox', { name: /cell reference/ }).fill('Sheet1!C7');
    await d.getByRole('button', { name: 'OK' }).click();
    await until(page, (s) => s.features.link?.location === 'Sheet1!C7', 'A11 links to C7');
  });

  test('insert: notes show on hover and with show all notes', async ({ page }) => {
    await budget(page);
    await select(page, 1, 0);
    await focusSheet(page);
    await page.keyboard.press('Shift+F2');
    const n = page.getByTestId('grid-note-editor');
    await expect(n).toBeVisible();
    await n.getByRole('textbox', { name: 'Note' }).fill('Bought in bulk');
    await page.keyboard.press('Escape');
    await expect(n).toBeHidden();
    await until(page, (s) => s.features.note, 'A2 has a note');
    await expectSheetFocused(page);
    // Hover shows the card.
    await select(page, 8, 4);
    const p = await cellPoint(page, 1, 0);
    await page.mouse.move(p.x, p.y);
    await expect(page.getByTestId('grid-hover-card')).toContainText('Bought in bulk');
    // Review ▸ Notes ▸ Show All Notes keeps it on the sheet.
    await page.mouse.move(p.x + 400, p.y + 300);
    await tab(page, 'Review');
    await page.getByRole('button', { name: 'Notes' }).click();
    await page
      .getByRole('menuitemcheckbox', { name: 'Show All Notes' })
      .or(page.getByRole('menuitem', { name: 'Show All Notes' }))
      .click();
    await expect(page.getByTestId('grid-note-card')).toContainText('Bought in bulk');
    await page.screenshot({ path: shot('features-notes.png') });
  });

  test('insert: threaded comments with replies, resolve and navigate', async ({ page }) => {
    await budget(page);
    await select(page, 3, 2);
    await focusSheet(page);
    await tab(page, 'Insert');
    await page.locator('[data-command="suite.review.new_comment"]').first().click();
    const pane = page.getByTestId('grid-comments-pane');
    await pane.getByTestId('grid-comment-draft').fill('Why so much gas?');
    await pane.getByTestId('grid-comment-post').click();
    await expect(pane.getByTestId('grid-comment')).toHaveCount(1);
    await until(page, (s) => s.features.comment, 'C4 has a thread');
    // Posting hands the keys back to the sheet.
    await expectSheetFocused(page);
    await select(page, 6, 2);
    await focusSheet(page);
    await page.locator('[data-command="suite.review.new_comment"]').first().click();
    await pane.getByTestId('grid-comment-draft').fill('Booked for March');
    await page.keyboard.press('Control+Enter');
    await expect(pane.getByTestId('grid-comment')).toHaveCount(2);
    await expectSheetFocused(page);
    await page.screenshot({ path: shot('features-comments.png') });
    // Review ▸ Next goes from thread to thread.
    await select(page, 0, 0);
    await tab(page, 'Review');
    await page.locator('[data-command="suite.review.next_comment"]').first().click();
    await until(page, (s) => s.name_box === 'C4', 'the first thread');
    await page.locator('[data-command="suite.review.next_comment"]').first().click();
    await until(page, (s) => s.name_box === 'C7', 'the second thread');
    const list = await ask<{ text: string; note: boolean }[]>(page, 'Comments');
    expect(list.filter((c) => !c.note).map((c) => c.text)).toEqual(['Why so much gas?', 'Booked for March']);
  });

  test('insert: header and footer', async ({ page }) => {
    await budget(page);
    await tab(page, 'Insert');
    await page.locator('[data-command="suite.insert.header_footer"]').first().click();
    const d = dialog(page, 'Page Setup');
    await expect(d.getByRole('tab', { name: 'Header/Footer', selected: true })).toBeVisible();
    await d.getByTestId('grid-header-code').fill('&CFiring log&RPage &P of &N');
    await d.getByRole('button', { name: 'OK' }).click();
    await expect(d).toBeHidden();
    const setup = await ask<{ header: string }>(page, 'PageSetup');
    expect(setup.header).toBe('&CFiring log&RPage &P of &N');
    await expectSheetFocused(page);
  });

  test('insert: symbol', async ({ page }) => {
    await openGrid(page);
    await select(page, 0, 0);
    await focusSheet(page);
    await tab(page, 'Insert');
    await page.locator('[data-command="suite.insert.symbol"]').first().click();
    const d = dialog(page, 'Symbol');
    await d.getByRole('option', { name: '€' }).click();
    await d.getByRole('button', { name: 'Insert' }).click();
    await expect(d).toBeHidden();
    await expectSheetFocused(page);
    await page.keyboard.press('Enter');
    await expect.poll(() => value(page, 0, 0)).toBe('€');
    // Any character by its code.
    await select(page, 1, 0);
    await focusSheet(page);
    await page.locator('[data-command="suite.insert.symbol"]').first().click();
    await d.getByTestId('grid-symbol-code').fill('2603');
    await d.getByRole('button', { name: 'Insert' }).click();
    await page.keyboard.press('Enter');
    await expect.poll(() => value(page, 1, 0)).toBe('☃');
  });

  test('page layout: margins, orientation and size', async ({ page }) => {
    await budget(page);
    await tab(page, 'Page Layout');
    await menu(page, 'Margins', [/^Narrow/]);
    await menu(page, 'Orientation', ['Landscape']);
    await menu(page, 'Size', [/^A4/]);
    const p = await ask<{ landscape: boolean; paper: number; margins: { left: number; right: number } }>(
      page,
      'PageSetup',
    );
    expect(p.landscape).toBe(true);
    expect(p.paper).toBe(9);
    expect([p.margins.left, p.margins.right]).toEqual([0.25, 0.25]);
    await expectSheetFocused(page);
  });

  test('page layout: print area', async ({ page }) => {
    await budget(page);
    await select(page, 0, 0, 3, 2);
    await focusSheet(page);
    await tab(page, 'Page Layout');
    await menu(page, 'Print Area', ['Set Print Area']);
    expect((await ask<{ print_area: string }>(page, 'PageSetup')).print_area).toBe('$A$1:$C$4');
    await menu(page, 'Print Area', ['Clear Print Area']);
    expect((await ask<{ print_area: string }>(page, 'PageSetup')).print_area).toBe('');
    await expectSheetFocused(page);
  });

  test('page layout: breaks', async ({ page }) => {
    await budget(page);
    await select(page, 4, 0);
    await focusSheet(page);
    await tab(page, 'Page Layout');
    await menu(page, 'Breaks', ['Insert Page Break']);
    const p = await ask<{ row_breaks: number[]; pages: number }>(page, 'PageSetup');
    expect(p.row_breaks).toEqual([4]);
    expect(p.pages).toBe(2);
    await menu(page, 'Breaks', ['Reset All Page Breaks']);
    expect((await ask<{ row_breaks: number[] }>(page, 'PageSetup')).row_breaks).toEqual([]);
  });

  test('page layout: scale to fit', async ({ page }) => {
    await openGrid(page);
    // Twenty-six columns of numbers: three pages wide at 100 %.
    await fill(page, [Array.from({ length: 26 }, (_, i) => String(i * 1000))]);
    const before = await ask<{ pages: number }>(page, 'PageSetup');
    expect(before.pages).toBeGreaterThan(1);
    await tab(page, 'Page Layout');
    await menu(page, 'Scale to Fit', ['Fit Sheet on One Page']);
    const p = await ask<{ fit: boolean; fit_width: number; fit_height: number; pages: number }>(
      page,
      'PageSetup',
    );
    expect([p.fit, p.fit_width, p.fit_height, p.pages]).toEqual([true, 1, 1, 1]);
  });

  test('page layout: sheet options view and print gridlines and headings', async ({ page }) => {
    await budget(page);
    await tab(page, 'Page Layout');
    await page.locator('[data-command="grid.page_layout.print_gridlines"]').click();
    await page.locator('[data-command="grid.page_layout.print_headings"]').click();
    const p = await ask<{ gridlines: boolean; headings: boolean }>(page, 'PageSetup');
    expect([p.gridlines, p.headings]).toEqual([true, true]);
    await page.locator('[data-command="grid.view.gridlines"]').click();
    await until(page, (s) => !(s as unknown as { gridlines: boolean }).gridlines, 'gridlines hidden');
    await expectSheetFocused(page);
  });

  test('view: page break preview shows and moves breaks', async ({ page }) => {
    await openGrid(page);
    const rows = Array.from({ length: 80 }, (_, i) => [`Row ${i + 1}`]);
    await fill(page, rows.slice(0, 80));
    await tab(page, 'View');
    await page
      .getByRole('tabpanel', { name: 'View' })
      .getByRole('button', { name: 'Page Break Preview' })
      .click();
    const s = await until(page, (s) => s.features.pages >= 2, 'the pages are drawn');
    // The automatic break is a dashed blue line; dragging it up makes it a manual break.
    const p = await ask<{ row_breaks: number[] }>(page, 'PageSetup');
    expect(p.row_breaks).toEqual([]);
    expect(s.features.pages).toBe((await ask<{ pages: number }>(page, 'PageSetup')).pages);
    // Insert a manual break at row 21 from the ribbon; the preview follows.
    await select(page, 20, 0);
    await tab(page, 'Page Layout');
    await menu(page, 'Breaks', ['Insert Page Break']);
    await until(page, (x) => x.features.pages >= s.features.pages, 'pages after the break');
    expect((await ask<{ row_breaks: number[] }>(page, 'PageSetup')).row_breaks).toEqual([20]);
    // Drag the break five rows up, as in Excel.
    await focusSheet(page);
    await page.keyboard.press('Control+Home');
    await page.waitForTimeout(300);
    const st = await gridState(page);
    const o = await origin(page);
    const y = o.y + st.layout.header_h + 20 * 20;
    const x = o.x + st.layout.header_w + 30;
    await page.mouse.move(x, y);
    await page.mouse.down();
    await page.mouse.move(x, y - 50, { steps: 5 });
    await page.mouse.move(x, y - 100, { steps: 5 });
    await page.mouse.up();
    await expect
      .poll(async () => (await ask<{ row_breaks: number[] }>(page, 'PageSetup')).row_breaks)
      .toEqual([15]);
    await page.screenshot({ path: shot('features-page-break-preview.png') });
  });

  test('view: page layout shows the printed pages', async ({ page }) => {
    await budget(page);
    await tab(page, 'View');
    await page.getByRole('tabpanel', { name: 'View' }).getByRole('button', { name: 'Page Layout' }).click();
    const v = page.getByTestId('grid-page-layout');
    await expect(v).toBeVisible();
    await expect(v).toContainText('1 page');
    await page.waitForTimeout(500);
    await page.screenshot({ path: shot('features-page-layout-view.png') });
    // The header area opens the header and footer settings.
    await v.getByTestId('grid-page-header').first().click();
    const d = dialog(page, 'Page Setup');
    await expect(d.getByRole('tab', { name: 'Header/Footer', selected: true })).toBeVisible();
    await d.getByRole('button', { name: 'Cancel' }).click();
    // Normal on the status bar returns to the grid (a double-click on the page edits a cell).
    await page.getByRole('radio', { name: 'Normal' }).click();
    await expect(v).toBeHidden();
  });

  test('formulas: insert function opens function arguments', async ({ page }) => {
    await openGrid(page);
    await fill(page, [['1'], ['2'], ['3']]);
    await select(page, 4, 0);
    await focusSheet(page);
    await tab(page, 'Formulas');
    await page.getByRole('button', { name: 'Insert Function' }).first().click();
    const d = page.getByTestId('grid-insert-function');
    await expect(d).toBeVisible();
    await d.getByRole('textbox', { name: 'Search for a function' }).fill('AVERAGE');
    await d.getByRole('option', { name: 'AVERAGE', exact: true }).click();
    await d.getByRole('button', { name: 'OK' }).click();
    const a = dialog(page, 'Function Arguments');
    await expect(a).toBeVisible();
    await a.getByTestId('grid-fn-arg-0').fill('A1:A3');
    await expect(a.getByTestId('grid-fn-value-0')).toHaveText('= {1;2;3}');
    await expect(a.getByTestId('grid-fn-result')).toHaveText('Formula result = 2');
    await page.screenshot({ path: shot('features-function-arguments.png') });
    await a.getByRole('button', { name: 'OK' }).click();
    await expect(a).toBeHidden();
    await expect.poll(() => value(page, 4, 0)).toBe('2');
    expect((await gridState(page)).formula).toBe('=AVERAGE(A1:A3)');
    await expectSheetFocused(page);
  });

  test('formulas: category menus list their functions', async ({ page }) => {
    await openGrid(page);
    await select(page, 0, 0);
    await focusSheet(page);
    await tab(page, 'Formulas');
    await menu(page, 'Financial', ['PMT']);
    const a = dialog(page, 'Function Arguments');
    await a.getByTestId('grid-fn-arg-0').fill('5%/12');
    await a.getByTestId('grid-fn-arg-1').fill('60');
    await a.getByTestId('grid-fn-arg-2').fill('-10000');
    await expect(a.getByTestId('grid-fn-result')).toContainText('188.71');
    await a.getByRole('button', { name: 'OK' }).click();
    await expect.poll(() => gridState(page).then((s) => s.formula)).toBe('=PMT(5%/12,60,-10000)');
    // More Functions has the other categories.
    await select(page, 1, 0);
    await page.getByRole('button', { name: 'More Functions' }).click();
    await page.getByRole('menuitem', { name: 'Statistical', exact: true }).hover();
    await expect(page.getByRole('menuitem', { name: 'MEDIAN', exact: true })).toBeVisible();
    await page.keyboard.press('Escape');
  });

  test('formulas: recently used lists the functions inserted last', async ({ page }) => {
    await openGrid(page);
    await page.evaluate(() => localStorage.removeItem('kiln.grid.recent_functions'));
    await select(page, 0, 0);
    await focusSheet(page);
    await tab(page, 'Formulas');
    await menu(page, 'Logical', ['IFERROR']);
    const a = dialog(page, 'Function Arguments');
    await a.getByTestId('grid-fn-arg-0').fill('1/0');
    await a.getByTestId('grid-fn-arg-1').fill('"none"');
    await a.getByRole('button', { name: 'OK' }).click();
    await expect.poll(() => value(page, 0, 0)).toBe('none');
    await expectSheetFocused(page);
    await page.getByRole('button', { name: 'Recently Used' }).click();
    const d = page.getByTestId('grid-insert-function');
    const list = d.getByRole('listbox', { name: 'Functions' });
    await expect(list.getByRole('option').first()).toHaveText('IFERROR');
    await expect(list.getByRole('option', { name: 'SUM', exact: true })).toBeVisible();
  });

  test('formulas: name manager, define name, paste names and create from selection', async ({ page }) => {
    await budget(page);
    await select(page, 1, 2, 6, 2);
    await focusSheet(page);
    await tab(page, 'Formulas');
    await page.getByRole('button', { name: 'Define Name…' }).first().click();
    const n = page.getByTestId('grid-define-name');
    await expect(n.getByTestId('grid-name-refers')).toHaveValue('=Sheet1!$C$2:$C$7');
    await n.getByTestId('grid-name-name').fill('Amounts');
    await n.getByRole('button', { name: 'OK' }).click();
    await expect(n).toBeHidden();
    // Use in a formula: Paste Names while typing.
    await select(page, 9, 2);
    await focusSheet(page);
    await page.keyboard.type('=SUM(');
    await page.getByRole('button', { name: 'Paste Names…' }).click();
    const pn = page.getByTestId('grid-paste-name');
    await pn.getByRole('option', { name: 'Amounts' }).click();
    await pn.getByRole('button', { name: 'OK' }).click();
    await page.keyboard.type(')');
    await page.keyboard.press('Enter');
    await expect.poll(() => value(page, 9, 2)).toBe('2930');
    // Create from Selection: the top row names the columns.
    await select(page, 0, 0, 6, 1);
    await focusSheet(page);
    await page.getByRole('button', { name: 'Create Names from Selection' }).click();
    const cn = page.getByTestId('grid-create-names');
    await cn.getByRole('button', { name: 'OK' }).click();
    // The Name Manager lists them all.
    await page.getByRole('button', { name: 'Name Manager' }).click();
    const nm = page.getByTestId('grid-name-manager');
    await expect(nm.getByTestId('grid-name-row')).toHaveText([/Amounts/, /Category/, /Item/]);
    await page.waitForTimeout(400);
    await page.screenshot({ path: shot('features-name-manager.png') });
    await nm.getByTestId('grid-name-row').filter({ hasText: 'Item' }).click();
    await nm.getByRole('button', { name: 'Delete' }).click();
    await expect(nm.getByTestId('grid-name-row')).toHaveCount(2);
    await nm.getByRole('button', { name: 'Close' }).last().click();
    await expectSheetFocused(page);
  });

  test('formulas: trace dependents', async ({ page }) => {
    await openGrid(page);
    await fill(page, [['5'], ['=A1*2'], ['=A1+A2']]);
    await select(page, 0, 0);
    await focusSheet(page);
    await tab(page, 'Formulas');
    await page.getByRole('button', { name: 'Trace Dependents' }).click();
    await until(page, (s) => s.features.arrows, 'arrows');
    await expectSheetFocused(page);
    await page.waitForTimeout(300);
    const a = await cellPoint(page, 0, 0, 0, 0);
    expect(
      await countColor(page, { x: a.x, y: a.y, width: 64, height: 60 }, [0x31, 0x59, 0xc6], 60),
    ).toBeGreaterThan(20);
    await menu(page, 'More options for Remove Arrows', ['Remove Dependent Arrows']);
    await until(page, (s) => !s.features.arrows, 'removed');
  });

  test('formulas: error checking walks the errors', async ({ page }) => {
    await openGrid(page);
    await fill(page, [['=1/0'], ['=NA()'], ['3']]);
    await tab(page, 'Formulas');
    await page.getByRole('button', { name: 'Error Checking' }).click();
    const d = page.getByTestId('grid-error-checking');
    await expect(d).toContainText('Divide by Zero Error');
    await expect(d).toContainText('1 of 2');
    await d.getByRole('button', { name: 'Next' }).click();
    await expect(d).toContainText('2 of 2');
    await page.waitForTimeout(400);
    await page.screenshot({ path: shot('features-error-checking.png') });
    await d.getByRole('button', { name: 'Close' }).last().click();
    await expectSheetFocused(page);
  });

  test('formulas: watch window follows a cell', async ({ page }) => {
    await budget(page);
    await fill(page, [['=SUM(C2:C7)']], 8, 2);
    await select(page, 8, 2);
    await focusSheet(page);
    await tab(page, 'Formulas');
    await page.getByRole('button', { name: 'Watch Window' }).click();
    const w = page.getByTestId('grid-watch-window');
    await w.getByRole('button', { name: /Add Watch/ }).click();
    await expect(w).toContainText('2930');
    await fill(page, [['1000']], 4, 2);
    await expect(w).toContainText('3930');
    await page.screenshot({ path: shot('features-watch-window.png') });
    await w.getByRole('button', { name: 'Close Watch Window' }).click();
    await expect(w).toBeHidden();
    await expectSheetFocused(page);
  });

  test('formulas: calculation options and calculate now', async ({ page }) => {
    await openGrid(page);
    await fill(page, [['1'], ['=A1*10']]);
    await tab(page, 'Formulas');
    await menu(page, 'Calculation Options', ['Manual']);
    await until(page, (s) => s.features.calc === 'Manual', 'manual');
    await fill(page, [['2']]);
    expect(await value(page, 1, 0)).toBe('10');
    await expect(page.getByText('Calculate', { exact: true }).last()).toBeVisible();
    await page.getByRole('button', { name: 'Calculate Now' }).click();
    await expect.poll(() => value(page, 1, 0)).toBe('20');
    await menu(page, 'Calculation Options', ['Automatic']);
    await until(page, (s) => s.features.calc === 'Automatic', 'automatic');
    await fill(page, [['3']]);
    await expect.poll(() => value(page, 1, 0)).toBe('30');
  });

  test('formulas: iterative calculation and the circular reference warning', async ({ page }) => {
    await openGrid(page);
    await fill(page, [['=B1+1']]);
    await fill(page, [['=A1/2']], 0, 1);
    const w = page.getByTestId('grid-circular');
    await expect(w).toBeVisible();
    await w.getByRole('button', { name: 'OK' }).click();
    await expect(page.getByText(/Circular References: A1/)).toBeVisible();
    await expectSheetFocused(page);
    await tab(page, 'Formulas');
    await menu(page, 'Calculation Options', ['Iterative Calculation…']);
    const d = page.getByTestId('grid-iterative');
    await d.getByTestId('grid-iterative-on').check();
    await d.getByTestId('grid-max-change').fill('0.00001');
    await d.getByRole('button', { name: 'OK' }).click();
    await expectSheetFocused(page);
    await page.keyboard.press('F9');
    await expect.poll(async () => Math.abs(Number(await value(page, 0, 0)) - 2) < 0.001).toBe(true);
    await expect(page.getByText(/Circular References/)).toBeHidden();
  });

  test('data: clear and reapply a filter', async ({ page }) => {
    await budget(page);
    await fill(page, [['=SUBTOTAL(103,A2:A7)']], 0, 5);
    await select(page, 1, 0);
    await focusSheet(page);
    await core(page, 'ToggleFilter');
    await core(page, {
      FilterColumn: { row: 0, col: 1, filter: { Values: { values: ['Firing'], blanks: false } } },
    });
    await expect.poll(() => value(page, 0, 5)).toBe('2');
    // A new Firing row is not shown until Reapply.
    await fill(page, [['Kiln wash', 'Firing', '45']], 3, 0);
    await tab(page, 'Data');
    await page.locator('[data-command="grid.data.reapply_filter"]').first().click();
    await expect.poll(() => value(page, 0, 5)).toBe('2');
    await page.locator('[data-command="grid.data.clear_filter"]').first().click();
    await expect.poll(() => value(page, 0, 5)).toBe('6');
    expect((await gridState(page)).features.filter).toBe(true);
    await expectSheetFocused(page);
  });

  test('data: validation rules with an error alert', async ({ page }) => {
    await openGrid(page);
    await select(page, 0, 0, 9, 0);
    await focusSheet(page);
    await tab(page, 'Data');
    await page.locator('[data-command="grid.data.data_validation"]').first().click();
    const d = dialog(page, 'Data Validation');
    await d.getByRole('combobox', { name: 'Allow' }).click();
    await page.getByRole('option', { name: 'Whole number' }).click();
    await d.getByTestId('grid-validation-source').fill('1');
    await d.getByRole('textbox', { name: 'Maximum' }).fill('10');
    await d.getByRole('tab', { name: 'Error Alert' }).click();
    await d.getByRole('textbox', { name: 'Title' }).fill('Kiln shelves');
    await d.getByRole('textbox', { name: 'Error message' }).fill('Between 1 and 10 shelves');
    await page.screenshot({ path: shot('features-validation-dialog.png') });
    await d.getByRole('button', { name: 'OK' }).click();
    await expect(d).toBeHidden();
    await expectSheetFocused(page);
    const v = await ask<{ kind: string; formula1: string; formula2: string; error: string } | null>(
      page,
      'Validation',
    );
    expect(v).toMatchObject({
      kind: 'Whole',
      formula1: '1',
      formula2: '10',
      error: 'Between 1 and 10 shelves',
    });
    await select(page, 2, 0);
    await focusSheet(page);
    await page.keyboard.type('12');
    await page.keyboard.press('Enter');
    // Excel's Stop alert: Retry goes back to the entry, Cancel leaves the cell as it was.
    const alert = page.getByTestId('grid-validation-alert');
    await expect(alert).toContainText('Between 1 and 10 shelves');
    await expect(
      alert.getByRole('heading', { name: 'Kiln shelves' }).or(alert.getByText('Kiln shelves')),
    ).toBeVisible();
    await page.screenshot({ path: shot('features-validation-alert.png') });
    await alert.getByRole('button', { name: 'Cancel', exact: true }).click();
    await expect(alert).toBeHidden();
    await expectSheetFocused(page);
    expect(await value(page, 2, 0)).toBe('');
    await select(page, 2, 0);
    await focusSheet(page);
    await page.keyboard.type('7');
    await page.keyboard.press('Enter');
    await expect.poll(() => value(page, 2, 0)).toBe('7');
  });

  test('review: spelling finds and changes a misspelling', async ({ page }) => {
    await openGrid(page);
    await fill(page, [['Blue glazze'], ['Stoneware']]);
    await select(page, 0, 0);
    await focusSheet(page);
    await page.keyboard.press('F7');
    const pane = page.getByTestId('grid-spelling');
    await expect(pane).toContainText('glazze', { timeout: 20_000 });
    await pane.getByRole('option', { name: 'glaze', exact: true }).click();
    await pane.getByTestId('grid-spell-change').click();
    await expect(pane).toContainText('Spell check complete');
    await expect.poll(() => value(page, 0, 0)).toBe('Blue glaze');
    await page.screenshot({ path: shot('features-spelling.png') });
    await pane.getByRole('button', { name: 'Close Spelling' }).click();
    await expectSheetFocused(page);
  });

  test('table design: rename and resize', async ({ page }) => {
    await budget(page);
    await core(page, {
      CreateTable: { range: { r0: 0, c0: 0, r1: 6, c1: 2 }, header: true, style: 'TableStyleMedium2' },
    });
    await select(page, 2, 1);
    await page.getByRole('tab', { name: /Table Design/ }).click();
    const name = page.getByRole('combobox', { name: 'Table Name' });
    await name.fill('Costs');
    await name.press('Enter');
    await until(page, (s) => s.features.table?.name === 'Costs', 'renamed');
    await page.locator('[data-command="ui.grid.resize_table"]').first().click();
    const d = dialog(page, 'Resize Table');
    await d.getByRole('textbox').fill('$A$1:$C$5');
    await d.getByRole('button', { name: 'OK' }).click();
    await until(page, (s) => s.features.table?.range.r1 === 4, 'resized');
    await expectSheetFocused(page);
    await fill(page, [['=SUM(Costs[Amount])']], 9, 0);
    await expect.poll(() => value(page, 9, 0)).toBe('1910');
  });

  test('table design: convert to range', async ({ page }) => {
    await budget(page);
    await core(page, {
      CreateTable: { range: { r0: 0, c0: 0, r1: 6, c1: 2 }, header: true, style: 'TableStyleMedium2' },
    });
    await select(page, 2, 1);
    await page.getByRole('tab', { name: /Table Design/ }).click();
    await page.locator('[data-command="ui.grid.convert_to_range"]').first().click();
    await until(page, (s) => !s.features.table, 'no table');
    // The banding stays as cell formatting.
    await select(page, 0, 0);
    expect((await gridState(page)).format.fill_color).not.toBeNull();
    await expectSheetFocused(page);
  });

  test('table design: styles and calculated columns', async ({ page }) => {
    await budget(page);
    await core(page, {
      CreateTable: { range: { r0: 0, c0: 0, r1: 6, c1: 2 }, header: true, style: 'TableStyleMedium2' },
    });
    await select(page, 2, 1);
    await page.getByRole('tab', { name: /Table Design/ }).click();
    await page
      .getByRole('listbox', { name: 'Table Styles' })
      .getByRole('option', { name: 'Table Style Medium 4' })
      .click();
    await until(page, (s) => s.features.table?.style === 'TableStyleMedium4', 'restyled');
    // A formula typed in a new column fills the column (a calculated column).
    await fill(page, [['Double']], 0, 3);
    await until(
      page,
      (s) => (s.features.table?.range as { c1?: number } | undefined)?.c1 === 3,
      'the table grew',
    );
    await fill(page, [['=[@Amount]*2']], 1, 3);
    await expect.poll(() => values(page, 1, 3, 6, 3)).toBe('840\n620\n2360\n0\n760\n1280');
    await page.screenshot({ path: shot('features-table-styles.png') });
  });

  test('insert: text box', async ({ page }) => {
    await openGrid(page);
    await select(page, 2, 1);
    await focusSheet(page);
    await tab(page, 'Insert');
    await page.locator('[data-command="suite.insert.text_box"]').first().click();
    const t = dialog(page, 'Edit Text');
    await t.getByTestId('grid-shape-text-input').fill('Firing schedule');
    await t.getByRole('button', { name: 'OK' }).click();
    const s = await until(page, (s) => s.drawing?.object?.text === 'Firing schedule', 'the text box');
    expect(s.drawing?.name).toBe('TextBox 1');
    await expectSheetFocused(page);
  });

  test('visual: the features in the dark theme and on a phone', async ({ page }) => {
    test.setTimeout(120_000);
    await page.emulateMedia({ colorScheme: 'dark' });
    await budget(page);
    await select(page, 1, 2, 6, 2);
    await focusSheet(page);
    await tab(page, 'Home');
    await menu(page, 'Conditional Formatting', ['Data Bars', 'Blue Data Bar']);
    await select(page, 1, 0);
    await core(page, 'ToggleFilter');
    await core(page, {
      CreateTable: { range: { r0: 0, c0: 4, r1: 3, c1: 5 }, header: true, style: 'TableStyleMedium2' },
    }).catch(() => {});
    await select(page, 3, 0);
    await page.waitForTimeout(400);
    await page.screenshot({ path: shot('features-dark-desktop.png') });
    const p = await cellPoint(page, 0, 1, 0.86, 0.5);
    await page.mouse.click(p.x, p.y);
    await expect(page.getByTestId('grid-filter-menu')).toBeVisible();
    await page.screenshot({ path: shot('features-filter-menu-dark.png') });
    await page.keyboard.press('Escape');
    await tab(page, 'Formulas');
    await select(page, 9, 2);
    await fill(page, [['=IF(SUM(C2:C4)>1000,"high","low")']], 9, 2);
    await select(page, 9, 2);
    await page.getByRole('button', { name: 'Formula Lab' }).last().click();
    await page.waitForTimeout(300);
    await page.screenshot({ path: shot('features-formula-lab-dark.png') });
    // A phone: the panes and dialogs fit the width.
    await page.setViewportSize({ width: 390, height: 844 });
    await page.waitForTimeout(500);
    await page.screenshot({ path: shot('features-formula-lab-dark-phone.png') });
    await page.emulateMedia({ colorScheme: 'light' });
    await page.keyboard.press('Control+h');
    await expect(dialog(page, 'Find and Replace')).toBeVisible();
    await page.waitForTimeout(400);
    await page.screenshot({ path: shot('features-find-phone.png') });
    await page.keyboard.press('Escape');
    await run(page, 'grid.data.data_validation');
    await expect(dialog(page, 'Data Validation')).toBeVisible();
    await page.waitForTimeout(400);
    await page.screenshot({ path: shot('features-validation-phone.png') });
  });

  // ── review fixes ─────────────────────────────────────────────────────────────────────────

  test('filter: Delete in a filtered list clears the shown cells and keeps the hidden ones', async ({
    page,
  }) => {
    await budget(page);
    await core(page, 'ToggleFilter');
    await core(page, {
      FilterColumn: { row: 0, col: 1, filter: { Values: { values: ['Firing'], blanks: false } } },
    });
    await until(page, (s) => s.features.filtered, 'rows are filtered');
    // The status bar counts the shown cells only.
    await select(page, 1, 2, 6, 2);
    await expect(page.getByText(/Sum\s*1,?820/)).toBeVisible();
    await select(page, 1, 0, 6, 2);
    await focusSheet(page);
    await page.keyboard.press('Delete');
    await expect.poll(() => value(page, 3, 0)).toBe('');
    await core(page, 'ClearFilter');
    await until(page, (s) => !s.features.filtered, 'the filter is cleared');
    expect(await values(page, 1, 0, 6, 0)).toBe(['Clay', 'Glaze', '', 'Shelves', 'Open day', ''].join('\n'));
    expect(await value(page, 2, 2)).toBe('310');
  });

  test('filter: Alt+Down opens the AutoFilter menu with the keys in it', async ({ page }) => {
    await budget(page);
    await core(page, 'ToggleFilter');
    await select(page, 0, 1);
    await focusSheet(page);
    await page.keyboard.press('Alt+ArrowDown');
    const m = page.getByTestId('grid-filter-menu');
    await expect(m).toBeVisible();
    // Never the header's editor; the menu has the focus at once.
    expect((await gridState(page)).mode).toBe('Ready');
    await expect(m.getByTestId('grid-filter-search')).toBeFocused();
    // Esc closes it and gives the keys back to the cell.
    await page.keyboard.press('Escape');
    await expect(m).toBeHidden();
    await expectSheetFocused(page);
    // Typing searches; Enter applies the values found.
    await page.keyboard.press('Alt+ArrowDown');
    await expect(m.getByTestId('grid-filter-search')).toBeFocused();
    await page.keyboard.type('fir');
    await expect(m.getByTestId('grid-filter-value')).toHaveCount(1);
    await page.keyboard.press('Enter');
    await expect(m).toBeHidden();
    await until(page, (s) => s.features.filtered, 'filtered to Firing');
    await expectSheetFocused(page);
    // The arrows walk the list; Space ticks a value; Enter on it applies.
    await page.keyboard.press('Alt+ArrowDown');
    await expect(m.getByTestId('grid-filter-search')).toBeFocused();
    const target = m.getByTestId('grid-filter-value').filter({ hasText: 'Materials' }).locator('input');
    for (let i = 0; i < 40; i++) {
      if (await target.evaluate((el) => el === document.activeElement)) break;
      await page.keyboard.press('ArrowDown');
    }
    await expect(target).toBeFocused();
    await page.keyboard.press('Space');
    await expect(target).toBeChecked();
    await page.keyboard.press('Enter');
    await expect(m).toBeHidden();
    await expect.poll(() => values(page, 1, 1, 3, 1)).toBe(['Materials', 'Materials', 'Firing'].join('\n'));
    await expectSheetFocused(page);
  });

  test('alt+down: a validation list and a table total row open their lists', async ({ page }) => {
    await budget(page);
    await core(page, {
      CreateTable: { range: { r0: 0, c0: 0, r1: 6, c1: 2 }, header: true, style: 'TableStyleMedium2' },
    });
    await select(page, 2, 1);
    const t = (await gridState(page)).features.table;
    if (!t) throw new Error('no table');
    await core(page, { TableOptions: { options: { ...t.options, total_row: true } } });
    await until(page, (s) => s.features.table?.range.r1 === 7, 'the total row');
    // The total row cell: its functions, by keyboard.
    await select(page, 7, 2);
    await focusSheet(page);
    await page.keyboard.press('Alt+ArrowDown');
    const totals = page.getByTestId('grid-totals-menu');
    await expect(totals).toBeVisible();
    // The column's function (Sum) has the focus; the arrows move from it.
    await expect(totals.getByRole('option', { name: 'Sum' })).toBeFocused();
    for (let i = 0; i < 5; i++) await page.keyboard.press('ArrowUp');
    await expect(totals.getByRole('option', { name: 'Average' })).toBeFocused();
    await page.keyboard.press('Enter');
    await expect(totals).toBeHidden();
    await until(page, (s) => s.formula.includes('SUBTOTAL(101'), 'the total row averages');
    await expectSheetFocused(page);
    // Its in-cell button opens the same list.
    const b = await cellPoint(page, 7, 2, 1.12, 0.6);
    await page.mouse.click(b.x, b.y);
    await expect(totals).toBeVisible();
    await page.keyboard.press('Escape');
    await expect(totals).toBeHidden();
    // A validation list cell: its pick list.
    await core(page, { SelectCell: { row: 10, col: 0, extend: false, add: false } });
    await core(page, {
      SetValidation: {
        rule: {
          kind: 'List',
          op: 'Between',
          formula1: 'Low,Medium,High',
          formula2: '',
          allow_blank: true,
          dropdown: true,
          show_input: false,
          input_title: '',
          input: '',
          show_error: true,
          style: 'Stop',
          error_title: '',
          error: '',
        },
      },
    });
    await select(page, 10, 0);
    await focusSheet(page);
    await page.keyboard.press('Alt+ArrowDown');
    const list = page.getByRole('listbox', { name: 'Pick from list' });
    await expect(list.getByRole('option')).toHaveCount(3);
    await page.keyboard.press('Escape');
  });

  test('alt+down: the list opens for the cell the core has, even before the editor catches up', async ({
    page,
  }) => {
    await budget(page);
    await core(page, {
      CreateTable: { range: { r0: 0, c0: 0, r1: 6, c1: 2 }, header: true, style: 'TableStyleMedium2' },
    });
    await select(page, 2, 1);
    const t = (await gridState(page)).features.table;
    if (!t) throw new Error('no table');
    await focusSheet(page);
    // Frames held back: the editor's copy of the state (fetched a frame after each change) is
    // still the old one when the key comes, as when a key follows another at once.
    await page.evaluate(() => {
      const w = window as unknown as { __raf: typeof requestAnimationFrame; __held: FrameRequestCallback[] };
      w.__raf = window.requestAnimationFrame;
      w.__held = [];
      window.requestAnimationFrame = (cb) => {
        w.__held.push(cb);
        return 0;
      };
    });
    await core(page, { TableOptions: { options: { ...t.options, total_row: true } } });
    await until(page, (s) => s.features.table?.range.r1 === 7, 'the total row');
    await select(page, 7, 2);
    await page.keyboard.press('Alt+ArrowDown');
    await page.evaluate(() => {
      const w = window as unknown as { __raf: typeof requestAnimationFrame; __held: FrameRequestCallback[] };
      window.requestAnimationFrame = w.__raf;
      for (const cb of w.__held.splice(0)) w.__raf(cb);
    });
    const totals = page.getByTestId('grid-totals-menu');
    await expect(totals).toBeVisible();
    await expect(totals.getByRole('option', { name: 'Sum' })).toBeFocused();
    await page.keyboard.press('Escape');
    await expect(totals).toBeHidden();
    await expectSheetFocused(page);
  });

  test('dialogs and panes keep their keys: Ctrl+A, Ctrl+B and Ctrl+1 stay in the text box', async ({
    page,
  }) => {
    await budget(page);
    await select(page, 1, 0);
    await focusSheet(page);
    await page.keyboard.press('Control+h');
    const d = dialog(page, 'Find and Replace');
    const what = d.getByRole('textbox', { name: 'Find what' });
    await what.fill('glaze');
    await what.focus();
    await page.keyboard.press('Control+a');
    await page.keyboard.type('kiln');
    await expect(what).toHaveValue('kiln');
    await page.keyboard.press('Control+b');
    await page.keyboard.press('Control+1');
    await expect(dialog(page, 'Format Cells')).toBeHidden();
    await page.keyboard.press('Escape');
    await expect(d).toBeHidden();
    await expectSheetFocused(page);
    expect((await gridState(page)).format.bold).toBe(false);
    // The comment composer of the Comments pane.
    await tab(page, 'Insert');
    await page.locator('[data-command="suite.review.new_comment"]').first().click();
    const draft = page.getByTestId('grid-comment-draft');
    await draft.fill('first');
    await draft.focus();
    await page.keyboard.press('Control+a');
    await page.keyboard.type('second');
    await expect(draft).toHaveValue('second');
    await page.keyboard.press('Control+b');
    expect((await gridState(page)).format.bold).toBe(false);
    await page.keyboard.press('Escape');
    await expectSheetFocused(page);
  });

  test('notes: the editor saves once, to the cell it was opened on', async ({ page }) => {
    await budget(page);
    await select(page, 0, 3);
    await focusSheet(page);
    await page.keyboard.press('Shift+F2');
    const n = page.getByTestId('grid-note-editor');
    await n.getByRole('textbox', { name: 'Note' }).fill('Units shipped');
    await page.keyboard.press('Escape');
    await expect(n).toBeHidden();
    await expectSheetFocused(page);
    await select(page, 3, 2);
    await focusSheet(page);
    // A comment elsewhere afterwards (Ctrl+Alt+M) leaves the note where it was.
    await page.keyboard.press('Control+Alt+m');
    await page.keyboard.type('Biggest order');
    await page.keyboard.press('Control+Enter');
    await expectSheetFocused(page);
    const list = await ask<{ cell: string; text: string; note: boolean }[]>(page, 'Comments');
    expect(list.filter((c) => c.note).map((c) => [c.cell, c.text])).toEqual([['D1', 'Units shipped']]);
    expect(list.filter((c) => !c.note).map((c) => [c.cell, c.text])).toEqual([['C4', 'Biggest order']]);
  });

  test('formulas: structured references autocomplete inside a table', async ({ page }) => {
    await budget(page);
    await core(page, {
      CreateTable: { range: { r0: 0, c0: 0, r1: 6, c1: 2 }, header: true, style: 'TableStyleMedium2' },
    });
    await select(page, 1, 2);
    const name = (await gridState(page)).features.table?.name ?? 'Table1';
    // Outside the table: the columns and the special items.
    await select(page, 9, 0);
    await focusSheet(page);
    await page.keyboard.type(`=SUM(${name}[`);
    const list = page.getByRole('listbox', { name: 'Suggestions' });
    for (const label of ['Item', 'Category', 'Amount', '#All', '#Data', '#Headers', '#Totals', '#This Row']) {
      await expect(list.getByRole('option').filter({ hasText: label })).toHaveCount(1);
    }
    await page.keyboard.type('Am');
    await expect(list.getByRole('option')).toHaveCount(1);
    await page.keyboard.press('Tab');
    await page.keyboard.type(')');
    await page.keyboard.press('Enter');
    await expect.poll(() => value(page, 9, 0)).toBe('2930');
    // In the table: @ (This Row), then its columns.
    await select(page, 1, 2);
    await focusSheet(page);
    await page.keyboard.type('=[');
    await expect(list.getByRole('option').first()).toContainText('@');
    await page.keyboard.press('Tab');
    await expect(list.getByRole('option', { name: /@Amount/ })).toHaveCount(1);
    await page.keyboard.press('Escape');
  });

  test('find: wildcards and long cells stay fast', { tag: '@budget' }, async ({ page }) => {
    test.setTimeout(120_000);
    await openGrid(page);
    const spec = (text: string) => ({
      text,
      match_case: false,
      whole_cell: false,
      look_in: 'Values',
      workbook: false,
      by_columns: false,
    });
    const timed = (q: unknown) =>
      page.evaluate(async (query) => {
        // biome-ignore lint/suspicious/noExplicitAny: the test handle
        const g = (window as any).__kilnGrid;
        const t = performance.now();
        await g.client.query(g.doc, query);
        return performance.now() - t;
      }, q);
    await core(page, { BeginEdit: { text: 'a'.repeat(40) } });
    await core(page, { CommitEdit: { advance: 'Down' } });
    const long = 'lorem ipsum '.repeat(1500);
    for (let i = 0; i < 10; i++) {
      await core(page, { BeginEdit: { text: long } });
      await core(page, { CommitEdit: { advance: 'Down' } });
    }
    expect(await timed({ Grid: { FindAll: { spec: spec('*a*a*a*a*a*a*b') } } })).toBeLessThan(1000);
    expect(await timed({ Grid: { FindAll: { spec: spec('zzz') } } })).toBeLessThan(1000);
    expect(await timed({ Grid: { FindAll: { spec: spec('*?*?*?*z') } } })).toBeLessThan(1000);
  });
});
