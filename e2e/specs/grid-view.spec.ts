/**
 * Kiln Grid ▸ View: macros as an Excel user meets them. A workbook with a VBA project
 * opens, View ▸ Macros says the macros are kept and never run, Save As ▸ Excel Macro-Enabled
 * Workbook keeps the VBA project byte for byte, and a macro-free save asks Excel's question first.
 */
import { readFileSync } from 'node:fs';
import { inflateRawSync } from 'node:zlib';
import { expect, type Page, test } from '@playwright/test';
import { expectSheetFocused, repo, tab } from './grid-helpers.ts';

type W = { __kilnGrid?: { doc: number } };

/** Opens a file through File ▸ Open ▸ Browse (the file input the picker falls back to). */
async function openFile(page: Page, file: { name: string; buffer: Buffer }) {
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
  await chooser.setFiles({ mimeType: 'application/octet-stream', ...file });
  await page.waitForFunction(
    () =>
      !!(window as unknown as W).__kilnGrid &&
      Number(document.querySelector<HTMLElement>('[data-kiln-grid-surface]')?.dataset.kilnFrames ?? 0) > 0,
    null,
    { timeout: 45_000 },
  );
}

/** One entry of a zip package, inflated. */
function zipEntry(zip: Buffer, name: string): Buffer | null {
  const eocd = zip.lastIndexOf(Buffer.from([0x50, 0x4b, 0x05, 0x06]));
  if (eocd < 0) return null;
  const count = zip.readUInt16LE(eocd + 10);
  let at = zip.readUInt32LE(eocd + 16);
  for (let i = 0; i < count; i++) {
    const method = zip.readUInt16LE(at + 10);
    const size = zip.readUInt32LE(at + 20);
    const nameLen = zip.readUInt16LE(at + 28);
    const extra = zip.readUInt16LE(at + 30);
    const comment = zip.readUInt16LE(at + 32);
    const local = zip.readUInt32LE(at + 42);
    const entry = zip.subarray(at + 46, at + 46 + nameLen).toString('utf8');
    if (entry === name) {
      const start = local + 30 + zip.readUInt16LE(local + 26) + zip.readUInt16LE(local + 28);
      const data = zip.subarray(start, start + size);
      return method === 0 ? Buffer.from(data) : inflateRawSync(data);
    }
    at += 46 + nameLen + extra + comment;
  }
  return null;
}

/** The bytes of a saved file (`documents/<name>` in the origin's private file system), once the
 * package is complete. */
async function savedBytes(page: Page, name: string): Promise<Buffer> {
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
        // Complete: the end-of-central-directory record is there.
        return bytes !== null && Buffer.from(bytes).lastIndexOf(Buffer.from([0x50, 0x4b, 0x05, 0x06])) > 0;
      },
      { message: `${name} is saved`, timeout: 15_000 },
    )
    .toBe(true);
  return Buffer.from(bytes ?? []);
}

async function clearSaved(page: Page) {
  await page.evaluate(async () => {
    const root = await navigator.storage.getDirectory();
    await root.removeEntry('documents', { recursive: true }).catch(() => undefined);
  });
}

async function saveAs(page: Page, format: RegExp) {
  const dialog = page.getByRole('dialog', { name: /File menu/ });
  // After a save the File menu can still be open (Backstage stays, as in Office).
  if (!(await dialog.isVisible())) await page.getByRole('button', { name: 'File' }).click();
  await dialog
    .getByRole('navigation', { name: 'File menu' })
    .getByRole('button', { name: 'Save As' })
    .click();
  await dialog.getByRole('button', { name: format }).click();
}

test.describe('Kiln Grid view', () => {
  test.describe.configure({ timeout: 120_000 });

  test('macros explained: an xlsm opens, View ▸ Macros explains, Save As .xlsm keeps the VBA project and .xlsx asks first', async ({
    page,
  }) => {
    const xlsm = readFileSync(repo('e2e/fixtures/macros.xlsm'));
    // A real VBA project: an OLE compound file.
    expect(zipEntry(xlsm, 'xl/vbaProject.bin')?.subarray(0, 8)).toEqual(
      Buffer.from([0xd0, 0xcf, 0x11, 0xe0, 0xa1, 0xb1, 0x1a, 0xe1]),
    );
    await openFile(page, { name: 'macros.xlsm', buffer: xlsm });
    await clearSaved(page);
    await expect(page.getByTestId('document-title')).toContainText('macros');

    // View ▸ Macros: what Kiln does with them.
    await tab(page, 'View');
    await page.locator('[data-command="ui.macros.explain"]').first().click();
    await expect(page.getByText('Kiln never runs macros')).toBeVisible();
    await expect(page.getByText(/kept exactly as they are/)).toBeVisible();
    await expect(page.getByText(/\.xlsm/).first()).toBeVisible();
    await expectSheetFocused(page);

    // An edit, then Save As ▸ Excel Macro-Enabled Workbook.
    await page.keyboard.press('Control+End');
    await page.keyboard.press('ArrowDown');
    await page.keyboard.type('Edited in Kiln');
    await page.keyboard.press('Enter');
    await saveAs(page, /Excel Macro-Enabled Workbook/);
    const saved = await savedBytes(page, 'macros.xlsm');
    expect(zipEntry(saved, 'xl/vbaProject.bin')).toEqual(zipEntry(xlsm, 'xl/vbaProject.bin'));
    expect(zipEntry(saved, '[Content_Types].xml')?.toString('utf8')).toContain('macroEnabled.main+xml');
    const sheet = zipEntry(saved, 'xl/worksheets/sheet1.xml')?.toString('utf8') ?? '';
    const strings = zipEntry(saved, 'xl/sharedStrings.xml')?.toString('utf8') ?? '';
    expect(sheet + strings).toContain('Edited in Kiln');

    // Save As ▸ Excel Workbook: Excel's macro-free question; Yes saves without the VBA project.
    await saveAs(page, /^\.xlsx Excel Workbook/);
    const ask = page.getByTestId('macro-free-question');
    await expect(ask).toBeVisible();
    await expect(ask).toContainText('VBA project');
    await ask.getByRole('button', { name: 'Yes' }).click();
    const plain = await savedBytes(page, 'macros.xlsx');
    expect(zipEntry(plain, 'xl/vbaProject.bin')).toBeNull();
    expect(zipEntry(plain, '[Content_Types].xml')?.toString('utf8')).not.toContain('macroEnabled');
  });
});
