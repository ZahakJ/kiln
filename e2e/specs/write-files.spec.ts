/**
 * Kiln Write's files end to end: plain text opened with its encoding
 * detected and saved back in it, Markdown and HTML export from File ▸ Export, and macro-enabled
 * documents saved through File ▸ Save As with their VBA project kept byte for byte.
 *
 * The web host saves to the browser's private file system when the File System Access API is
 * not there (the tests remove it); the tests read the saved bytes back from it.
 */
import { readFileSync } from 'node:fs';
import { fileURLToPath } from 'node:url';
import { inflateRawSync } from 'node:zlib';
import { expect, type Page, test } from '@playwright/test';

const repo = (p: string) => fileURLToPath(new URL(`../../${p}`, import.meta.url));

interface WriteHandle {
  doc: number;
  client: { query(doc: number, q: unknown): Promise<Record<string, unknown>> };
  controller: { getStats(): { tiles: number } } | null;
}
type KilnWindow = { __kilnWrite?: WriteHandle };

/** Opens a file through File ▸ Open ▸ Browse (the file input the picker falls back to). */
async function openFile(page: Page, file: { name: string; buffer: Buffer; mimeType?: string }) {
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
    () => ((window as unknown as KilnWindow).__kilnWrite?.controller?.getStats().tiles ?? 0) > 0,
    null,
    { timeout: 45_000 },
  );
  await page.evaluate(() => {
    // biome-ignore lint/suspicious/noExplicitAny: the API is not in the DOM typings
    const w = window as any;
    delete w.showOpenFilePicker;
    delete w.showSaveFilePicker;
  });
}

/** One entry of a zip package, inflated (the central directory gives its offset and sizes). */
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

async function paragraphs(page: Page): Promise<string[]> {
  return page.evaluate(async () => {
    const w = (window as unknown as KilnWindow).__kilnWrite as WriteHandle;
    const r = (await w.client.query(w.doc, 'Paragraphs')) as { Paragraphs?: { text: string }[] };
    return (r.Paragraphs ?? []).map((p) => p.text);
  });
}

/** The bytes of a file in the origin's private file system (`documents/<name>`). */
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
        // The host creates the file empty and fills it when its stream closes.
        return bytes !== null && bytes.length > 0;
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

/** File ▸ Save As or Export ▸ a format button. */
async function saveAs(page: Page, page_: 'Save As' | 'Export', format: RegExp) {
  await page.getByRole('button', { name: 'File' }).click();
  const dialog = page.getByRole('dialog', { name: /File menu/ });
  await dialog.getByRole('navigation', { name: 'File menu' }).getByRole('button', { name: page_ }).click();
  await dialog.getByRole('button', { name: format }).click();
}

test.describe('Kiln Write files', () => {
  test.describe.configure({ timeout: 120_000 });

  test('plain text opens with its encoding detected and saves back in it', async ({ page }) => {
    // Windows-1252 ("ANSI"): é, the en dash and the euro sign are single bytes.
    const ansi = Buffer.from([
      ...Buffer.from('Caf'),
      0xe9,
      ...Buffer.from(' menu '),
      0x96,
      ...Buffer.from(' prices in '),
      0x80,
      0x0d,
      0x0a,
      ...Buffer.from('Second line'),
      0x0d,
      0x0a,
    ]);
    await openFile(page, { name: 'menu.txt', buffer: ansi, mimeType: 'text/plain' });
    await clearSaved(page);
    expect(await paragraphs(page)).toEqual(['Café menu – prices in €', 'Second line']);
    await expect(page.getByTestId('document-title')).toContainText('menu');
    // Edit as a Word user would: a new line at the end, then Ctrl+S.
    await page.getByTestId('kiln-input-proxy').focus();
    await page.keyboard.press('Control+End');
    await page.keyboard.press('Enter');
    await page.keyboard.type('Third line – done');
    await expect
      .poll(() => paragraphs(page))
      .toEqual(['Café menu – prices in €', 'Second line', 'Third line – done']);
    await page.keyboard.press('Control+s');
    const expected = Buffer.concat([
      ansi,
      Buffer.from('Third line '),
      Buffer.from([0x96]),
      Buffer.from(' done\r\n'),
    ]);
    // An earlier write (autosave) may land first: wait for the saved bytes to settle.
    let saved: Buffer = Buffer.alloc(0);
    await expect
      .poll(async () => {
        saved = await savedBytes(page, 'menu.txt');
        return [...saved];
      })
      .toEqual([...expected]);
    // Reopened, the text is the same.
    await openFile(page, { name: 'menu.txt', buffer: saved });
    expect(await paragraphs(page)).toEqual(['Café menu – prices in €', 'Second line', 'Third line – done']);
    // UTF-16 with a byte order mark stays UTF-16.
    const utf16 = Buffer.concat([Buffer.from([0xff, 0xfe]), Buffer.from('Ünïcode text\r\n', 'utf16le')]);
    await openFile(page, { name: 'wide.txt', buffer: utf16 });
    await clearSaved(page);
    expect(await paragraphs(page)).toEqual(['Ünïcode text']);
    await page.getByTestId('kiln-input-proxy').focus();
    await page.keyboard.press('Control+s');
    // The file can exist before the write has finished: wait for the bytes to settle.
    await expect.poll(async () => [...(await savedBytes(page, 'wide.txt'))]).toEqual([...utf16]);
  });

  test('markdown and html export from File Export', async ({ page }) => {
    await openFile(page, {
      name: 'numbering.docx',
      buffer: readFileSync(repo('fixtures/docx/features/numbering.docx')),
    });
    await clearSaved(page);
    await saveAs(page, 'Export', /Markdown/);
    const md = (await savedBytes(page, 'numbering.md')).toString('utf8');
    expect(md).toContain('# Numbering and bullets');
    expect(md).toContain('\n- bullet item 1\n    - bullet item 1.1\n');
    expect(md).toContain('\n1. decimal item 1\n');
    await openFile(page, {
      name: 'tables.docx',
      buffer: readFileSync(repo('fixtures/docx/features/tables.docx')),
    });
    await saveAs(page, 'Export', /Web Page/);
    const html = (await savedBytes(page, 'tables.html')).toString('utf8');
    expect(html.startsWith('<!doctype html>')).toBe(true);
    expect(html).toContain('<td colspan="3">Merged across three columns</td>');
    expect(html).toContain('rowspan="3"');
    // The exported Markdown opens again in Kiln Write as a styled document.
    await openFile(page, { name: 'numbering.md', buffer: Buffer.from(md) });
    const paras = await paragraphs(page);
    expect(paras[0]).toBe('Numbering and bullets');
    expect(paras).toContain('bullet item 1.1');
  });

  test('text encodings: File Conversion when unsure, a warning when a save cannot hold a character', async ({
    page,
  }) => {
    // "Привет" in Windows-1251: six high bytes that GB18030 reads as well, so Kiln asks.
    const cyr = Buffer.from([0xcf, 0xf0, 0xe8, 0xe2, 0xe5, 0xf2]);
    await openFile(page, { name: 'greeting.txt', buffer: cyr, mimeType: 'text/plain' });
    await clearSaved(page);
    const ask = page.getByTestId('file-conversion');
    await expect(ask).toBeVisible();
    await ask.getByLabel('Text encoding').selectOption('windows-1251');
    await expect(page.getByTestId('file-conversion-preview')).toHaveText('Привет');
    await ask.getByRole('button', { name: 'OK' }).click();
    await expect.poll(() => paragraphs(page)).toEqual(['Привет']);
    // A character Windows-1251 cannot hold: saved as ?, after a warning that offers UTF-8.
    await page.getByTestId('kiln-input-proxy').focus();
    await page.keyboard.press('Control+End');
    await page.keyboard.insertText(' 日');
    await expect.poll(() => paragraphs(page)).toEqual(['Привет 日']);
    await page.keyboard.press('Control+s');
    const warn = page.getByTestId('encoding-warning');
    await expect(warn).toBeVisible();
    await expect(page.getByTestId('encoding-warning-chars')).toHaveText('日');
    await expect
      .poll(async () => [...(await savedBytes(page, 'greeting.txt'))])
      .toEqual([...cyr, 0x20, 0x3f, 0x0d, 0x0a]);
    await warn.getByRole('button', { name: 'Save as Unicode (UTF-8)' }).click();
    await expect
      .poll(async () => (await savedBytes(page, 'greeting.txt')).toString('utf8'))
      .toBe('Привет 日\r\n');
    await expect(warn).toHaveCount(0);
  });

  test('docm preserve: Save As keeps the macros and a docx save drops them', async ({ page }) => {
    const docm = readFileSync(repo('e2e/fixtures/macros.docm'));
    await openFile(page, { name: 'macros.docm', buffer: docm });
    await clearSaved(page);
    await expect(page.getByTestId('document-title')).toContainText('macros');
    // Edit, then File ▸ Save As ▸ Word Macro-Enabled Document.
    await page.getByTestId('kiln-input-proxy').focus();
    await page.keyboard.press('Control+End');
    await page.keyboard.type(' Edited in Kiln.');
    await saveAs(page, 'Save As', /Word Macro-Enabled Document/);
    const saved = await savedBytes(page, 'macros.docm');
    // The VBA project is copied byte for byte and the package stays macro-enabled.
    expect(zipEntry(saved, 'word/vbaProject.bin')).toEqual(zipEntry(docm, 'word/vbaProject.bin'));
    expect(zipEntry(saved, '[Content_Types].xml')?.toString('utf8')).toContain('macroEnabled.main+xml');
    await openFile(page, { name: 'macros-reopened.docm', buffer: saved });
    const paras = await paragraphs(page);
    expect(paras.join('\n')).toContain('Edited in Kiln.');
    const summary = await page.evaluate(async () => {
      const w = (window as unknown as KilnWindow).__kilnWrite as WriteHandle;
      return (await w.client.query(w.doc, { CompatReport: { format: 'Docx' } })) as {
        CompatReport: { code: string; message: string }[];
      };
    });
    // Saving it as .docx would drop the macros, and the Compatibility Checker says so.
    expect(summary.CompatReport.map((i) => i.message).join(' ')).toMatch(/macro/i);
  });

  test("docm as docx: Word's macro-free question, No goes back to Save As, Yes saves without the macros", async ({
    page,
  }) => {
    const docm = readFileSync(repo('e2e/fixtures/macros.docm'));
    // The fixture's VBA project is a real OLE compound file (Apache POI's SimpleMacro.docm).
    expect(zipEntry(docm, 'word/vbaProject.bin')?.subarray(0, 8)).toEqual(
      Buffer.from([0xd0, 0xcf, 0x11, 0xe0, 0xa1, 0xb1, 0x1a, 0xe1]),
    );
    await openFile(page, { name: 'macros.docm', buffer: docm });
    await clearSaved(page);
    await saveAs(page, 'Save As', /^\.docx Word Document/);
    const ask = page.getByTestId('macro-free-question');
    await expect(ask).toBeVisible();
    await expect(ask).toContainText('The following cannot be saved in a macro-free document');
    await expect(ask).toContainText('VBA project');
    await expect(ask).toContainText('Continue saving as a macro-free document?');
    // No: back to Save As, with the macro-enabled format ready.
    await ask.getByRole('button', { name: 'No' }).click();
    await expect(ask).toHaveCount(0);
    await expect(page.locator('[data-format="Docm"]')).toBeFocused();
    // Yes: saved as .docx, without the VBA project.
    const dialog = page.getByRole('dialog', { name: /File menu/ });
    await dialog.getByRole('button', { name: /^\.docx Word Document/ }).click();
    await ask.getByRole('button', { name: 'Yes' }).click();
    const saved = await savedBytes(page, 'macros.docx');
    expect(zipEntry(saved, 'word/vbaProject.bin')).toBeNull();
    expect(zipEntry(saved, '[Content_Types].xml')?.toString('utf8')).not.toContain('macroEnabled');
  });
});
