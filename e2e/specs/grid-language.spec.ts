/**
 * Kiln Grid's formula language end to end: dynamic arrays and GROUPBY spilling on the
 * sheet, structured references to a table, 3-D references across sheets, the 1904 date system from
 * File ▸ Options ▸ Advanced, Excel's display precision and Ctrl+Shift+Enter array formulas. Each
 * test types as an Excel user does (the keyboard on the sheet) and reads the cells from the core.
 */
import { expect, type Page, test } from '@playwright/test';
import {
  BUDGET,
  clearSaved,
  core,
  expectSheetFocused,
  fill,
  focusSheet,
  gridState,
  noFilePickers,
  openFile,
  openGrid,
  savedBytes,
  select,
  shot,
  until,
  value,
  values,
} from './grid-helpers.ts';

/** Types an entry into a cell with the keyboard and presses Enter. */
async function type(page: Page, row: number, col: number, text: string): Promise<void> {
  await select(page, row, col);
  await focusSheet(page);
  await page.keyboard.type(text);
  await page.keyboard.press('Enter');
}

test.describe('grid language', () => {
  test('dynamic arrays: SEQUENCE and GROUPBY spill, the # operator follows, a blocked spill is #SPILL!', async ({
    page,
  }) => {
    await openGrid(page);
    await type(page, 0, 0, '=SEQUENCE(3,2)');
    await expect.poll(() => values(page, 0, 0, 3, 1)).toBe('1\t2\n3\t4\n5\t6\n\t');
    await type(page, 0, 3, '=SUM(A1#)');
    await expect.poll(() => value(page, 0, 3)).toBe('21');
    // Something in the way: the anchor shows #SPILL! until it is cleared.
    await type(page, 2, 1, 'x');
    await expect.poll(() => value(page, 0, 0)).toBe('#SPILL!');
    await select(page, 2, 1);
    await focusSheet(page);
    await page.keyboard.press('Delete');
    await expect.poll(() => value(page, 0, 0)).toBe('1');
    // GROUPBY with SUM passed by name.
    await fill(page, BUDGET, 0, 6);
    await type(page, 0, 10, '=GROUPBY(H2:H7,I2:I7,SUM)');
    await expect
      .poll(() => values(page, 0, 10, 4, 11))
      .toBe('Equipment\t0\nEvents\t380\nFiring\t1820\nMaterials\t730\nTotal\t2930');
    await select(page, 0, 10);
    await expect.poll(async () => (await gridState(page)).formula).toBe('=GROUPBY(H2:H7,I2:I7,SUM)');
    await page.screenshot({ path: shot('language-dynamic-arrays.png') });
  });

  test('structured references: a table column, its headers and a new row the sum follows', async ({
    page,
  }) => {
    await openGrid(page);
    await fill(page, BUDGET);
    await select(page, 1, 0);
    await focusSheet(page);
    await page.keyboard.press('Control+t');
    const d = page.getByRole('dialog', { name: /Create Table|Format As Table/ });
    await expect(d).toBeVisible();
    await d.getByRole('button', { name: 'OK' }).click();
    await until(page, (s) => s.features.table?.name === 'Table1', 'Table1 exists');
    await expectSheetFocused(page);
    await type(page, 0, 5, '=SUM(Table1[Amount])');
    await expect.poll(() => value(page, 0, 5)).toBe('2930');
    await type(page, 1, 5, '=COUNTA(Table1[#Headers])');
    await expect.poll(() => value(page, 1, 5)).toBe('3');
    // A row typed under the table joins it, and the sum follows.
    await type(page, 7, 0, 'Wages');
    await type(page, 7, 2, '100');
    await select(page, 7, 0);
    await until(page, (s) => s.features.table?.range.r1 === 7, 'the table grows');
    await expect.poll(() => value(page, 0, 5)).toBe('3030');
  });

  test('3-D references: a sum across sheets in workbook order follows each sheet', async ({ page }) => {
    await openGrid(page);
    await type(page, 0, 0, '5');
    await core(page, 'InsertSheet');
    await until(page, (s) => s.sheets.length === 2, 'a second sheet');
    const second = (await gridState(page)).sheets[(await gridState(page)).active_sheet]?.name ?? '';
    await type(page, 0, 0, '7');
    const names = (await gridState(page)).sheets.map((s) => s.name);
    const [first, last] = [names[0] ?? '', names[names.length - 1] ?? ''];
    expect(second).not.toBe('');
    await type(page, 2, 2, `=SUM(${first}:${last}!A1)`);
    await expect.poll(() => value(page, 2, 2)).toBe('12');
    await type(page, 0, 0, '10');
    await expect.poll(() => value(page, 2, 2)).toBe('15');
  });

  test('date 1904: Options ▸ Advanced switches the date system; DATE and the shown dates follow', async ({
    page,
  }) => {
    await openGrid(page);
    await type(page, 0, 0, '1/1/2024');
    await type(page, 0, 1, '=DATE(2024,1,1)-0');
    await expect.poll(() => value(page, 0, 1)).toBe('45292');
    await page.getByRole('button', { name: 'File' }).click();
    await page.getByRole('button', { name: 'Options' }).click();
    const options = page.getByRole('dialog', { name: /Options/ });
    await expect(options).toBeVisible();
    await options
      .getByRole('navigation', { name: 'Options sections' })
      .getByRole('button', { name: 'Advanced' })
      .click();
    const box = options.getByTestId('grid-date-1904');
    await expect(box).not.toBeChecked();
    await box.click();
    await until(page, (s) => (s.features as unknown as { date1904: boolean }).date1904, 'the 1904 system');
    await expect(box).toBeChecked();
    await page.screenshot({ path: shot('language-options-1904.png') });
    await page.keyboard.press('Escape');
    await expect(options).toBeHidden();
    // The keys are back on the sheet (File ▸ Options came from the Backstage, which has gone).
    await expectSheetFocused(page);
    // The serial stays and shows four years and a day later; DATE counts from 1904.
    await expect.poll(() => value(page, 0, 0)).toBe('1/2/2028');
    await expect.poll(() => value(page, 0, 1)).toBe('43830');
    // Typed straight after: the keys reach the sheet.
    await page.keyboard.type('x');
    await until(page, (s) => s.mode !== 'Ready', 'the keys start an entry on the sheet');
    await page.keyboard.press('Escape');
    // Saved and opened again: the workbook keeps the 1904 system and the dates it shows.
    await clearSaved(page);
    await noFilePickers(page);
    await focusSheet(page);
    await page.keyboard.press('Control+s');
    const saved = await savedBytes(page, 'Book1.xlsx');
    await openFile(page, { name: 'date1904.xlsx', buffer: saved });
    await until(
      page,
      (s) => (s.features as unknown as { date1904: boolean }).date1904,
      'the 1904 system kept',
    );
    await expect.poll(() => value(page, 0, 0)).toBe('1/2/2028');
    await expect.poll(() => value(page, 0, 1)).toBe('43830');
    await page.getByRole('button', { name: 'File' }).click();
    await page.getByRole('button', { name: 'Options' }).click();
    const again = page.getByRole('dialog', { name: /Options/ });
    await again
      .getByRole('navigation', { name: 'Options sections' })
      .getByRole('button', { name: 'Advanced' })
      .click();
    await expect(again.getByTestId('grid-date-1904')).toBeChecked();
    await page.keyboard.press('Escape');
    await expectSheetFocused(page);
  });

  test('precision: 15 significant digits, equal sums and a negative number shown as zero keeps its sign', async ({
    page,
  }) => {
    await openGrid(page);
    await type(page, 0, 0, '=0.1+0.2=0.3');
    await expect.poll(() => value(page, 0, 0)).toBe('TRUE');
    await type(page, 1, 0, '=TEXT(-0.4,"0")');
    await expect.poll(() => value(page, 1, 0)).toBe('-0');
    await type(page, 2, 0, '123456789012345678');
    await expect.poll(() => value(page, 2, 0)).toBe('1.23457E+17');
    await type(page, 3, 0, '=1-0.9');
    await expect.poll(() => value(page, 3, 0)).toBe('0.1');
  });

  test('legacy CSE: Ctrl+Shift+Enter fills the selected block, shows braces and refuses edits to part of it', async ({
    page,
  }) => {
    await openGrid(page);
    await fill(page, [['5'], ['6'], ['7']]);
    await select(page, 0, 2, 3, 2);
    await focusSheet(page);
    await page.keyboard.type('=A1:A3*2');
    await page.keyboard.press('Control+Shift+Enter');
    await expect.poll(() => values(page, 0, 2, 3, 2)).toBe('10\n12\n14\n#N/A');
    await select(page, 1, 2);
    await expect.poll(async () => (await gridState(page)).formula).toBe('{=A1:A3*2}');
    await expect(page.getByRole('textbox', { name: 'Formula Bar' })).toHaveValue('{=A1:A3*2}');
    await page.screenshot({ path: shot('language-cse.png') });
    // Typing into one cell of the block is refused with Excel's message.
    await focusSheet(page);
    await page.keyboard.type('1');
    await page.keyboard.press('Enter');
    await expect(page.getByText("You can't change part of an array.")).toBeVisible();
    await page.keyboard.press('Escape');
    await expect.poll(() => value(page, 1, 2)).toBe('12');
  });
});
