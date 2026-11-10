/**
 * Keys typed right after a double-click on a cell: every one lands in the cell's in-cell
 * entry, in order, as in Excel, however soon it comes and however busy the core is. Each trial
 * double-clicks a cell that holds "7" (a person's double-click: two presses 90 ms apart), waits
 * `d` ms, types a digit other than 7 and Enter: the cell must hold "7<digit>". A key that went before the edit
 * leaves "<digit>" (replaced); a lost key leaves "7". `busy`: an entry is committed elsewhere
 * right before each double-click (the core recalculating), as when a person types, presses Enter
 * and double-clicks the next cell. `slow`: every answer of the core comes 40 ms late (a hook in
 * the page's client), so the keys always arrive before the double-click's hit test and the
 * cell's text have come back.
 *
 * Also here: text from an input method (a composition) or with no key at all (insertText) right
 * after the double-click lands once, after the cell's text; Ctrl+Z typed then takes its turn and
 * cancels the entry, as it does once the entry is open; an edit start the core never answers is
 * given up after a few seconds with the written notice, and the keys carry on; and Enter in the
 * sheet tab's rename box, the Name Box or the find bar leaves no entry open in the active cell.
 *
 * These cases reach into the core's client (filling cells, a slow or a silent core), which only
 * the dev build exposes, so they run in the chromium project; `pwa-dblclick.spec.ts` checks the
 * release build with the keys at once after the double-click.
 */
import { expect, type Page, test } from '@playwright/test';
import { cellPoint, fill, gridState, openGrid, value } from './grid-helpers.ts';

test.describe.configure({ timeout: 180_000 });

const TRIALS = 20;
const COL = 2;
/** The digit typed in trial `r` (never 7, which would read as a key typed over the cell's "7"). */
const digitOf = (r: number) => '12345689'[r % 8] as string;

/**
 * A hook in the page's client: `slow` holds every answer of the core `ms` late, as a busy machine
 * would; `hang` never answers an EditCell (a core that stopped).
 */
async function hookCore(page: Page, kind: 'slow' | 'hang', ms = 40): Promise<void> {
  await page.evaluate(
    ([kind, delay]) => {
      type C = {
        query: (...a: unknown[]) => Promise<unknown>;
        execute: (...a: unknown[]) => Promise<unknown>;
      };
      const c = (window as unknown as { __kilnGrid: { client: C } }).__kilnGrid.client;
      const later = <T>(p: Promise<T>) =>
        p.then((r) => new Promise<T>((ok) => setTimeout(() => ok(r), delay as number)));
      const query = c.query.bind(c);
      const execute = c.execute.bind(c);
      if (kind === 'hang') {
        c.execute = (...a) =>
          JSON.stringify(a[1]).includes('"EditCell"') ? new Promise(() => undefined) : execute(...a);
        return;
      }
      c.query = (...a) => later(query(...a));
      c.execute = (...a) => later(execute(...a));
    },
    [kind, ms] as const,
  );
}
const slowCore = (page: Page, ms: number) => hookCore(page, 'slow', ms);

type Mode = 'idle' | 'busy' | 'slow';
const BUCKETS: [Mode, number][] = [
  ['idle', 0],
  ['idle', 4],
  ['idle', 10],
  ['idle', 24],
  ['busy', 0],
  ['busy', 4],
  ['busy', 10],
  ['slow', 0],
  ['slow', 4],
];

/** A person's double-click on `p` (two presses 90 ms apart), then `d` ms. */
async function doubleClick(page: Page, p: { x: number; y: number }, d: number): Promise<void> {
  await page.mouse.move(p.x, p.y);
  await page.mouse.down({ clickCount: 1 });
  await page.mouse.up({ clickCount: 1 });
  await page.waitForTimeout(90);
  await page.mouse.down({ clickCount: 2 });
  const t0 = Date.now();
  await page.mouse.up({ clickCount: 2 });
  if (d > 0) await page.waitForTimeout(Math.max(0, d - (Date.now() - t0)));
}

/** Waits for the trial's keys, and closes anything left open so the next starts in Ready. */
async function endTrial(page: Page, ms = 250): Promise<void> {
  await page.waitForTimeout(ms);
  if ((await gridState(page)).mode !== 'Ready') {
    await page.keyboard.press('Escape');
    await page.waitForTimeout(100);
  }
}

for (const [mode, d] of BUCKETS)
  test(`a key ${d} ms after a double-click lands in the cell (${mode})`, async ({ page }) => {
    await openGrid(page);
    await fill(
      page,
      Array.from({ length: TRIALS }, () => ['7']),
      0,
      COL,
    );
    const points = [];
    for (let r = 0; r < TRIALS; r += 1) points.push(await cellPoint(page, r, COL));
    const other = await cellPoint(page, 0, 5);
    if (mode === 'slow') await slowCore(page, 40);
    for (let r = 0; r < TRIALS; r += 1) {
      if (mode === 'busy') {
        await page.mouse.click(other.x, other.y + 20 * r);
        await page.keyboard.type(`=SUM(C1:C${TRIALS})*${r + 1}\n`);
        await page.waitForTimeout(100);
      }
      await doubleClick(page, points[r] as { x: number; y: number }, d);
      await page.keyboard.type(digitOf(r));
      await page.keyboard.press('Enter');
      await endTrial(page);
    }
    const count = { edited: 0, replaced: 0, lost: 0, other: 0 };
    const bad: string[] = [];
    for (let r = 0; r < TRIALS; r += 1) {
      const digit = digitOf(r);
      const v = await value(page, r, COL);
      const kind = v === `7${digit}` ? 'edited' : v === digit ? 'replaced' : v === '7' ? 'lost' : 'other';
      count[kind] += 1;
      if (kind !== 'edited') bad.push(`C${r + 1}=${JSON.stringify(v)}`);
    }
    console.log(
      `DBLKEY ${mode} d=${d}: ${TRIALS} trials, edited ${count.edited}, replaced ${count.replaced}, lost ${count.lost}, other ${count.other} ${bad.join(' ')}`,
    );
    expect(bad, `${mode} d=${d}`).toEqual([]);
  });

test('keys typed before the text of the cell arrives go in, in order, caret keys too (slow)', async ({
  page,
}) => {
  await openGrid(page);
  await fill(
    page,
    Array.from({ length: TRIALS }, () => ['7']),
    0,
    COL,
  );
  const points = [];
  for (let r = 0; r < TRIALS; r += 1) points.push(await cellPoint(page, r, COL));
  await slowCore(page, 40);
  for (let r = 0; r < TRIALS; r += 1) {
    await doubleClick(page, points[r] as { x: number; y: number }, 0);
    // In Edit mode the caret keys move in the entry: "7" + "1", Left, "2" is "721".
    await page.keyboard.type('1');
    await page.keyboard.press('ArrowLeft');
    await page.keyboard.type('2');
    await page.keyboard.press('Enter');
    await endTrial(page);
  }
  const got: string[] = [];
  for (let r = 0; r < TRIALS; r += 1) got.push(await value(page, r, COL));
  console.log(
    `DBLKEY slow order: ${got.filter((v) => v === '721').length}/${TRIALS} as typed ${JSON.stringify(got)}`,
  );
  expect(got).toEqual(Array.from({ length: TRIALS }, () => '721'));
});

for (const mode of ['idle', 'slow'] as const)
  for (const d of [0, 20, 60, 150])
    test(`text from an input method ${d} ms after a double-click lands once (${mode})`, async ({ page }) => {
      await openGrid(page);
      const cdp = await page.context().newCDPSession(page);
      const n = 10;
      await fill(
        page,
        Array.from({ length: n }, () => ['7']),
        0,
        COL,
      );
      const points = [];
      for (let r = 0; r < n; r += 1) points.push(await cellPoint(page, r, COL));
      if (mode === 'slow') await slowCore(page, 40);
      for (let r = 0; r < n; r += 1) {
        await doubleClick(page, points[r] as { x: number; y: number }, d);
        if (r % 2 === 0) {
          // A composition (a dead key or an IME): its keydown is "Process", then the text.
          const key = { key: 'Process', code: 'KeyU', windowsVirtualKeyCode: 229 };
          await cdp.send('Input.dispatchKeyEvent', { type: 'rawKeyDown', ...key });
          await cdp.send('Input.imeSetComposition', { text: 'ü', selectionStart: 1, selectionEnd: 1 });
          await cdp.send('Input.dispatchKeyEvent', { type: 'keyUp', ...key });
          await page.waitForTimeout(30);
          await cdp.send('Input.insertText', { text: 'ü' });
        } else await page.keyboard.insertText('ü');
        await page.keyboard.press('Enter');
        await endTrial(page, 600);
      }
      const got: string[] = [];
      for (let r = 0; r < n; r += 1) got.push(await value(page, r, COL));
      const comp = got.filter((_, i) => i % 2 === 0);
      const plain = got.filter((_, i) => i % 2 === 1);
      console.log(`DBLKEY ime ${mode} d=${d}: composition ${comp.join(',')} insertText ${plain.join(',')}`);
      expect(got).toEqual(Array.from({ length: n }, () => '7ü'));
    });

for (const mode of ['idle', 'slow'] as const)
  for (const d of [0, 300])
    test(`Ctrl+Z ${d} ms after a double-click cancels the entry in its turn (${mode})`, async ({ page }) => {
      await openGrid(page);
      const n = 5;
      const got: string[] = [];
      for (let r = 0; r < n; r += 1) {
        const e = await cellPoint(page, r, 4);
        await page.mouse.click(e.x, e.y);
        await page.keyboard.type('x');
        await page.keyboard.press('Enter');
        const c = await cellPoint(page, r, COL);
        await page.mouse.click(c.x, c.y);
        await page.keyboard.type('7');
        await page.keyboard.press('Enter');
        await page.waitForTimeout(400);
        if (mode === 'slow' && r === 0) await slowCore(page, 40);
        await doubleClick(page, c, d);
        await page.keyboard.press('Control+z');
        // The entry is cancelled: Ready, the cell's 7 still there (not undone before the entry
        // opened, which leaves an empty cell in Edit).
        await page.waitForTimeout(500);
        const mid = await gridState(page);
        await page.keyboard.type('5');
        await page.keyboard.press('Enter');
        await endTrial(page, 800);
        got.push(`${mid.mode}:${mid.formula}|${await value(page, r, COL)}|${await value(page, r, 4)}`);
      }
      console.log(`DBLKEY ctrl+z ${mode} d=${d}: ${got.join(' ')}`);
      // Ctrl+Z in the open entry cancels it (the cell keeps 7, the entry before it stays); the 5
      // typed next replaces the cell.
      expect(got).toEqual(Array.from({ length: n }, () => 'Ready:7|5|x'));
    });

test('an edit start the core never answers is given up, with the notice, and the keys carry on', async ({
  page,
}) => {
  await openGrid(page);
  await fill(page, [['7']], 0, COL);
  await hookCore(page, 'hang');
  const t0 = Date.now();
  await doubleClick(page, await cellPoint(page, 0, COL), 0);
  await page.keyboard.type('5');
  await page.keyboard.press('Enter');
  await page.waitForTimeout(200);
  const e1 = await cellPoint(page, 0, 4);
  await page.mouse.click(e1.x, e1.y);
  await page.keyboard.type('9');
  await page.keyboard.press('Enter');
  // Nothing asks the core meanwhile (reading a cell selects it, which would move the keys).
  await expect.poll(async () => (await gridState(page)).name_box, { timeout: 10_000 }).toBe('E2');
  const took = Date.now() - t0;
  expect(await value(page, 0, 4)).toBe('9');
  // Given up as a refused edit start is: the keys typed meanwhile are an entry of their own.
  expect(await value(page, 0, COL)).toBe('5');
  const notice = page.locator('.kiln-grid__notice');
  await expect(notice).toContainText('Kiln could not finish this');
  await page.keyboard.press('ArrowDown');
  await expect.poll(async () => (await gridState(page)).name_box).toBe('C2');
  console.log(`DBLKEY hang: the keys carried on after ${took} ms`);
  expect(took).toBeLessThan(8000);
});

const tabs = (page: Page) => page.getByRole('tablist', { name: 'Sheets' }).getByRole('tab');

for (const mode of ['alone', 'after a double-click edit', 'slow'] as const)
  test(`Enter in the sheet tab's rename box leaves no entry open (${mode})`, async ({ page }) => {
    await openGrid(page);
    await fill(
      page,
      Array.from({ length: TRIALS }, () => ['7']),
      0,
      COL,
    );
    const points = [];
    for (let r = 0; r < TRIALS; r += 1) points.push(await cellPoint(page, r, COL));
    if (mode === 'slow') await slowCore(page, 40);
    const bad: string[] = [];
    for (let r = 0; r < TRIALS; r += 1) {
      const name = `S${100 + r}`;
      const p = points[r] as { x: number; y: number };
      if (mode === 'after a double-click edit') {
        await page.mouse.dblclick(p.x, p.y);
        await page.keyboard.type('3\n');
      } else await page.mouse.click(p.x, p.y);
      await tabs(page).and(page.locator('[aria-selected="true"]')).first().dblclick();
      await page.keyboard.press('Control+a');
      await page.keyboard.type(`${name}\n`);
      await page.waitForTimeout(400);
      const s = await gridState(page);
      const names = (await tabs(page).allInnerTexts()).map((t) => t.trim());
      if (!names.includes(name)) bad.push(`#${r} tabs ${JSON.stringify(names)}`);
      if (s.mode !== 'Ready') {
        bad.push(`#${r} ${s.mode} ${JSON.stringify(s.formula)} at ${s.name_box}`);
        await page.keyboard.press('Escape');
        await page.waitForTimeout(100);
      }
    }
    const cells: string[] = [];
    if (mode === 'after a double-click edit')
      for (let r = 0; r < TRIALS; r += 1) cells.push(await value(page, r, COL));
    console.log(`DBLKEY rename ${mode}: ${TRIALS} trials, ${bad.length} wrong ${bad.join(' | ')}`);
    expect(bad).toEqual([]);
    if (cells.length) expect(cells).toEqual(Array.from({ length: TRIALS }, () => '73'));
  });

test('Enter in the Name Box and the find bar leaves no entry open', async ({ page }) => {
  await openGrid(page);
  const nameBox = page.getByRole('textbox', { name: 'Name Box' });
  const bad: string[] = [];
  for (let r = 0; r < 5; r += 1) {
    await nameBox.click();
    await page.keyboard.type(`D${r + 3}\n`);
    await page.waitForTimeout(300);
    const s = await gridState(page);
    if (s.mode !== 'Ready' || s.name_box !== `D${r + 3}`) bad.push(`name box ${s.mode} ${s.name_box}`);
  }
  await page.keyboard.press('Control+f');
  const find = page.getByTestId('grid-find');
  await expect(find).toBeVisible();
  await page.keyboard.type('kiln\n');
  await page.keyboard.press('Escape');
  await page.waitForTimeout(300);
  const s = await gridState(page);
  if (s.mode !== 'Ready') bad.push(`find bar ${s.mode} ${JSON.stringify(s.formula)}`);
  expect(bad).toEqual([]);
});
