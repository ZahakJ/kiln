/**
 * Document Recovery at the worst moment, on the installable web app (the Pages artefact): the page
 * goes the moment after an edit. Grid rounds repeat, one document through all of them: the edit, a
 * click on its cell 100 ms after Enter, and Reload or a crash of the page at once, with nothing
 * waited for. Every round must bring back that round's value, not the round's before.
 *
 * `KILN_RECOVERY_ROUNDS` sets the rounds (50): `pnpm exec playwright test --project=pwa --workers=1
 * specs/pwa-recovery.spec.ts`.
 */

import { existsSync, readFileSync } from 'node:fs';
import { fileURLToPath } from 'node:url';
import { type BrowserContext, expect, type Page, test } from '@playwright/test';
import { CORE_START, installLaunchQueue, type LaunchHandle, openIsolated, prepare } from './pwa-helpers.ts';

const ROUNDS = Number(process.env.KILN_RECOVERY_ROUNDS ?? 50);
const EDITS_PAST_THE_UNDO_LIMIT = 1100;
const CELL = { x: 420, y: 420 };
/** The worst timing: the click lands while the edit's own writes are under way. */
const CLICK_AFTER_ENTER_MS = 100;

test.skip(!!process.env.KILN_E2E_BASE, 'needs a page of its own origin');
test.describe.configure({ mode: 'default', timeout: 300_000 });

/**
 * Why a core stopped, as its pages logged it (a check core's "journal check: …", any panic): a
 * failing test prints it, so a red run says why and not only which expectation timed out.
 */
const stops = new Map<string, string[]>();
test.beforeEach(({ context }, info) => {
  const lines: string[] = [];
  stops.set(info.testId, lines);
  context.on('console', (m) => {
    const text = m.text();
    if (m.type() === 'error' && /the core stopped|journal check|panicked/.test(text)) lines.push(text);
  });
});
test.afterEach(({ context: _context }, info) => {
  const lines = stops.get(info.testId) ?? [];
  stops.delete(info.testId);
  if (info.status !== info.expectedStatus && lines.length > 0) {
    const why = lines.join('\n');
    console.log(`The core stopped in "${info.title}":\n${why}`);
    info.annotations.push({ type: 'core stopped', description: why });
  }
});

const bar = (page: Page) => page.getByRole('textbox', { name: 'Formula Bar' });
const proxy = (page: Page) => page.locator('[data-kiln-grid-proxy]');
const nameBox = (page: Page) =>
  page
    .getByRole('combobox', { name: 'Name Box' })
    .or(page.getByRole('textbox', { name: 'Name Box' }))
    .first();

/**
 * A click on the grid at a point given for the grid where it starts with no message bar (its top
 * left at 10, 206 in this viewport): a notice in the message bar pushes the grid down.
 */
async function onGrid(
  page: Page,
  p: { x: number; y: number },
  how: 'click' | 'dblclick' = 'click',
): Promise<void> {
  const box = await page.getByTestId('grid-editor').boundingBox();
  const [dx, dy] = box ? [box.x - 10, box.y - 206] : [0, 0];
  await page.mouse[how](p.x + dx, p.y + dy);
}

async function newWorkbook(page: Page): Promise<void> {
  await openIsolated(page);
  await page.goto('./#/grid');
  await expect(page.getByTestId('document-title')).not.toContainText('Opening', { timeout: CORE_START });
  await expect(bar(page)).toBeVisible({ timeout: CORE_START });
}

/** The one document Document Recovery lists opens. */
async function recoverOnly(page: Page): Promise<void> {
  const row = page.getByTestId('document-recovery').locator('[data-recovery]');
  await expect(row).toHaveCount(1, { timeout: CORE_START });
  await row.getByRole('button', { name: 'Open' }).click();
  await expect(page).toHaveURL(/#\/grid\?doc=/, { timeout: CORE_START });
  await expect(row).toHaveCount(0, { timeout: CORE_START });
  await expect(page.getByTestId('save-status').first()).toContainText(/Not saved|Unsaved/, {
    timeout: CORE_START,
  });
}

async function cellShows(page: Page, value: string, at = CELL): Promise<void> {
  await expect
    .poll(
      async () => {
        await onGrid(page, at);
        return bar(page).inputValue();
      },
      { timeout: 30_000 },
    )
    .toBe(value);
}

/** A real crash of the page's process; the next page of the context finds what it left. */
async function crash(context: BrowserContext, page: Page): Promise<Page> {
  const cdp = await context.newCDPSession(page);
  const crashed = page.waitForEvent('crash');
  cdp.send('Page.crash').catch(() => undefined);
  await crashed;
  const next = await context.newPage();
  await prepare(next);
  await next.goto('./#/grid');
  await page.close().catch(() => undefined);
  return next;
}

/** The browser stops finishing OPFS writes here: as far as they go, the page has already gone. */
const stallWrites = (page: Page) =>
  page.evaluate(() => {
    FileSystemWritableFileStream.prototype.close = () => new Promise(() => undefined);
  });

/** Checkpoints of a tab that asks never finish: the one under way is under way when the page goes. */
function stallScript(): void {
  const open = FileSystemFileHandle.prototype.createWritable;
  FileSystemFileHandle.prototype.createWritable = async function (options) {
    const stream = await open.call(this, options);
    if (this.name.includes('.base.') && sessionStorage.getItem('stall-checkpoints') === '1')
      stream.close = () => new Promise(() => undefined);
    return stream;
  };
}

/** Pastes `rows` rows of ten columns at A1. */
async function pasteSheet(page: Page, rows: number): Promise<void> {
  await onGrid(page, { x: 100, y: 290 });
  await page.evaluate((n) => {
    const lines: string[] = [];
    for (let r = 0; r < n; r += 1) {
      const cells: string[] = [];
      for (let k = 0; k < 10; k += 1) cells.push(k % 3 === 0 ? `Customer ${r}-${k}` : String(r * k + 0.5));
      lines.push(cells.join('\t'));
    }
    const data = new DataTransfer();
    data.setData('text/plain', lines.join('\n'));
    document
      .querySelector('[data-kiln-grid-proxy]')
      ?.dispatchEvent(new ClipboardEvent('paste', { clipboardData: data, bubbles: true, cancelable: true }));
  }, rows);
  await expect.poll(() => storedBytes(page, '.base.'), { timeout: 60_000 }).toBeGreaterThan(rows * 100);
}

/** The largest journal file whose name has `part`. */
const storedBytes = (page: Page, part: string) =>
  page.evaluate(async (p) => {
    const root = await navigator.storage.getDirectory();
    const top = await root.getDirectoryHandle('kiln-store').catch(() => null);
    const dir = await top?.getDirectoryHandle('Journal').catch(() => null);
    let most = 0;
    if (!dir) return 0;
    for await (const [name, handle] of (
      dir as unknown as { entries(): AsyncIterable<[string, FileSystemFileHandle]> }
    ).entries())
      if (decodeURIComponent(name).includes(p)) most = Math.max(most, (await handle.getFile()).size);
    return most;
  }, part);

async function edit(page: Page, value: string, press: (page: Page) => Promise<void>): Promise<void> {
  await onGrid(page, CELL);
  await page.keyboard.type(`${value}\n`);
  await page.waitForTimeout(CLICK_AFTER_ENTER_MS);
  await press(page);
}

for (const [name, press] of [
  ['a click', (page: Page) => onGrid(page, CELL)],
  ['a double-click', (page: Page) => onGrid(page, CELL, 'dblclick')],
] as const)
  test(`Grid: ${ROUNDS} edits in a row come back after ${name} and Reload`, async ({ page }) => {
    test.setTimeout(60_000 + ROUNDS * 30_000);
    await newWorkbook(page);
    for (let i = 0; i < ROUNDS; i += 1) {
      const value = String(1000 + i);
      await test.step(`round ${i + 1} of ${ROUNDS}`, async () => {
        await edit(page, value, press);
        await page.reload();
        await recoverOnly(page);
        await cellShows(page, value);
      });
    }
  });

test(`Grid: ${ROUNDS} edits in a row come back after a click and a crash of the page`, async ({
  page: first,
  context,
}) => {
  test.setTimeout(60_000 + ROUNDS * 30_000);
  let page = first;
  await newWorkbook(page);
  for (let i = 0; i < ROUNDS; i += 1) {
    const value = String(2000 + i);
    await test.step(`round ${i + 1} of ${ROUNDS}`, async () => {
      await edit(page, value, (p) => onGrid(p, CELL));
      page = await crash(context, page);
      await recoverOnly(page);
      await cellShows(page, value);
    });
  }
});

test(`Grid: ${ROUNDS} crashes with more than the undo limit of edits before each`, async ({
  page: first,
  context,
}) => {
  test.setTimeout(60_000 + ROUNDS * 120_000);
  let page = first;
  await newWorkbook(page);
  for (let i = 0; i < ROUNDS; i += 1) {
    await test.step(`round ${i + 1} of ${ROUNDS}`, async () => {
      await onGrid(page, CELL);
      for (let n = 1; n <= EDITS_PAST_THE_UNDO_LIMIT; n += 1) {
        await page.keyboard.type(`r${i}n${n}\n`);
        await page.keyboard.press('ArrowUp');
      }
      const last = `r${i}n${EDITS_PAST_THE_UNDO_LIMIT}`;
      await expect(bar(page)).toHaveValue(last, { timeout: 60_000 });
      page = await crash(context, page);
      await recoverOnly(page);
      await cellShows(page, last);
    });
  }
});

test('Grid: an edit past the undo limit, after a save, still asks to be saved', async ({ page }) => {
  await newWorkbook(page);
  await onGrid(page, CELL);
  for (let n = 1; n <= EDITS_PAST_THE_UNDO_LIMIT; n += 1) {
    await page.keyboard.type(`${n}\n`);
    await page.keyboard.press('ArrowUp');
  }
  await page.getByRole('button', { name: 'Save', exact: true }).first().click();
  await page.keyboard.press('Enter');
  await expect(page.getByTestId('save-status').first()).not.toContainText(/Not saved|Unsaved/, {
    timeout: 20_000,
  });
  await onGrid(page, CELL);
  await page.keyboard.type('after the save\n');
  await expect(page.getByTestId('save-status').first()).toContainText(/Not saved|Unsaved/, {
    timeout: 20_000,
  });
});

test('Grid: the edit comes back when the page goes before any of its writes finish', async ({ page }) => {
  await newWorkbook(page);
  await onGrid(page, CELL);
  await page.keyboard.type('1111\n');
  await page.waitForTimeout(300);
  await stallWrites(page);
  await edit(page, '4242', (p) => onGrid(p, CELL));
  await page.reload();
  await recoverOnly(page);
  await cellShows(page, '4242');
});

/** The status bar says the sheet is ready: no cell is open for editing. */
const closed = (page: Page) => expect(page.getByText('Ready', { exact: true }).last()).toBeVisible();

/** The message bar's offer of what was being typed (references and text bidi-isolated). */
const notice = (page: Page) => page.getByTestId('storage-notice').filter({ hasText: 'was not entered' });

test('Grid: text being typed is offered back by the message, never reopened on its own', async ({ page }) => {
  const dependant = { x: CELL.x + 64, y: CELL.y };
  await newWorkbook(page);
  await onGrid(page, CELL);
  await page.keyboard.type('5\n');
  await onGrid(page, dependant);
  await page.keyboard.type('=F8*2\n');
  await onGrid(page, CELL);
  await page.keyboard.type('9');
  await expect(bar(page)).toHaveValue('9');
  await page.reload();
  await recoverOnly(page);

  // Committed values only, nothing open; the message names the cell and the text.
  await expect(notice(page)).toContainText('“\u20689\u2069” was being typed in \u2068Sheet1!F8\u2069', {
    timeout: 15_000,
  });
  await closed(page);
  await cellShows(page, '5');
  await cellShows(page, '=F8*2', dependant);

  // The user puts it back: the cell opens with the text, and Enter enters it.
  await notice(page).getByRole('button', { name: 'Put it back' }).click();
  await expect(bar(page)).toHaveValue('9', { timeout: 15_000 });
  await proxy(page).focus();
  await page.keyboard.press('Enter');
  await cellShows(page, '9');

  // A formula that does not parse yet is offered as it was typed.
  await onGrid(page, dependant);
  await page.keyboard.type('=F8+');
  await expect(bar(page)).toHaveValue('=F8+');
  await page.reload();
  await recoverOnly(page);
  await expect(notice(page)).toContainText('“\u2068=F8+\u2069” was being typed in \u2068Sheet1!G8\u2069', {
    timeout: 15_000,
  });
  await closed(page);
  await cellShows(page, '=F8*2', dependant);
});

test('Grid: Put it back from the keyboard, on a sheet renamed since, the notice off the grid', async ({
  page,
}) => {
  await newWorkbook(page);
  await onGrid(page, CELL);
  await page.keyboard.type('kept');
  await expect(bar(page)).toHaveValue('kept');
  await page.reload();
  await recoverOnly(page);
  await expect(notice(page)).toContainText('“\u2068kept\u2069” was being typed in \u2068Sheet1!F8\u2069', {
    timeout: 15_000,
  });
  // It takes the keys as it appears: Put it back is focused, and read out (an alert).
  const put = notice(page).getByRole('button', { name: 'Put it back' });
  await expect(put).toBeFocused({ timeout: 10_000 });
  await expect(notice(page)).toHaveAttribute('role', 'alert');
  // It is in the message bar under the ribbon: the document, Formula Bar and Name Box included,
  // starts below it, so it covers none of them.
  const box = await notice(page).boundingBox();
  for (const part of [
    page.locator('.kiln-formulabar').first(),
    nameBox(page),
    page.getByTestId('grid-editor'),
  ]) {
    const b = await part.boundingBox();
    expect(box && b && b.y >= box.y + box.height - 1, 'the notice is above the document').toBe(true);
  }
  // The sheet is renamed in the meantime; F6 brings the keys back to the notice.
  await page.getByRole('tablist', { name: 'Sheets' }).getByRole('tab').first().dblclick();
  await page.keyboard.press('Control+a');
  await page.keyboard.type('Renamed since\n');
  await expect(page.getByRole('tablist', { name: 'Sheets' }).getByRole('tab').first()).toHaveText(
    'Renamed since',
  );
  await proxy(page).focus();
  for (let i = 0; i < 6 && !(await put.evaluate((b) => b === document.activeElement)); i += 1)
    await page.keyboard.press('F6');
  await expect(put).toBeFocused();
  await page.keyboard.press('Enter');
  await expect(bar(page)).toHaveValue('kept', { timeout: 15_000 });
  await proxy(page).focus();
  await page.keyboard.press('Enter');
  await cellShows(page, 'kept');
});

test('Grid: a cell still being typed is offered back after a crash', async ({ page: first, context }) => {
  await newWorkbook(first);
  await onGrid(first, CELL);
  await first.keyboard.type('committed\n');
  await onGrid(first, CELL);
  await first.keyboard.type('half typed');
  await expect(bar(first)).toHaveValue('half typed');
  const page = await crash(context, first);
  await recoverOnly(page);
  await expect(notice(page)).toContainText('“\u2068half typed\u2069” was being typed', { timeout: 15_000 });
  await closed(page);
  await cellShows(page, 'committed');
});

for (const [name, wait] of [
  ['once its frame shows', true],
  ['at once', false],
] as const)
  test(`Grid: ${ROUNDS} committed cells come back closed after Reload ${name}, and typing goes where it should`, async ({
    page,
  }) => {
    test.setTimeout(60_000 + ROUNDS * 30_000);
    const below = { x: CELL.x, y: CELL.y + 25 };
    await newWorkbook(page);
    await onGrid(page, CELL);
    await page.keyboard.type('777\n');
    let previous = '777';
    for (let i = 0; i < ROUNDS; i += 1) {
      const value = String(7100 + i);
      await test.step(`round ${i + 1} of ${ROUNDS}`, async () => {
        await onGrid(page, CELL);
        await expect(bar(page)).toHaveValue(previous);
        await page.keyboard.type(`${value}\n`);
        // Enter moved the selection down: the commit is on screen.
        if (wait) await expect(nameBox(page)).toHaveValue('F9', { timeout: 15_000 });
        await page.reload();
        await recoverOnly(page);
        await closed(page);
        await onGrid(page, below);
        await page.keyboard.type('55\n');
        await cellShows(page, '55', below);
        // The cell has the value committed (or, with Reload at once, the one before it), never
        // the next typing added to it.
        await onGrid(page, CELL);
        await expect(nameBox(page)).toHaveValue('F8');
        await page.waitForTimeout(200);
        const got = await bar(page).inputValue();
        if (wait) expect(got).toBe(value);
        else expect([value, previous]).toContain(got);
        previous = got;
      });
    }
  });

test('Grid: what is typed straight after the Name Box lands whole in the cell gone to', async ({ page }) => {
  await newWorkbook(page);
  for (let i = 0; i < 10; i += 1) {
    const target = `D${13 + i}`;
    await nameBox(page).click();
    await nameBox(page).fill(target);
    await nameBox(page).press('Enter');
    await page.keyboard.type(`=SUM(D1:D${i + 2})\n`);
    await nameBox(page).click();
    await nameBox(page).fill(target);
    await nameBox(page).press('Enter');
    await expect(bar(page)).toHaveValue(`=SUM(D1:D${i + 2})`);
  }
});

test('Grid: the edit comes back after the tab closes at once', async ({ page, context }) => {
  await newWorkbook(page);
  await onGrid(page, CELL);
  await page.keyboard.type('1111\n');
  await page.waitForTimeout(300);
  await edit(page, '4242', (p) => onGrid(p, CELL));
  await page.close();
  const next = await context.newPage();
  await prepare(next);
  await next.goto('./#/grid');
  await recoverOnly(next);
  await cellShows(next, '4242');
});

test('Grid: a sheet in the workbook is shown again after recovery', async ({ page }) => {
  await newWorkbook(page);
  await onGrid(page, CELL);
  await page.keyboard.type('first\n');
  await page
    .getByRole('button', { name: /new sheet|insert sheet/i })
    .first()
    .click();
  await onGrid(page, CELL);
  await page.keyboard.type('second\n');
  await onGrid(page, CELL);
  await expect(bar(page)).toHaveValue('second');
  await page.reload();
  await recoverOnly(page);
  await cellShows(page, 'second');
});

for (const rows of [10_000, 50_000])
  test(`Grid: ${rows} rows: every stage of a checkpoint survives a crash`, async ({
    page: first,
    context,
  }) => {
    // Check cores replay every journal write, and a CI runner is slower still: 210 commits on a
    // sheet of 50,000 rows took over 300 s there, about 3 s each.
    const slow = (test.info().project.name === 'pwa-check' ? 2 : 1) * (process.env.CI ? 2 : 1);
    test.setTimeout(900_000 * slow);
    let page = first;
    await context.addInitScript(stallScript);
    await newWorkbook(page);
    await pasteSheet(page, rows);
    const typed = async (value: string) => {
      await onGrid(page, CELL);
      await page.keyboard.type(`${value}\n`);
      await onGrid(page, CELL);
      await expect(bar(page)).toHaveValue(value, { timeout: 30_000 });
    };
    const again = async (value: string) => {
      page = await crash(context, page);
      await recoverOnly(page);
      await cellShows(page, value);
    };

    // After an undo, with no checkpoint to wait for.
    await typed('before undo');
    await typed('undone');
    await page.keyboard.press('Control+z');
    await expect(bar(page)).toHaveValue('before undo', { timeout: 30_000 });
    await typed('after undo');
    await again('after undo');

    // A paste over cells, then a crash.
    await onGrid(page, CELL);
    await page.evaluate(() => {
      const data = new DataTransfer();
      data.setData('text/plain', 'pasted');
      document
        .querySelector('[data-kiln-grid-proxy]')
        ?.dispatchEvent(
          new ClipboardEvent('paste', { clipboardData: data, bubbles: true, cancelable: true }),
        );
    });
    await expect(bar(page)).toHaveValue('pasted', { timeout: 30_000 });
    await again('pasted');

    // During the write: the checkpoint of the 200th edit never lands.
    await page.evaluate(() => sessionStorage.setItem('stall-checkpoints', '1'));
    await onGrid(page, CELL);
    for (let n = 1; n <= 210; n += 1) {
      await page.keyboard.type(`${n}\n`);
      await page.keyboard.press('ArrowUp');
    }
    // A commit on a sheet this size takes the core a while (about a second at 50,000 rows).
    await expect(bar(page)).toHaveValue('210', { timeout: 300_000 * slow });
    await again('210');
  });

test(`Grid: ${ROUNDS} crashes during a large checkpoint`, async ({ page: first, context }) => {
  test.setTimeout(120_000 + ROUNDS * 30_000);
  let page = first;
  await newWorkbook(page);
  await pasteSheet(page, 10_000);
  for (let i = 0; i < ROUNDS; i += 1) {
    const value = String(3000 + i);
    await test.step(`round ${i + 1} of ${ROUNDS}`, async () => {
      // Opening a recovered document writes a checkpoint: the edit lands on one under way.
      await onGrid(page, CELL);
      await page.keyboard.type(`${value}\n`);
      await onGrid(page, CELL);
      await expect(bar(page)).toHaveValue(value, { timeout: 30_000 });
      page = await crash(context, page);
      await recoverOnly(page);
      await cellShows(page, value);
    });
  }
});

const XLSX = fileURLToPath(new URL('../../fixtures/external/xlsx/poi-65016.xlsx', import.meta.url));

test('Grid: a workbook opened from a file, with its original beside it, survives a crash', async ({
  page: first,
  context,
}) => {
  test.skip(!existsSync(XLSX), 'needs the downloaded corpus');
  await installLaunchQueue(first);
  await openIsolated(first);
  await first.evaluate(
    ([name, bytes]) => (window as unknown as LaunchHandle).__launch(name as string, bytes as number[]),
    ['stress.xlsx', Array.from(readFileSync(XLSX))] as const,
  );
  await expect(first).toHaveURL(/#\/grid\?doc=/, { timeout: CORE_START });
  await expect(bar(first)).toBeVisible({ timeout: CORE_START });
  // The first checkpoint takes the original with it.
  await expect.poll(() => storedBytes(first, '.base.'), { timeout: 30_000 }).toBeGreaterThan(100_000);
  await onGrid(first, CELL);
  await first.keyboard.type('after opening\n');
  await onGrid(first, CELL);
  await expect(bar(first)).toHaveValue('after opening', { timeout: 15_000 });
  const page = await crash(context, first);
  await recoverOnly(page);
  await cellShows(page, 'after opening');
});

for (const threads of [true, false])
  test(`Grid: keys keep their order and cell while the core is busy (${
    threads ? 'threads' : 'single'
  } core)`, async ({ browser, baseURL }) => {
    test.setTimeout(300_000);
    const context = await browser.newContext({
      baseURL,
      viewport: { width: 1280, height: 800 },
      serviceWorkers: threads ? 'allow' : 'block',
    });
    const page = await context.newPage();
    if (threads) await openIsolated(page);
    else await prepare(page);
    await page.goto('./#/grid');
    await expect(bar(page)).toBeVisible({ timeout: CORE_START });
    await onGrid(page, { x: 100, y: 290 });
    await page.evaluate(() => {
      const lines: string[] = [];
      for (let i = 0; i < 3000; i += 1) lines.push('=SUMPRODUCT(($F$1:$F$6000>0)*($F$1:$F$6000<9))+ROW()');
      const data = new DataTransfer();
      data.setData('text/plain', lines.join('\n'));
      document
        .querySelector('[data-kiln-grid-proxy]')
        ?.dispatchEvent(
          new ClipboardEvent('paste', { clipboardData: data, bubbles: true, cancelable: true }),
        );
    });
    await page.waitForTimeout(8000);
    const places = [CELL.y, CELL.y + 60, CELL.y + 120, CELL.y - 60].map((y) => ({ x: CELL.x, y }));
    for (const enter of [true, false]) {
      let value = enter ? 100 : 500;
      const wanted: { at: (typeof places)[number]; value: string }[] = [];
      for (const at of places) {
        value += 1;
        wanted.push({ at, value: String(value) });
        await onGrid(page, at);
        await page.waitForTimeout(90);
        await page.keyboard.type(enter ? `${value}\n` : `${value}`, { delay: 45 });
        await page.waitForTimeout(110);
      }
      if (!enter) await page.keyboard.press('Enter');
      for (const { at, value: v } of wanted) await cellShows(page, v, at);
    }
    await context.close();
  });
