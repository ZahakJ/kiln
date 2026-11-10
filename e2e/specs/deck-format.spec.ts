/**
 * Kiln Deck's Shape Format, Picture Format and Table Layout tabs in the browser: a
 * connector drawn onto two shapes that follows them, Crop to Shape, aspect ratios, Fill and Fit,
 * and table cells selected by dragging, merged, split, given margins and aligned — each kept
 * through a save. Screenshots go to e2e/screenshots/deck-format/.
 */
import { fileURLToPath } from 'node:url';
import { expect, type Page, test } from '@playwright/test';
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

const shot = (name: string) => fileURLToPath(new URL(`../screenshots/deck-format/${name}`, import.meta.url));
const slideXml = async (page: Page) => zipEntry(await saved(page), 'ppt/slides/slide1.xml');

/** The selection's box in slide px (the state has points). */
async function selectionPx(page: Page) {
  const b = (await state(page)).selection_box as { x: number; y: number; width: number; height: number };
  return { x: (b.x * 4) / 3, y: (b.y * 4) / 3, w: (b.width * 4) / 3, h: (b.height * 4) / 3 };
}

test.describe('deck-format', () => {
  test('connectors: a line drawn between two shapes stays attached', async ({ page }) => {
    await newDeck(page);
    await deck(page, { InsertShape: { preset: 'rect', rect: { x: 120, y: 480, width: 160, height: 120 } } });
    await deck(page, {
      InsertShape: { preset: 'ellipse', rect: { x: 800, y: 520, width: 160, height: 120 } },
    });
    await ribbonTab(page, 'Home');
    await deck(page, { ArmInsert: { preset: 'straightConnector1' } });
    // From near the rectangle's right side to near the ellipse's left: both ends snap on.
    await dragOnSlide(page, [284, 538], [796, 582]);
    await expect.poll(async () => (await state(page)).shapes.some((s) => s.kind === 'connector')).toBe(true);
    const xml = await slideXml(page);
    expect(xml).toContain('<a:stCxn');
    expect(xml).toContain('<a:endCxn');
    await page.getByTestId('deck-editor').screenshot({ path: shot('connector.png') });
    // Moving the ellipse up drags the connector's end along.
    await dragOnSlide(page, [880, 580], [880, 380]);
    await page.getByTestId('deck-editor').screenshot({ path: shot('connector-follows.png') });
    // The end is now above the start: the connector flips.
    await expect.poll(slideXml.bind(null, page)).toMatch(/<p:cxnSp>[\s\S]*?flipV="1"/);
    await saveAndReopen(page, 'Connected.pptx');
    expect(zipEntry(await saved(page), 'ppt/slides/slide1.xml')).toContain('<a:endCxn');
  });

  test('picture crop: crop to shape, aspect ratio, fill and fit', async ({ page }) => {
    await newDeck(page);
    const png = await twoTonePng(page);
    await deck(page, {
      InsertPicture: { name: 'Two tone.png', bytes: png, rect: { x: 340, y: 160, width: 600, height: 300 } },
    });
    await page.getByRole('tab', { name: 'Picture Format' }).click();
    const menu = async (...path: string[]) => {
      await page.getByRole('button', { name: 'More options for Crop' }).click();
      for (const [i, p] of path.entries()) {
        await page.getByRole('menuitem', { name: p, exact: true }).click();
        if (i < path.length - 1)
          await expect(page.getByRole('menuitem', { name: path[i + 1], exact: true })).toBeVisible();
      }
    };
    await menu('Aspect Ratio', '1:1');
    await expect.poll(async () => Math.round((await selectionPx(page)).w)).toBe(300);
    expect(Math.round((await selectionPx(page)).h)).toBe(300);
    await page.getByTestId('deck-editor').screenshot({ path: shot('aspect-1-1.png') });
    await menu('Crop to Shape', 'Oval');
    await expect.poll(slideXml.bind(null, page)).toContain('<a:prstGeom prst="ellipse"');
    await page.getByTestId('deck-editor').screenshot({ path: shot('crop-to-shape.png') });
    await menu('Fit');
    await expect.poll(slideXml.bind(null, page)).toMatch(/<a:srcRect t="-50000" b="-50000"\/>/);
    await page.getByTestId('deck-editor').screenshot({ path: shot('fit.png') });
    await menu('Fill');
    await expect.poll(slideXml.bind(null, page)).toMatch(/<a:srcRect l="25000" r="25000"\/>/);
    await saveAndReopen(page, 'Cropped.pptx');
    expect(zipEntry(await saved(page), 'ppt/slides/slide1.xml')).toContain('prst="ellipse"');
  });

  test('table layout: cells merged, split, given margins and aligned', async ({ page }) => {
    await newDeck(page);
    await deck(page, { InsertTable: { rows: 3, cols: 4 } });
    const b = await selectionPx(page);
    const cell = (r: number, c: number): [number, number] => [
      b.x + (b.w * (c + 0.5)) / 4,
      b.y + (b.h * (r + 0.5)) / 3,
    ];
    const first = await onSlide(page, ...cell(0, 0));
    await page.mouse.click(first.x, first.y);
    await page.keyboard.type('Q1');
    // A drag from the first cell to the second row's second selects four cells.
    await dragOnSlide(page, cell(0, 0), cell(1, 1));
    await expect.poll(async () => (await state(page)).cells).toEqual({ row: 0, col: 0, rows: 2, cols: 2 });
    await page.getByTestId('deck-editor').screenshot({ path: shot('cells-selected.png') });
    await page.getByRole('tab', { name: 'Layout', exact: true }).click();
    await page.getByRole('button', { name: 'Cell Margins' }).click();
    await page.getByRole('menuitem', { name: 'Wide' }).click();
    await page.getByRole('button', { name: 'Merge Cells' }).click();
    await expect
      .poll(slideXml.bind(null, page))
      .toMatch(/<a:tc rowSpan="2" gridSpan="2">|<a:tc gridSpan="2" rowSpan="2">/);
    const xml = await slideXml(page);
    expect(xml).toContain('marL="137160"');
    await page.getByRole('button', { name: 'Alignment' }).click();
    await page.getByRole('menuitem', { name: 'Middle Centre' }).click();
    await expect.poll(slideXml.bind(null, page)).toContain('anchor="ctr"');
    await page.getByTestId('deck-editor').screenshot({ path: shot('merged.png') });
    // Split Cells: the merged cell back to four, then a cell into two columns.
    await page.getByRole('button', { name: 'Split Cells' }).click();
    const dialog = page.getByTestId('deck-split-cells');
    await dialog.getByRole('button', { name: 'OK' }).click();
    await expect.poll(slideXml.bind(null, page)).not.toContain('gridSpan="2" rowSpan="2"');
    await page.getByRole('button', { name: 'Split Cells' }).click();
    await dialog.getByRole('button', { name: 'OK' }).click();
    await expect.poll(async () => (await slideXml(page)).match(/<a:gridCol /g)?.length).toBe(5);
    await page.getByTestId('deck-editor').screenshot({ path: shot('split.png') });
    await saveAndReopen(page, 'Table.pptx');
    expect(zipEntry(await saved(page), 'ppt/slides/slide1.xml').match(/<a:gridCol /g)?.length).toBe(5);
  });
});
