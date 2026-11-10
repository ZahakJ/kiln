/**
 * Kiln Grid ▸ Data end to end: From Text/CSV and From JSON through the browser's file picker and
 * the import dialog (delimiter, decimal comma, where to load), the quick sorts, Remove Duplicates,
 * Text to Columns and Group, each with the keys back on the sheet afterwards.
 */
import { expect, type Page, test } from '@playwright/test';
import {
  BUDGET,
  expectSheetFocused,
  fill,
  focusSheet,
  gridState,
  openGrid,
  select,
  shot,
  tab,
  until,
  value,
  values,
} from './grid-helpers.ts';

async function pick(page: Page, button: string, name: string, text: string, mimeType: string): Promise<void> {
  await tab(page, 'Data');
  const chooser = page.waitForEvent('filechooser');
  await page.getByRole('button', { name: button, exact: true }).first().click();
  await (await chooser).setFiles({ name, mimeType, buffer: Buffer.from(text, 'utf8') });
}

test.describe('Kiln Grid data', () => {
  test('from text/csv: a semicolon file with decimal commas into a new sheet', async ({ page }) => {
    await openGrid(page);
    await pick(page, 'From Text/CSV', 'prices.csv', 'Item;Price\nClay;1.234,50\nGlaze;-0,75\n', 'text/csv');
    const d = page.getByTestId('grid-import');
    await expect(d).toBeVisible();
    await expect(d.getByText('Detect (;)')).toBeVisible();
    await d.getByRole('combobox', { name: 'Numbers' }).click();
    await page.getByRole('option', { name: /decimal comma/ }).click();
    await expect(d.getByText('3 rows, 2 columns.')).toBeVisible();
    await page.screenshot({ path: shot('data-import-csv.png') });
    await d.getByRole('button', { name: 'Load' }).click();
    await expect(d).toBeHidden();
    const s = await until(page, (s) => s.sheets.length === 2, 'a new sheet holds the import');
    expect(s.sheets[s.active_sheet]?.name).toBe('prices');
    await expect.poll(() => values(page, 1, 1, 2, 1)).toBe('1234.5\n-0.75');
    await expectSheetFocused(page);
  });

  test('from json: an array of records at the active cell', async ({ page }) => {
    await openGrid(page);
    await select(page, 4, 2);
    await focusSheet(page);
    const json = JSON.stringify([
      { name: 'Clay', kg: 25 },
      { name: 'Glaze', kg: 4 },
    ]);
    await pick(page, 'From JSON', 'stock.json', json, 'application/json');
    const d = page.getByTestId('grid-import');
    await expect(d).toBeVisible();
    await d.getByRole('combobox', { name: 'Load to' }).click();
    await page.getByRole('option', { name: /The active cell \(C5\)/ }).click();
    await d.getByRole('button', { name: 'Load' }).click();
    await expect(d).toBeHidden();
    await expect.poll(() => values(page, 4, 2, 6, 3)).toBe('name\tkg\nClay\t25\nGlaze\t4');
    expect((await gridState(page)).sheets).toHaveLength(1);
    await expectSheetFocused(page);
  });

  test('sort a to z from the ribbon sorts the region by the active column', async ({ page }) => {
    await openGrid(page);
    await fill(page, BUDGET);
    await select(page, 3, 2);
    await focusSheet(page);
    await tab(page, 'Data');
    await page
      .getByRole('button', { name: 'Sort Smallest to Largest' })
      .or(page.getByRole('button', { name: 'Sort A to Z' }))
      .first()
      .click();
    await expect.poll(() => values(page, 1, 2, 6, 2)).toBe('0\n310\n380\n420\n640\n1180');
    await expect.poll(() => value(page, 0, 0)).toBe('Item');
    await expectSheetFocused(page);
  });

  test('a column whose header is blank or only a line break is "Column A" in the data dialogs', async ({
    page,
  }) => {
    await openGrid(page);
    await fill(page, [
      ['\n', 'Qty'],
      ['a', '1'],
      ['b', '2'],
      ['a', '3'],
    ]);
    await select(page, 0, 0, 3, 1);
    await focusSheet(page);
    await tab(page, 'Data');
    await page.locator('[data-command="grid.data.remove_duplicates"]').first().click();
    const dd = page.getByTestId('grid-remove-duplicates');
    await expect(dd).toBeVisible();
    await expect(dd.getByRole('checkbox', { name: 'Column A', exact: true })).toBeVisible();
    await expect(dd.getByRole('checkbox', { name: 'Qty', exact: true })).toBeVisible();
    await page.keyboard.press('Escape');
    await expect(dd).toBeHidden();
    await page.locator('[data-command="grid.data.subtotal"]').first().click();
    const st = page.getByTestId('grid-subtotal-columns');
    await expect(st.getByRole('checkbox', { name: 'Column A', exact: true })).toBeVisible();
    await expect(st.getByRole('checkbox', { name: 'Qty', exact: true })).toBeVisible();
    await page.keyboard.press('Escape');
  });

  test('remove duplicates and text to columns', async ({ page }) => {
    await openGrid(page);
    await fill(page, [['Name'], ['Ana Lopes'], ['Ben Ito'], ['Ana Lopes'], ['Cleo Hart']]);
    await select(page, 0, 0, 4, 0);
    await focusSheet(page);
    await tab(page, 'Data');
    await page.locator('[data-command="grid.data.remove_duplicates"]').first().click();
    const dd = page.getByTestId('grid-remove-duplicates');
    await expect(dd).toBeVisible();
    await dd.getByRole('button', { name: 'OK' }).click();
    await until(page, (s) => !!s.features.message?.includes('1 duplicate value'), 'the count is reported');
    await expect.poll(() => values(page, 1, 0, 4, 0)).toBe('Ana Lopes\nBen Ito\nCleo Hart\n');
    await select(page, 1, 0, 3, 0);
    await focusSheet(page);
    await page.locator('[data-command="grid.data.text_to_columns"]').first().click();
    const t = page.getByTestId('grid-text-to-columns');
    await expect(t).toBeVisible();
    await t.getByRole('checkbox', { name: 'Space' }).check();
    await page.screenshot({ path: shot('data-text-to-columns.png') });
    await t.getByRole('button', { name: 'Finish' }).click();
    await expect(t).toBeHidden();
    await expect.poll(() => values(page, 1, 0, 3, 1)).toBe('Ana\tLopes\nBen\tIto\nCleo\tHart');
    await expectSheetFocused(page);
  });

  test('group rows and hide the detail', async ({ page }) => {
    await openGrid(page);
    await fill(page, BUDGET);
    // A count of the visible rows that follows the outline.
    await fill(page, [['=SUBTOTAL(103,A2:A7)']], 0, 5);
    await select(page, 1, 0, 3, 0);
    await focusSheet(page);
    await tab(page, 'Data');
    await page.locator('[data-command="grid.data.group_rows"]').first().click();
    await expectSheetFocused(page);
    await page.getByRole('button', { name: 'Hide Detail' }).click();
    await expect.poll(() => value(page, 0, 5)).toBe('3');
    await expectSheetFocused(page);
  });
});
