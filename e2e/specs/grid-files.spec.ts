/**
 * Kiln Grid ▸ Data ▸ Export end to end: the selection or the sheet as CSV (UTF-8), tab-separated
 * text or JSON, downloaded through the browser, with the displayed values Excel would write.
 */
import { readFileSync } from 'node:fs';
import { expect, type Page, test } from '@playwright/test';
import { BUDGET, expectSheetFocused, fill, focusSheet, openGrid, select, tab } from './grid-helpers.ts';

async function exportAs(page: Page, item: string): Promise<{ name: string; text: string }> {
  await tab(page, 'Data');
  await page.getByRole('button', { name: 'Export' }).first().click();
  const download = page.waitForEvent('download');
  await page.getByRole('menuitem', { name: item }).click();
  const d = await download;
  const path = await d.path();
  return { name: d.suggestedFilename(), text: readFileSync(path, 'utf8') };
}

test.describe('Kiln Grid files', () => {
  test.beforeEach(async ({ page }) => {
    await openGrid(page);
    await fill(page, BUDGET);
    await select(page, 0, 0, 2, 2);
    await focusSheet(page);
  });

  test('csv and tsv: the selection as text', async ({ page }) => {
    const csv = await exportAs(page, 'Selection as CSV (UTF-8)');
    expect(csv.name).toMatch(/\.csv$/);
    expect(csv.text.replace(/^﻿/, '').replace(/\r\n/g, '\n').trim()).toBe(
      'Item,Category,Amount\nClay,Materials,420\nGlaze,Materials,310',
    );
    await expectSheetFocused(page);
    const tsv = await exportAs(page, 'Sheet as Tab-Separated Text');
    expect(tsv.name).toMatch(/\.(tsv|txt)$/);
    expect(tsv.text.replace(/\r\n/g, '\n').trim().split('\n')).toHaveLength(7);
    expect(tsv.text).toContain('Kiln hire\tFiring\t640');
  });

  test('json: the selection as an array of records', async ({ page }) => {
    const json = await exportAs(page, 'Selection as JSON');
    expect(json.name).toMatch(/\.json$/);
    expect(JSON.parse(json.text)).toEqual([
      { Item: 'Clay', Category: 'Materials', Amount: 420 },
      { Item: 'Glaze', Category: 'Materials', Amount: 310 },
    ]);
    await expectSheetFocused(page);
  });
});
