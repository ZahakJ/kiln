/**
 * The way back to Kiln Home from a document in the web build: the title bar's app mark is a
 * button to Kiln Home, the browser's Back shows Kiln Home and Forward the document again, and the
 * document stays open in the tab (Home lists it under Open in this window), its unsaved work
 * kept and nothing asked. For a new blank document, one from a template and one opened from a file,
 * in English and in Arabic (right to left).
 */
import { readFileSync } from 'node:fs';
import { fileURLToPath } from 'node:url';
import { expect, type Page, test } from '@playwright/test';

const repo = (p: string) => fileURLToPath(new URL(`../../${p}`, import.meta.url));
type WriteHandle = {
  client: { query: (doc: unknown, q: unknown) => Promise<unknown> };
  doc: unknown;
  controller: { getStats(): { tiles: number } } | null;
};
type Win = { __kilnWrite?: WriteHandle };

async function home(page: Page): Promise<void> {
  await expect(
    page.getByTestId('home-open-document').or(page.locator('.kiln-home__files')).first(),
  ).toBeVisible({
    timeout: 30_000,
  });
  await expect(page).toHaveURL(/#\/home$/);
}

async function writeReady(page: Page): Promise<void> {
  await page.waitForFunction(
    () => ((window as unknown as Win).__kilnWrite?.controller?.getStats().tiles ?? 0) > 0,
    null,
    {
      timeout: 45_000,
    },
  );
}

async function paragraphs(page: Page): Promise<string[]> {
  return page.evaluate(async () => {
    const w = (window as unknown as Win).__kilnWrite as WriteHandle;
    const r = (await w.client.query(w.doc, 'Paragraphs')) as { Paragraphs?: { text: string }[] };
    return (r.Paragraphs ?? []).map((p) => p.text);
  });
}

/** Opens a document the way named, from Kiln Home, and types a line in it. */
async function open(page: Page, how: 'blank' | 'template' | 'file'): Promise<void> {
  if (how === 'blank') await page.evaluate(() => (window.location.hash = '#/write'));
  if (how === 'template')
    await page.evaluate(() => (window.location.hash = '#/write?template=write/letter.kwrite'));
  if (how === 'file') {
    await page.evaluate(() => {
      // biome-ignore lint/suspicious/noExplicitAny: not in the DOM typings
      delete (window as any).showOpenFilePicker;
    });
    const [chooser] = await Promise.all([page.waitForEvent('filechooser'), page.keyboard.press('Control+o')]);
    await chooser.setFiles({
      name: 'tables.docx',
      mimeType: 'application/octet-stream',
      buffer: readFileSync(repo('fixtures/docx/features/tables.docx')),
    });
  }
  await writeReady(page);
  await page.getByTestId('kiln-input-proxy').focus();
  await page.keyboard.press('Control+End');
  await page.keyboard.press('Enter');
  await page.keyboard.type('Kept on the way home');
  await expect.poll(() => paragraphs(page)).toContain('Kept on the way home');
}

async function backInTheDocument(page: Page): Promise<void> {
  await writeReady(page);
  await expect(page).toHaveURL(/#\/write\?doc=\d+/);
  await expect.poll(() => paragraphs(page), { timeout: 20_000 }).toContain('Kept on the way home');
}

for (const lang of ['en', 'ar'] as const)
  for (const how of ['blank', 'template', 'file'] as const)
    test(`${lang}: from a ${how} document, the app mark and Back go to Kiln Home and the document stays`, async ({
      page,
    }) => {
      test.setTimeout(180_000);
      if (lang === 'ar')
        await page.addInitScript(() =>
          localStorage.setItem('kiln.options', JSON.stringify({ language: { ui: 'ar' } })),
        );
      const asked = page.getByTestId('save-changes');
      await page.goto('/#/home');
      await home(page);
      await open(page, how);

      // The app mark: a button named Kiln Home, reached from the keyboard.
      const mark = page.getByTestId('titlebar-home');
      await expect(mark).toHaveAccessibleName('Kiln Home');
      if (lang === 'ar') await expect(page.locator('html')).toHaveAttribute('dir', 'rtl');
      await mark.focus();
      await expect(mark).toBeFocused();
      await page.keyboard.press('Enter');
      await home(page);
      await expect(asked).toHaveCount(0);
      // Home lists the document, unsaved, under Open in this window.
      const listed = page.getByTestId('home-open-document');
      await expect(listed).toHaveCount(1);
      if (how === 'file') await expect(listed).toContainText('tables');
      // Back returns to the document, Forward to Kiln Home.
      await page.goBack();
      await backInTheDocument(page);
      await page.goForward();
      await home(page);
      // From Home's list, the document again.
      await listed.click();
      await backInTheDocument(page);

      // The browser's Back from the document: Kiln Home, with nothing asked; Forward: the document.
      await page.goBack();
      await home(page);
      await expect(asked).toHaveCount(0);
      await page.goForward();
      await backInTheDocument(page);
    });
