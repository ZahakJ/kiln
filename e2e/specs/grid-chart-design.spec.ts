/**
 * Kiln Grid ▸ Chart Design and Format: every control of the contextual tabs on a chart
 * of cells, checked against the core's chart state, with the keys back on the sheet after each
 * menu and dialog. Literal titles: the parity rows cite these tests by name.
 */
import { expect, type Page, test } from '@playwright/test';
import { decodePng } from '../../packages/testing/src/png.ts';
import {
  expectSheetFocused,
  fill,
  focusSheet,
  gridState,
  openGrid,
  run,
  select,
  shot,
  tab,
} from './grid-helpers.ts';

interface Chart {
  kind: string | null;
  style: number | null;
  color_set: number;
  title: string | null;
  legend: string | null;
  data_labels: string | null;
  elements: string[];
  data_range: string;
  series: { idx: number; name: string; fill: string | null }[];
  categories: string[];
  blanks: string;
  show_hidden: boolean;
  chart_area: { fill: string | null; shadow: boolean; rounded: boolean; no_line: boolean };
  plot_area: { fill: string | null; line: string | null; line_width: number; shadow: boolean };
  axes: {
    target: string;
    min: number | null;
    max: number | null;
    min_auto: boolean;
    number_format: string | null;
  }[];
}

const DATA = [
  ['Item', 'Q1', 'Q2', 'Q3', 'Q4'],
  ['Clay', '420', '380', '455', '510'],
  ['Glaze', '310', '125', '290', '140'],
  ['Firing', '1180', '640', '1240', '1320'],
  ['Events', '0', '120', '', '380'],
];

async function chart(page: Page): Promise<Chart> {
  const s = (await gridState(page)) as unknown as { drawing: { chart: Chart | null } | null };
  const c = s.drawing?.chart;
  if (!c) throw new Error('no chart selected');
  return c;
}

async function until(page: Page, check: (c: Chart) => boolean, what: string): Promise<Chart> {
  let last: Chart | null = null;
  const end = Date.now() + 10_000;
  while (Date.now() < end) {
    last = await chart(page).catch(() => null);
    if (last !== null && check(last)) return last;
    await page.waitForTimeout(100);
  }
  throw new Error(`${what}: the chart is ${JSON.stringify(last)}`);
}

/** A clustered column chart of A1:E4 (three series), selected, on the Chart Design tab. */
async function withChart(page: Page, rows = DATA.slice(0, 4)): Promise<void> {
  await openGrid(page);
  await fill(page, rows);
  await select(page, 0, 0, rows.length - 1, 4);
  await focusSheet(page);
  await run(page, 'grid.insert.chart_column');
  await until(page, (c) => c.series.length > 0, 'the chart is inserted');
  // A phone has the ribbon sheet instead of the tab row; the chart's tab is there.
  if ((page.viewportSize()?.width ?? 1440) >= 640) await tab(page, 'Chart Design');
}

/** Opens a ribbon menu button and picks a path of items. */
async function pick(page: Page, button: string, path: readonly string[]): Promise<void> {
  await page.getByRole('button', { name: button }).first().click();
  for (const [i, item] of path.entries()) {
    const last = i === path.length - 1;
    const m = page
      .getByRole('menuitem', { name: item, exact: true })
      .or(page.getByRole('menuitemcheckbox', { name: item, exact: true }))
      .or(page.getByRole('menuitemradio', { name: item, exact: true }))
      .first();
    await m.hover();
    if (last) await m.click();
    else await m.press('ArrowRight');
  }
}

test.describe('Kiln Grid chart design', () => {
  test.describe.configure({ timeout: 120_000 });

  test('add chart element: titles, gridlines, legend, data labels, trendline and error bars', async ({
    page,
  }) => {
    await withChart(page);
    await pick(page, 'Add Chart Element', ['Axis Titles', 'Primary Vertical']);
    await until(page, (c) => c.elements.includes('PrimaryVerticalAxisTitle'), 'vertical axis title');
    await expectSheetFocused(page);
    await pick(page, 'Add Chart Element', ['Gridlines', 'Primary Minor Horizontal']);
    await until(page, (c) => c.elements.includes('MinorGridlines'), 'minor gridlines');
    await pick(page, 'Add Chart Element', ['Legend', 'Top']);
    await until(page, (c) => c.legend === 'Top', 'legend at the top');
    await pick(page, 'Add Chart Element', ['Data Labels', 'Inside End']);
    await until(page, (c) => c.data_labels === 'InsideEnd', 'data labels inside the ends');
    await pick(page, 'Add Chart Element', ['Trendline', 'Linear']);
    await until(page, (c) => c.elements.includes('LinearTrendline'), 'a linear trendline');
    await pick(page, 'Add Chart Element', ['Error Bars', 'Standard Error']);
    await until(page, (c) => c.elements.includes('StandardErrorBars'), 'standard error bars');
    await pick(page, 'Add Chart Element', ['Error Bars', 'Percentage']);
    const c = await until(
      page,
      (c) => c.elements.includes('PercentageErrorBars') && !c.elements.includes('StandardErrorBars'),
      'percentage error bars replace the standard error ones',
    );
    expect(c.elements).toContain('ChartTitle');
    await pick(page, 'Add Chart Element', ['Chart Title']);
    await until(page, (c) => c.title === null, 'no title');
    await expectSheetFocused(page);
    // Undo takes the last element back.
    await page.keyboard.press('Control+z');
    await until(page, (c) => c.title !== null, 'the title is back');
  });

  test('quick layout: layout 5 adds axis titles and removes the legend', async ({ page }) => {
    await withChart(page);
    await page.getByRole('button', { name: 'Quick Layout' }).click();
    await page.getByRole('menuitem', { name: /^Layout 5/ }).click();
    const c = await until(
      page,
      (c) => c.legend === null && c.elements.includes('PrimaryVerticalAxisTitle'),
      'layout 5',
    );
    expect(c.elements).toContain('PrimaryHorizontalAxisTitle');
    await expectSheetFocused(page);
    await page.getByRole('button', { name: 'Quick Layout' }).click();
    await page.getByRole('menuitem', { name: /^Layout 11/ }).click();
    await until(page, (c) => c.title === null && c.legend !== null, 'layout 11: legend only');
  });

  test('colours and styles: Change Colors and Chart Styles set the palette and the style', async ({
    page,
  }) => {
    await withChart(page);
    await page.getByRole('button', { name: 'Change Colors' }).click();
    await page.getByRole('menuitemradio', { name: 'Grayscale' }).click();
    await until(page, (c) => c.color_set === 7, 'greys');
    await expectSheetFocused(page);
    await page.getByRole('button', { name: 'Chart Styles' }).click();
    await page.getByRole('menuitemradio', { name: 'Style 4' }).click();
    const c = await until(page, (c) => c.style === 4, 'style 4');
    expect(c.color_set).toBe(2);
    // Dark mode draws the same chart with the dark palette.
    await page.emulateMedia({ colorScheme: 'dark' });
    expect((await chart(page)).style).toBe(4);
  });

  test('switch row/column and select data: series added, edited, reordered and removed, hidden and empty cells', async ({
    page,
  }) => {
    await withChart(page, DATA);
    // Square data: the quarters are the series, as Excel charts it.
    await until(page, (c) => c.series.map((s) => s.name).join() === 'Q1,Q2,Q3,Q4', 'series in columns');
    await page.getByRole('button', { name: 'Switch Row/Column' }).click();
    await until(
      page,
      (c) =>
        c.series.map((s) => s.name).join() === 'Clay,Glaze,Firing,Events' &&
        c.categories.join() === 'Q1,Q2,Q3,Q4',
      'switched',
    );
    await expectSheetFocused(page);

    await page.getByRole('button', { name: 'Select Data' }).click();
    const dialog = page.getByRole('dialog', { name: 'Select Data Source' });
    await expect(dialog).toBeVisible();
    // Remove Events, then add it back from its row by reference.
    await dialog.getByRole('option', { name: /Events/ }).click();
    await dialog.getByRole('button', { name: 'Remove' }).click();
    await until(page, (c) => c.series.map((s) => s.name).join() === 'Clay,Glaze,Firing', 'Events removed');
    await dialog.getByRole('button', { name: 'Add' }).first().click();
    await dialog.getByLabel('Series name').fill('=Sheet1!$A$5');
    await dialog.getByLabel('Series values').fill('=Sheet1!$B$5:$E$5');
    await dialog.getByRole('button', { name: 'Apply' }).click();
    await until(
      page,
      (c) => c.series.map((s) => s.name).join() === 'Clay,Glaze,Firing,Events',
      'Events added',
    );
    // Move Firing up.
    await dialog.getByRole('option', { name: /Firing/ }).click();
    await dialog.getByRole('button', { name: 'Move Up' }).click();
    await until(
      page,
      (c) => c.series.map((s) => s.name).join() === 'Clay,Firing,Glaze,Events',
      'Firing moved up',
    );
    // Edit Glaze's name to a literal.
    await dialog.getByRole('option', { name: /Glaze/ }).click();
    await dialog.getByRole('button', { name: 'Edit' }).first().click();
    await dialog.getByLabel('Series name').fill('Glazes');
    await dialog.getByRole('button', { name: 'Apply' }).click();
    await until(page, (c) => c.series.some((s) => s.name === 'Glazes'), 'Glaze renamed');
    // Hidden and Empty Cells: Events' empty Q3 is connected.
    await dialog.getByRole('radio', { name: 'Connect data points with line' }).click();
    await until(page, (c) => c.blanks === 'Span', 'blanks connected');
    await dialog.getByRole('checkbox', { name: 'Show data in hidden rows and columns' }).click();
    await until(page, (c) => c.show_hidden, 'hidden data shown');
    await dialog.getByRole('button', { name: 'OK' }).click();
    await expect(dialog).toHaveCount(0);
    await expectSheetFocused(page);
  });

  test('change chart type: column to line with markers and to a pie', async ({ page }) => {
    await withChart(page);
    await page.getByRole('button', { name: 'Change Chart Type' }).click();
    const dialog = page.getByRole('dialog', { name: 'Change Chart Type' });
    await dialog.getByRole('tab', { name: 'Line' }).click();
    await dialog.getByRole('option', { name: 'Line with Markers' }).click();
    await dialog.getByRole('button', { name: 'OK' }).click();
    await until(page, (c) => c.kind === 'LineMarkers', 'line with markers');
    await expectSheetFocused(page);
    await page.getByRole('button', { name: 'Change Chart Type' }).click();
    await dialog.getByRole('tab', { name: 'Pie' }).click();
    await dialog.getByRole('option', { name: 'Doughnut' }).click();
    await dialog.getByRole('button', { name: 'OK' }).click();
    await until(page, (c) => c.kind === 'Doughnut', 'doughnut');
    await page.keyboard.press('Control+z');
    await until(page, (c) => c.kind === 'LineMarkers', 'undo');
  });

  test('insert chart: the new chart is selected and the Chart Design tab shows, as in Excel', async ({
    page,
  }) => {
    await openGrid(page);
    await fill(page, DATA.slice(0, 4));
    await select(page, 0, 0, 3, 4);
    await tab(page, 'Insert');
    await expect(page.getByRole('tab', { name: 'Insert', exact: true })).toHaveAttribute(
      'aria-selected',
      'true',
    );
    await run(page, 'grid.insert.chart_column');
    await until(page, (c) => c.series.length > 0, 'the chart is inserted and selected');
    await expect(page.getByRole('tab', { name: 'Chart Design' })).toHaveAttribute('aria-selected', 'true');
    await expect(page.getByRole('button', { name: 'Move Chart' })).toBeVisible();
  });

  test('move chart: to a new chart sheet and back as an object', async ({ page }) => {
    await withChart(page);
    await page.getByRole('button', { name: 'Move Chart' }).click();
    const dialog = page.getByRole('dialog', { name: 'Move Chart' });
    await expect(dialog.getByLabel('New sheet name')).toHaveValue('Chart1');
    await dialog.getByRole('button', { name: 'OK' }).click();
    let s = (await gridState(page)) as unknown as {
      active_sheet: number;
      sheets: { name: string; chart: boolean }[];
      drawing: unknown;
    };
    await expect
      .poll(async () => {
        s = (await gridState(page)) as unknown as typeof s;
        return s.sheets.map((t) => `${t.name}${t.chart ? '*' : ''}`).join();
      })
      .toBe('Chart1*,Sheet1');
    expect(s.active_sheet).toBe(0);
    expect(s.drawing).not.toBeNull();
    await expect(page.locator('[data-sheet="Chart1"] .kiln-sheettab__chart')).toHaveCount(1);
    await expectSheetFocused(page);
    // Typing on a chart sheet is refused: it has no cells.
    await page.keyboard.press('Escape');
    await page.keyboard.type('5');
    await expect(page.getByText(/This is a chart sheet/)).toBeVisible();
    await page.keyboard.press('Escape');
    // Back to Sheet1 as an object: the chart sheet goes.
    await page.evaluate(async () => {
      // biome-ignore lint/suspicious/noExplicitAny: the test handle
      const g = (window as any).__kilnGrid;
      await g.client.execute(g.doc, { Grid: { SelectDrawing: { index: 0 } } });
    });
    await tab(page, 'Chart Design');
    await page.getByRole('button', { name: 'Move Chart' }).click();
    await dialog.getByRole('radio', { name: /Object in/ }).check();
    await dialog.getByRole('button', { name: 'OK' }).click();
    await expect
      .poll(async () => ((await gridState(page)) as unknown as typeof s).sheets.map((t) => t.name).join())
      .toBe('Sheet1');
  });
});

/** The zoom the status bar shows, as a fraction. */
async function zoomOf(page: Page): Promise<number> {
  const t = await page.locator('.kiln-statusbar').innerText();
  return Number(/(\d+)%\s*$/.exec(t)?.[1] ?? 'NaN') / 100;
}

/** Moves the chart of `withChart` to a new chart sheet, which becomes the active sheet. */
async function toChartSheet(page: Page, phone = false): Promise<void> {
  if (phone) {
    // The phone's ribbon keeps Move Chart in its overflow menu: the command itself.
    await run(page, 'suite.chart.move', { to: { NewSheet: { name: 'Chart1' } } });
  } else {
    await page.getByRole('button', { name: 'Move Chart' }).click();
    await page.getByRole('dialog', { name: 'Move Chart' }).getByRole('button', { name: 'OK' }).click();
  }
  await expect
    .poll(
      async () => ((await gridState(page)) as unknown as { sheets: { chart: boolean }[] }).sheets[0]?.chart,
    )
    .toBe(true);
}

/**
 * The chart sheet's chart is drawn whole in the window: its rectangle at the zoom is inside the
 * sheet's area, and the selection's outline is drawn along its bottom edge (which a chart cut
 * off by the window does not show).
 */
async function expectChartFits(page: Page): Promise<void> {
  const box = await page.locator('[data-kiln-grid-surface]').boundingBox();
  if (!box) throw new Error('no grid surface');
  await expect.poll(() => zoomOf(page), { message: 'the chart sheet is zoomed to fit' }).toBeLessThan(1);
  const z = await zoomOf(page);
  const d = (await gridState(page)).drawing;
  if (!d) throw new Error('the chart is not selected');
  const right = (d.x + d.width) * z;
  const bottom = (d.y + d.height) * z;
  expect(right, 'the chart fits the width').toBeLessThanOrEqual(box.width);
  expect(bottom, 'the chart fits the height').toBeLessThanOrEqual(box.height);
  // The window does not leave much more room than the chart takes (it is fitted, not shrunk).
  expect(Math.max(right / box.width, bottom / box.height)).toBeGreaterThan(0.9);
  await page.waitForTimeout(300);
  // The sheet's own colour, just below the chart.
  const below = decodePng(
    await page.screenshot({
      clip: { x: box.x + right / 2, y: box.y + Math.min(box.height - 3, bottom + 8), width: 1, height: 1 },
    }),
  ).data;
  // Along the bottom edge: one row of pixels at least is the outline, across the chart.
  const band = decodePng(
    await page.screenshot({
      clip: {
        x: box.x + (d.x + d.width * 0.2) * z,
        y: box.y + bottom - 4,
        width: d.width * 0.6 * z,
        height: 8,
      },
    }),
  );
  let line = 0;
  for (let y = 0; y < band.height; y++) {
    let n = 0;
    for (let x = 0; x < band.width; x++) {
      const i = (y * band.width + x) * 4;
      const diff =
        Math.abs((band.data[i] ?? 0) - (below[0] ?? 0)) +
        Math.abs((band.data[i + 1] ?? 0) - (below[1] ?? 0)) +
        Math.abs((band.data[i + 2] ?? 0) - (below[2] ?? 0));
      if (diff > 30) n += 1;
    }
    line = Math.max(line, n / band.width);
  }
  expect(line, "the outline along the chart's bottom edge is drawn in the window").toBeGreaterThan(0.8);
}

test.describe('Kiln Grid chart sheet', () => {
  test.describe.configure({ timeout: 120_000 });

  test('chart sheet: fitted to the window in light and dark, desktop and phone; zooming still works', async ({
    page,
    browser,
    baseURL,
  }) => {
    await withChart(page);
    await toChartSheet(page);
    await expectChartFits(page);
    await page.screenshot({ path: shot('chart-sheet-fit-light-desktop.png') });
    await page.emulateMedia({ colorScheme: 'dark' });
    await page.waitForTimeout(400);
    await expectChartFits(page);
    await page.screenshot({ path: shot('chart-sheet-fit-dark-desktop.png') });
    await page.emulateMedia({ colorScheme: 'light' });
    // A zoom the user picks stays (the fitting stops), and the sheet before gets its own back.
    const fitted = await zoomOf(page);
    await run(page, 'suite.view.zoom', { zoom: 1.5 });
    await expect.poll(() => zoomOf(page)).toBe(1.5);
    await page.setViewportSize({ width: 1300, height: 860 });
    await page.waitForTimeout(600);
    expect(await zoomOf(page)).toBe(1.5);
    await page.locator('[data-sheet="Sheet1"]').click();
    await expect.poll(() => zoomOf(page)).toBe(1);
    // Back on the chart sheet: fitted again to the window as it is now.
    await page.locator('[data-sheet="Chart1"]').click();
    await expect.poll(() => zoomOf(page)).not.toBe(1);
    expect(await zoomOf(page)).not.toBe(fitted);
    for (const scheme of ['light', 'dark'] as const) {
      const ctx = await browser.newContext({
        baseURL,
        viewport: { width: 412, height: 915 },
        colorScheme: scheme,
      });
      const phone = await ctx.newPage();
      try {
        await withChart(phone);
        await toChartSheet(phone, true);
        await expectChartFits(phone);
        await phone.screenshot({ path: shot(`chart-sheet-fit-${scheme}-phone.png`) });
      } finally {
        await ctx.close();
      }
    }
  });
});

test.describe('Kiln Grid chart format', () => {
  test.describe.configure({ timeout: 120_000 });

  test('format pane: series fill, chart area shadow, plot area border, axis bounds and number format', async ({
    page,
  }) => {
    await withChart(page);
    await tab(page, 'Format');
    await page.getByRole('button', { name: 'Format Selection' }).click();
    const pane = page.getByTestId('grid-chart-pane');
    await expect(pane).toBeVisible();
    await pane.getByRole('button', { name: 'Fill: Ember', exact: true }).click();
    await until(page, (c) => c.series[0]?.fill === 'E05A27', 'series fill');
    await pane.getByRole('button', { name: 'Chart Area fill: Ash', exact: true }).click();
    await until(page, (c) => c.chart_area.fill === '8F7A6A', 'chart area fill');
    await pane.getByRole('checkbox', { name: 'Shadow' }).first().click();
    await until(page, (c) => c.chart_area.shadow, 'shadow');
    await pane.getByRole('checkbox', { name: 'Rounded corners' }).click();
    await until(page, (c) => c.chart_area.shadow && c.chart_area.rounded, 'shadow and rounded corners');
    await pane.getByRole('button', { name: 'Plot Area border: Soot', exact: true }).click();
    await pane.getByLabel('Plot Area border width').fill('1.5');
    await pane.getByLabel('Plot Area border width').press('Enter');
    await until(
      page,
      (c) => c.plot_area.line === '2B1C14' && c.plot_area.line_width === 1.5,
      'plot area border',
    );
    // Axis options: a fixed minimum, then a number format.
    await pane.getByLabel('Minimum').fill('100');
    await pane.getByLabel('Minimum').press('Enter');
    await until(
      page,
      (c) => c.axes.some((a) => a.target === 'PrimaryValue' && a.min === 100 && !a.min_auto),
      'fixed minimum',
    );
    await pane.getByRole('combobox', { name: 'Number' }).click();
    await page.getByRole('option', { name: 'Thousands (#,##0)' }).click();
    await until(
      page,
      (c) => c.axes.some((a) => a.target === 'PrimaryValue' && a.number_format === '#,##0'),
      'axis number format',
    );
    // Shape Fill from the ribbon (Format ▸ Shape Styles) recolours a series too.
    await page.getByRole('button', { name: /Shape Fill/ }).click();
    await page.getByRole('menuitem', { name: /Glaze/ }).hover();
    await page.getByRole('menuitem', { name: /Glaze/ }).press('ArrowRight');
    await page.getByRole('menuitemradio', { name: 'Cobalt' }).click();
    await until(page, (c) => c.series[1]?.fill === '3159C6', 'Glaze in cobalt');
    await expectSheetFocused(page);
  });
});
