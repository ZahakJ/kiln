/**
 * File ▸ Options: eight sections, every option kept and in effect (ScreenTips,
 * the Mini Toolbar, Enter's direction in Grid, the speller's UPPERCASE words, the recent files),
 * Keyboard remapping with conflicts, export, import and reset, and screenshots of the dialog in
 * light and dark, desktop and phone, and Arabic.
 */
import { readFileSync, writeFileSync } from 'node:fs';
import { fileURLToPath } from 'node:url';
import { expect, type Page, test } from '@playwright/test';
import { expectNoSeriousA11yViolations } from '../../packages/testing/src/playwright/a11y.ts';
import { loaded, settle } from './i18n-helpers.ts';

const repo = (p: string) => fileURLToPath(new URL(`../../${p}`, import.meta.url));
const shot = (name: string) => repo(`e2e/screenshots/options/${name}`);

type Json = Record<string, unknown>;

async function open(page: Page, app: 'write' | 'grid' | 'deck', options?: Json): Promise<void> {
  if (options)
    await page.addInitScript((o) => localStorage.setItem('kiln.options', JSON.stringify(o)), options);
  await page.goto(`/#/${app}`);
  await loaded(page, {});
}

/** A query to the core about the window's document. */
async function query<T = Json>(page: Page, q: unknown): Promise<T> {
  return page.evaluate(async (q) => {
    // biome-ignore lint/suspicious/noExplicitAny: the development handle
    const k = (window as any).__kiln;
    const doc = k.registry.context.core.summary.doc;
    return k.client.query(doc, q);
  }, q) as Promise<T>;
}

/** The screen point of a character offset in the first paragraph of the Write document. */
async function pointAt(page: Page, offset: number): Promise<{ x: number; y: number }> {
  const rect = await page.evaluate(async (o) => {
    // biome-ignore lint/suspicious/noExplicitAny: the development handle
    const w = (window as any).__kilnWrite;
    const r = await w.client.query(w.doc, 'Paragraphs');
    const block = r.Paragraphs[0]?.id ?? 0;
    await w.client.execute(w.doc, { Write: { SelectTo: { pos: { block, offset: o }, extend: false } } });
    const s = await w.client.query(w.doc, 'Selection');
    return w.controller?.clientRectOf(s.Selection.caret.rect) ?? null;
  }, offset);
  if (!rect) throw new Error('no caret');
  return { x: rect.left + 1, y: rect.top + rect.height / 2 };
}

/** Selects text with the mouse, as a person does (the Mini Toolbar follows mouse selections). */
async function dragOver(page: Page, from: number, to: number): Promise<void> {
  const a = await pointAt(page, from);
  const b = await pointAt(page, to);
  await page.mouse.move(a.x, a.y);
  await page.mouse.down();
  await page.mouse.move(b.x, b.y, { steps: 6 });
  await page.waitForTimeout(400);
  await page.mouse.up();
}

async function openOptions(page: Page) {
  await page.getByRole('button', { name: 'File' }).click();
  await page.getByRole('button', { name: 'Options' }).click();
  const dialog = page.getByRole('dialog', { name: /Options/ });
  await expect(dialog).toBeVisible();
  await settle(page);
  return dialog;
}

const SECTIONS = ['General', 'Display', 'Proofing', 'Save', 'Language', 'Advanced', 'Keyboard', 'Privacy'];

test.describe('options', () => {
  test('eight sections, each with its title, in light and dark, desktop and phone', async ({ page }) => {
    await open(page, 'write');
    const dialog = await openOptions(page);
    const nav = dialog.getByRole('navigation', { name: 'Options sections' });
    expect((await nav.getByRole('button').allInnerTexts()).map((t) => t.trim())).toEqual(SECTIONS);
    for (const s of SECTIONS) {
      await nav.getByRole('button', { name: s }).click();
      await expect(dialog.getByRole('heading', { level: 2, name: s })).toBeVisible();
    }
    await nav.getByRole('button', { name: 'General' }).click();
    await page.screenshot({ path: shot('general-light-desktop.png') });
    await expectNoSeriousA11yViolations(page);
    await nav.getByRole('button', { name: 'Advanced' }).click();
    await page.screenshot({ path: shot('advanced-light-desktop.png') });
    await page.emulateMedia({ colorScheme: 'dark' });
    await settle(page);
    await nav.getByRole('button', { name: 'Save' }).click();
    await page.screenshot({ path: shot('save-dark-desktop.png') });
    await nav.getByRole('button', { name: 'Privacy' }).click();
    await page.screenshot({ path: shot('privacy-dark-desktop.png') });
    await expectNoSeriousA11yViolations(page);
    await page.setViewportSize({ width: 390, height: 844 });
    await nav.getByRole('button', { name: 'Proofing' }).click();
    await settle(page);
    await page.screenshot({ path: shot('proofing-dark-phone.png') });
    await page.emulateMedia({ colorScheme: 'light' });
    await nav.getByRole('button', { name: 'General' }).click();
    await settle(page);
    await page.screenshot({ path: shot('general-light-phone.png') });
  });

  test('in Arabic the dialog mirrors and reads in Arabic', async ({ page }) => {
    await open(page, 'grid', { language: { ui: 'ar' } });
    await page.getByRole('button', { name: 'ملف' }).click();
    await page.getByRole('button', { name: 'خيارات' }).click();
    const dialog = page.getByRole('dialog');
    await expect(dialog.getByRole('navigation')).toBeVisible();
    await settle(page);
    expect(await page.evaluate(() => document.documentElement.dir)).toBe('rtl');
    await dialog.getByRole('navigation').getByRole('button').nth(5).click();
    await expect(dialog.getByTestId('options-enter-direction')).toBeVisible();
    await page.screenshot({ path: shot('advanced-ar-desktop.png') });
    await expectNoSeriousA11yViolations(page);
  });

  test('ScreenTips and the shortcut in them follow Display; the Mini Toolbar follows General', async ({
    page,
  }) => {
    await open(page, 'write');
    await page.getByTestId('kiln-input-proxy').focus();
    await page.keyboard.type('Glaze the bowls on Friday before the firing.');
    // With the defaults, a mouse selection shows the Mini Toolbar and Bold has its ScreenTip.
    await dragOver(page, 2, 20);
    await expect(page.locator('.kiln-minitoolbar')).toBeVisible();
    await page.keyboard.press('End');
    const bold = page.locator('.kiln-ribbon [data-command="suite.format.bold"]').first();
    await bold.hover();
    await expect(page.getByRole('tooltip').first()).toContainText('Bold');
    await page.mouse.move(700, 700);
    const dialog = await openOptions(page);
    await dialog.getByRole('button', { name: 'Display' }).click();
    await dialog.getByLabel('ScreenTip style').selectOption('off');
    await dialog.getByRole('button', { name: 'General' }).click();
    await dialog.getByRole('checkbox', { name: /Show Mini Toolbar/ }).uncheck();
    await page.keyboard.press('Escape');
    await page.keyboard.press('Escape');
    await bold.hover();
    await page.waitForTimeout(900);
    await expect(page.getByRole('tooltip')).toHaveCount(0);
    await dragOver(page, 2, 20);
    await page.waitForTimeout(400);
    await expect(page.locator('.kiln-minitoolbar')).toHaveCount(0);
    const stored = await page.evaluate(() => JSON.parse(localStorage.getItem('kiln.options') ?? '{}'));
    expect(stored.display.screenTips).toBe('off');
    expect(stored.general.showMiniToolbar).toBe(false);
  });

  test('Classic Office look: colourful ribbon icons and a solid title bar in the app colour', async ({
    page,
  }) => {
    await open(page, 'grid');
    const bar = page.locator('.kiln-titlebar');
    const before = await bar.evaluate((el) => getComputedStyle(el).backgroundColor);
    const dialog = await openOptions(page);
    await dialog.getByRole('checkbox', { name: /Classic Office look/ }).check();
    await page.keyboard.press('Escape');
    await page.keyboard.press('Escape');
    await expect(page.locator('.kiln-appframe--classic')).toHaveCount(1);
    expect(await page.evaluate(() => document.documentElement.dataset.iconStyle)).toBe('colorful');
    const after = await bar.evaluate((el) => getComputedStyle(el).backgroundColor);
    expect(after).not.toBe(before);
    await page.mouse.move(700, 700);
    await page.screenshot({ path: shot('classic-look-grid-light-desktop.png') });
    await expectNoSeriousA11yViolations(page);
  });

  test('the shortcut sheet (Ctrl+/) shows a remapped key', async ({ page }) => {
    await open(page, 'write', {});
    await page.evaluate(() =>
      // biome-ignore lint/suspicious/noExplicitAny: the development handle
      (window as any).__kiln.keymap.assign('suite.format.strikethrough', 'Mod+Shift+X', 'write'),
    );
    await page.getByTestId('kiln-input-proxy').focus();
    await page.keyboard.press('Control+/');
    const sheet = page.getByRole('dialog', { name: 'Keyboard shortcuts' });
    await expect(sheet).toBeVisible();
    await sheet.getByLabel('Filter shortcuts').fill('strikethrough');
    await expect(sheet).toContainText('Ctrl+Shift+X');
    await settle(page);
    await page.screenshot({ path: shot('shortcut-sheet-light-desktop.png') });
  });

  test('Advanced: After pressing Enter, move selection Right in Kiln Grid', async ({ page }) => {
    await open(page, 'grid');
    const dialog = await openOptions(page);
    await dialog.getByRole('button', { name: 'Advanced' }).click();
    await dialog.getByTestId('options-enter-direction').selectOption('right');
    await page.keyboard.press('Escape');
    await page.keyboard.press('Escape');
    // A new workbook starts on A1.
    await page.locator('[data-kiln-grid-proxy]').focus();
    await page.keyboard.type('5');
    await page.keyboard.press('Enter');
    await expect
      .poll(
        async () =>
          (await query<{ Grid: { State: { name_box: string } } }>(page, { Grid: 'State' })).Grid.State
            .name_box,
      )
      .toMatch(/^B\d$/);
  });

  test('Proofing: words in UPPERCASE are checked once the option is off', async ({ page }) => {
    await open(page, 'write');
    await page.getByTestId('kiln-input-proxy').focus();
    await page.keyboard.type('The QWZX kiln.');
    const first = async () => {
      const p = await query<{ Paragraphs: { id: number }[] }>(page, 'Paragraphs');
      const r = await query<{ Write: { Spelling: { word: string } | null } }>(page, {
        Write: { NextMisspelling: { from: { block: p.Paragraphs[0]?.id, offset: 0 } } },
      });
      return r.Write.Spelling?.word ?? null;
    };
    await expect.poll(first).toBeNull();
    const dialog = await openOptions(page);
    await dialog.getByRole('button', { name: 'Proofing' }).click();
    await dialog.getByRole('checkbox', { name: 'Ignore words in UPPERCASE' }).uncheck();
    await page.keyboard.press('Escape');
    await page.keyboard.press('Escape');
    await expect.poll(first).toBe('QWZX');
  });

  test('Keyboard: remap Ctrl+B, the conflict is flagged, export, import and reset', async ({ page }) => {
    await open(page, 'write');
    const dialog = await openOptions(page);
    await dialog.getByRole('button', { name: 'Keyboard' }).click();
    await dialog.getByRole('searchbox', { name: /Search commands/ }).fill('strikethrough');
    await dialog.getByRole('option', { name: /Strikethrough/ }).click();
    const capture = dialog.getByLabel('Press new shortcut');
    await capture.click();
    await page.keyboard.press('Control+b');
    await expect(capture).toHaveValue('Ctrl+B');
    await expect(dialog.getByText(/Currently assigned to: Bold/)).toBeVisible();
    await page.screenshot({ path: shot('keyboard-conflict-light-desktop.png') });
    await dialog.getByRole('button', { name: 'Assign' }).click();
    await expect(dialog.getByText(/Ctrl\+B now runs Strikethrough; removed from Bold/)).toBeVisible();
    // Export: the remapping as a JSON file.
    const [download] = await Promise.all([
      page.waitForEvent('download'),
      dialog.getByRole('button', { name: 'Export' }).click(),
    ]);
    expect(download.suggestedFilename()).toBe('kiln-shortcuts.json');
    const exported = JSON.parse(readFileSync(await download.path(), 'utf8'));
    expect(exported.kind).toBe('kiln-keyboard');
    expect(exported.overrides.layers.write['suite.format.strikethrough']).toContain('Mod+B');
    // In the document, Ctrl+B now strikes through.
    await page.keyboard.press('Escape');
    await page.keyboard.press('Escape');
    await page.getByTestId('kiln-input-proxy').focus();
    await page.keyboard.type('Cone 6');
    await page.keyboard.press('Shift+Home');
    await page.keyboard.press('Control+b');
    await expect
      .poll(async () => {
        const f = await query<{ Write: { Format: { strike?: boolean; bold?: boolean } } }>(page, {
          Write: 'Format',
        });
        return [f.Write.Format.strike, f.Write.Format.bold];
      })
      .toEqual([true, false]);
    // Reset All, then import the exported file: the remapping is back.
    const again = await openOptions(page);
    await again.getByRole('button', { name: 'Keyboard' }).click();
    await again.getByRole('button', { name: 'Reset All' }).click();
    await expect(again.getByText('All shortcuts restored to the Office defaults.')).toBeVisible();
    await again.getByTestId('keyboard-import-file').setInputFiles({
      name: 'kiln-shortcuts.json',
      mimeType: 'application/json',
      buffer: readFileSync(await download.path()),
    });
    await expect(again.getByText(/Shortcuts imported: 2 commands changed/)).toBeVisible();
    // A file that maps two commands to one key shows the clash, with a way to each.
    const clash = repo('target/scratch/clash.json');
    writeFileSync(
      clash,
      JSON.stringify({
        kind: 'kiln-keyboard',
        version: 1,
        overrides: {
          version: 1,
          layers: {
            write: { 'suite.format.italic': ['Mod+Shift+Y'], 'suite.format.underline': ['Mod+Shift+Y'] },
          },
        },
      }),
    );
    await again.getByTestId('keyboard-import-file').setInputFiles(clash);
    await expect(again.getByTestId('keyboard-conflicts')).toContainText('Ctrl+Shift+Y');
    await page.screenshot({ path: shot('keyboard-clash-light-desktop.png') });
    // Not a keyboard file: a plain message, nothing changes.
    await again.getByTestId('keyboard-import-file').setInputFiles({
      name: 'notes.json',
      mimeType: 'application/json',
      buffer: Buffer.from('{"hello":1}'),
    });
    await expect(again.getByRole('alert')).toContainText('notes.json is not a Kiln keyboard file');
    await expectNoSeriousA11yViolations(page);
    await again.getByRole('button', { name: 'Reset All' }).click();
  });
});
