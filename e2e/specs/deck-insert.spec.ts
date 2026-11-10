/**
 * Kiln Deck's Insert tab in the browser: Icons as shapes, a highlighted Code Block,
 * Symbol with its font and character code, a link to an existing file, and a picture cropped
 * with the crop handles — each kept through a save. Screenshots go to
 * e2e/screenshots/deck-insert/.
 */
import { fileURLToPath } from 'node:url';
import { expect, test } from '@playwright/test';
import {
  deck,
  dragOnSlide,
  newDeck,
  onSlide,
  ribbonTab,
  saveAndReopen,
  saved,
  state,
  twoTonePng,
  zipEntry,
} from './deck-helpers.ts';

const shot = (name: string) => fileURLToPath(new URL(`../screenshots/deck-insert/${name}`, import.meta.url));

/** Clicks into the title placeholder of a new presentation's first slide. */
async function intoTitle(page: import('@playwright/test').Page) {
  const at = await onSlide(page, 640, 250);
  await page.mouse.click(at.x, at.y);
  await expect.poll(async () => (await state(page)).editing).toBe(true);
}

test.describe('deck-insert', () => {
  test('illustrations: icons', async ({ page }) => {
    await newDeck(page);
    await ribbonTab(page, 'Insert');
    await page.getByRole('button', { name: 'Icons', exact: true }).click();
    const dialog = page.getByTestId('deck-icons');
    await dialog.getByRole('searchbox', { name: 'Search icons' }).fill('rock');
    await dialog.getByRole('button', { name: 'Rocket', exact: true }).click();
    await dialog.getByRole('searchbox', { name: 'Search icons' }).fill('');
    await dialog.getByRole('button', { name: 'Idea', exact: true }).click();
    await dialog.screenshot({ path: shot('icons-dialog.png') });
    await dialog.getByRole('button', { name: 'Insert (2)' }).click();
    await expect
      .poll(async () => (await state(page)).shapes.filter((s) => s.name.startsWith('Graphic')).length)
      .toBe(2);
    // The icon is a shape: Shape Outline recolours it.
    await page.getByRole('tab', { name: 'Shape Format' }).click();
    await expect(page.getByRole('tab', { name: 'Shape Format' })).toHaveAttribute('aria-selected', 'true');
    await page.getByTestId('deck-editor').screenshot({ path: shot('icons.png') });
    const xml = zipEntry(await saved(page), 'ppt/slides/slide1.xml');
    expect(xml.match(/<a:custGeom>/g)?.length).toBe(2);
    await saveAndReopen(page, 'Icons.pptx');
    const names = (await state(page)).shapes.map((s) => s.name);
    expect(names.some((n) => n.includes('Rocket'))).toBe(true);
    expect(names.some((n) => n.includes('Idea'))).toBe(true);
  });

  test('code block', async ({ page }) => {
    await newDeck(page);
    await ribbonTab(page, 'Insert');
    await page.getByRole('button', { name: 'Code Block', exact: true }).click();
    const dialog = page.getByTestId('deck-code-block');
    await dialog.getByRole('combobox', { name: 'Language' }).click();
    await page.getByRole('option', { name: 'Rust', exact: true }).click();
    await dialog
      .getByRole('textbox', { name: 'Code' })
      .fill('fn main() {\n    let total = 42; // the answer\n    println!("{total}");\n}');
    await dialog.getByRole('button', { name: 'Insert' }).click();
    await expect
      .poll(async () => (await state(page)).shapes.find((s) => s.name.startsWith('Code Block'))?.name)
      .toMatch(/\(Rust\)$/);
    await page.getByTestId('deck-editor').screenshot({ path: shot('code-block.png') });
    const xml = zipEntry(await saved(page), 'ppt/slides/slide1.xml');
    expect(xml).toContain('typeface="Consolas"');
    // The keyword, the number, the comment and the string each have their colour.
    const colours = new Set([...xml.matchAll(/<a:srgbClr val="([0-9A-F]{6})"/g)].map((m) => m[1]));
    expect(colours.size).toBeGreaterThanOrEqual(4);
    await saveAndReopen(page, 'Code.pptx');
    expect((await state(page)).shapes.some((s) => s.name.startsWith('Code Block'))).toBe(true);
  });

  test('symbols: symbol with its font and character code', async ({ page }) => {
    await newDeck(page);
    await intoTitle(page);
    await page.keyboard.type('Next ');
    await ribbonTab(page, 'Insert');
    await page.getByRole('button', { name: 'Symbol…', exact: true }).click();
    const dialog = page.getByTestId('deck-symbol');
    await dialog.getByRole('combobox', { name: 'Font' }).click();
    await page.getByRole('option', { name: 'Segoe UI Symbol' }).click();
    await dialog.getByRole('combobox', { name: 'Subset' }).click();
    await page.getByRole('option', { name: 'Arrows' }).click();
    await dialog.getByRole('button', { name: 'Insert →' }).click();
    await dialog.getByRole('textbox', { name: 'Character code' }).fill('2605');
    await dialog.getByRole('button', { name: 'Insert', exact: true }).click();
    await dialog.screenshot({ path: shot('symbol-dialog.png') });
    await dialog.getByRole('button', { name: 'Close' }).last().click();
    await expect.poll(async () => (await state(page)).slides[0]?.title).toBe('Next →★');
    const xml = zipEntry(await saved(page), 'ppt/slides/slide1.xml');
    expect(xml).toContain('typeface="Segoe UI Symbol"');
  });

  test('links: link to an existing file', async ({ page }) => {
    await newDeck(page);
    await intoTitle(page);
    await page.keyboard.type('Budget');
    await page.keyboard.press('Control+a');
    await ribbonTab(page, 'Insert');
    await page.getByRole('button', { name: 'Link', exact: true }).click();
    const dialog = page.getByTestId('deck-link');
    await dialog.getByRole('radio', { name: 'Existing file' }).check();
    const [chooser] = await Promise.all([
      page.waitForEvent('filechooser'),
      dialog.getByRole('button', { name: 'Browse…' }).click(),
    ]);
    await chooser.setFiles({
      name: 'Budget 2027.xlsx',
      mimeType: 'application/octet-stream',
      buffer: Buffer.from('x'),
    });
    await expect(dialog.getByRole('textbox', { name: 'File' })).toHaveValue('Budget 2027.xlsx');
    await dialog.screenshot({ path: shot('link-file.png') });
    await dialog.getByRole('button', { name: 'OK' }).click();
    const bytes = await saved(page);
    const rels = zipEntry(bytes, 'ppt/slides/_rels/slide1.xml.rels');
    expect(rels).toMatch(/Target="Budget%202027.xlsx"|Target="Budget 2027.xlsx"/);
    expect(rels).toContain('TargetMode="External"');
    expect(zipEntry(bytes, 'ppt/slides/slide1.xml')).toContain('<a:hlinkClick');
  });

  test('images: pictures cropped with the crop handles', async ({ page }) => {
    await newDeck(page);
    // A picture from bytes, as Insert ▸ Pictures does after the host's file picker.
    const png = await twoTonePng(page);
    await deck(page, {
      InsertPicture: { name: 'Two tone.png', bytes: png, rect: { x: 340, y: 160, width: 600, height: 300 } },
    });
    await expect.poll(async () => (await state(page)).contexts).toContain('picture');
    const box = (await state(page)).selection_box as { x: number; y: number; width: number; height: number };
    // The box is in points; the stage works in slide px (4/3 of a point).
    const [x, y, w, h] = [box.x, box.y, box.width, box.height].map((v) => (v * 4) / 3) as [
      number,
      number,
      number,
      number,
    ];
    await page.getByRole('tab', { name: 'Picture Format' }).click();
    await page.getByRole('button', { name: 'Crop', exact: true }).click();
    await expect.poll(async () => (await state(page)).cropping).toBe(true);
    // The left crop handle a quarter of the way in: the orange half is half gone.
    await dragOnSlide(page, [x, y + h / 2], [x + w / 4, y + h / 2]);
    await page.getByTestId('deck-editor').screenshot({ path: shot('crop-handles.png') });
    await page.keyboard.press('Escape');
    await expect.poll(async () => (await state(page)).cropping).toBe(false);
    const xml = zipEntry(await saved(page), 'ppt/slides/slide1.xml');
    const l = Number(/<a:srcRect l="(\d+)"/.exec(xml)?.[1] ?? 0);
    expect(l).toBeGreaterThan(23_000);
    expect(l).toBeLessThan(27_000);
    await page.getByTestId('deck-editor').screenshot({ path: shot('cropped.png') });
  });
});
