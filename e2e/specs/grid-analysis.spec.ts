/**
 * Kiln Grid analysis end to end, as an Excel user does it in the web build: Insert ▸
 * PivotTable and the PivotTable Fields pane (ticks and drags between the areas), Value Field
 * Settings, the field buttons' sort and filter, the Design tab's layouts and styles, Refresh after
 * the source changes, Show Details by double-clicking a value; Insert ▸ Sparklines and the
 * Sparkline tab; Goal Seek; Data ▸ Subtotal; Flash Fill with Ctrl+E and its suggestions; Protect
 * Sheet with a password and locked cells refusing edits; Protect Workbook; the Themes gallery; and
 * saving an .xlsx that opens again. The workbook is checked through the core, the keys come back
 * to the sheet afterwards, and screenshots go to e2e/screenshots/grid/ for the visual review.
 */
import { execFileSync } from 'node:child_process';
import { existsSync, mkdirSync, writeFileSync } from 'node:fs';
import { expect, type Page, test } from '@playwright/test';
import {
  core,
  expectSheetFocused,
  fill,
  focusSheet,
  gridState,
  openGrid,
  origin,
  repo,
  run,
  select,
  shot,
  tab,
  value,
  values,
} from './grid-helpers.ts';

/** Region, Product, Quarter, Units at A1:D13. */
const SALES: readonly (readonly string[])[] = [
  ['Region', 'Product', 'Quarter', 'Units'],
  ['East', 'Apple', 'Q1', '10'],
  ['West', 'Banana', 'Q1', '5'],
  ['East', 'Banana', 'Q2', '20'],
  ['West', 'Apple', 'Q2', '7'],
  ['East', 'Apple', 'Q3', '1'],
  ['North', 'Cherry', 'Q3', '12'],
  ['North', 'Apple', 'Q4', '8'],
  ['West', 'Cherry', 'Q4', '14'],
  ['East', 'Cherry', 'Q4', '3'],
  ['South', 'Banana', 'Q1', '9'],
  ['South', 'Apple', 'Q2', '11'],
  ['North', 'Banana', 'Q3', '6'],
];

const dialog = (page: Page, name: string | RegExp) => page.getByRole('dialog', { name });

/**
 * A dialog's first field has the keys: it is focused, and what the keyboard types lands in it
 * (not in the sheet behind).
 */
async function typeInFocused(field: ReturnType<Page['getByRole']>, page: Page, text: string): Promise<void> {
  await expect(field).toBeFocused();
  await page.keyboard.type(text);
  await expect(field).toHaveValue(text);
}

interface Analysis {
  pivot: {
    name: string;
    source: string;
    rows: number[];
    cols: number[];
    pages: [number, string | null][];
    values: { name: string; function: string; show_as: string }[];
    fields: { name: string; area: string | null; filtered: boolean }[];
    options: { layout: string; style: string; subtotals: string };
    on_value: boolean;
  } | null;
  sparkline: { options: { kind: string; high: boolean; colors: string[] }; count: number } | null;
  locked: boolean;
  goal_seek: { found: boolean; current: string } | null;
  flash: { text: string }[];
  sheet_protected: boolean;
  workbook_protected: boolean;
  alert: string | null;
  theme: string;
}

/** The workbook the last Save As wrote into the origin's private file system. */
async function savedXlsx(page: Page): Promise<Buffer> {
  let bytes: number[] | null = null;
  await expect
    .poll(
      async () => {
        bytes = await page.evaluate(async () => {
          try {
            const root = await navigator.storage.getDirectory();
            const dir = await root.getDirectoryHandle('documents');
            for await (const [name, h] of dir as unknown as AsyncIterable<[string, FileSystemFileHandle]>) {
              if (name.endsWith('.xlsx'))
                return Array.from(new Uint8Array(await (await h.getFile()).arrayBuffer()));
            }
          } catch {
            // Not saved yet.
          }
          return null;
        });
        return bytes !== null && bytes.length > 0;
      },
      { message: 'the workbook is saved', timeout: 20_000 },
    )
    .toBe(true);
  return Buffer.from(bytes ?? []);
}

async function analysis(page: Page): Promise<Analysis> {
  return ((await gridState(page)) as unknown as { analysis: Analysis }).analysis;
}

async function salesSheet(page: Page): Promise<void> {
  await openGrid(page);
  await fill(page, SALES);
  await select(page, 1, 0);
  await focusSheet(page);
}

/** Insert ▸ PivotTable on a new sheet, from the data around the active cell. */
async function createPivot(page: Page): Promise<void> {
  await tab(page, 'Insert');
  await page.getByRole('button', { name: 'PivotTable', exact: true }).click();
  const d = dialog(page, 'Create PivotTable');
  await expect(d).toBeVisible();
  await expect(d.getByTestId('grid-pivot-source')).toHaveValue('Sheet1!$A$1:$D$13');
  await expect(d.getByTestId('grid-pivot-source')).toBeFocused();
  await d.getByRole('button', { name: 'OK' }).click();
  await expect(d).toBeHidden();
  await expect(page.getByTestId('grid-pivot-fields')).toBeVisible();
}

/** The page point of a cell (from the core's rectangle of the active cell), `fx`/`fy` across it. */
async function cellAt(
  page: Page,
  row: number,
  col: number,
  fx = 0.5,
  fy = 0.5,
): Promise<{ x: number; y: number }> {
  await select(page, row, col);
  const s = (await gridState(page)) as unknown as { features: { cell_rect: number[] } };
  const [x = 0, y = 0, w = 0, h = 0] = s.features.cell_rect;
  const o = await origin(page);
  return { x: o.x + x + w * fx, y: o.y + y + h * fy };
}

/** A Select of a dialog: open it and pick an option. */
async function choose(
  page: Page,
  scope: ReturnType<Page['getByRole']>,
  label: string,
  option: string | RegExp,
) {
  await scope.getByRole('combobox', { name: label }).click();
  await page.getByRole('option', { name: option, exact: typeof option === 'string' }).click();
}

const pane = (page: Page) => page.getByTestId('grid-pivot-fields');
const fieldBox = (page: Page, name: string) =>
  pane(page).locator('.kiln-gan__field').filter({ hasText: name }).getByRole('checkbox');

test.describe('Kiln Grid analysis', () => {
  test.beforeEach(({ page }) => {
    page.on('pageerror', (e) => console.log('PAGEERROR', e.message, e.stack));
  });
  test('pivot: create from the data, tick fields, drag between areas, refresh', async ({ page }) => {
    await salesSheet(page);
    await createPivot(page);
    // A new sheet before the data, the report's top-left selected, Excel's prompt.
    const s = await gridState(page);
    expect(s.sheets.map((x) => x.name)).toEqual(['Sheet2', 'Sheet1']);
    expect(s.name_box).toBe('A3');
    await expect(page.getByTestId('grid-pivot-placeholder')).toContainText('PivotTable1');
    await page.screenshot({ path: shot('analysis-pivot-empty.png') });
    // Region ticks into Rows (text), Units into Values (numbers).
    await fieldBox(page, 'Region').click();
    await expect(fieldBox(page, 'Region')).toBeChecked();
    await fieldBox(page, 'Units').click();
    await expect(fieldBox(page, 'Units')).toBeChecked();
    await expect
      .poll(() => values(page, 2, 0, 7, 1))
      .toBe(
        [
          'Row Labels\tSum of Units',
          'East\t34',
          'North\t26',
          'South\t20',
          'West\t26',
          'Grand Total\t106',
        ].join('\n'),
      );
    await expect(pane(page).getByTestId('grid-pivot-area-rows')).toContainText('Region');
    await expect(pane(page).getByTestId('grid-pivot-area-values')).toContainText('Sum of Units');
    // Product dragged into Columns.
    await pane(page)
      .locator('.kiln-gan__field')
      .filter({ hasText: 'Product' })
      .dragTo(pane(page).getByTestId('grid-pivot-area-columns'));
    await expect
      .poll(() => values(page, 2, 0, 4, 4))
      .toBe(
        [
          'Sum of Units\tColumn Labels\t\t\t',
          'Row Labels\tApple\tBanana\tCherry\tGrand Total',
          'East\t11\t20\t3\t34',
        ].join('\n'),
      );
    expect((await analysis(page)).pivot?.cols).toEqual([1]);
    // Region dragged from Rows to Filters: the report moves down to make room.
    await pane(page)
      .getByTestId('grid-pivot-area-rows')
      .getByTestId('grid-pivot-chip')
      .dragTo(pane(page).getByTestId('grid-pivot-area-filters'));
    await expect.poll(async () => (await analysis(page)).pivot?.pages).toEqual([[0, null]]);
    await expect.poll(() => values(page, 0, 0, 0, 1)).toBe('Region\t(All)');
    // Back to Rows.
    await pane(page)
      .getByTestId('grid-pivot-area-filters')
      .getByTestId('grid-pivot-chip')
      .dragTo(pane(page).getByTestId('grid-pivot-area-rows'));
    await expect.poll(async () => (await analysis(page)).pivot?.rows).toEqual([0]);
    await page.screenshot({ path: shot('analysis-pivot-fields.png') });
    await tab(page, 'PivotTable Analyze');
    await page.screenshot({ path: shot('analysis-pivot-analyze.png') });
    // The source changes; Refresh brings it in.
    await core(page, { ActivateSheet: { index: 1 } });
    await fill(page, [['100']], 1, 3);
    await core(page, { ActivateSheet: { index: 0 } });
    await select(page, 4, 1);
    const before = await values(page, 4, 0, 4, 4);
    expect(before.split('\t')[1]).toBe('11');
    await tab(page, 'PivotTable Analyze');
    await page.getByRole('button', { name: 'Refresh', exact: true }).first().click();
    await expect.poll(() => value(page, 4, 1)).toBe('101');
    await expectSheetFocused(page);
  });

  test('pivot: value field settings, field filter, layout, style and show details', async ({ page }) => {
    await salesSheet(page);
    await createPivot(page);
    await fieldBox(page, 'Region').click();
    await fieldBox(page, 'Units').click();
    await expect.poll(() => value(page, 3, 1)).toBe('34');
    // Value Field Settings from the field's menu: % of Grand Total.
    await pane(page).getByTestId('grid-pivot-area-values').getByTestId('grid-pivot-chip').click();
    await page.getByRole('menuitem', { name: 'Value Field Settings…' }).click();
    let d = dialog(page, 'Value Field Settings');
    await expect(d).toBeVisible();
    await expect(d.getByTestId('grid-value-name')).toBeFocused();
    await d.getByRole('tab', { name: 'Show Values As' }).click();
    await choose(page, d, 'Show values as', '% of Grand Total');
    await page.screenshot({ path: shot('analysis-value-settings.png') });
    await d.getByRole('button', { name: 'OK' }).click();
    await expect(d).toBeHidden();
    await expect.poll(() => value(page, 3, 1)).toBe('32.08%');
    // Back to numbers, summarised by Average (the name follows the function).
    await pane(page).getByTestId('grid-pivot-area-values').getByTestId('grid-pivot-chip').click();
    await page.getByRole('menuitem', { name: 'Value Field Settings…' }).click();
    d = dialog(page, 'Value Field Settings');
    await d.getByRole('option', { name: 'Average' }).click();
    await expect(d.getByTestId('grid-value-name')).toHaveValue('Average of Units');
    await d.getByRole('tab', { name: 'Show Values As' }).click();
    await choose(page, d, 'Show values as', 'No Calculation');
    await d.getByRole('button', { name: 'OK' }).click();
    await expect.poll(() => values(page, 2, 0, 3, 1)).toBe('Row Labels\tAverage of Units\nEast\t8.5');
    // The Row Labels button: North unticked.
    const b = await cellAt(page, 2, 0, 1, 0.5);
    await page.mouse.click(b.x - 8, b.y);
    const m = page.getByTestId('grid-pivot-field-menu');
    await expect(m).toBeVisible();
    await m.getByTestId('grid-pivot-item').filter({ hasText: 'North' }).locator('input').uncheck();
    await page.screenshot({ path: shot('analysis-field-menu.png') });
    await m.getByTestId('grid-pivot-field-ok').click();
    await expect(m).toBeHidden();
    await expect
      .poll(() => values(page, 3, 0, 6, 0))
      .toBe(['East', 'South', 'West', 'Grand Total'].join('\n'));
    expect((await analysis(page)).pivot?.fields[0]?.filtered).toBe(true);
    // Design: Tabular Form and a Medium style.
    await select(page, 3, 1);
    await tab(page, 'Design');
    await page.getByRole('button', { name: 'Report Layout' }).click();
    await page.getByRole('menuitemcheckbox', { name: 'Show in Tabular Form' }).click();
    await expect.poll(() => value(page, 2, 0)).toBe('Region');
    await page.getByRole('button', { name: 'More PivotTable Styles' }).click();
    await page.getByRole('option', { name: 'Pivot Style Medium 9' }).first().click();
    await expect.poll(async () => (await analysis(page)).pivot?.options.style).toBe('PivotStyleMedium9');
    await page.screenshot({ path: shot('analysis-pivot-design.png') });
    await expectSheetFocused(page);
    // Double-click East's value: its rows on a new sheet.
    const v = await cellAt(page, 3, 1);
    await page.mouse.dblclick(v.x, v.y);
    await expect.poll(async () => (await gridState(page)).sheets.length).toBe(3);
    await expect
      .poll(() => values(page, 0, 0, 4, 3))
      .toBe(
        [
          'Region\tProduct\tQuarter\tUnits',
          'East\tApple\tQ1\t10',
          'East\tBanana\tQ2\t20',
          'East\tApple\tQ3\t1',
          'East\tCherry\tQ4\t3',
        ].join('\n'),
      );
    expect((await gridState(page)).features.table).not.toBeNull();
  });

  test('pivot: the field menu takes the keys, arrows move in it, Esc and a click outside close it', async ({
    page,
  }) => {
    await salesSheet(page);
    await createPivot(page);
    await fieldBox(page, 'Region').click();
    await fieldBox(page, 'Product').click();
    await fieldBox(page, 'Units').click();
    await expect.poll(() => analysis(page).then((x) => x.pivot?.rows.length)).toBe(2);
    const chip = pane(page).getByTestId('grid-pivot-area-rows').getByTestId('grid-pivot-chip').first();
    await chip.click();
    const m = page.getByRole('menu', { name: 'Field' });
    await expect(m).toBeVisible();
    // Move Up is off for the first field: Move Down has the keys.
    await expect(m.getByRole('menuitem', { name: 'Move Up' })).toBeDisabled();
    await expect(m.getByRole('menuitem', { name: 'Move Down' })).toBeFocused();
    // The arrows skip the items that cannot run (Move to Beginning) and wrap round.
    await page.keyboard.press('ArrowDown');
    await expect(m.getByRole('menuitem', { name: 'Move to End' })).toBeFocused();
    await page.keyboard.press('ArrowUp');
    await page.keyboard.press('ArrowUp');
    await expect(m.getByRole('menuitem', { name: 'Field Settings…' })).toBeFocused();
    await page.keyboard.press('Home');
    await expect(m.getByRole('menuitem', { name: 'Move Down' })).toBeFocused();
    // Esc closes it, back on the field's button.
    await page.keyboard.press('Escape');
    await expect(m).toBeHidden();
    await expect(chip).toBeFocused();
    // Enter opens it again from the keyboard; a click outside closes it, and the focus is not lost.
    await page.keyboard.press('Enter');
    await expect(m).toBeVisible();
    const head = await pane(page).getByRole('heading', { name: 'PivotTable Fields' }).boundingBox();
    await page.mouse.click((head?.x ?? 0) + 4, (head?.y ?? 0) + 4);
    await expect(m).toBeHidden();
    expect(await page.evaluate(() => document.activeElement === document.body)).toBe(false);
    await expect(chip).toBeFocused();
    // A move runs from the keyboard and hands the keys to the sheet.
    await chip.click();
    await expect(m.getByRole('menuitem', { name: 'Move Down' })).toBeFocused();
    await page.keyboard.press('Enter');
    await expect(m).toBeHidden();
    await expect
      .poll(async () => {
        const p = (await analysis(page)).pivot;
        return p?.rows.map((f) => p.fields[f]?.name);
      })
      .toEqual(['Product', 'Region']);
    await expectSheetFocused(page);
    // Esc in the pane (not in a menu) hands the keys back to the sheet.
    await pane(page).getByLabel('Search fields').focus();
    await page.keyboard.press('Escape');
    await expectSheetFocused(page);
    await expect(pane(page)).toBeVisible();
  });

  test('pivot: editing the report is refused with Excel’s message', async ({ page }) => {
    await salesSheet(page);
    await createPivot(page);
    await fieldBox(page, 'Region').click();
    await fieldBox(page, 'Units').click();
    await expect.poll(() => value(page, 3, 1)).toBe('34');
    await select(page, 3, 1);
    await focusSheet(page);
    await page.keyboard.type('9');
    const a = dialog(page, 'Kiln Grid');
    await expect(a).toContainText('Cannot change this part of a PivotTable report.');
    await page.keyboard.press('Enter');
    await expect(a).toBeHidden();
    await expect.poll(() => value(page, 3, 1)).toBe('34');
    await expectSheetFocused(page);
  });

  test('sparklines: insert from the ribbon and change them on the Sparkline tab', async ({ page }) => {
    await openGrid(page);
    await fill(page, [
      ['Kiln', 'Jan', 'Feb', 'Mar', 'Apr', 'May', 'Jun'],
      ['North', '12', '18', '9', '22', '30', '26'],
      ['South', '8', '-4', '6', '14', '-2', '10'],
      ['West', '20', '19', '25', '17', '28', '31'],
    ]);
    await select(page, 1, 1, 3, 6);
    await focusSheet(page);
    await tab(page, 'Insert');
    await page.getByRole('button', { name: 'Line Sparkline' }).click();
    const d = dialog(page, 'Create Sparklines');
    await expect(d.getByTestId('grid-spark-data')).toHaveValue('$B$2:$G$4');
    await typeInFocused(d.getByTestId('grid-spark-location'), page, '$H$2:$H$4');
    await d.getByRole('button', { name: 'OK' }).click();
    await expect(d).toBeHidden();
    await select(page, 1, 7);
    await expect.poll(async () => (await analysis(page)).sparkline?.count).toBe(3);
    await tab(page, 'Sparkline');
    await page.getByRole('button', { name: 'Column', exact: true }).click();
    await page.locator('label[data-command="ui.grid.spark_high"]').click();
    await page.locator('label[data-command="ui.grid.spark_negative"]').click();
    await expect.poll(async () => (await analysis(page)).sparkline?.options.kind).toBe('Column');
    expect((await analysis(page)).sparkline?.options.high).toBe(true);
    const before = (await analysis(page)).sparkline?.options.colors[0];
    await page
      .getByRole('option', { name: /Accent 6, Darker 50%/ })
      .first()
      .click();
    await expect.poll(async () => (await analysis(page)).sparkline?.options.colors[0]).not.toBe(before);
    await page.screenshot({ path: shot('analysis-sparklines.png') });
    // They follow their data.
    await fill(page, [['40']], 3, 1);
    await select(page, 2, 7);
    await page.getByRole('button', { name: 'Win/Loss', exact: true }).click();
    await expect.poll(async () => (await analysis(page)).sparkline?.options.kind).toBe('WinLoss');
    await expectSheetFocused(page);
  });

  test('goal seek: the input that makes a formula reach a target', async ({ page }) => {
    await openGrid(page);
    await fill(page, [
      ['Rate', '0.05'],
      ['Years', '10'],
      ['Loan', '1000'],
      ['Payment', '=PMT(B1,B2,-B3)'],
    ]);
    await select(page, 3, 1);
    await focusSheet(page);
    await tab(page, 'Data');
    await page.getByRole('button', { name: 'What-If Analysis' }).click();
    await page.getByRole('menuitem', { name: 'Goal Seek' }).click();
    const d = dialog(page, 'Goal Seek');
    await expect(d.getByTestId('grid-goal-set')).toHaveValue('$B$4');
    await typeInFocused(d.getByTestId('grid-goal-to'), page, '250');
    await d.getByTestId('grid-goal-by').fill('B3');
    await d.getByRole('button', { name: 'OK' }).click();
    const st = dialog(page, 'Goal Seek Status');
    await expect(st).toContainText('found a solution');
    await expect(st.getByTestId('grid-goal-current')).toHaveText('250');
    await page.screenshot({ path: shot('analysis-goal-seek.png') });
    await st.getByRole('button', { name: 'OK' }).click();
    await expect(st).toBeHidden();
    await expect.poll(async () => Number(await value(page, 2, 1))).toBeCloseTo(1930.43, 1);
    await expectSheetFocused(page);
  });

  test('subtotal: totals at each change with outline levels, then remove all', async ({ page }) => {
    await openGrid(page);
    await fill(page, [
      ['Region', 'Product', 'Units'],
      ['East', 'Apple', '10'],
      ['East', 'Banana', '20'],
      ['North', 'Cherry', '12'],
      ['West', 'Apple', '7'],
      ['West', 'Cherry', '14'],
    ]);
    await select(page, 1, 0);
    await focusSheet(page);
    await tab(page, 'Data');
    await page.getByRole('button', { name: 'Subtotal' }).click();
    const d = dialog(page, 'Subtotal');
    await expect(d.getByRole('combobox', { name: 'At each change in' })).toContainText('Region');
    await expect.poll(() => d.evaluate((e) => e.contains(document.activeElement))).toBe(true);
    await page.screenshot({ path: shot('analysis-subtotal.png') });
    await d.getByRole('button', { name: 'OK' }).click();
    await expect(d).toBeHidden();
    await expect
      .poll(() => values(page, 3, 0, 9, 2))
      .toBe(
        [
          'East Total\t\t30',
          'North\tCherry\t12',
          'North Total\t\t12',
          'West\tApple\t7',
          'West\tCherry\t14',
          'West Total\t\t21',
          'Grand Total\t\t63',
        ].join('\n'),
      );
    await page.screenshot({ path: shot('analysis-subtotal-outline.png') });
    // The outline gutter: button 2 shows only the totals; East Total's + opens East again.
    const o = await origin(page);
    const rowHeight = async (row: number) => {
      await select(page, row, 2);
      const st = (await gridState(page)) as unknown as { features: { cell_rect: number[] } };
      return st.features.cell_rect[3] ?? 0;
    };
    await page.mouse.click(o.x + 26, o.y + 12);
    await expect.poll(() => rowHeight(1)).toBe(0);
    expect(await rowHeight(3)).toBeGreaterThan(0);
    expect(await rowHeight(4)).toBe(0);
    await page.screenshot({ path: shot('analysis-outline-level-2.png') });
    const east = await cellAt(page, 3, 0);
    await page.mouse.click(o.x + 26, east.y);
    await expect.poll(() => rowHeight(1)).toBeGreaterThan(0);
    expect(await rowHeight(4)).toBe(0);
    await page.screenshot({ path: shot('analysis-outline-expanded.png') });
    await page.emulateMedia({ colorScheme: 'dark' });
    await page.screenshot({ path: shot('analysis-outline-dark.png') });
    await page.emulateMedia({ colorScheme: 'light' });
    // Level 3 shows every row again.
    await page.mouse.click(o.x + 42, o.y + 12);
    await expect.poll(() => rowHeight(4)).toBeGreaterThan(0);
    await page.mouse.click(o.x + 26, o.y + 12);
    await expect.poll(() => rowHeight(4)).toBe(0);
    await page.emulateMedia({ colorScheme: 'dark' });
    await page.setViewportSize({ width: 390, height: 844 });
    await expect(page.locator('[data-kiln-grid-surface]')).toBeVisible();
    await page.screenshot({ path: shot('analysis-outline-dark-phone.png') });
    await page.emulateMedia({ colorScheme: 'light' });
    await page.setViewportSize({ width: 1440, height: 900 });
    await run(page, 'grid.data.show_outline_level', { level: 3 });
    await expect.poll(() => rowHeight(4)).toBeGreaterThan(0);
    await select(page, 1, 0);
    await page.getByRole('button', { name: 'Subtotal' }).click();
    await dialog(page, 'Subtotal').getByRole('button', { name: 'Remove All' }).click();
    await expect.poll(() => values(page, 4, 0, 6, 0)).toBe(['West', 'West', ''].join('\n'));
    await expectSheetFocused(page);
  });

  test('flash fill: Ctrl+E fills the column, and suggestions show after two examples', async ({ page }) => {
    await openGrid(page);
    await fill(page, [['Full name'], ['Ana Lima'], ['Ben Okoro'], ['Caro Wit'], ['Dev Patel'], ['Eli Ng']]);
    await fill(page, [['First'], ['Ana']], 0, 1);
    await select(page, 2, 1);
    await focusSheet(page);
    await page.keyboard.press('Control+e');
    await expect.poll(() => values(page, 1, 1, 5, 1)).toBe(['Ana', 'Ben', 'Caro', 'Dev', 'Eli'].join('\n'));
    // Last names: two examples typed, the rest appear greyed; Enter takes them.
    await fill(page, [['Last'], ['Lima']], 0, 2);
    await select(page, 2, 2);
    // One example already gives suggestions, which the selection change clears: wait until the
    // window shows that, or the Enter below would accept the stale suggestions instead of
    // committing the second example.
    await expect(page.getByTestId('grid-flash-suggestion')).toHaveCount(0);
    await focusSheet(page);
    await page.keyboard.type('Okoro');
    await page.keyboard.press('Enter');
    await expect(page.getByTestId('grid-flash-suggestion')).toHaveCount(3);
    await page.screenshot({ path: shot('analysis-flash-fill.png') });
    await page.keyboard.press('Enter');
    await expect.poll(() => values(page, 3, 2, 5, 2)).toBe(['Wit', 'Patel', 'Ng'].join('\n'));
    await expect(page.getByTestId('grid-flash-suggestion')).toHaveCount(0);
    await expectSheetFocused(page);
  });

  test('protection: a protected sheet refuses edits to locked cells; the password unprotects it', async ({
    page,
  }) => {
    await openGrid(page);
    await fill(page, [
      ['Rate', '0.05'],
      ['Input', ''],
    ]);
    // B2 is for input: unlocked first (Format ▸ Lock Cell).
    await select(page, 1, 1);
    await focusSheet(page);
    await tab(page, 'Home');
    await page.getByRole('button', { name: 'Format', exact: true }).last().click();
    await page.getByRole('menuitemcheckbox', { name: 'Lock Cell' }).click();
    expect((await analysis(page)) as unknown as { locked: boolean }).toMatchObject({ locked: false });
    await tab(page, 'Review');
    await page.getByRole('button', { name: 'Protect Sheet' }).click();
    const d = dialog(page, 'Protect Sheet');
    await typeInFocused(d.getByLabel('Password to unprotect sheet'), page, 'Kiln 26');
    await page.screenshot({ path: shot('analysis-protect-sheet.png') });
    await d.getByRole('button', { name: 'OK' }).click();
    const c = dialog(page, 'Confirm Password');
    await typeInFocused(c.getByLabel('Reenter password to proceed'), page, 'Kiln 26');
    await c.getByRole('button', { name: 'OK' }).click();
    await expect(c).toBeHidden();
    await expect.poll(async () => (await analysis(page)).sheet_protected).toBe(true);
    // A locked cell: Excel's message.
    await select(page, 0, 1);
    await focusSheet(page);
    await page.keyboard.type('7');
    const a = dialog(page, 'Kiln Grid');
    await expect(a).toContainText("The cell or chart you're trying to change is on a protected sheet.");
    await page.screenshot({ path: shot('analysis-protected-alert.png') });
    await a.getByRole('button', { name: 'OK' }).click();
    await expect(a).toBeHidden();
    await expect.poll(() => value(page, 0, 1)).toBe('0.05');
    // The unlocked cell takes entries.
    await select(page, 1, 1);
    await focusSheet(page);
    await page.keyboard.type('42');
    await page.keyboard.press('Enter');
    await expect.poll(() => value(page, 1, 1)).toBe('42');
    // A wrong password, then the right one.
    await tab(page, 'Review');
    await page.getByRole('button', { name: 'Protect Sheet' }).click();
    const u = dialog(page, 'Unprotect Sheet');
    await typeInFocused(u.getByLabel('Password'), page, 'kiln 26');
    await u.getByRole('button', { name: 'OK' }).click();
    await expect(u).toContainText('The password you supplied is not correct');
    await u.getByLabel('Password').fill('Kiln 26');
    await u.getByRole('button', { name: 'OK' }).click();
    await expect(u).toBeHidden();
    await expect.poll(async () => (await analysis(page)).sheet_protected).toBe(false);
    await expectSheetFocused(page);
  });

  test('dialogs: one opened as another closes has the keys (Protect Sheet, Goal Seek, Protect Workbook)', async ({
    page,
  }) => {
    await openGrid(page);
    await fill(page, [
      ['', '0.05'],
      ['', '10'],
      ['', '1000'],
      ['', '=PMT(B1,B2,-B3)'],
    ]);
    // Goal Seek, Enter, then Goal Seek Status: Enter keeps the solution.
    await select(page, 3, 1);
    await focusSheet(page);
    await tab(page, 'Data');
    await page.getByRole('button', { name: 'What-If Analysis' }).click();
    await page.getByRole('menuitem', { name: 'Goal Seek' }).click();
    const g = dialog(page, 'Goal Seek');
    await typeInFocused(g.getByTestId('grid-goal-to'), page, '250');
    await page.keyboard.press('Tab');
    await page.keyboard.type('B3');
    await page.keyboard.press('Enter');
    const st = dialog(page, 'Goal Seek Status');
    await expect(st).toContainText('found a solution');
    // Past the first dialog's close, the status dialog still has the focus.
    await page.waitForTimeout(150);
    await expect.poll(() => st.evaluate((d) => d.contains(document.activeElement))).toBe(true);
    await page.keyboard.press('Enter');
    await expect(st).toBeHidden();
    await expect.poll(async () => Number(await value(page, 2, 1))).toBeCloseTo(1930.43, 1);
    await expectSheetFocused(page);

    // Protect Sheet: type the password, Enter, type it again, Enter. Nothing reaches A1.
    await select(page, 0, 0);
    await focusSheet(page);
    await tab(page, 'Review');
    await page.getByRole('button', { name: 'Protect Sheet' }).click();
    const d = dialog(page, 'Protect Sheet');
    await typeInFocused(d.getByLabel('Password to unprotect sheet'), page, 'pw');
    await page.keyboard.press('Enter');
    const c = dialog(page, 'Confirm Password');
    await expect(c).toBeVisible();
    await page.waitForTimeout(150);
    await typeInFocused(c.getByLabel('Reenter password to proceed'), page, 'pw');
    await page.keyboard.press('Enter');
    await expect(c).toBeHidden();
    await expect.poll(async () => (await analysis(page)).sheet_protected).toBe(true);
    expect(await value(page, 0, 0)).toBe('');
    await expectSheetFocused(page);

    // Unprotect Sheet takes the keys too.
    await page.getByRole('button', { name: 'Protect Sheet' }).click();
    const u = dialog(page, 'Unprotect Sheet');
    await typeInFocused(u.getByLabel('Password'), page, 'pw');
    await page.keyboard.press('Enter');
    await expect(u).toBeHidden();
    await expect.poll(async () => (await analysis(page)).sheet_protected).toBe(false);

    // Protect Workbook with a password, then Confirm Password.
    await page.getByRole('button', { name: 'Protect Workbook' }).click();
    const w = dialog(page, 'Protect Structure and Windows');
    await typeInFocused(w.getByLabel('Password (optional)'), page, 'wb');
    await page.keyboard.press('Enter');
    const cw = dialog(page, 'Confirm Password');
    await expect(cw).toBeVisible();
    await page.waitForTimeout(150);
    await typeInFocused(cw.getByLabel('Reenter password to proceed'), page, 'wb');
    await page.keyboard.press('Enter');
    await expect(cw).toBeHidden();
    await expect.poll(async () => (await analysis(page)).workbook_protected).toBe(true);
    expect(await value(page, 0, 0)).toBe('');
    // Unprotect Workbook.
    await page.getByRole('button', { name: 'Protect Workbook' }).click();
    const uw = dialog(page, 'Unprotect Workbook');
    await typeInFocused(uw.getByLabel('Password'), page, 'wb');
    await page.keyboard.press('Enter');
    await expect(uw).toBeHidden();
    await expect.poll(async () => (await analysis(page)).workbook_protected).toBe(false);
    await expectSheetFocused(page);
  });

  test('protect workbook: the structure is locked until unprotected', async ({ page }) => {
    await openGrid(page);
    await tab(page, 'Review');
    await page.getByRole('button', { name: 'Protect Workbook' }).click();
    const d = dialog(page, 'Protect Structure and Windows');
    await d.getByRole('button', { name: 'OK' }).click();
    await expect.poll(async () => (await analysis(page)).workbook_protected).toBe(true);
    await core(page, 'InsertSheet');
    await expect(dialog(page, 'Kiln Grid')).toContainText('Workbook is protected');
    await dialog(page, 'Kiln Grid').getByRole('button', { name: 'OK' }).click();
    expect((await gridState(page)).sheets).toHaveLength(1);
    await page.getByRole('button', { name: 'Protect Workbook' }).click();
    await expect.poll(async () => (await analysis(page)).workbook_protected).toBe(false);
  });

  test('themes: the Page Layout gallery applies a theme, its colours or its fonts', async ({ page }) => {
    await openGrid(page);
    await tab(page, 'Page Layout');
    await page.getByRole('option', { name: 'Tenmoku', exact: true }).first().click();
    await expect.poll(async () => (await analysis(page)).theme).toBe('Tenmoku');
    await page.screenshot({ path: shot('analysis-themes.png') });
    await page.getByRole('button', { name: 'Colors' }).click();
    await page.getByRole('menuitem', { name: 'Office', exact: true }).click();
    await expect.poll(async () => (await analysis(page)).theme).toBe('Office Theme');
    await run(page, 'suite.edit.undo');
    await expect.poll(async () => (await analysis(page)).theme).toBe('Tenmoku');
    await expectSheetFocused(page);
  });

  test('files: a PivotTable and sparklines saved as .xlsx open again, and LibreOffice refreshes the PivotTable', async ({
    page,
  }) => {
    test.setTimeout(120_000);
    await salesSheet(page);
    await createPivot(page);
    await fieldBox(page, 'Region').click();
    await fieldBox(page, 'Units').click();
    await expect.poll(() => value(page, 3, 1)).toBe('34');
    await core(page, { InsertSparklines: { kind: 'Column', data: 'B4:B7', location: 'D4' } });
    // File ▸ Save As ▸ Excel Workbook, into the origin's file system (no native picker in tests).
    await page.evaluate(() => {
      delete (window as unknown as { showSaveFilePicker?: unknown }).showSaveFilePicker;
    });
    await page.getByRole('button', { name: 'File' }).click();
    const fm = page.getByRole('dialog', { name: /File menu/ });
    await fm.getByRole('navigation', { name: 'File menu' }).getByRole('button', { name: 'Save As' }).click();
    await fm.getByRole('button', { name: /Excel Workbook/ }).click();
    const saved = await savedXlsx(page);
    const dir = repo('target/scratch/e2e');
    mkdirSync(dir, { recursive: true });
    writeFileSync(`${dir}/pivot-from-web.xlsx`, saved);
    // LibreOffice (when installed) opens it and refreshes the PivotTable to the same values.
    // LibreOffice (where installed) opens it and refreshes the PivotTable to the same values.
    if (existsSync('/usr/bin/soffice')) {
      const lo = JSON.parse(
        execFileSync(
          'python3',
          [repo('crates/kiln-io-xlsx/tests/oracle/libreoffice-uno.py'), `${dir}/pivot-from-web.xlsx`],
          {
            encoding: 'utf8',
            timeout: 90_000,
            stdio: ['ignore', 'pipe', 'pipe'],
          },
        ),
      ) as { sheets: { name: string; pivots: { cells: string[][] }[] }[] };
      const cells = lo.sheets.flatMap((sh) => sh.pivots).flatMap((p) => p.cells);
      expect(cells.find((r) => r[0] === 'East')?.[1]).toBe('34');
      expect(cells.find((r) => r[0] === 'Total Result')?.[1]).toBe('106');
    }
    // Kiln opens it again with the PivotTable and the sparklines.
    await page.goto('/#/home');
    await expect(
      page.getByText('Your recent files will gather here').or(page.locator('.kiln-home__table')).first(),
    ).toBeVisible({ timeout: 45_000 });
    await page.evaluate(() => {
      delete (window as unknown as { showOpenFilePicker?: unknown }).showOpenFilePicker;
    });
    const [chooser] = await Promise.all([page.waitForEvent('filechooser'), page.keyboard.press('Control+o')]);
    await chooser.setFiles({
      name: 'pivot-from-web.xlsx',
      mimeType: 'application/octet-stream',
      buffer: saved,
    });
    await page.waitForFunction(
      () =>
        Number(document.querySelector<HTMLElement>('[data-kiln-grid-surface]')?.dataset.kilnFrames ?? 0) > 0,
      null,
      { timeout: 60_000 },
    );
    await select(page, 3, 1);
    await expect.poll(async () => (await analysis(page)).pivot?.values[0]?.name).toBe('Sum of Units');
    await select(page, 3, 3);
    await expect.poll(async () => (await analysis(page)).sparkline?.options.kind).toBe('Column');
  });

  test('visual: the PivotTable and its pane in the dark theme and on a phone', async ({ page }) => {
    await page.emulateMedia({ colorScheme: 'dark' });
    await salesSheet(page);
    await createPivot(page);
    await fieldBox(page, 'Region').click();
    await fieldBox(page, 'Product').click();
    await fieldBox(page, 'Units').click();
    await expect.poll(() => value(page, 3, 1)).not.toBe('');
    await core(page, { InsertSparklines: { kind: 'Line', data: 'B5:B8', location: 'F5' } });
    await select(page, 3, 1);
    await page.screenshot({ path: shot('analysis-pivot-dark.png') });
    await page.setViewportSize({ width: 390, height: 844 });
    await page.waitForTimeout(300);
    await page.screenshot({ path: shot('analysis-pivot-dark-phone.png') });
    await page.emulateMedia({ colorScheme: 'light' });
    await page.waitForTimeout(300);
    await page.screenshot({ path: shot('analysis-pivot-phone.png') });
  });
});

test.describe('Kiln Grid analysis on a touch screen', () => {
  test.use({ hasTouch: true });

  test('outline: a finger taps the 12 px level and ± buttons from 10 px away', async ({ page }) => {
    await openGrid(page);
    await fill(page, [
      ['Region', 'Product', 'Units'],
      ['East', 'Apple', '10'],
      ['East', 'Banana', '20'],
      ['North', 'Cherry', '12'],
      ['West', 'Apple', '7'],
      ['West', 'Cherry', '14'],
    ]);
    await select(page, 1, 0);
    await focusSheet(page);
    await tab(page, 'Data');
    await page.getByRole('button', { name: 'Subtotal' }).click();
    await dialog(page, 'Subtotal').getByRole('button', { name: 'OK' }).click();
    await expect.poll(() => value(page, 3, 0)).toBe('East Total');
    const rowHeight = async (row: number) => {
      await select(page, row, 2);
      const st = (await gridState(page)) as unknown as { features: { cell_rect: number[] } };
      return st.features.cell_rect[3] ?? 0;
    };
    const o = await origin(page);
    // Button 1's centre is 10 px from the gutter's edge: a tap at the edge shows level 1.
    await page.touchscreen.tap(o.x + 1, o.y + 12);
    await expect.poll(() => rowHeight(3)).toBe(0);
    await page.touchscreen.tap(o.x + 26, o.y + 12);
    await expect.poll(() => rowHeight(3)).toBeGreaterThan(0);
    expect(await rowHeight(1)).toBe(0);
    // East Total's + sits at x 26: a tap 10 px right of it opens East.
    const east = await cellAt(page, 3, 0);
    await page.touchscreen.tap(o.x + 36, east.y);
    await expect.poll(() => rowHeight(1)).toBeGreaterThan(0);
  });
});
