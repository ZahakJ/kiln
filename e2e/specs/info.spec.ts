/**
 * File ▸ Info: Inspect Document, the Compatibility Checker and Properties in Kiln Write,
 * Grid and Deck, each from its designed dialog; and Ctrl+N, which makes a document in the default
 * editing language and brings it to the front of the window.
 */
import { readFileSync } from 'node:fs';
import { fileURLToPath } from 'node:url';
import { expect, type Page, test } from '@playwright/test';
import { expectNoSeriousA11yViolations } from '../../packages/testing/src/playwright/a11y.ts';
import { loaded, settle } from './i18n-helpers.ts';

const repo = (p: string) => fileURLToPath(new URL(`../../${p}`, import.meta.url));
const shot = (name: string) => repo(`e2e/screenshots/info/${name}`);

type Json = Record<string, unknown>;

async function open(page: Page, app: 'write' | 'grid' | 'deck', options?: Json): Promise<void> {
  if (options)
    await page.addInitScript((o) => localStorage.setItem('kiln.options', JSON.stringify(o)), options);
  await page.goto(`/#/${app}`);
  await loaded(page, {});
}

// While the frame remounts after Ctrl+N there is no document for a moment; that reads as -1.
async function doc(page: Page): Promise<number> {
  return page.evaluate(
    // biome-ignore lint/suspicious/noExplicitAny: the development handle
    () => ((window as any).__kiln?.registry?.context?.core?.summary?.doc as number | undefined) ?? -1,
  );
}

async function query<T = Json>(page: Page, q: unknown): Promise<T> {
  return page.evaluate(async (q) => {
    // biome-ignore lint/suspicious/noExplicitAny: the development handle
    const k = (window as any).__kiln;
    return k.client.query(k.registry.context.core.summary.doc, q);
  }, q) as Promise<T>;
}

async function core(page: Page, command: unknown): Promise<void> {
  await page.evaluate(async (c) => {
    // biome-ignore lint/suspicious/noExplicitAny: the development handle
    const k = (window as any).__kiln;
    await k.client.execute(k.registry.context.core.summary.doc, c);
  }, command);
}

async function info(page: Page) {
  await page.getByRole('button', { name: 'File' }).click();
  await page.getByRole('button', { name: 'Info' }).click();
  const main = page.getByRole('dialog', { name: /File menu/ }).getByRole('main');
  await expect(main.getByRole('heading', { level: 1 })).toHaveText('Info');
  return main;
}

async function inspectCounts(page: Page): Promise<Record<string, number>> {
  const r = await query<{ Inspect: { kind: string; count: number }[] }>(page, 'Inspect');
  return Object.fromEntries(r.Inspect.map((i) => [i.kind, i.count]));
}

test.describe('info', () => {
  test('Write: Inspect Document finds comments and tracked changes, Remove All takes them out', async ({
    page,
  }) => {
    await open(page, 'write');
    await core(page, { Write: { InsertText: { text: 'Bisque firing at 950 degrees.' } } });
    await page.evaluate(async () => {
      // biome-ignore lint/suspicious/noExplicitAny: the development handle
      const r = (window as any).__kiln.registry;
      await r.execute('write.caret.select_all');
      await r.execute('write.comment.new', { text: 'Check the cones.' });
      await r.execute('write.caret.move', { motion: 'DocumentEnd', extend: false });
    });
    await core(page, { Write: 'TrackChanges' });
    await core(page, { Write: { InsertText: { text: ' Glaze next.' } } });
    await expect.poll(async () => (await inspectCounts(page)).Comments).toBe(1);
    expect((await inspectCounts(page)).Revisions).toBeGreaterThan(0);
    const main = await info(page);
    await page.screenshot({ path: shot('info-write-light-desktop.png') });
    await expectNoSeriousA11yViolations(page);
    await main.getByTestId('info-inspect').click();
    const dialog = page.getByTestId('inspect-dialog');
    await expect(dialog).toContainText('1 comment found');
    await expect(dialog).toContainText('tracked change');
    await settle(page);
    await page.screenshot({ path: shot('inspect-write-light-desktop.png') });
    await expectNoSeriousA11yViolations(page);
    await dialog.getByRole('button', { name: 'Remove All: Comments' }).click();
    await expect(dialog.locator('[data-kind="Comments"]')).toContainText('Removed');
    await dialog.getByRole('button', { name: 'Remove All: Tracked changes' }).click();
    await expect(dialog.locator('[data-kind="Revisions"]')).toContainText('Removed');
    expect(await inspectCounts(page)).toMatchObject({ Comments: 0, Revisions: 0 });
    // The text of the tracked insertion stays: Remove All accepted it.
    const p = await query<{ Paragraphs: { text: string }[] }>(page, 'Paragraphs');
    expect(p.Paragraphs.map((x) => x.text).join('\n')).toContain('Glaze next.');
  });

  test('the Compatibility Checker lists the macros a .docx drops from a .docm fixture', async ({ page }) => {
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
    await chooser.setFiles({
      name: 'macros.docm',
      mimeType: 'application/octet-stream',
      buffer: readFileSync(repo('e2e/fixtures/macros.docm')),
    });
    await expect(page.getByTestId('document-title')).toContainText('macros', { timeout: 45_000 });
    const main = await info(page);
    await main.getByTestId('info-compat').click();
    const dialog = page.getByTestId('compat-dialog');
    await dialog.getByRole('radio', { name: /\.docx/ }).click();
    await expect(dialog.locator('[data-severity="Loss"]')).toBeVisible();
    await expect(dialog.locator('[data-code="macros.removed"]')).toBeVisible();
    await settle(page);
    await page.screenshot({ path: shot('compat-docx-light-desktop.png') });
    await expectNoSeriousA11yViolations(page);
    await page.emulateMedia({ colorScheme: 'dark' });
    await settle(page);
    await page.screenshot({ path: shot('compat-docx-dark-desktop.png') });
    await dialog.getByRole('radio', { name: /\.docm/ }).click();
    await expect(dialog.locator('[data-code="macros.removed"]')).toHaveCount(0);
  });

  test('Grid: Inspect Document finds a hidden worksheet and a note; Properties reads and writes', async ({
    page,
  }) => {
    await open(page, 'grid');
    await core(page, { Grid: 'InsertSheet' });
    await core(page, { Grid: 'InsertSheet' });
    await core(page, { Grid: { HideSheet: { index: 2 } } });
    await core(page, {
      Grid: { SetNote: { text: 'Kiln 2 needs a new element.', author: 'Ana', cell: 'A1' } },
    });
    await expect.poll(async () => inspectCounts(page)).toMatchObject({ Comments: 1, HiddenSheets: 1 });
    const main = await info(page);
    await main.getByTestId('info-inspect').click();
    const inspect = page.getByTestId('inspect-dialog');
    await expect(inspect).toContainText('1 hidden worksheet found');
    await page.emulateMedia({ colorScheme: 'dark' });
    await settle(page);
    await page.screenshot({ path: shot('inspect-grid-dark-desktop.png') });
    await inspect.getByRole('button', { name: 'Remove All: Hidden worksheets' }).click();
    await expect(inspect.locator('[data-kind="HiddenSheets"]')).toContainText('Removed');
    await inspect.getByRole('button', { name: 'Done' }).click();
    await page.emulateMedia({ colorScheme: 'light' });
    // Properties: title, company and tags, kept by the core.
    await main.getByRole('button', { name: 'Edit Properties' }).click();
    const props = page.getByTestId('properties-dialog');
    await expect(props.getByLabel('Title')).toBeVisible();
    await props.getByLabel('Title').fill('Firing budget 2027');
    await props.getByLabel('Company').fill('Kiln & Co');
    await props.getByLabel('Tags').fill('budget; kiln');
    await settle(page);
    await page.screenshot({ path: shot('properties-grid-light-desktop.png') });
    await expectNoSeriousA11yViolations(page);
    await props.getByRole('button', { name: 'OK' }).click();
    await expect(props).toBeHidden();
    await expect
      .poll(async () => (await query<{ Properties: Json }>(page, 'Properties')).Properties)
      .toMatchObject({
        title: 'Firing budget 2027',
        company: 'Kiln & Co',
        keywords: 'budget; kiln',
        sheets: 2,
      });
    await expect(main.getByText('Firing budget 2027')).toBeVisible();
    await page.screenshot({ path: shot('info-grid-light-desktop.png') });
    await page.setViewportSize({ width: 390, height: 844 });
    await main.getByRole('button', { name: 'Edit Properties' }).click();
    await settle(page);
    await page.screenshot({ path: shot('properties-grid-light-phone.png') });
  });

  test('Deck: speaker notes, a hidden slide and properties', async ({ page }) => {
    await open(page, 'deck');
    await core(page, { Deck: { NewSlide: { layout: null } } });
    await core(page, { Deck: { SetNotes: { text: 'Mention the cone chart.' } } });
    await core(page, { Deck: { HideSlides: { hidden: true } } });
    await expect.poll(async () => inspectCounts(page)).toMatchObject({ SpeakerNotes: 1, HiddenSlides: 1 });
    const main = await info(page);
    await main.getByTestId('info-inspect').click();
    const inspect = page.getByTestId('inspect-dialog');
    await expect(inspect).toContainText('1 slide with notes found');
    await expect(inspect).toContainText('1 hidden slide found');
    await settle(page);
    await page.screenshot({ path: shot('inspect-deck-light-desktop.png') });
    await inspect.getByRole('button', { name: 'Remove All: Presentation notes' }).click();
    await expect(inspect.locator('[data-kind="SpeakerNotes"]')).toContainText('Removed');
    await inspect.getByRole('button', { name: 'Done' }).click();
    await main.getByRole('button', { name: 'Edit Properties' }).click();
    const props = page.getByTestId('properties-dialog');
    await props.getByLabel('Subject').fill('Studio open day');
    await props.getByRole('button', { name: 'OK' }).click();
    await expect
      .poll(async () => (await query<{ Properties: Json }>(page, 'Properties')).Properties)
      .toMatchObject({ subject: 'Studio open day', slides: 2 });
  });

  test('Arabic: Inspect Document and Properties read in Arabic and mirror', async ({ page }) => {
    await open(page, 'write', { language: { ui: 'ar' } });
    await page.getByRole('button', { name: 'ملف' }).click();
    await page.getByRole('button', { name: 'معلومات' }).click();
    await page.getByTestId('info-inspect').click();
    await expect(page.getByTestId('inspect-dialog')).toBeVisible();
    await settle(page);
    expect(await page.evaluate(() => document.documentElement.dir)).toBe('rtl');
    await page.screenshot({ path: shot('inspect-write-ar-desktop.png') });
    await expectNoSeriousA11yViolations(page);
  });

  test('Ctrl+N makes a document in the default editing language and brings it to the front', async ({
    page,
  }) => {
    await open(page, 'grid', { language: { editing: 'ar-SA' } });
    const first = await doc(page);
    await expect(page.getByTestId('document-title')).toContainText('مصنف1');
    await page.locator('[data-kiln-grid-proxy]').focus();
    await page.keyboard.press('Control+n');
    await expect.poll(() => doc(page)).not.toBe(first);
    await expect.poll(() => doc(page)).not.toBe(-1);
    await expect(page.getByTestId('document-title')).toContainText('مصنف2');
    expect(page.url()).toContain(`doc=${await doc(page)}`);
    const st = await query<{ Grid: { State: { sheets: { name: string }[]; view: Json } } }>(page, {
      Grid: 'State',
    });
    expect(st.Grid.State.sheets[0]?.name).toBe('ورقة1');
    expect(JSON.stringify(st)).toContain('"rtl":true');
    await page.waitForTimeout(300);
    await page.screenshot({ path: shot('ctrl-n-grid-ar-light-desktop.png') });
  });
});
