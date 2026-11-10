/**
 * Charts on slides in the browser: every chart type inserted from Insert ▸ Chart with its
 * data sheet, edited from Chart Design and the Format Chart pane, saved and reopened; Edit Data
 * changing a value and adding a series, kept through a save. Screenshots go to
 * e2e/screenshots/deck-charts/.
 */
import { fileURLToPath } from 'node:url';
import { expect, type Page, test } from '@playwright/test';
import { deck, newDeck, ribbonTab, saveAndReopen, selectKind, state } from './deck-helpers.ts';

const shot = (name: string) => fileURLToPath(new URL(`../screenshots/deck-charts/${name}`, import.meta.url));

/** A Title and Content slide, then Insert ▸ Chart with the type `label` of `family`. */
async function insertChart(page: Page, family: string, label: string) {
  await newDeck(page);
  await deck(page, { NewSlide: { layout: null } });
  await ribbonTab(page, 'Insert');
  await page.getByRole('button', { name: 'Chart', exact: true }).click();
  const dialog = page.getByRole('dialog', { name: 'Insert Chart' });
  await expect(dialog).toBeVisible();
  await dialog.getByRole('tab', { name: family, exact: true }).click();
  await dialog.getByRole('option', { name: label, exact: true }).click();
  await dialog.getByRole('button', { name: 'OK' }).click();
  await expect(dialog).toBeHidden();
}

/** The chart is on the slide with Chart Design showing; after a save and reopen it still is. */
async function checkKind(page: Page, kind: string, name: string) {
  await expect.poll(async () => (await state(page)).chart?.kind).toBe(kind);
  const st = await state(page);
  expect(st.contexts).toEqual(['chart']);
  expect(st.chart?.has_cells).toBe(true);
  await expect(page.getByRole('tab', { name: 'Chart Design' })).toBeVisible();
  await page.getByTestId('deck-editor').screenshot({ path: shot(`${name}.png`) });
  await saveAndReopen(page, `${name}.pptx`);
  await deck(page, { SelectSlide: { index: 1, extend: false, toggle: false } });
  await selectKind(page, 'chart');
  await expect.poll(async () => (await state(page)).chart?.kind).toBe(kind);
}

test.describe('deck-charts', () => {
  test('column charts insert with a data sheet, take Chart Design and Format, and reopen', async ({
    page,
  }) => {
    await insertChart(page, 'Column', 'Clustered Column');
    await expect.poll(async () => (await state(page)).chart?.kind).toBe('ColumnClustered');
    // Chart Design: a style, then the Format Chart pane from Format ▸ Format Selection.
    await page.getByRole('tab', { name: 'Chart Design' }).click();
    await page.getByRole('button', { name: 'Chart Styles' }).click();
    await page.getByRole('menuitemradio', { name: /Style 6/ }).click();
    await expect.poll(async () => (await state(page)).chart?.style).toBe(6);
    await page.getByRole('tab', { name: 'Format', exact: true }).click();
    await page.getByRole('button', { name: 'Format Selection' }).click();
    await expect(page.getByTestId('deck-chart-pane')).toBeVisible();
    await page.getByTestId('deck-editor').screenshot({ path: shot('column-format-pane.png') });
    await page.getByRole('button', { name: 'Close Format Chart Area' }).click();
    await checkKind(page, 'ColumnClustered', 'column');
    expect((await state(page)).chart?.style).toBe(6);
  });

  test('bar charts insert, save and reopen', async ({ page }) => {
    await insertChart(page, 'Bar', 'Clustered Bar');
    await checkKind(page, 'BarClustered', 'bar');
  });

  test('line charts insert, save and reopen', async ({ page }) => {
    await insertChart(page, 'Line', 'Line');
    await checkKind(page, 'Line', 'line');
  });

  test('area charts insert, save and reopen', async ({ page }) => {
    await insertChart(page, 'Area', 'Area');
    await checkKind(page, 'Area', 'area');
  });

  test('pie charts insert, save and reopen', async ({ page }) => {
    await insertChart(page, 'Pie', 'Pie');
    await checkKind(page, 'Pie', 'pie');
  });

  test('doughnut charts insert, save and reopen', async ({ page }) => {
    await insertChart(page, 'Pie', 'Doughnut');
    await checkKind(page, 'Doughnut', 'doughnut');
  });

  test('scatter charts insert, save and reopen', async ({ page }) => {
    await insertChart(page, 'X Y (Scatter)', 'Scatter');
    await checkKind(page, 'ScatterMarkers', 'scatter');
  });

  test('bubble charts insert, save and reopen', async ({ page }) => {
    await insertChart(page, 'X Y (Scatter)', 'Bubble');
    await checkKind(page, 'Bubble', 'bubble');
  });

  test('combo charts insert, save and reopen', async ({ page }) => {
    await insertChart(page, 'Combo', 'Clustered Column – Line');
    await checkKind(page, 'ComboColumnLine', 'combo');
  });

  test('treemap charts insert, save and reopen', async ({ page }) => {
    await insertChart(page, 'Hierarchy', 'Treemap');
    await checkKind(page, 'Treemap', 'treemap');
  });

  test('sunburst charts insert, save and reopen', async ({ page }) => {
    await insertChart(page, 'Hierarchy', 'Sunburst');
    await checkKind(page, 'Sunburst', 'sunburst');
  });

  test('radar charts insert, save and reopen', async ({ page }) => {
    await insertChart(page, 'Radar', 'Radar');
    await checkKind(page, 'Radar', 'radar');
  });

  test('Edit Data changes a value and adds a series, kept through a save', async ({ page }) => {
    await insertChart(page, 'Column', 'Clustered Column');
    await page.getByRole('tab', { name: 'Chart Design' }).click();
    await page.getByTestId('chart-edit-data').click();
    const sheet = page.getByTestId('deck-chart-data');
    await expect(sheet).toBeVisible();
    const b2 = sheet.getByRole('textbox', { name: 'B2', exact: true });
    await expect(b2).toHaveValue('4.3');
    await b2.fill('9');
    await b2.press('Enter');
    await expect.poll(async () => (await state(page)).chart?.series.length).toBe(3);
    await sheet.getByTestId('chart-data-add-col').click();
    const e1 = sheet.getByRole('textbox', { name: 'E1', exact: true });
    await e1.fill('Kilns');
    await e1.press('Tab');
    for (const [cell, v] of [
      ['E2', '1'],
      ['E3', '2'],
      ['E4', '3'],
      ['E5', '4'],
    ] as const) {
      const input = sheet.getByRole('textbox', { name: cell, exact: true });
      await input.fill(v);
      await input.press('Enter');
    }
    await expect
      .poll(async () => (await state(page)).chart?.series.map((s) => s.name))
      .toEqual(['Series 1', 'Series 2', 'Series 3', 'Kilns']);
    await sheet.screenshot({ path: shot('edit-data.png') });
    await sheet.getByRole('button', { name: 'Close', exact: true }).last().click();
    await page.getByTestId('deck-editor').screenshot({ path: shot('edit-data-chart.png') });
    await saveAndReopen(page, 'Edited chart.pptx');
    await deck(page, { SelectSlide: { index: 1, extend: false, toggle: false } });
    await selectKind(page, 'chart');
    await expect.poll(async () => (await state(page)).chart?.series.length).toBe(4);
    await page.getByRole('tab', { name: 'Chart Design' }).click();
    await page.getByTestId('chart-edit-data').click();
    await expect(
      page.getByTestId('deck-chart-data').getByRole('textbox', { name: 'B2', exact: true }),
    ).toHaveValue('9');
  });
});
