/**
 * Storage end to end: the autosave journal and Document Recovery after the page
 * reloads in the middle of an edit, AutoSave to file (to the Origin Private File System and to a
 * picked file, never over another program's change, from one window at a time), closing with
 * unsaved changes, the recent and pinned files of Kiln Home, native bundles, and Version History
 * of documents that are not shared. The web keeps its app storage in the Origin Private File
 * System (`kiln-store/`); saved files go to `documents/` there when the tests remove the File
 * System Access pickers, and to `picked/` when the pickers hand out real file handles of that
 * folder (`pickInOpfs`), as the browser's pickers hand out handles of the user's files.
 */
import { readFileSync } from 'node:fs';
import { fileURLToPath } from 'node:url';
import { inflateRawSync } from 'node:zlib';
import { expect, type Page, test } from '@playwright/test';
import { newDeck, state } from './deck-helpers.ts';

const repo = (p: string) => fileURLToPath(new URL(`../../${p}`, import.meta.url));
const shot = (name: string) => repo(`e2e/screenshots/storage/${name}`);

interface Handle {
  doc: number;
  client: {
    query(doc: number | null, q: unknown): Promise<Record<string, unknown>>;
    execute(doc: number | null, c: unknown): Promise<unknown>;
  };
  controller?: { getStats(): { tiles: number } } | null;
}
type KilnWindow = {
  __kilnWrite?: Handle;
  __kilnDeck?: Handle;
  __kilnDeckStage?: { stats: { frames: number } } | null;
  __kiln?: { registry: { execute(id: string): Promise<unknown> } };
};

function removePickers(page: Page) {
  return page.addInitScript(() => {
    // biome-ignore lint/suspicious/noExplicitAny: the API is not in the DOM typings
    const w = window as any;
    delete w.showOpenFilePicker;
    delete w.showSaveFilePicker;
  });
}

/**
 * File System Access pickers that pick `picked/<name>` in the Origin Private File System: real
 * `FileSystemFileHandle`s (written through `createWritable`, kept in IndexedDB across reloads).
 */
function pickInOpfs(page: Page, name: string) {
  return page.addInitScript((n) => {
    const pick = async () => {
      const root = await navigator.storage.getDirectory();
      const dir = await root.getDirectoryHandle('picked', { create: true });
      return dir.getFileHandle(n, { create: true });
    };
    // biome-ignore lint/suspicious/noExplicitAny: the API is not in the DOM typings
    const w = window as any;
    w.showSaveFilePicker = pick;
    w.showOpenFilePicker = async () => [await pick()];
  }, name);
}

/** The bytes of `picked/<name>` now. */
function picked(page: Page, name: string): Promise<Buffer> {
  return page
    .evaluate(async (n) => {
      const root = await navigator.storage.getDirectory();
      const dir = await root.getDirectoryHandle('picked', { create: true });
      const file = await (await dir.getFileHandle(n, { create: true })).getFile();
      return Array.from(new Uint8Array(await file.arrayBuffer()));
    }, name)
    .then((b) => Buffer.from(b));
}

/** Another program writes `picked/<name>`. */
async function writeElsewhere(page: Page, name: string, bytes: Buffer) {
  await page.evaluate(
    async ([n, b]) => {
      const root = await navigator.storage.getDirectory();
      const dir = await root.getDirectoryHandle('picked', { create: true });
      const stream = await (await dir.getFileHandle(n as string, { create: true })).createWritable();
      await stream.write(new Uint8Array(b as number[]));
      await stream.close();
    },
    [name, Array.from(bytes)] as const,
  );
}

async function summaryOf(page: Page): Promise<{ dirty: boolean; path: string | null } | null> {
  return page.evaluate(async () => {
    const h = (window as unknown as KilnWindow).__kilnWrite;
    if (!h) return null;
    const r = (await h.client.query(h.doc, 'Summary').catch(() => ({}))) as {
      Summary?: { dirty: boolean; path: string | null };
    };
    return r.Summary ?? null;
  });
}

async function waitForWrite(page: Page) {
  await page.waitForFunction(
    () => ((window as unknown as KilnWindow).__kilnWrite?.controller?.getStats().tiles ?? 0) > 0,
    null,
    { timeout: 60_000 },
  );
}

async function newWrite(page: Page) {
  await page.goto('/#/write');
  await waitForWrite(page);
  await page.getByTestId('kiln-input-proxy').focus();
}

async function paragraphs(page: Page): Promise<string[]> {
  return page.evaluate(async () => {
    // The editor of the window shown (absent for a moment while a window changes).
    const w = (window as unknown as KilnWindow).__kilnWrite;
    if (!w) return [];
    const r = (await w.client.query(w.doc, 'Paragraphs').catch(() => ({}))) as {
      Paragraphs?: { text: string }[];
    };
    return (r.Paragraphs ?? []).map((p) => p.text);
  });
}

async function run(page: Page, cmd: unknown, which: '__kilnWrite' | '__kilnDeck' = '__kilnWrite') {
  await page.evaluate(
    async ([c, w]) => {
      const h = (window as unknown as Record<string, Handle>)[w as string] as Handle;
      await h.client.execute(h.doc, c);
    },
    [cmd, which] as const,
  );
}

/** Saves (or saves as) and waits until the file is written (the host writes it later). */
async function save(
  page: Page,
  as: { path: string | null; format: string } | null,
  which: '__kilnWrite' | '__kilnDeck' = '__kilnWrite',
) {
  await run(page, as ? { Suite: { SaveAs: as } } : { Suite: 'Save' }, which);
  await expect
    .poll(() =>
      page.evaluate(async (w) => {
        const h = (window as unknown as Record<string, Handle>)[w] as Handle;
        const r = (await h.client.query(h.doc, 'Summary')) as {
          Summary?: { dirty: boolean; path: string | null };
        };
        return !!r.Summary && !r.Summary.dirty && r.Summary.path !== null;
      }, which),
    )
    .toBe(true);
}

/** The files of an area of the app storage, with their sizes. */
async function stored(page: Page, area: string): Promise<Record<string, number>> {
  return page.evaluate(async (a) => {
    const out: Record<string, number> = {};
    try {
      const root = await navigator.storage.getDirectory();
      const dir = await (await root.getDirectoryHandle('kiln-store')).getDirectoryHandle(a);
      for await (const [name, h] of dir as unknown as AsyncIterable<[string, FileSystemFileHandle]>) {
        out[decodeURIComponent(name)] = (await h.getFile()).size;
      }
    } catch {
      // Nothing stored yet.
    }
    return out;
  }, area);
}

/** The bytes of a saved file (`opfs:documents/<name>`), once written. */
async function savedBytes(page: Page, name: string, contains?: string): Promise<Buffer> {
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
        if (bytes === null || bytes.length === 0) return false;
        return contains === undefined || docxText(Buffer.from(bytes)).includes(contains);
      },
      { message: `${name} is saved${contains ? ` with “${contains}”` : ''}`, timeout: 20_000 },
    )
    .toBe(true);
  return Buffer.from(bytes ?? []);
}

/** One entry of a zip archive, inflated. */
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
    if (zip.subarray(at + 46, at + 46 + nameLen).toString('utf8') === name) {
      const start = local + 30 + zip.readUInt16LE(local + 26) + zip.readUInt16LE(local + 28);
      const data = zip.subarray(start, start + size);
      return method === 0 ? Buffer.from(data) : inflateRawSync(data);
    }
    at += 46 + nameLen + extra + comment;
  }
  return null;
}

/** The text of a Word file's body. */
function docxText(zip: Buffer): string {
  const xml = zipEntry(zip, 'word/document.xml')?.toString('utf8') ?? '';
  return [...xml.matchAll(/<w:t(?: [^>]*)?>([^<]*)<\/w:t>/g)].map((m) => m[1]).join('');
}

async function versionHistory(page: Page) {
  await page.evaluate(async () => {
    await (window as unknown as KilnWindow).__kiln?.registry.execute('suite.file.version_history');
  });
  const pane = page.getByTestId('version-history');
  await expect(pane).toBeVisible();
  return pane;
}

test.describe('Storage', () => {
  test.describe.configure({ timeout: 180_000 });

  test('the recovery pane after a reload shows the document with the edit', async ({ page }) => {
    await newWrite(page);
    await page.keyboard.type('Words the reload must not lose');
    await expect.poll(() => paragraphs(page)).toEqual(['Words the reload must not lose']);
    // The journal is on the disk: a marker, a checkpoint and the log after it.
    await expect
      .poll(async () => {
        const files = await stored(page, 'Journal');
        return (
          Object.keys(files).some((k) => k.endsWith('.meta')) &&
          Object.keys(files).some((k) => k.includes('.base'))
        );
      })
      .toBe(true);
    await page.waitForTimeout(800);
    // The page goes away without saving or closing the document.
    await page.reload();
    await waitForWrite(page);
    const pane = page.getByTestId('document-recovery');
    await expect(pane).toBeVisible({ timeout: 30_000 });
    const item = pane.locator('[data-recovery]');
    await expect(item).toHaveCount(1);
    await expect(item).toContainText('Document1');
    await expect(item).toContainText('Recovery copy from');
    await page.screenshot({ path: shot('recovery-pane.png') });
    await page.emulateMedia({ colorScheme: 'dark' });
    // The theme fades over.
    await page.waitForTimeout(600);
    await page.screenshot({ path: shot('recovery-pane-dark.png') });
    await page.emulateMedia({ colorScheme: 'light' });
    await item.getByTestId('recovery-open').click();
    // The recovered document opens in its own window, with the edit, marked as not saved.
    await expect
      .poll(() => paragraphs(page), { timeout: 30_000 })
      .toEqual(['Words the reload must not lose']);
    await expect(page.getByTestId('save-status')).toContainText(/Not saved|Unsaved/);
    await expect(pane.locator('[data-recovery]')).toHaveCount(0);
    // Deleting the last one empties the pane; a fresh reload finds nothing (the document is open).
    await page.screenshot({ path: shot('recovered.png') });
  });

  /** The browser stops finishing OPFS writes here: as far as they go, the page has already gone. */
  const stallWrites = (page: Page) =>
    page.evaluate(() => {
      FileSystemWritableFileStream.prototype.close = () => new Promise(() => undefined);
    });

  test('Write: the last words come back when the page goes before their writes finish', async ({ page }) => {
    await newWrite(page);
    await page.keyboard.type('First');
    await expect
      .poll(async () => Object.keys(await stored(page, 'Journal')).some((k) => k.includes('.base')))
      .toBe(true);
    await page.waitForTimeout(300);
    await stallWrites(page);
    await page.keyboard.type(' and last');
    await expect.poll(() => paragraphs(page)).toEqual(['First and last']);
    await page.reload();
    await waitForWrite(page);
    await page.getByTestId('document-recovery').getByTestId('recovery-open').click({ timeout: 30_000 });
    await expect.poll(() => paragraphs(page), { timeout: 30_000 }).toEqual(['First and last']);
  });

  test('Deck: the last change comes back when the page goes before its writes finish', async ({ page }) => {
    await newDeck(page);
    await run(page, { Deck: { NewSlide: { layout: null } } }, '__kilnDeck');
    await expect
      .poll(async () => Object.keys(await stored(page, 'Journal')).some((k) => k.includes('.base')))
      .toBe(true);
    await page.waitForTimeout(300);
    await stallWrites(page);
    await run(page, { Deck: { NewSlide: { layout: null } } }, '__kilnDeck');
    await page.reload();
    await page.getByTestId('document-recovery').getByTestId('recovery-open').click({ timeout: 60_000 });
    await expect
      .poll(
        () =>
          state(page).then(
            (s) => s.slides.length,
            () => 0,
          ),
        { timeout: 30_000 },
      )
      .toBe(3);
  });

  test('a recovered copy can be deleted', async ({ page }) => {
    await newWrite(page);
    await page.keyboard.type('Unwanted');
    await expect
      .poll(async () => Object.keys(await stored(page, 'Journal')).some((k) => k.includes('.base')))
      .toBe(true);
    await page.waitForTimeout(500);
    await page.reload();
    await waitForWrite(page);
    const pane = page.getByTestId('document-recovery');
    await expect(pane).toBeVisible({ timeout: 30_000 });
    await pane.getByTestId('recovery-delete').click();
    await expect(pane).toBeHidden();
    await expect.poll(async () => Object.keys(await stored(page, 'Journal')).length).toBe(0);
  });

  test('autosave writes the file to OPFS two seconds after the last edit', async ({ page }) => {
    await removePickers(page);
    await newWrite(page);
    const chip = page.getByTestId('save-status');
    // A new document has no file yet: its chip shows the save state, not the switch.
    await expect(chip).not.toHaveAttribute('role', 'switch');
    await page.keyboard.type('First words');
    await save(page, { path: 'opfs:documents/Auto.docx', format: 'Docx' });
    await expect(chip).toHaveAttribute('role', 'switch');
    await expect(chip).toHaveAttribute('aria-checked', 'false');
    await chip.click();
    await expect(chip).toHaveAttribute('aria-checked', 'true');
    await page.getByTestId('kiln-input-proxy').focus();
    await page.keyboard.press('End');
    await page.keyboard.type(' and more');
    await expect(chip).toContainText('Unsaved');
    // Nothing is written while typing goes on; two seconds after the last key the file is.
    const bytes = await savedBytes(page, 'Auto.docx', 'First words and more');
    expect(docxText(bytes)).toBe('First words and more');
    await expect(chip).toContainText('Saved');
    await page.screenshot({ path: shot('autosave-on.png'), clip: { x: 0, y: 0, width: 1440, height: 48 } });
  });

  test('autosave writes a picked file once typing stops, never over another change, from one window', async ({
    page,
    context,
  }) => {
    await pickInOpfs(page, 'Picked.docx');
    await newWrite(page);
    await page.keyboard.type('Picked words');
    await save(page, { path: null, format: 'Docx' });
    expect((await summaryOf(page))?.path).toMatch(/^handle:.*\/Picked\.docx$/);
    const first = await picked(page, 'Picked.docx');
    expect(docxText(first)).toBe('Picked words');
    const chip = page.getByTestId('save-status');
    await chip.click();
    await expect(chip).toHaveAttribute('aria-checked', 'true');
    // Typing for four seconds with short pauses: nothing is written while it goes on.
    await page.getByTestId('kiln-input-proxy').focus();
    await page.keyboard.press('End');
    for (const word of [' one', ' two', ' three', ' four', ' five', ' six', ' seven', ' eight']) {
      await page.keyboard.type(word);
      await page.waitForTimeout(500);
      expect(docxText(await picked(page, 'Picked.docx')), `while typing${word}`).toBe('Picked words');
    }
    // Two seconds after the last key, the picked file itself is written.
    await expect
      .poll(async () => docxText(await picked(page, 'Picked.docx')), { timeout: 20_000 })
      .toBe('Picked words one two three four five six seven eight');
    await expect(chip).toContainText('Saved');

    // Another program changes the file: AutoSave does not write over it, and asks.
    const theirs = readFileSync(repo('fixtures/docx/features/tables.docx'));
    await writeElsewhere(page, 'Picked.docx', theirs);
    await page.getByTestId('kiln-input-proxy').focus();
    await page.keyboard.type(' mine');
    const question = page.getByTestId('file-changed');
    await expect(question).toBeVisible({ timeout: 20_000 });
    expect((await picked(page, 'Picked.docx')).equals(theirs)).toBe(true);
    await page.screenshot({ path: shot('file-changed.png') });
    await question.getByTestId('file-changed-keep').click();
    await expect
      .poll(async () => docxText(await picked(page, 'Picked.docx')), { timeout: 20_000 })
      .toContain('eight mine');

    // A second tab opens the same file: it is the same file there (one path), and AutoSave stays
    // with the first window.
    const other = await context.newPage();
    await pickInOpfs(other, 'Picked.docx');
    await other.goto('/#/home');
    await expect(other.getByText(/Recent|recent/).first()).toBeVisible({ timeout: 45_000 });
    await other.keyboard.press('Control+o');
    await waitForWrite(other);
    await expect.poll(async () => (await summaryOf(other))?.path).toBe((await summaryOf(page))?.path);
    const otherChip = other.getByTestId('save-status');
    await otherChip.click();
    await expect(other.getByTestId('storage-notice')).toContainText(
      'AutoSave is on for Picked.docx in another window',
    );
    await expect(otherChip).toHaveAttribute('aria-checked', 'false');
    await other.close();
  });

  test('closing with unsaved changes writes them with AutoSave on, and otherwise asks', async ({ page }) => {
    await removePickers(page);
    await newWrite(page);
    await page.keyboard.type('Kept on close');
    await save(page, { path: 'opfs:documents/Close.docx', format: 'Docx' });
    const chip = page.getByTestId('save-status');
    await chip.click();
    await expect(chip).toHaveAttribute('aria-checked', 'true');
    // AutoSave on: Close right after typing writes the change first, then closes, asking nothing.
    await page.getByTestId('kiln-input-proxy').focus();
    await page.keyboard.press('End');
    await page.keyboard.type(' and this');
    await page.keyboard.press('Control+w');
    await savedBytes(page, 'Close.docx', 'Kept on close and this');
    await expect(page.getByTestId('save-changes')).toHaveCount(0);
    const closed = () =>
      page.evaluate(
        () =>
          (
            window as unknown as { __kiln?: { registry: { getState(id: string): { enabled: boolean } } } }
          ).__kiln?.registry.getState('suite.file.save').enabled === false,
      );
    await expect.poll(closed).toBe(true);
    await expect.poll(async () => Object.keys(await stored(page, 'Journal')).length).toBe(0);

    // AutoSave off: Close asks; Cancel keeps the document, Don't Save closes it and its
    // recovery copy goes.
    await page.evaluate(() => {
      window.location.hash = `#/write?new=${Date.now()}`;
    });
    await waitForWrite(page);
    await page.getByTestId('kiln-input-proxy').focus();
    await page.keyboard.type('Thrown away');
    await expect
      .poll(async () => Object.keys(await stored(page, 'Journal')).some((k) => k.endsWith('.meta')))
      .toBe(true);
    await page.keyboard.press('Control+w');
    const ask = page.getByTestId('save-changes');
    await expect(ask).toBeVisible();
    await expect(ask).toContainText('Document2');
    await page.screenshot({ path: shot('save-changes.png') });
    await ask.getByRole('button', { name: 'Cancel' }).click();
    await expect(ask).toBeHidden();
    await expect.poll(() => paragraphs(page)).toEqual(['Thrown away']);
    await page.keyboard.press('Control+w');
    await expect(ask).toBeVisible();
    await ask.getByTestId('save-changes-discard').click();
    await expect.poll(closed).toBe(true);
    await expect.poll(async () => Object.keys(await stored(page, 'Journal')).length).toBe(0);
  });

  test('recents update on open and a file can be pinned in Kiln Home', async ({ page }) => {
    await removePickers(page);
    await page.goto('/#/home');
    await expect(page.getByText('Your recent files will gather here')).toBeVisible({ timeout: 45_000 });
    const [chooser] = await Promise.all([page.waitForEvent('filechooser'), page.keyboard.press('Control+o')]);
    await chooser.setFiles({
      name: 'Tables.docx',
      mimeType: 'application/octet-stream',
      buffer: readFileSync(repo('fixtures/docx/features/tables.docx')),
    });
    await waitForWrite(page);
    await expect.poll(async () => Object.keys(await stored(page, 'Recents')).length).toBe(1);
    await page.goto('/#/home');
    const row = page.getByRole('row', { name: /Tables/ });
    await expect(row).toBeVisible({ timeout: 30_000 });
    await expect(row).toContainText('.docx');
    await row.getByRole('button', { name: 'Pin Tables.docx' }).click();
    await page.getByRole('tab', { name: 'Pinned' }).click();
    await expect(page.getByRole('row', { name: /Tables/ })).toBeVisible();
    await page.screenshot({ path: shot('home-pinned.png') });
    // The pin is kept: Home again, in a fresh load of the page.
    await page.reload();
    await page.getByRole('tab', { name: 'Pinned' }).click();
    await expect(page.getByRole('row', { name: /Tables/ })).toBeVisible({ timeout: 30_000 });
  });

  test('local version history lists the saves, shows, compares and restores one', async ({ page }) => {
    await removePickers(page);
    await newWrite(page);
    await page.keyboard.type('First draft');
    await save(page, { path: 'opfs:documents/Story.docx', format: 'Docx' });
    await page.getByTestId('kiln-input-proxy').focus();
    await page.keyboard.press('End');
    await page.keyboard.type(' grows');
    await page.keyboard.press('Control+s');
    await savedBytes(page, 'Story.docx', 'First draft grows');
    const pane = await versionHistory(page);
    const saved = pane.getByRole('button', { name: /Saved.*You/ });
    await expect(saved).toHaveCount(2, { timeout: 20_000 });
    // The older one, read-only in place of the document.
    await saved.nth(1).click();
    await expect(page.getByTestId('version-bar')).toBeVisible();
    await expect.poll(() => paragraphs(page)).toEqual(['First draft']);
    await pane.getByTestId('version-compare').click();
    await expect(pane.getByTestId('version-diff').locator('.kiln-versions__para ins')).toContainText('grows');
    await page.screenshot({ path: shot('local-version-history.png') });
    await pane.getByTestId('version-restore').click();
    await expect(page.getByTestId('version-bar')).toBeHidden();
    await expect.poll(() => paragraphs(page)).toEqual(['First draft']);
    await expect(pane.getByRole('button', { name: /Restored from/ })).toBeVisible();
    // The restored text is a change of the document: saved, it is the file.
    await page.keyboard.press('Control+s');
    await savedBytes(page, 'Story.docx', 'First draft');
  });

  test('a native bundle keeps its version history in the file', async ({ page }) => {
    await removePickers(page);
    await newWrite(page);
    await page.keyboard.type('Bundled');
    await save(page, { path: 'opfs:documents/Plan.kwrite', format: 'KWrite' });
    await page.getByTestId('kiln-input-proxy').focus();
    await page.keyboard.press('End');
    await page.keyboard.type(' words');
    await page.keyboard.press('Control+s');
    // The bundle on the disk: the manifest, the model and both versions.
    let zip: Buffer = Buffer.alloc(0);
    await expect
      .poll(async () => {
        zip = await savedBytes(page, 'Plan.kwrite');
        const index = zipEntry(zip, 'versions/index.json');
        return index ? (JSON.parse(index.toString('utf8')) as unknown[]).length : 0;
      })
      .toBe(2);
    const manifest = JSON.parse(zipEntry(zip, 'manifest.json')?.toString('utf8') ?? '{}');
    expect(manifest).toMatchObject({ format: 'kiln.bundle', version: 1, kind: 'Write', versions: 2 });
    expect(zipEntry(zip, 'model.bin')?.subarray(0, 4).toString('latin1')).toBe('KDOC');
    // Opened again, it has its text and its versions.
    await page.goto('/#/home');
    const row = page.getByRole('row', { name: /Plan/ });
    await expect(row).toContainText('.kwrite', { timeout: 30_000 });
    await row.locator('.kiln-home__file').click();
    await waitForWrite(page);
    await expect.poll(() => paragraphs(page)).toEqual(['Bundled words']);
    const pane = await versionHistory(page);
    await expect(pane.getByRole('button', { name: /Saved.*You/ })).toHaveCount(2, { timeout: 20_000 });
  });

  test('a presentation that is not shared keeps versions with its changed slides', async ({ page }) => {
    await removePickers(page);
    await page.goto('/#/deck?blink=0');
    await page.waitForFunction(
      () => ((window as unknown as KilnWindow).__kilnDeckStage?.stats.frames ?? 0) > 0,
      null,
      { timeout: 60_000 },
    );
    await save(page, { path: 'opfs:documents/Pitch.pptx', format: 'Pptx' }, '__kilnDeck');
    await run(page, { Deck: { NewSlide: { layout: null } } }, '__kilnDeck');
    await save(page, null, '__kilnDeck');
    const pane = await versionHistory(page);
    const saved = pane.getByRole('button', { name: /Saved.*You/ });
    await expect(saved).toHaveCount(2, { timeout: 20_000 });
    await saved.first().click();
    const slides = pane.getByTestId('version-slide');
    await expect(slides).toHaveCount(2);
    await expect(slides.nth(0)).toHaveAttribute('data-mark', 'Same');
    await expect(slides.nth(1)).toHaveAttribute('data-mark', 'Added');
    await page.screenshot({ path: shot('deck-local-versions.png') });
  });
});
