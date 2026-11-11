/**
 * Kiln Grid's Home tab completions end to end, as an Excel user does them in the web
 * build: Paste Special's comments, validation, column widths and Paste Link; the border pen
 * (Draw Border, Draw Border Grid, Erase Border); bold and colour on selected characters inside
 * the cell editor; the Custom number formats list; Fill ▸ Series; the fill handle with a custom
 * list and the Auto Fill Options button; and the command palette running Grid commands by their
 * Excel names. Keys are typed with `page.keyboard`, results are read from the core, and the keys
 * must come back to the sheet after every dialog and menu.
 */
import { execFileSync } from 'node:child_process';
import { mkdirSync, writeFileSync } from 'node:fs';
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
  repo,
  ribbonButton,
  select,
  shot,
  until,
  value,
  values,
} from './grid-helpers.ts';

interface Home {
  pen: { mode: string; style: string; color: string | null } | null;
  auto_fill: { x: number; y: number; current: unknown } | null;
}

async function home(page: Page): Promise<Home> {
  return ((await gridState(page)) as unknown as { home: Home }).home;
}

interface Side {
  style: string;
  color: string | null;
}
interface FormatCells {
  border_top: Side | null;
  border_bottom: Side | null;
  border_left: Side | null;
  border_right: Side | null;
  custom_codes: string[];
  number_format: string;
}

/** The Format Cells values of one cell. */
async function formatAt(page: Page, row: number, col: number): Promise<FormatCells> {
  await select(page, row, col);
  return ask<FormatCells>(page, 'FormatCells');
}

/** Clicks a cell and gives the keys to the sheet. */
async function clickCell(page: Page, row: number, col: number): Promise<void> {
  const p = await cellPoint(page, row, col);
  await page.mouse.click(p.x, p.y);
  await expectSheetFocused(page);
  const name = `${String.fromCharCode(65 + col)}${row + 1}`;
  await until(page, (s) => s.name_box === name, `${name} is selected`);
}

test.describe('grid home', () => {
  test('paste special pastes comments, validation, column widths and a link', async ({ page }) => {
    await openGrid(page);
    await fill(page, [['5'], ['7']]);
    await select(page, 0, 0);
    await core(page, { SetNote: { text: 'Checked', author: 'Ana', cell: null } });
    await select(page, 0, 0, 1, 0);
    await core(page, {
      SetValidation: {
        rule: {
          kind: 'Whole',
          op: 'Between',
          formula1: '1',
          formula2: '10',
          allow_blank: true,
          dropdown: false,
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
    await core(page, { ColumnWidth: { chars: 24 } });
    await focusSheet(page);
    await page.keyboard.press('Control+c');
    // Column A is wider now: cells are picked through the core, the keys stay on the sheet.
    const pick = async (row: number, col: number) => {
      await select(page, row, col);
      await focusSheet(page);
    };
    // Validation only: C1 and C2 refuse 50, and no value comes with it.
    await pick(0, 2);
    await page.keyboard.press('Control+Alt+v');
    const dialog = page.getByRole('dialog', { name: 'Paste Special' });
    await expect(dialog).toBeVisible();
    await expect(dialog).toHaveAttribute('data-kiln-shortcuts', 'off');
    await dialog.getByRole('radio', { name: 'Validation' }).check();
    await page.screenshot({ path: shot('home-paste-special.png') });
    await dialog.getByRole('button', { name: 'OK' }).click();
    await expect(dialog).toBeHidden();
    await expectSheetFocused(page);
    expect(await value(page, 0, 2)).toBe('');
    await pick(1, 2);
    await page.keyboard.type('50');
    await page.keyboard.press('Enter');
    await expect(page.getByRole('alertdialog').or(page.getByRole('dialog')).first()).toBeVisible();
    await page.keyboard.press('Escape');
    // Comments and notes: the note arrives in D1.
    await pick(0, 3);
    await page.keyboard.press('Control+Alt+v');
    await dialog.getByRole('radio', { name: 'Comments and notes' }).check();
    await dialog.getByRole('button', { name: 'OK' }).click();
    await until(page, (s) => s.features.note, 'D1 has the note');
    // Column widths: E takes A's width (a point 150 px into E's header is still column E).
    await pick(0, 4);
    await page.keyboard.press('Control+Alt+v');
    await dialog.getByRole('radio', { name: 'Column widths' }).check();
    await dialog.getByRole('button', { name: 'OK' }).click();
    await expectSheetFocused(page);
    const header = await gridState(page);
    const aWidth = 24 * 7 + 5; // 24 characters of Calibri 11 (7 px each) and the padding.
    const hit = await ask<unknown>(page, {
      HitTest: {
        x: header.layout.header_w + aWidth + 64 * 3 + 150,
        y: header.layout.header_h / 2,
        coarse: false,
      },
    });
    expect(hit).toEqual({ ColumnHeader: { col: 4 } });
    // Paste Link from the dialog's button: G1:G2 follow A1:A2.
    await pick(0, 6);
    await page.keyboard.press('Control+Alt+v');
    await dialog.getByRole('button', { name: 'Paste Link' }).click();
    await expect(dialog).toBeHidden();
    await expectSheetFocused(page);
    await until(page, (s) => s.formula === '=A1', 'G1 links to A1');
    expect(await values(page, 0, 6, 1, 6)).toBe('5\n7');
    await pick(1, 0);
    await page.keyboard.type('9');
    await page.keyboard.press('Enter');
    expect(await value(page, 1, 6)).toBe('9');
  });

  test('borders: the pen draws a border grid, an edge, erases, and Esc puts it away', async ({ page }) => {
    await openGrid(page);
    await page.getByRole('button', { name: 'More options for Borders' }).click();
    await page.getByRole('menuitem', { name: 'Draw Border Grid' }).click();
    await expect.poll(async () => (await home(page)).pen?.mode).toBe('Grid');
    await expect(page.getByTestId('grid-border-pen')).toContainText('Draw Border Grid');
    await expectSheetFocused(page);
    // Drag from B2 to C3.
    const a = await cellPoint(page, 1, 1);
    const b = await cellPoint(page, 2, 2);
    await page.mouse.move(a.x, a.y);
    await page.mouse.down();
    await page.mouse.move((a.x + b.x) / 2, (a.y + b.y) / 2, { steps: 4 });
    await page.mouse.move(b.x, b.y, { steps: 4 });
    await page.mouse.up();
    await expect.poll(async () => (await formatAt(page, 1, 1)).border_bottom?.style ?? null).toBe('Thin');
    expect((await formatAt(page, 2, 2)).border_right?.style).toBe('Thin');
    // Line Style ▸ Thick keeps the pen and takes the style; a click on E6's bottom edge draws it.
    await page.getByRole('button', { name: 'More options for Borders' }).click();
    await page.getByRole('menuitem', { name: 'Draw Border', exact: true }).click();
    await page.getByRole('button', { name: 'More options for Borders' }).click();
    await page.getByRole('menuitem', { name: 'Line Style' }).hover();
    await page.getByRole('menuitem', { name: 'Thick', exact: true }).click();
    await expect.poll(async () => (await home(page)).pen?.style).toBe('Thick');
    const edge = await cellPoint(page, 5, 4, 0.5, 0.97);
    await page.mouse.click(edge.x, edge.y);
    await expect.poll(async () => (await formatAt(page, 5, 4)).border_bottom?.style ?? null).toBe('Thick');
    expect((await formatAt(page, 5, 4)).border_top).toBeNull();
    // Erase Border: a click on B2's bottom edge erases it (and C3 keeps its lines).
    await page.getByRole('button', { name: 'More options for Borders' }).click();
    await page.getByRole('menuitem', { name: 'Erase Border' }).click();
    const e = await cellPoint(page, 1, 1, 0.5, 0.97);
    await page.mouse.click(e.x, e.y);
    await expect.poll(async () => (await formatAt(page, 1, 1)).border_bottom).toBeNull();
    expect((await formatAt(page, 2, 2)).border_right?.style).toBe('Thin');
    await page.screenshot({ path: shot('home-border-pen.png') });
    // Esc puts the pen away; a click selects again.
    await focusSheet(page);
    await page.keyboard.press('Escape');
    await expect.poll(async () => (await home(page)).pen).toBeNull();
    await expect(page.getByTestId('grid-border-pen')).toBeHidden();
    await clickCell(page, 7, 7);
  });

  test('rich text: characters selected in the cell editor are bolded and coloured', async ({ page }) => {
    test.setTimeout(90_000);
    await openGrid(page);
    await page.keyboard.type('Kiln Grid');
    // F2 switches to Edit mode, where Shift+Left selects characters.
    await page.keyboard.press('F2');
    await until(page, (s) => s.mode === 'Edit', 'Edit mode');
    await page.waitForTimeout(150);
    for (let i = 0; i < 4; i++) await page.keyboard.press('Shift+ArrowLeft');
    // The core knows the selected characters ("Grid", UTF-16 offsets 5 to 9).
    await expect
      .poll(async () => {
        const e = ((await gridState(page)) as unknown as { editor: { start: number; end: number } | null })
          .editor;
        return e ? [Math.min(e.start, e.end), Math.max(e.start, e.end)] : null;
      })
      .toEqual([5, 9]);
    await page.keyboard.press('Control+b');
    // The entry stays open, and the cell itself is not bold.
    await until(page, (s) => s.mode !== 'Ready', 'still editing');
    // Font Color from the ribbon, on the same characters.
    await page.getByRole('button', { name: 'More options for Font Color' }).click();
    const picker = page.getByRole('dialog').or(page.getByRole('menu')).last();
    await picker.getByRole('button', { name: /Red/ }).first().click();
    await page.locator('[data-kiln-grid-proxy]').focus();
    await until(page, (s) => s.mode !== 'Ready', 'still editing after the colour');
    await page.screenshot({ path: shot('home-rich-text-editing.png') });
    await page.keyboard.press('Enter');
    await until(page, (s) => s.name_box === 'A2', 'committed');
    expect(await value(page, 0, 0)).toBe('Kiln Grid');
    await select(page, 0, 0);
    expect((await gridState(page)).format.bold).toBe(false);
    // Saved as an xlsx, the cell is a rich string: "Grid" bold and red, "Kiln " plain.
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
    writeFileSync(`${dir}/rich-text.xlsx`, saved);
    const strings = execFileSync('unzip', ['-p', `${dir}/rich-text.xlsx`, 'xl/sharedStrings.xml']).toString();
    expect(strings).toMatch(/<r><rPr><b\/>.*?<\/rPr><t[^>]*>Grid<\/t><\/r>/);
    expect(strings).toMatch(/<t xml:space="preserve">Kiln <\/t>|<t>Kiln <\/t>/);
  });

  test('custom formats: the Custom list takes a code with a preview and deletes it', async ({ page }) => {
    await openGrid(page);
    await page.keyboard.type('12.5');
    await page.keyboard.press('Enter');
    await page.keyboard.press('ArrowUp');
    await page.keyboard.press('Control+1');
    const dialog = page.getByRole('dialog', { name: 'Format Cells' });
    await dialog.getByRole('option', { name: 'Custom' }).click();
    await expect(dialog.getByRole('listbox', { name: 'Format codes' }).getByRole('option')).not.toHaveCount(
      0,
    );
    const type = dialog.getByLabel('Type');
    await type.fill('0.0" kg"');
    await expect(dialog.locator('output')).toHaveText('12.5 kg');
    await dialog.getByRole('button', { name: 'OK' }).click();
    await expect(dialog).toBeHidden();
    await expectSheetFocused(page);
    expect(await value(page, 0, 0)).toBe('12.5 kg');
    // The workbook's code is listed after Excel's own, and Delete takes it away.
    await focusSheet(page);
    await page.keyboard.press('Control+1');
    await dialog.getByRole('option', { name: 'Custom' }).click();
    await dialog
      .getByRole('listbox', { name: 'Format codes' })
      .getByRole('option', { name: '0.0" kg"' })
      .click();
    await page.screenshot({ path: shot('home-custom-formats.png') });
    await dialog.getByRole('button', { name: 'Delete' }).click();
    await expect(
      dialog.getByRole('listbox', { name: 'Format codes' }).getByRole('option', { name: '0.0" kg"' }),
    ).toHaveCount(0);
    await dialog.getByRole('button', { name: 'Cancel' }).click();
    await expectSheetFocused(page);
    expect(await value(page, 0, 0)).toBe('12.5');
    expect((await formatAt(page, 0, 0)).number_format).toBe('General');
  });

  test('editing: Fill Series fills a step and stops at the stop value', async ({ page }) => {
    await openGrid(page);
    await page.keyboard.type('3');
    await page.keyboard.press('Enter');
    await page.keyboard.press('ArrowUp');
    for (let i = 0; i < 7; i++) await page.keyboard.press('Shift+ArrowDown');
    await (await ribbonButton(page, 'Fill', true)).click();
    await page.getByRole('menuitem', { name: 'Series…' }).click();
    const dialog = page.getByRole('dialog', { name: 'Series' });
    await expect(dialog).toBeVisible();
    await expect(dialog.getByRole('radio', { name: 'Columns' })).toBeChecked();
    await dialog.getByLabel('Step value:').fill('4');
    await dialog.getByLabel('Stop value:').fill('20');
    await page.screenshot({ path: shot('home-fill-series.png') });
    await dialog.getByRole('button', { name: 'OK' }).click();
    await expect(dialog).toBeHidden();
    await expectSheetFocused(page);
    expect(await values(page, 0, 0, 7, 0)).toBe('3\n7\n11\n15\n19\n\n\n');
    // A growth series along a row.
    await clickCell(page, 10, 0);
    await page.keyboard.type('2');
    await page.keyboard.press('Enter');
    await select(page, 10, 0, 10, 4);
    await focusSheet(page);
    await (await ribbonButton(page, 'Fill', true)).click();
    await page.getByRole('menuitem', { name: 'Series…' }).click();
    await expect(dialog.getByRole('radio', { name: 'Rows' })).toBeChecked();
    await dialog.getByRole('radio', { name: 'Growth' }).check();
    await dialog.getByLabel('Step value:').fill('3');
    await dialog.getByRole('button', { name: 'OK' }).click();
    expect(await values(page, 10, 0, 10, 4)).toBe('2\t6\t18\t54\t162');
  });

  test('fill handle: a custom list continues, and Auto Fill Options changes the fill', async ({ page }) => {
    await openGrid(page);
    await (await ribbonButton(page, 'Fill', true)).click();
    await page.getByRole('menuitem', { name: 'Edit Custom Lists…' }).click();
    const lists = page.getByRole('dialog', { name: 'Custom Lists' });
    await expect(lists).toBeVisible();
    await lists.getByLabel('List entries:').fill('North\nEast\nSouth\nWest');
    await lists.getByRole('button', { name: 'Add' }).click();
    await expect(lists.getByRole('option', { name: 'North, East, South, West' })).toBeVisible();
    await expect(lists.getByRole('button', { name: 'Delete' })).toBeEnabled();
    await page.screenshot({ path: shot('home-custom-lists.png') });
    await lists.getByRole('button', { name: 'OK' }).click();
    await expect(lists).toBeHidden();
    await expectSheetFocused(page);
    await page.keyboard.type('East');
    await page.keyboard.press('Enter');
    await clickCell(page, 0, 0);
    // Drag the fill handle (the selection's bottom-right corner) down to A4.
    const handle = await cellPoint(page, 0, 0, 1, 1);
    const to = await cellPoint(page, 3, 0);
    await page.mouse.move(handle.x - 1, handle.y - 1);
    await page.mouse.down();
    await page.mouse.move(to.x, to.y, { steps: 6 });
    await page.mouse.up();
    await expect.poll(() => values(page, 0, 0, 3, 0)).toBe('East\nSouth\nWest\nNorth');
    // The Auto Fill Options button: Copy Cells fills East again.
    const button = page.getByTestId('grid-auto-fill-options');
    await expect(button).toBeVisible();
    await button.click();
    await expect(page.getByRole('menuitemradio', { name: 'Fill Series' })).toBeChecked();
    await page.screenshot({ path: shot('home-auto-fill-options.png') });
    await page.getByRole('menuitemradio', { name: 'Copy Cells' }).click();
    await expectSheetFocused(page);
    await expect.poll(() => values(page, 0, 0, 3, 0)).toBe('East\nEast\nEast\nEast');
    // The list lasts: a reload gives it to the core again.
    await page.reload();
    await openGrid(page);
    await page.keyboard.type('West');
    await page.keyboard.press('Enter');
    await core(page, { SelectCell: { row: 0, col: 0, extend: false, add: false } });
    await core(page, { AutoFill: { row: 1, col: 0, mode: 'Auto' } });
    expect(await value(page, 1, 0)).toBe('North');
  });

  test('palette: Grid commands by their Excel names, with the keys back on the sheet', async ({ page }) => {
    await openGrid(page);
    await page.keyboard.press('Control+Shift+P');
    const palette = page.getByRole('dialog');
    await expect(palette).toBeVisible();
    // An Office name and a synonym find Grid's commands, with their shortcuts.
    await page.keyboard.type('autosum');
    await expect(palette.getByRole('option', { name: /AutoSum/ }).first()).toContainText('Alt+=');
    await palette.getByRole('combobox').fill('number series');
    await expect(palette.getByRole('option', { name: /Series…/ }).first()).toBeVisible();
    await page.screenshot({ path: shot('home-palette.png') });
    await palette.getByRole('combobox').fill('freeze top row');
    await page.keyboard.press('Enter');
    await expect(palette).toBeHidden();
    await expect.poll(async () => (await gridState(page)).layout).toMatchObject({});
    await until(
      page,
      (s) => (s.layout as unknown as { freeze_rows: number }).freeze_rows === 1,
      'the top row is frozen',
    );
    await expectSheetFocused(page);
    await page.keyboard.type('after');
    await page.keyboard.press('Enter');
    expect(await value(page, 0, 0)).toBe('after');
  });
});

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
              if (name.endsWith('.xlsx')) {
                const b = new Uint8Array(await (await h.getFile()).arrayBuffer());
                // A complete zip ends with its end-of-central-directory record.
                const tail = b.subarray(Math.max(0, b.length - 22));
                if (tail[0] === 0x50 && tail[1] === 0x4b && tail[2] === 5 && tail[3] === 6)
                  return Array.from(b);
              }
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
