/**
 * Kiln Deck's files in the browser: PowerPoint presentations opened through File ▸ Open
 * with their masters, layouts, pictures, tables, charts, motion and notes; File ▸ Save writing a
 * package whose untouched parts are the original bytes and which LibreOffice opens with every
 * slide; a PowerPoint Show (.ppsx) starting its slide show as it opens; a macro-enabled
 * presentation keeping its VBA project through Save As .pptm, and View ▸ Macros; and the status
 * bar. Saved files go to e2e/screenshots/deck-files/.
 *
 * The web host saves to the browser's private file system when the File System Access API is not
 * there (the tests remove it); the tests read the saved bytes back from it.
 */
import { execFileSync } from 'node:child_process';
import { existsSync, mkdirSync, readdirSync, readFileSync, rmSync, writeFileSync } from 'node:fs';
import { fileURLToPath } from 'node:url';
import { inflateRawSync } from 'node:zlib';
import { expect, type Page, test } from '@playwright/test';
import { deck, repoFile, ribbonTab, state, waitForStage } from './deck-helpers.ts';

const out = (name: string) => fileURLToPath(new URL(`../screenshots/deck-files/${name}`, import.meta.url));
const SOFFICE = process.env.SOFFICE ?? '/usr/bin/soffice';

/** Opens a file through File ▸ Open ▸ Browse (the file input the picker falls back to). */
async function openFile(page: Page, name: string, buffer: Buffer) {
  await page.goto('/#/home?blink=0');
  await expect(
    page.getByText('Your recent files will gather here').or(page.locator('.kiln-home__table')).first(),
  ).toBeVisible({ timeout: 45_000 });
  await page.evaluate(async () => {
    // biome-ignore lint/suspicious/noExplicitAny: the API is not in the DOM typings
    const w = window as any;
    delete w.showOpenFilePicker;
    delete w.showSaveFilePicker;
    const root = await navigator.storage.getDirectory();
    await root.removeEntry('documents', { recursive: true }).catch(() => undefined);
  });
  const [chooser] = await Promise.all([page.waitForEvent('filechooser'), page.keyboard.press('Control+o')]);
  await chooser.setFiles({ name, mimeType: 'application/octet-stream', buffer });
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

/** The bytes of a saved file in the origin's private file system, once the package is complete. */
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
        return bytes !== null && Buffer.from(bytes).lastIndexOf(Buffer.from([0x50, 0x4b, 0x05, 0x06])) > 0;
      },
      { message: `${name} is saved`, timeout: 20_000 },
    )
    .toBe(true);
  return Buffer.from(bytes ?? []);
}

async function saveAs(page: Page, format: RegExp) {
  const dialog = page.getByRole('dialog', { name: /File menu/ });
  if (!(await dialog.isVisible())) await page.getByRole('button', { name: 'File' }).click();
  await dialog
    .getByRole('navigation', { name: 'File menu' })
    .getByRole('button', { name: 'Save As' })
    .click();
  await dialog.getByRole('button', { name: format }).click();
}

/** LibreOffice's PDF of a presentation: its page count (one page per slide). */
function libreOfficePages(file: string): number {
  const dir = out('lo');
  rmSync(dir, { recursive: true, force: true });
  mkdirSync(dir, { recursive: true });
  execFileSync(repoFile('tools/oracle-soffice.sh'), [file, 'pdf', dir], {
    stdio: ['ignore', 'pipe', 'pipe'],
    timeout: 120_000,
  });
  const pdf = readdirSync(dir).find((f) => f.endsWith('.pdf'));
  if (!pdf) throw new Error(`LibreOffice wrote no PDF for ${file}`);
  return (readFileSync(`${dir}/${pdf}`, 'latin1').match(/\/Type\s*\/Page\b/g) ?? []).length;
}

test.describe('deck-files', () => {
  test.describe.configure({ timeout: 120_000 });

  test('pptx open: masters, layouts, pictures, tables, charts, motion and notes open from File ▸ Open', async ({
    page,
  }) => {
    for (const [name, check] of [
      ['charts.pptx', (s: Awaited<ReturnType<typeof state>>) => s.shapes.some((x) => x.kind === 'chart')],
      ['tables.pptx', (s: Awaited<ReturnType<typeof state>>) => s.shapes.some((x) => x.kind === 'table')],
      ['pictures.pptx', (s: Awaited<ReturnType<typeof state>>) => s.shapes.some((x) => x.kind === 'picture')],
      ['notes.pptx', (s: Awaited<ReturnType<typeof state>>) => s.notes.length > 0],
      ['animations.pptx', (s: Awaited<ReturnType<typeof state>>) => s.slides.length > 1],
    ] as const) {
      await openFile(page, name, readFileSync(repoFile(`fixtures/pptx/features/${name}`)));
      await expect(page.getByTestId('document-title')).toContainText(name.replace('.pptx', ''), {
        timeout: 45_000,
      });
      await waitForStage(page);
      let st = await state(page);
      // The slide with the feature (the first one that has it).
      for (let i = 0; i < st.slides.length && !check(st); i++) {
        await deck(page, { SelectSlide: { index: i, extend: false, toggle: false } });
        st = await state(page);
      }
      expect(check(st), name).toBe(true);
      expect(st.slides.length).toBeGreaterThan(0);
      await expect(page.getByTestId('deck-rail').locator('canvas').first()).toBeVisible();
      await page.getByTestId('deck-editor').screenshot({ path: out(name.replace('.pptx', '.png')) });
    }
    // Transitions and animations are read: the motion tabs show them.
    const st = await state(page);
    expect(st.slides.some((s) => (s as unknown as { motion: boolean }).motion)).toBe(true);
  });

  test('pptx save: File ▸ Save keeps untouched parts byte for byte and LibreOffice opens every slide', async ({
    page,
  }) => {
    const original = readFileSync(repoFile('fixtures/pptx/features/sections.pptx'));
    await openFile(page, 'sections.pptx', original);
    await waitForStage(page);
    const st = await state(page);
    const title = st.shapes.find((s) => /^Title/.test(s.name));
    if (!title) throw new Error('no title');
    await deck(page, { SelectShapes: { ids: [title.id] } });
    await deck(page, 'EditText');
    await deck(page, { InsertText: { text: 'Kiln ' } });
    await deck(page, 'Escape');
    await page.getByTestId('deck-stage').focus();
    await page.keyboard.press('Control+s');
    // The first save of an opened file goes to Save As: the same format.
    const dialog = page.getByRole('dialog', { name: /File menu/ });
    if (await dialog.isVisible())
      await dialog.getByRole('button', { name: /PowerPoint Presentation/ }).click();
    const saved = await savedBytes(page, 'sections.pptx');
    writeFileSync(out('sections-saved.pptx'), saved);
    // The edited slide changed; the other slides, layouts and the theme are the original bytes.
    expect(zipEntry(saved, 'ppt/slides/slide1.xml')?.toString('utf8')).toContain('Kiln ');
    for (const part of [
      'ppt/slides/slide2.xml',
      'ppt/slideLayouts/slideLayout1.xml',
      'ppt/theme/theme1.xml',
    ]) {
      expect(zipEntry(saved, part), part).toEqual(zipEntry(original, part));
    }
    if (!existsSync(SOFFICE) && process.env.KILN_E2E_NO_LIBREOFFICE === '1') {
      console.log(`pptx save: LibreOffice check skipped, no LibreOffice at ${SOFFICE}`);
      return;
    }
    expect(libreOfficePages(out('sections-saved.pptx'))).toBe(st.slides.length);
  });

  test('ppsx: a PowerPoint Show opens straight into its slide show and saves as a show', async ({ page }) => {
    await openFile(page, 'show.ppsx', readFileSync(repoFile('e2e/fixtures/show.ppsx')));
    await expect(page).toHaveURL(/#\/present/, { timeout: 45_000 });
    await expect(page.getByTestId('deck-show')).toBeVisible();
    await page.keyboard.press('Escape');
    await expect(page.getByTestId('deck-editor')).toBeVisible({ timeout: 30_000 });
    await waitForStage(page);
    await saveAs(page, /PowerPoint Show/);
    const saved = await savedBytes(page, 'show.ppsx');
    expect(zipEntry(saved, '[Content_Types].xml')?.toString('utf8')).toContain('slideshow.main+xml');
  });

  test('macros explained: a pptm opens, View ▸ Macros explains, Save As .pptm keeps the VBA project and .pptx asks first', async ({
    page,
  }) => {
    const pptm = readFileSync(repoFile('e2e/fixtures/macros.pptm'));
    await openFile(page, 'macros.pptm', pptm);
    await waitForStage(page);
    await ribbonTab(page, 'View');
    await page.locator('[data-command="ui.macros.explain"]').first().click();
    await expect(page.getByText('Kiln never runs macros')).toBeVisible();
    await expect(page.getByText(/\.pptm/).first()).toBeVisible();
    await saveAs(page, /PowerPoint Macro-Enabled Presentation/);
    const saved = await savedBytes(page, 'macros.pptm');
    expect(zipEntry(saved, 'ppt/vbaProject.bin')).toEqual(zipEntry(pptm, 'ppt/vbaProject.bin'));
    expect(zipEntry(saved, '[Content_Types].xml')?.toString('utf8')).toContain('macroEnabled.main+xml');
    await saveAs(page, /^\.pptx PowerPoint Presentation/);
    const ask = page.getByTestId('macro-free-question');
    await expect(ask).toBeVisible();
    await ask.getByRole('button', { name: 'Yes' }).click();
    const plain = await savedBytes(page, 'macros.pptx');
    expect(zipEntry(plain, 'ppt/vbaProject.bin')).toBeNull();
  });

  test('status bar: slide n of m, theme, notes, comments, views, slide show, zoom and fit', async ({
    page,
  }) => {
    await page.goto('/#/deck?blink=0');
    await waitForStage(page);
    await deck(page, { NewSlide: { layout: null } });
    const bar = page.locator('.kiln-statusbar');
    await expect(bar.getByText('Slide 2 of 2')).toBeVisible();
    await expect(bar.getByText('Kiln', { exact: true })).toBeVisible();
    // Notes hides and shows the notes pane.
    await page.getByTestId('deck-notes-toggle').click();
    await expect(page.getByTestId('deck-notes')).toBeHidden();
    await page.getByTestId('deck-notes-toggle').click();
    await expect(page.getByTestId('deck-notes')).toBeVisible();
    // Comments opens the Comments pane.
    await page.getByTestId('deck-comments-toggle').click();
    await expect(page.getByTestId('deck-comments-pane')).toBeVisible();
    await page.getByTestId('deck-comments-toggle').click();
    await expect(page.getByTestId('deck-comments-pane')).toBeHidden();
    // The view buttons.
    await bar.getByRole('radio', { name: 'Slide Sorter' }).click();
    await expect(page.getByTestId('deck-editor')).toHaveAttribute('data-view', 'SlideSorter');
    await bar.getByRole('radio', { name: 'Normal' }).click();
    await expect(page.getByTestId('deck-editor')).toHaveAttribute('data-view', 'SlideNormal');
    // Zoom, then Fit Slide to Current Window.
    await bar.getByRole('button', { name: /^Zoom level/ }).click();
    await page.getByRole('group', { name: 'Zoom presets' }).getByRole('button', { name: '200%' }).click();
    await expect(bar.getByRole('button', { name: 'Zoom level 200%' })).toBeVisible();
    await expect(page.getByTestId('deck-fit')).toHaveAttribute('aria-pressed', 'false');
    await page.getByTestId('deck-fit').click();
    await expect(page.getByTestId('deck-fit')).toHaveAttribute('aria-pressed', 'true');
    await bar.screenshot({ path: out('status-bar.png') });
    // Slide Show starts the show from the current slide.
    await page.getByTestId('deck-status-show').click();
    await expect(page).toHaveURL(/#\/present/);
    await expect.poll(async () => page.evaluate(() => location.hash)).toContain('present');
  });
});
