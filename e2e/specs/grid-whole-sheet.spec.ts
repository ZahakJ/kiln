/**
 * The whole sheet selected (the corner button, Ctrl+A twice): every Grid command and dialog runs
 * against a selection of 1,048,576 × 16,384 cells. None may walk that grid cell by cell: the core
 * keeps answering, and a command that cannot work on the whole sheet says so in written words.
 */
import { expect, type Page, test } from '@playwright/test';
import { fill, gridState, openGrid, value } from './grid-helpers.ts';

test.describe.configure({ timeout: 2_400_000 });
// Every step fails in a few seconds, in written words, rather than waiting on a core that stopped.
test.use({ actionTimeout: 10_000 });

const ROWS = 1_048_576;
const COLS = 16_384;
const DATA = [
  ['Name', 'Pair', 'Score'],
  ['Ada', 'a,b', '3'],
  ['Bo', 'c,d', '1'],
  ['Cy', 'e,f', '2'],
];

/** Whether the core answers a question within `ms` (a stopped or busy core does not). */
const alive = (page: Page, ms = 6000) =>
  page.evaluate(async (limit) => {
    const g = (
      window as unknown as {
        __kilnGrid?: { client: { query: (d: unknown, q: unknown) => Promise<unknown> }; doc: unknown };
      }
    ).__kilnGrid;
    if (!g) return false;
    try {
      await Promise.race([
        g.client.query(g.doc, { Grid: 'State' }),
        new Promise((_, no) => setTimeout(() => no(new Error('no answer')), limit)),
      ]);
      return true;
    } catch {
      return false;
    }
  }, ms);

/** Selects every cell of the sheet through the core. */
const selectAll = (page: Page) =>
  page.evaluate(
    async ([r1, c1]) => {
      const g = (
        window as unknown as {
          __kilnGrid: { client: { execute: (d: unknown, c: unknown) => Promise<unknown> }; doc: unknown };
        }
      ).__kilnGrid;
      await g.client.execute(g.doc, {
        Grid: { SelectRange: { range: { r0: 0, c0: 0, r1, c1 }, add: false } },
      });
    },
    [ROWS - 1, COLS - 1] as const,
  );

/** The corner button above the row numbers: the whole sheet, as a person selects it. */
async function clickCorner(page: Page): Promise<void> {
  const box = await page.locator('[data-kiln-grid-surface]').boundingBox();
  const { header_w, header_h } = (await gridState(page)).layout;
  if (!box) throw new Error('the sheet is not on screen');
  await page.mouse.click(box.x + header_w / 2, box.y + header_h / 2);
}

/** The text of every notice on the page. */
const notices = (page: Page) =>
  page.locator('[role="alert"], [role="status"], [role="alertdialog"]').allInnerTexts();

const dialogs = (page: Page) => page.locator('[role="dialog"], [role="alertdialog"]').count();

async function closeAll(page: Page): Promise<void> {
  for (let i = 0; i < 3; i += 1) {
    await page.keyboard.press('Escape');
    await page.waitForTimeout(60);
  }
}

test('Text to Columns with the whole sheet selected says it converts one column at a time', async ({
  page,
}) => {
  await openGrid(page);
  await fill(page, DATA);
  await clickCorner(page);
  await expect.poll(async () => (await gridState(page)).name_box, { timeout: 5000 }).toBe('A1');
  await page.getByRole('tab', { name: 'Data', exact: true }).click();
  await page
    .getByRole('button', { name: /Text to Columns/ })
    .first()
    .click();
  const notice = page
    .getByRole('status')
    .filter({ hasText: 'Text to Columns converts one column at a time' });
  await expect(notice.first()).toBeVisible({ timeout: 10_000 });
  // Guidance, not a failure: an informational notice titled after the command.
  await expect(notice.first()).not.toContainText('did not work');
  await expect(notice.first()).not.toContainText('…');
  await expect(page.getByRole('dialog', { name: /Text to Columns/ })).toHaveCount(0);
  expect(await alive(page)).toBe(true);
  await page.waitForTimeout(3000);
  expect(await alive(page)).toBe(true);
  expect(await value(page, 1, 1)).toBe('a,b');
});

test('Text to Columns on a whole column opens and splits that column', async ({ page }) => {
  await openGrid(page);
  await fill(page, [['a,b'], ['c,d'], ['e,f']], 0, 0);
  await page.evaluate(async (r1) => {
    const g = (
      window as unknown as {
        __kilnGrid: { client: { execute: (d: unknown, c: unknown) => Promise<unknown> }; doc: unknown };
      }
    ).__kilnGrid;
    await g.client.execute(g.doc, {
      Grid: { SelectRange: { range: { r0: 0, c0: 0, r1, c1: 0 }, add: false } },
    });
  }, ROWS - 1);
  await page.getByRole('tab', { name: 'Data', exact: true }).click();
  await page
    .getByRole('button', { name: /Text to Columns/ })
    .first()
    .click();
  const dialog = page.getByTestId('grid-text-to-columns');
  await expect(dialog).toBeVisible({ timeout: 10_000 });
  expect(await alive(page)).toBe(true);
  await dialog.getByRole('checkbox', { name: 'Comma' }).check();
  await dialog.getByRole('button', { name: 'Finish', exact: true }).click();
  await expect.poll(async () => value(page, 2, 1), { timeout: 10_000 }).toBe('f');
  expect(await value(page, 0, 0)).toBe('a');
});

/** Ids the sweep leaves alone: they leave the sheet, open the system's own windows or end the session. */
const SKIP =
  /^(suite\.file\.|suite\.help\.|suite\.app\.|suite\.home\.|suite\.window\.|suite\.dev\.)|print|close|quit|exit|share|export|save|open_|_open|import|new_window|sign|download|collab|presence|reload|full_?screen|dictate|read_aloud|record|tour|feedback/i;

interface Fault {
  id: string;
  what: 'stopped' | 'no answer';
  enter: boolean;
}

/** One pass of the sweep: what the sheet holds and what is selected before each command. */
interface Pass {
  name: string;
  /** Formatted rows and columns besides the sample (rows 6:2005 bold, columns K:AD percent). */
  styled: boolean;
  select: (page: Page) => Promise<unknown>;
}

/** Runs a Grid core command, as the chrome sends it. */
const exec = (page: Page, command: unknown) =>
  page.evaluate(async (c) => {
    const g = (
      window as unknown as {
        __kilnGrid: { client: { execute: (d: unknown, c: unknown) => Promise<unknown> }; doc: unknown };
      }
    ).__kilnGrid;
    await g.client.execute(g.doc, c);
  }, command);

const selectRange = (page: Page, r0: number, c0: number, r1: number, c1: number) =>
  exec(page, { Grid: { SelectRange: { range: { r0, c0, r1, c1 }, add: false } } });

/** Formatted rows and columns, as an imported workbook often has: they meet every whole-sheet format. */
async function styleFixture(page: Page): Promise<void> {
  await selectRange(page, 5, 0, 2004, COLS - 1);
  await exec(page, { Suite: { Edit: 'Bold' } });
  await selectRange(page, 0, 10, ROWS - 1, 29);
  await exec(page, { Grid: 'PercentStyle' });
  await selectRange(page, 0, 0, 0, 0);
}

/** Runs `ids` one by one with the pass's selection; returns the commands after which the core stopped or did not answer. */
async function sweep(
  page: Page,
  ids: string[],
  enter: boolean,
  pass: Pass,
): Promise<{ faults: Fault[]; ran: number; slowest: string[] }> {
  const faults: Fault[] = [];
  const took: [string, number][] = [];
  let ran = 0;
  const fresh = async () => {
    await page.goto('about:blank');
    await openGrid(page);
    await fill(page, DATA);
    if (pass.styled) await styleFixture(page);
  };
  await fresh();
  for (const id of ids) {
    await pass.select(page).catch(() => undefined);
    const t0 = Date.now();
    let waited = 150;
    // The command (as the ribbon, menus and Tell me run it), given 5 s to return.
    await page
      .evaluate(
        (cmd) =>
          Promise.race([
            (
              window as unknown as { __kiln: { registry: { execute: (i: string) => Promise<unknown> } } }
            ).__kiln.registry
              .execute(cmd)
              .catch(() => undefined),
            new Promise((ok) => setTimeout(ok, 5000)),
          ]),
        id,
      )
      .catch(() => undefined);
    ran += 1;
    await page.waitForTimeout(150);
    if ((await dialogs(page)) > 0) {
      // A dialog: its own questions to the core run now; then its default button or Cancel.
      await page.waitForTimeout(700);
      waited += 700;
      if (enter) {
        await page.keyboard.press('Enter');
        await page.waitForTimeout(400);
        waited += 400;
      }
    }
    const ok = await alive(page);
    // How long the command kept the core (the fixed waits above taken off).
    took.push([id, Date.now() - t0 - waited]);
    const stopped =
      !ok &&
      (await page
        .getByText('stopped working')
        .count()
        .catch(() => 0)) > 0;
    if (!ok) {
      faults.push({ id, what: stopped ? 'stopped' : 'no answer', enter });
      await fresh();
      continue;
    }
    await closeAll(page);
    // A command that filled the selection's million rows or 16,384 columns (Fill Down over rows
    // 2:1048576, as Excel does) did real work; the next one meets the sample again, not that.
    const far = await Promise.all([value(page, 9_999, 0), value(page, 1, 999)].map((v) => v.catch(() => '')));
    if (far.some((v) => v !== '')) {
      await fresh();
      continue;
    }
    // Commands that cleared or moved the sample get it back, so each one meets data.
    if ((await value(page, 0, 0).catch(() => '')) !== 'Name') {
      await page.evaluate(async () => {
        const g = (
          window as unknown as {
            __kilnGrid: { client: { execute: (d: unknown, c: unknown) => Promise<unknown> }; doc: unknown };
          }
        ).__kilnGrid;
        for (let i = 0; i < 4; i += 1)
          await g.client.execute(g.doc, { Suite: 'Undo' }).catch(() => undefined);
      });
      if ((await value(page, 0, 0).catch(() => '')) !== 'Name') await fresh();
    }
  }
  // KILN_SWEEP_LOG: every command's time, in order (to find what a slow one followed).
  if (process.env.KILN_SWEEP_LOG)
    console.log(`sweep order: ${took.map(([id, ms]) => `${id} ${ms}`).join(', ')}`);
  const slowest = took
    .sort((a, b) => b[1] - a[1])
    .slice(0, 8)
    .map(([id, ms]) => `${id} ${ms} ms`);
  return { faults, ran, slowest };
}

const PASSES: [Pass, boolean][] = [
  [{ name: 'the whole sheet selected', styled: false, select: selectAll }, false],
  [{ name: 'the whole sheet selected', styled: false, select: selectAll }, true],
  [{ name: 'the whole sheet over formatted rows and columns', styled: true, select: selectAll }, true],
  [
    {
      name: 'rows 2:1048576 selected',
      styled: false,
      select: (page) => selectRange(page, 1, 0, ROWS - 1, COLS - 1),
    },
    true,
  ],
];

for (const [pass, enter] of PASSES)
  test(`every Grid command and dialog with ${pass.name} keeps the core answering (${enter ? 'dialogs confirmed' : 'dialogs cancelled'})`, async ({
    page,
  }) => {
    await openGrid(page);
    const ids = await page.evaluate((skip) => {
      const rx = new RegExp(skip, 'i');
      return (window as unknown as { __kiln: { registry: { list: () => { id: string }[] } } }).__kiln.registry
        .list()
        .map((c) => c.id)
        .filter((id) => /^(grid\.|suite\.|ui\.grid\.)/.test(id) && !rx.test(id));
    }, SKIP.source);
    // KILN_SWEEP_ONLY: a pattern of the ids to run (the slow ones again, on a throttled CPU).
    const only = process.env.KILN_SWEEP_ONLY ? new RegExp(process.env.KILN_SWEEP_ONLY) : null;
    if (only) ids.splice(0, ids.length, ...ids.filter((id) => only.test(id)));
    expect(ids.length).toBeGreaterThan(only ? 0 : 150);
    const { faults, ran, slowest } = await sweep(page, ids, enter, pass);
    console.log(
      `sweep, ${pass.name} (${enter ? 'Enter' : 'Escape'}): ${ran} commands, faults ${JSON.stringify(faults)}; slowest ${slowest.join(', ')}`,
    );
    // Afterwards a new workbook takes typing as before.
    await page.goto('about:blank');
    await openGrid(page);
    await page.keyboard.type('done');
    await page.keyboard.press('Enter');
    await expect.poll(() => value(page, 0, 0), { timeout: 10_000 }).toBe('done');
    expect(faults).toEqual([]);
    void notices;
  });

test('a whole-sheet format, then 2,000 whole rows formatted, answers at once', async ({ page }) => {
  await openGrid(page);
  await fill(page, DATA);
  await selectAll(page);
  await exec(page, { Grid: 'PercentStyle' });
  await selectRange(page, 0, 0, 1999, COLS - 1);
  const t0 = Date.now();
  await exec(page, { Suite: { Edit: 'Bold' } });
  expect(await alive(page)).toBe(true);
  expect(Date.now() - t0).toBeLessThan(5000);
  // And the other way round: formatted rows, then the whole sheet.
  await selectAll(page);
  const t1 = Date.now();
  await exec(page, { Grid: 'CurrencyStyle' });
  expect(await alive(page)).toBe(true);
  expect(Date.now() - t1).toBeLessThan(5000);
});

test('whole column B on an empty sheet, then Data Validation, covers the whole column', async ({ page }) => {
  await openGrid(page);
  await selectRange(page, 0, 1, ROWS - 1, 1);
  await exec(page, {
    Grid: {
      SetValidation: {
        rule: {
          kind: 'List',
          op: 'Between',
          formula1: 'a,b',
          formula2: '',
          allow_blank: true,
          dropdown: true,
          show_input: false,
          input_title: '',
          input: '',
          show_error: true,
          style: 'Stop',
          error_title: '',
          error: '',
        },
      },
    },
  });
  await selectRange(page, 999_999, 1, 999_999, 1);
  const rule = await page.evaluate(async () => {
    const g = (
      window as unknown as {
        __kilnGrid: { client: { query: (d: unknown, q: unknown) => Promise<unknown> }; doc: unknown };
      }
    ).__kilnGrid;
    return g.client.query(g.doc, { Grid: 'Validation' });
  });
  expect(JSON.stringify(rule)).toContain('a,b');
});
