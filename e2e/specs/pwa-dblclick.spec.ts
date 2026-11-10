/**
 * Keys typed right after a double-click on a cell, on the installable web app as it ships (the
 * release build, which has none of the dev server's test handles): a cell holds "7", a person's
 * double-click (two presses 90 ms apart), a digit at once and Enter must give "7<digit>", idle and
 * right after an entry elsewhere. The cells are read from the sheet's accessible grid. The full
 * set of cases (delays, a slow core, input methods, Ctrl+Z, a core that never answers) is
 * `grid-dblclick-keys.spec.ts`, which needs the dev build's hooks into the core's client.
 */
import { expect, type Page, test } from '@playwright/test';
import { CORE_START, openIsolated } from './pwa-helpers.ts';

test.describe.configure({ timeout: 180_000 });

const ROWS = 20;
const at = (col: number, row: number) => ({ x: 97 + 64 * col, y: 289 + 20 * (row - 1) });
const digitOf = (r: number) => '12345689'[r % 8] as string;

/** A point of the sheet in page coordinates (the layout of the 1280 × 800 window). */
async function onGrid(page: Page, p: { x: number; y: number }) {
  const box = await page.getByTestId('grid-editor').boundingBox();
  const [dx, dy] = box ? [box.x - 10, box.y - 206] : [0, 0];
  return { x: p.x + dx, y: p.y + dy };
}

async function cellsNow(page: Page): Promise<Record<string, string>> {
  return page.evaluate(() => {
    const out: Record<string, string> = {};
    for (const cell of document.querySelectorAll('[role="grid"] [role="gridcell"]')) {
      const name = (cell.getAttribute('aria-label') ?? '').split(' ')[0] ?? '';
      if (name) out[name] = cell.textContent ?? '';
    }
    return out;
  });
}

/** The cells once they have stopped changing. */
async function settle(page: Page): Promise<Record<string, string>> {
  let last = '';
  for (let i = 0, same = 0; i < 80 && same < 4; i += 1) {
    const now = JSON.stringify(await cellsNow(page));
    same = now === last ? same + 1 : 0;
    last = now;
    await page.waitForTimeout(150);
  }
  return JSON.parse(last) as Record<string, string>;
}

for (const mode of ['idle', 'busy'] as const)
  test(`a key at once after a double-click lands in the cell, release build (${mode})`, async ({ page }) => {
    await openIsolated(page);
    await page.goto('./#/grid');
    await expect(page.getByRole('textbox', { name: 'Formula Bar' })).toBeVisible({ timeout: CORE_START });
    await page.waitForTimeout(1500);
    const c1 = await onGrid(page, at(2, 1));
    await page.mouse.click(c1.x, c1.y);
    for (let r = 1; r <= ROWS; r += 1) await page.keyboard.type('7\n');
    await settle(page);
    for (let r = 1; r <= ROWS; r += 1) {
      if (mode === 'busy') {
        const a = await onGrid(page, at(5, r));
        await page.mouse.click(a.x, a.y);
        await page.keyboard.type(`=SUM(C1:C${ROWS})*${r}\n`);
        await page.waitForTimeout(100);
      }
      const p = await onGrid(page, at(2, r));
      await page.mouse.move(p.x, p.y);
      await page.mouse.down({ clickCount: 1 });
      await page.mouse.up({ clickCount: 1 });
      await page.waitForTimeout(90);
      await page.mouse.down({ clickCount: 2 });
      await page.mouse.up({ clickCount: 2 });
      await page.keyboard.type(digitOf(r));
      await page.keyboard.press('Enter');
      await page.waitForTimeout(250);
      if (!(await page.getByText('Ready', { exact: true }).last().isVisible())) {
        await page.keyboard.press('Escape');
        await page.waitForTimeout(100);
      }
    }
    const got = await settle(page);
    const bad: string[] = [];
    for (let r = 1; r <= ROWS; r += 1)
      if (got[`C${r}`] !== `7${digitOf(r)}`) bad.push(`C${r}=${JSON.stringify(got[`C${r}`])}`);
    console.log(`DBLKEY release ${mode} d=0: ${ROWS} trials, ${bad.length} wrong ${bad.join(' ')}`);
    expect(bad).toEqual([]);
  });
