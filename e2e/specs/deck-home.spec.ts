/**
 * Kiln Deck's Home tab in the browser: Paste ▸ Picture, the Character Spacing menu,
 * Format Shape's gradient and pattern fills, and Convert to List Diagram — each kept through a
 * save. Screenshots go to e2e/screenshots/deck-home/.
 */
import { fileURLToPath } from 'node:url';
import { expect, type Page, test } from '@playwright/test';
import { deck, newDeck, onSlide, ribbonTab, saveAndReopen, saved, state, zipEntry } from './deck-helpers.ts';

const shot = (name: string) => fileURLToPath(new URL(`../screenshots/deck-home/${name}`, import.meta.url));

async function clickOnSlide(page: Page, x: number, y: number) {
  const at = await onSlide(page, x, y);
  await page.mouse.click(at.x, at.y);
}

const slideXml = async (page: Page) => zipEntry(await saved(page), 'ppt/slides/slide1.xml');

test.describe('deck-home', () => {
  test('clipboard: paste as picture', async ({ page }) => {
    await newDeck(page);
    await deck(page, { InsertShape: { preset: 'star5', rect: { x: 900, y: 420, width: 200, height: 200 } } });
    await clickOnSlide(page, 1000, 520);
    await page.keyboard.press('Control+c');
    await ribbonTab(page, 'Home');
    await page.getByRole('button', { name: 'More options for Paste' }).click();
    await expect(page.getByRole('menuitem', { name: 'Picture', exact: true })).toBeVisible();
    await page.getByRole('menuitem', { name: 'Picture', exact: true }).click();
    await expect.poll(async () => (await state(page)).shapes.map((s) => s.kind)).toContain('picture');
    // It lands where the star was, over it.
    await page.getByTestId('deck-editor').screenshot({ path: shot('paste-picture.png') });
    await saveAndReopen(page, 'Pasted picture.pptx');
    expect((await state(page)).shapes.some((s) => s.kind === 'picture')).toBe(true);
  });

  test('font: character spacing', async ({ page }) => {
    await newDeck(page);
    await clickOnSlide(page, 640, 250);
    await page.keyboard.type('Spaced out');
    await page.keyboard.press('Control+a');
    await ribbonTab(page, 'Home');
    await page.getByRole('button', { name: 'Character Spacing' }).click();
    await page.getByRole('menuitem', { name: 'Very Loose' }).click();
    await expect.poll(slideXml.bind(null, page)).toContain('spc="600"');
    await page.getByRole('button', { name: 'Character Spacing' }).click();
    await page.getByRole('menuitem', { name: 'Very Tight' }).click();
    await expect.poll(slideXml.bind(null, page)).toContain('spc="-300"');
    await page.getByTestId('deck-editor').screenshot({ path: shot('character-spacing.png') });
    // More Spacing… is the Font dialog's spacing field.
    await page.getByRole('button', { name: 'Character Spacing' }).click();
    await page.getByRole('menuitem', { name: 'More Spacing…' }).click();
    await expect(
      page.getByTestId('deck-font').getByRole('spinbutton', { name: 'Character spacing' }),
    ).toBeVisible();
  });

  test('drawing: format shape fills', async ({ page }) => {
    await newDeck(page);
    await deck(page, {
      InsertShape: { preset: 'roundRect', rect: { x: 820, y: 400, width: 360, height: 240 } },
    });
    await ribbonTab(page, 'Home');
    await page.getByRole('button', { name: 'Format Shape…' }).first().click();
    const dialog = page.getByTestId('deck-format-shape');
    await dialog.getByRole('radio', { name: 'Gradient fill' }).check();
    await dialog.screenshot({ path: shot('format-shape.png') });
    await dialog.getByRole('button', { name: 'Apply' }).click();
    await expect.poll(slideXml.bind(null, page)).toContain('<a:gradFill');
    await page.getByTestId('deck-editor').screenshot({ path: shot('gradient-fill.png') });
    await page.getByRole('button', { name: 'Format Shape…' }).first().click();
    await dialog.getByRole('radio', { name: 'Pattern fill' }).check();
    await dialog.getByRole('button', { name: 'Apply' }).click();
    await expect.poll(slideXml.bind(null, page)).toContain('<a:pattFill prst="pct25"');
    await page.getByTestId('deck-editor').screenshot({ path: shot('pattern-fill.png') });
    await saveAndReopen(page, 'Fills.pptx');
    expect(zipEntry(await saved(page), 'ppt/slides/slide1.xml')).toContain('<a:pattFill');
  });

  test('paragraph: list diagram', async ({ page }) => {
    await newDeck(page);
    await deck(page, { NewSlide: { layout: null } });
    // The content placeholder of Title and Content.
    await clickOnSlide(page, 640, 420);
    await page.keyboard.type('Plan');
    for (const line of ['Build', 'Test', 'Ship']) {
      await page.keyboard.press('Enter');
      await page.keyboard.type(line);
    }
    // Convert to List Diagram is a P1 command (hidden in builds of the v0.1 tier): it runs on the
    // core here, as Home ▸ Convert to List Diagram ▸ Basic Process does.
    await deck(page, { ConvertToDiagram: { layout: 'Process' } });
    await expect
      .poll(async () => (await state(page)).shapes.find((s) => s.kind === 'group')?.name)
      .toBe('List Diagram 1');
    await page.getByTestId('deck-editor').screenshot({ path: shot('list-diagram.png') });
    await saveAndReopen(page, 'Diagram.pptx');
    await deck(page, { SelectSlide: { index: 1, extend: false, toggle: false } });
    expect((await state(page)).shapes.some((s) => s.name === 'List Diagram 1')).toBe(true);
  });
});
