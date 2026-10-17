/**
 * Charts in Kiln Grid (WP-73's chart model and chrome, hosted by the Grid surface of WP-71): a
 * chart inserted from a range through the ribbon, its type changed with Change Chart Type, its
 * rows and columns switched, moved by a drag, formatted in the Format pane and deleted; the
 * Chart Design and Format tabs and the pane pass axe; and one reviewed PNG per chart type, light
 * and dark, under `e2e/screenshots/charts/`.
 */
import { mkdirSync } from 'node:fs';
import { fileURLToPath } from 'node:url';
import { expect, type Page, test } from '@playwright/test';
import type { ChartKind } from '../../packages/protocol/src/generated/ChartKind.ts';
import { expectNoSeriousA11yViolations } from '../../packages/testing/src/playwright/a11y.ts';
import { ALL_CHART_KINDS } from '../../packages/ui-kit/src/charts/kinds.ts';

const SHOTS = fileURLToPath(new URL('../screenshots/charts/', import.meta.url));
mkdirSync(SHOTS, { recursive: true });

interface Drawing {
  index: number;
  x: number;
  y: number;
  width: number;
  height: number;
  chart: { kind: ChartKind | null; series: { name: string }[]; categories: string[]; title: string | null };
}
interface GridHandle {
  doc: number;
  client: {
    query(doc: number, q: unknown): Promise<{ Grid: { State: { drawing: Drawing | null; layout: Layout } } }>;
    execute(doc: number, c: unknown): Promise<unknown>;
  };
}
interface Layout {
  header_w: number;
  header_h: number;
}
type W = {
  __kilnGrid: GridHandle;
  __kiln: { registry: { execute(id: string, a?: unknown): Promise<unknown> } };
};

const DATA = [
  ['Item', 'Q1', 'Q2', 'Q3', 'Q4'],
  ['Clay', '420', '380', '455', '510'],
  ['Glaze', '310', '125', '290', '140'],
  ['Firing', '1180', '640', '1240', '1320'],
];

async function openWithData(page: Page): Promise<void> {
  await page.goto('/#/grid?blink=0');
  await page.waitForFunction(
    () =>
      Number(document.querySelector<HTMLElement>('[data-kiln-grid-surface]')?.dataset.kilnFrames ?? 0) > 0,
    null,
    { timeout: 60_000 },
  );
  await page.locator('[data-kiln-grid-proxy]').focus();
  for (const row of DATA) {
    for (const [i, v] of row.entries()) {
      await page.keyboard.type(v);
      await page.keyboard.press(i === row.length - 1 ? 'Enter' : 'Tab');
    }
  }
  await page.keyboard.press('Control+Home');
}

async function state(page: Page): Promise<{ drawing: Drawing | null; layout: Layout }> {
  return page.evaluate(async () => {
    const g = (window as unknown as W).__kilnGrid;
    return (await g.client.query(g.doc, { Grid: 'State' })).Grid.State;
  });
}

async function drawing(page: Page): Promise<Drawing> {
  let d: Drawing | null = null;
  await expect
    .poll(async () => {
      d = (await state(page)).drawing;
      return d !== null;
    })
    .toBe(true);
  return d as unknown as Drawing;
}

/** The chart's box on the page (its rectangle in sheet pixels, through the surface's origin). */
async function chartBox(page: Page): Promise<{ x: number; y: number; width: number; height: number }> {
  const s = await state(page);
  const d = await drawing(page);
  const surface = await page.locator('[data-kiln-grid-surface]').boundingBox();
  if (!surface) throw new Error('no surface');
  return {
    x: surface.x + s.layout.header_w + d.x,
    y: surface.y + s.layout.header_h + d.y,
    width: d.width,
    height: d.height,
  };
}

/** Waits until the surface has painted what follows a change (its frame count settles). */
async function painted(page: Page): Promise<void> {
  const frames = () =>
    page.evaluate(() =>
      Number(document.querySelector<HTMLElement>('[data-kiln-grid-surface]')?.dataset.kilnFrames ?? 0),
    );
  let last = await frames();
  for (let i = 0; i < 20; i++) {
    await page.waitForTimeout(250);
    const now = await frames();
    if (now === last && i > 0) break;
    last = now;
  }
}

test.describe('Kiln Grid charts', () => {
  test('charts: insert from a range, change the type, switch row/column, move, format and delete', async ({
    page,
  }) => {
    test.setTimeout(180_000);
    await openWithData(page);
    // Insert ▸ Chart from the ribbon: the recommended chart of the selection's region.
    await page.getByRole('tab', { name: 'Insert' }).click();
    await page.getByRole('button', { name: 'Chart', exact: true }).click();
    let d = await drawing(page);
    expect(d.chart.kind).toBe('ColumnClustered');
    expect(d.chart.series.map((s) => s.name)).toEqual(['Clay', 'Glaze', 'Firing']);
    expect(d.chart.categories).toEqual(['Q1', 'Q2', 'Q3', 'Q4']);
    await painted(page);
    await page.screenshot({ path: `${SHOTS}inserted-light.png` });

    // Chart Design ▸ Change Chart Type ▸ Line.
    await page.getByRole('tab', { name: 'Chart Design' }).click();
    await expectNoSeriousA11yViolations(page);
    await page.getByRole('button', { name: 'Change Chart Type' }).click();
    const dialog = page.getByRole('dialog', { name: 'Change Chart Type' });
    await dialog.getByRole('tab', { name: 'Line' }).click();
    await dialog.getByRole('option', { name: 'Line with Markers' }).click();
    await dialog.getByRole('button', { name: 'OK' }).click();
    await expect.poll(async () => (await drawing(page)).chart.kind).toBe('LineMarkers');

    // Switch Row/Column: the quarters become the series.
    await page.getByRole('button', { name: 'Switch Row/Column' }).click();
    await expect.poll(async () => (await drawing(page)).chart.series.length).toBe(4);
    d = await drawing(page);
    expect(d.chart.categories).toEqual(['Clay', 'Glaze', 'Firing']);

    // Undo restores the rows as series.
    await page.locator('[data-kiln-grid-proxy]').focus();
    await page.keyboard.press('Control+z');
    await expect.poll(async () => (await drawing(page)).chart.series.length).toBe(3);

    // A drag moves it.
    const box = await chartBox(page);
    await page.mouse.move(box.x + box.width / 2, box.y + box.height / 2);
    await page.mouse.down();
    await page.mouse.move(box.x + box.width / 2 + 120, box.y + box.height / 2 + 60, { steps: 6 });
    await page.mouse.up();
    await expect.poll(async () => Math.round((await drawing(page)).x - d.x)).toBe(120);

    // Format ▸ Format Selection opens the pane; a title typed there names the chart.
    await page.getByRole('tab', { name: 'Format', exact: true }).click();
    await page.getByRole('button', { name: 'Format Selection' }).click();
    const pane = page.getByTestId('grid-chart-pane');
    await expect(pane).toBeVisible();
    const title = pane.getByRole('textbox', { name: 'Chart title' });
    await title.fill('Studio costs');
    await title.press('Enter');
    await expect.poll(async () => (await drawing(page)).chart.title).toBe('Studio costs');
    await painted(page);
    await expectNoSeriousA11yViolations(page);
    await page.screenshot({ path: `${SHOTS}format-pane-light.png` });
    await page.emulateMedia({ colorScheme: 'dark' });
    await painted(page);
    await expectNoSeriousA11yViolations(page);
    await page.screenshot({ path: `${SHOTS}format-pane-dark.png` });

    // Delete removes the selected chart (and Chart Tools go away).
    await page.locator('[data-kiln-grid-proxy]').focus();
    await page.keyboard.press('Delete');
    await expect.poll(async () => (await state(page)).drawing).toBeNull();
    await expect(page.getByRole('tab', { name: 'Chart Design' })).toHaveCount(0);
  });

  for (const scheme of ['light', 'dark'] as const) {
    test(`charts: every chart type renders (${scheme})`, async ({ page }) => {
      test.setTimeout(240_000);
      await page.emulateMedia({ colorScheme: scheme });
      await openWithData(page);
      for (const kind of ALL_CHART_KINDS) {
        // Each type is inserted from the data, as Insert ▸ Charts does (a pie keeps one series,
        // so changing types in a row would chart less than the data).
        await page.evaluate(async (k) => {
          const g = (window as unknown as W).__kilnGrid;
          await g.client.execute(g.doc, { Grid: 'DeleteDrawing' });
          await g.client.execute(g.doc, {
            Grid: { SelectCell: { row: 0, col: 0, extend: false, add: false } },
          });
          await g.client.execute(g.doc, { Suite: { ChartInsert: { kind: k } } });
        }, kind);
        await expect.poll(async () => (await drawing(page)).chart.kind).toBe(kind);
        await painted(page);
        const box = await chartBox(page);
        await page.screenshot({
          path: `${SHOTS}${kind}-${scheme}.png`,
          clip: { x: box.x - 6, y: box.y - 6, width: box.width + 12, height: box.height + 12 },
        });
      }
    });
  }
});
