/**
 * Random crashes through the real interface, on the installable web app (the Pages artefact):
 * each seed drives random actions as a person does them (typing and Enter, double-click edits,
 * the Name Box, Tell Me for sort, Remove Duplicates, Paste Values to another sheet and Text to
 * Columns over formulas, Ctrl+D, undo and redo, new sheets and sheet switches, an entry left
 * open) and crashes the page's process, or reloads the page in the moment its cells are read,
 * 0 to 300 ms after the last one showed (at once half the time; its own signal: cells, sheet tabs,
 * the ribbon's toggles, Undo and Redo), while its writes are under way: nothing waits for the
 * window to settle. Every round compares every cell it read, both ways. Three seeds in four run on a slow disk, so those writes are
 * still going when the page goes and only the write-ahead tail holds them. What the sheet on show showed at that
 * moment, and the order of the sheets, must come back. Half the seeds work with Show Formulas
 * on, so formulas are compared as typed, column A's formulas over `RAND` included; the others
 * compare what the cells show, column A (worked out afresh) left out.
 *
 * `KILN_RANDOM_SEEDS` sets the seeds (3), `KILN_RANDOM_FIRST_SEED` the first (1). A seed that
 * ever fails gets a named case at the end. `tools/chaos-planted.sh` runs it against a build whose
 * tail drops the copies of records, and expects it to fail.
 */

import { existsSync, readFileSync } from 'node:fs';
import { fileURLToPath } from 'node:url';
import { type BrowserContext, expect, type Page, test } from '@playwright/test';
import { CORE_START, installLaunchQueue, type LaunchHandle, openIsolated, prepare } from './pwa-helpers.ts';

const SEEDS = Number(process.env.KILN_RANDOM_SEEDS ?? 3);
const FIRST = Number(process.env.KILN_RANDOM_FIRST_SEED ?? 1);

test.skip(!!process.env.KILN_E2E_BASE, 'needs a page of its own origin');
test.describe.configure({ mode: 'default', timeout: 600_000 });
test.use({ actionTimeout: 15_000 });

/** A small seeded generator (mulberry32). */
function rng(seed: number): () => number {
  let a = seed >>> 0;
  return () => {
    a = (a + 0x6d2b79f5) >>> 0;
    let t = a;
    t = Math.imul(t ^ (t >>> 15), t | 1);
    t ^= t + Math.imul(t ^ (t >>> 7), t | 61);
    return ((t ^ (t >>> 14)) >>> 0) / 4294967296;
  };
}

const bar = (page: Page) => page.getByRole('textbox', { name: 'Formula Bar' });
const nameBox = (page: Page) =>
  page
    .getByRole('combobox', { name: 'Name Box' })
    .or(page.getByRole('textbox', { name: 'Name Box' }))
    .first();
const ready = (page: Page) => page.getByText('Ready', { exact: true }).last();

/**
 * An entry an earlier action left open by accident (keys that came faster than the editor, a
 * rename's Enter that reached the sheet) is cancelled, so the next step's click does not enter it
 * behind the round's back: what is compared is what the actions did.
 */
async function closeEntry(page: Page): Promise<void> {
  if (
    !(await ready(page)
      .isVisible()
      .catch(() => false))
  ) {
    await page.keyboard.press('Escape');
    await page.waitForTimeout(100);
  }
}

/** The centre of a cell at the default zoom and scroll (`col` 0 is A, `row` 1 is the first). */
const at = (col: number, row: number) => ({ x: 97 + 64 * col, y: 289 + 20 * (row - 1) });
const ref = (col: number, row: number) => `${String.fromCharCode(65 + col)}${row}`;

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

async function goTo(page: Page, reference: string): Promise<void> {
  await answerDialogs(page);
  await nameBox(page).click();
  await nameBox(page).fill(reference);
  await nameBox(page).press('Enter');
}

async function tellMe(page: Page, command: string): Promise<void> {
  await page
    .getByRole('button', { name: /^Search/ })
    .first()
    .click();
  const search = page.getByPlaceholder('Search for a command, or type what you want to do');
  await expect(search).toBeFocused({ timeout: 10_000 });
  await search.fill(command);
  await page.keyboard.press('Enter');
  await expect(search).toBeHidden({ timeout: 10_000 });
}

/** Show Formulas is on in this round: formulas are compared as typed, column A included. */
let formulas = false;

/** What the active sheet's cells show, by name (`F8` → `7100`); column A only as formulas. */
const shown = (page: Page, thenReload = false) =>
  page.evaluate(
    ([withA, reload]) => {
      const out: Record<string, string> = {};
      // The cells on screen, empty or not: only what was on screen in both reads is compared.
      const visible: string[] = [];
      for (const cell of document.querySelectorAll('[role="grid"] [role="gridcell"]')) {
        const label = cell.getAttribute('aria-label') ?? '';
        const name = label.split(' ')[0] ?? '';
        if (name) visible.push(name);
        if (!name || (!withA && /^A\d/.test(name))) continue;
        const text = cell.textContent ?? '';
        // A Text to Columns source not split yet is a formula over RAND too.
        if (/^x,\d,\d+$/.test(text)) continue;
        if (text) out[name] = text;
      }
      // The Formula Bar as the cells were read: the text of an entry still being typed.
      const bar = document.querySelector<HTMLTextAreaElement>('[aria-label="Formula Bar"]');
      out[' bar'] = bar?.value ?? '';
      out[' visible'] = JSON.stringify(visible);
      // The sheets, in their order, and the one on show, read in the same moment.
      const tabs = [...document.querySelectorAll('[role="tablist"][aria-label="Sheets"] [role="tab"]')];
      out[' tabs'] = JSON.stringify(tabs.map((t) => (t.textContent ?? '').trim()));
      out[' active'] = (
        tabs.find((t) => t.getAttribute('aria-selected') === 'true')?.textContent ?? ''
      ).trim();
      // The page goes in the same moment the cells were read: no write gets a head start.
      if (reload) setTimeout(() => location.reload(), 0);
      return out;
    },
    [formulas, thenReload] as const,
  );

/** What the active sheet shows once it has stopped changing for a while. */
async function settled(page: Page): Promise<Record<string, string>> {
  let last = '';
  let same = 0;
  for (let i = 0; i < 100 && same < 4; i += 1) {
    const now = JSON.stringify(await shown(page));
    same = now === last ? same + 1 : 0;
    last = now;
    await page.waitForTimeout(150);
  }
  return JSON.parse(last);
}

/** A message box an action raised (a circular reference, a question): answered with OK. */
async function answerDialogs(page: Page): Promise<void> {
  for (let i = 0; i < 3; i += 1) {
    const box = page.getByRole('alertdialog').or(page.getByRole('dialog')).first();
    if (!(await box.isVisible().catch(() => false))) return;
    const ok = box.getByRole('button', { name: /^(OK|Yes|Close|Delete)$/ }).first();
    if (await ok.isVisible().catch(() => false)) await ok.click();
    else await page.keyboard.press('Escape');
    await page.waitForTimeout(200);
  }
}

const tabs = (page: Page) => page.getByRole('tablist', { name: 'Sheets' }).getByRole('tab');

async function sheetNames(page: Page): Promise<string[]> {
  return (await tabs(page).allInnerTexts()).map((s) => s.trim());
}

async function showSheet(page: Page, name: string): Promise<void> {
  await answerDialogs(page);
  await tabs(page).filter({ hasText: name }).first().click();
  await expect(page.locator(`[role="grid"][aria-label$="– ${name}"]`)).toBeAttached({ timeout: 15_000 });
}

type Action = (page: Page, r: () => number) => Promise<string>;

const pick = <T>(r: () => number, list: readonly T[]): T => list[Math.floor(r() * list.length)] as T;

const ACTIONS: [string, Action][] = [
  [
    'type',
    async (page, r) => {
      const col = 1 + Math.floor(r() * 5);
      const row = 1 + Math.floor(r() * 12);
      const value = pick(r, [String(Math.floor(r() * 1000)), `w${Math.floor(r() * 99)}`, `=G${row}+1`]);
      await onGrid(page, at(col, row));
      await page.keyboard.type(`${value}\n`);
      return `type ${value} in ${ref(col, row)}`;
    },
  ],
  [
    'name box',
    async (page, r) => {
      const target = ref(1 + Math.floor(r() * 5), 1 + Math.floor(r() * 12));
      await goTo(page, target);
      const value = `=SUM(G1:G${2 + Math.floor(r() * 9)})`;
      await page.keyboard.type(`${value}\n`);
      return `Name Box ${target}, ${value}`;
    },
  ],
  [
    'double-click',
    async (page, r) => {
      const col = 1 + Math.floor(r() * 5);
      const row = 1 + Math.floor(r() * 12);
      await onGrid(page, at(col, row), 'dblclick');
      await page.keyboard.type(`${Math.floor(r() * 9)}\n`);
      return `double-click ${ref(col, row)}`;
    },
  ],
  [
    'rand',
    async (page, r) => {
      const row = 1 + Math.floor(r() * 10);
      await onGrid(page, at(0, row));
      await page.keyboard.type(`=INT(RAND()*4)+${row}*0\n`);
      return `rand in A${row}`;
    },
  ],
  [
    'sort',
    async (page) => {
      await goTo(page, 'A1:E10');
      await tellMe(page, 'Sort A to Z');
      return 'sort A1:E10 by A';
    },
  ],
  [
    'remove duplicates',
    async (page) => {
      await goTo(page, 'A1:C10');
      await tellMe(page, 'Remove Duplicates');
      const dialog = page.getByRole('dialog', { name: 'Remove Duplicates' });
      await dialog.getByRole('button', { name: 'Unselect All' }).click();
      await dialog.getByRole('checkbox', { name: /Column A/ }).check();
      await dialog.getByRole('button', { name: 'OK' }).click();
      await page
        .getByRole('dialog')
        .getByRole('button', { name: 'OK' })
        .click({ timeout: 5_000 })
        .catch(() => undefined);
      return 'remove duplicates A1:C10 by A';
    },
  ],
  [
    'paste values to another sheet',
    async (page, r) => {
      const names = await sheetNames(page);
      const from = (await tabs(page).and(page.locator('[aria-selected="true"]')).first().innerText()).trim();
      await goTo(page, 'B1:B6');
      await page.keyboard.press('Control+c');
      const to = pick(r, names);
      await showSheet(page, to);
      const row = 1 + Math.floor(r() * 8);
      await goTo(page, `F${row}`);
      await tellMe(page, 'Paste Values');
      await showSheet(page, from);
      return `paste values B1:B6 to ${to}!F${row}`;
    },
  ],
  [
    'text to columns',
    async (page, r) => {
      const row = 1 + Math.floor(r() * 10);
      await goTo(page, `H${row}`);
      await page.keyboard.type(`="x,"&INT(RAND()*9)&",${row}"\n`);
      await goTo(page, `H${row}`);
      await tellMe(page, 'Text to Columns');
      const dialog = page.getByRole('dialog', { name: 'Convert Text to Columns' });
      await dialog.getByRole('checkbox', { name: 'Comma' }).check();
      await dialog.getByRole('button', { name: 'Finish' }).click();
      await page
        .getByRole('dialog')
        .getByRole('button', { name: 'OK' })
        .click({ timeout: 3_000 })
        .catch(() => undefined);
      return `text to columns H${row}`;
    },
  ],
  [
    'fill down',
    async (page, r) => {
      const col = 1 + Math.floor(r() * 4);
      const row = 1 + Math.floor(r() * 6);
      await goTo(page, `${ref(col, row)}:${ref(col, row + 3)}`);
      await page.keyboard.press('Control+d');
      return `fill down ${ref(col, row)}:${ref(col, row + 3)}`;
    },
  ],
  [
    'undo',
    async (page) => {
      await page.keyboard.press('Control+z');
      return 'undo';
    },
  ],
  [
    'redo',
    async (page) => {
      await page.keyboard.press('Control+y');
      return 'redo';
    },
  ],
  [
    'sheets',
    async (page, r) => {
      const names = await sheetNames(page);
      if (names.length < 3 && r() < 0.5) {
        await page.getByRole('button', { name: 'New Sheet' }).click();
        if (formulas) {
          await expect(tabs(page)).toHaveCount(names.length + 1);
          await tellMe(page, 'Show Formulas');
        }
        return 'new sheet';
      }
      const name = pick(r, names);
      await showSheet(page, name);
      return `show ${name}`;
    },
  ],
  [
    'clear',
    async (page, r) => {
      const col = 1 + Math.floor(r() * 5);
      const row = 1 + Math.floor(r() * 10);
      await goTo(page, `${ref(col, row)}:${ref(col + 1, row + 2)}`);
      await page.keyboard.press('Delete');
      return `clear ${ref(col, row)}:${ref(col + 1, row + 2)}`;
    },
  ],
  [
    'cut and paste',
    async (page, r) => {
      const row = 1 + Math.floor(r() * 8);
      await goTo(page, `B${row}:C${row + 2}`);
      await page.keyboard.press('Control+x');
      const to = `E${1 + Math.floor(r() * 10)}`;
      await goTo(page, to);
      await page.keyboard.press('Control+v');
      return `cut B${row}:C${row + 2}, paste at ${to}`;
    },
  ],
  [
    'copy and paste',
    async (page, r) => {
      const row = 1 + Math.floor(r() * 8);
      await goTo(page, `B${row}:D${row + 1}`);
      await page.keyboard.press('Control+c');
      const to = `F${1 + Math.floor(r() * 10)}`;
      await goTo(page, to);
      await page.keyboard.press('Control+v');
      return `copy B${row}:D${row + 1}, paste at ${to}`;
    },
  ],
  [
    'bold',
    async (page, r) => {
      const row = 1 + Math.floor(r() * 10);
      await goTo(page, `B${row}:E${row}`);
      await page.keyboard.press('Control+b');
      return `bold B${row}:E${row}`;
    },
  ],
  [
    'merge',
    async (page, r) => {
      const row = 12 + Math.floor(r() * 4);
      await goTo(page, `B${row}:C${row}`);
      await tellMe(page, 'Merge & Center');
      return `merge B${row}:C${row}`;
    },
  ],
  [
    'rows and columns',
    async (page, r) => {
      const which = pick(r, [
        'Insert Sheet Rows',
        'Delete Sheet Rows',
        'Insert Sheet Columns',
        'Delete Sheet Columns',
      ]);
      const at = which.endsWith('Rows')
        ? `B${2 + Math.floor(r() * 8)}`
        : `${ref(1 + Math.floor(r() * 4), 1)}`;
      await goTo(page, at);
      await tellMe(page, which);
      return `${which.toLowerCase()} at ${at}`;
    },
  ],
  [
    'rename sheet',
    async (page, r) => {
      const name = `S${Math.floor(r() * 900) + 100}`;
      await tabs(page).and(page.locator('[aria-selected="true"]')).first().dblclick();
      await page.keyboard.press('Control+a');
      await page.keyboard.type(`${name}\n`);
      await expect(tabs(page).filter({ hasText: name })).toHaveCount(1);
      return `rename the sheet ${name}`;
    },
  ],
  [
    'move or delete a sheet',
    async (page, r) => {
      const names = await sheetNames(page);
      if (names.length < 2) {
        await page.getByRole('button', { name: 'New Sheet' }).click();
        await expect(tabs(page)).toHaveCount(names.length + 1);
        return 'new sheet';
      }
      if (r() < 0.5) {
        const from = tabs(page).nth(names.length - 1);
        await from.dragTo(tabs(page).first());
        return `move ${names[names.length - 1]} first`;
      }
      const name = names[names.length - 1] as string;
      await showSheet(page, name);
      // The sheet tab's own menu (Tell me can offer the command that wants an index).
      await tabs(page).filter({ hasText: name }).first().click({ button: 'right' });
      await page.getByRole('menuitem', { name: 'Delete', exact: true }).click();
      await answerDialogs(page);
      await expect(tabs(page)).toHaveCount(names.length - 1);
      return `delete ${name}`;
    },
  ],
];

/** The process crashes (`reloaded`: the page was reloaded as the cells were read instead). */
async function crashAndRecover(
  context: BrowserContext,
  page: Page,
  maybeNothing: boolean,
  what: string,
  reloaded: boolean,
): Promise<Page | null> {
  let next = page;
  if (reloaded) {
    await page.waitForLoadState('domcontentloaded');
  } else {
    const cdp = await context.newCDPSession(page);
    const crashed = page.waitForEvent('crash');
    cdp.send('Page.crash').catch(() => undefined);
    await crashed;
    next = await context.newPage();
    await prepare(next);
    await next.goto('./#/grid');
    await page.close().catch(() => undefined);
  }
  const row = next.getByTestId('document-recovery').locator('[data-recovery]');
  if (maybeNothing) {
    await next.waitForTimeout(5000);
    if ((await row.count()) === 0) return null;
  }
  await expect(row, what).toHaveCount(1, { timeout: CORE_START });
  await row.getByRole('button', { name: 'Open' }).click();
  await expect(row).toHaveCount(0, { timeout: CORE_START });
  await expect(ready(next)).toBeVisible({ timeout: CORE_START });
  return next;
}

/**
 * A slow disk: every write to the site's private file system waits `ms` before it starts, so the
 * writes of the last actions are still under way when the page goes (the tail must hold them).
 */
async function slowDisk(context: BrowserContext, ms: number): Promise<void> {
  await context.addInitScript((delay) => {
    const proto = (globalThis as { FileSystemFileHandle?: { prototype: { createWritable: unknown } } })
      .FileSystemFileHandle?.prototype;
    if (!proto || delay <= 0) return;
    const open = proto.createWritable as (this: unknown, ...a: unknown[]) => Promise<unknown>;
    proto.createWritable = async function (this: unknown, ...a: unknown[]) {
      await new Promise((resolve) => setTimeout(resolve, delay));
      return open.apply(this, a);
    };
  }, ms);
}

/** The cells of what [`shown`] read, without the bar, the tabs and the sheet on show. */
function cells(read: Record<string, string>): Record<string, string> {
  const out = { ...read };
  for (const k of [' bar', ' tabs', ' active', ' visible']) delete out[k];
  return out;
}

/** What the screen shows, in the parts an action's signal is read from. */
interface Signal {
  /** Each cell's text by name, as the accessibility tree has it. */
  cells: Record<string, string>;
  /** The sheet tabs and the one on show. */
  tabs: string;
  /** Every pressed toggle of the ribbon (Bold, Merge & Center: the active cell's format). */
  toggles: string;
  /** Whether Undo and Redo are available. */
  history: string;
  /** The cells read are those of the sheet on show (a new tab can come a frame first). */
  whole: boolean;
}

const signal = (page: Page): Promise<Signal> =>
  page.evaluate(() => {
    const cells: Record<string, string> = {};
    for (const cell of document.querySelectorAll('[role="grid"] [role="gridcell"]')) {
      const label = cell.getAttribute('aria-label') ?? '';
      const name = label.split(' ')[0] ?? '';
      if (name) cells[name] = cell.textContent ?? '';
    }
    const tabs = [...document.querySelectorAll('[role="tablist"][aria-label="Sheets"] [role="tab"]')];
    const toggles = [...document.querySelectorAll('[aria-pressed]')]
      .map((b) => `${b.getAttribute('aria-label') ?? b.textContent}:${b.getAttribute('aria-pressed')}`)
      .join(' ');
    const history = ['Undo', 'Redo']
      .map(
        (n) => `${n}:${document.querySelector(`[aria-label="${n}"]`)?.getAttribute('aria-disabled') ?? ''}`,
      )
      .join(' ');
    const active = tabs.find((t) => t.getAttribute('aria-selected') === 'true')?.textContent?.trim();
    const grid = document.querySelector('[role="grid"]')?.getAttribute('aria-label') ?? '';
    return {
      cells,
      tabs: tabs.map((t) => `${t.textContent}:${t.getAttribute('aria-selected')}`).join(' '),
      toggles,
      history,
      whole: !!active && grid.endsWith(`– ${active}`),
    };
  });

/**
 * Whether the last action has shown, by its own signal: an action on cells when one of its cells
 * shows something else; Bold and Merge & Center by the ribbon's toggles; a sheet's name, a new,
 * moved or deleted sheet, or another sheet on show by the tabs; anything else (undo, redo, a sort,
 * rows and columns) by any of them.
 */
function shownBy(what: string, before: Signal, now: Signal): boolean {
  if (!now.whole) return false;
  if (/^(bold|merge)/.test(what)) return now.toggles !== before.toggles;
  if (/^(rename|new sheet|show |move |delete S)/.test(what)) return now.tabs !== before.tabs;
  const cells = footprint(what);
  if (cells && !/^(sort|remove|undo|redo)/.test(what))
    return [...cells].some((k) => (now.cells[k] ?? '') !== (before.cells[k] ?? ''));
  return JSON.stringify(now) !== JSON.stringify(before);
}

/**
 * Waits for the last action's own signal (see {@link shownBy}), not a fixed time, in a frame that
 * is whole: the crash then comes 0 to 300 ms after the action showed, while its writes are under
 * way. An action whose signal does not come in two seconds (the slowest shows in about a third
 * of one) changed nothing on the screen (a redo with nothing to redo, a value typed over the same
 * value): the state before it is the state after it, and the crash is as hot. Returns how long
 * the action took to show (or `null`).
 */
async function seen(page: Page, what: string, before: Signal, ms = 2000): Promise<number | null> {
  const start = Date.now();
  while (Date.now() - start < ms) {
    const now = await signal(page);
    // The signal, in a whole frame (`shownBy` asks for one): the crash is as hot as it can be.
    if (shownBy(what, before, now)) return Date.now() - start;
    await page.waitForTimeout(25);
  }
  return null;
}

/** The cells an action acts on (its references, ranges expanded), or the whole sheet (`null`). */
function footprint(what: string): Set<string> | null {
  const refs = [...what.matchAll(/\b([A-Z])(\d+)(?::([A-Z])(\d+))?\b/g)];
  if (refs.length === 0 || /sort|duplicates|rows|columns|undo|redo|sheet/.test(what)) return null;
  const out = new Set<string>();
  for (const [, c0, r0, c1, r1] of refs) {
    const [a, b] = [(c0 as string).charCodeAt(0), (c1 ?? (c0 as string)).charCodeAt(0)];
    for (let c = a; c <= b; c += 1)
      for (let r = Number(r0); r <= Number(r1 ?? r0); r += 1) out.add(`${String.fromCharCode(c)}${r}`);
  }
  return out;
}

/** A workbook over 256 KB, so a round has a large first checkpoint under way. */
const LARGE = fileURLToPath(new URL('../../fixtures/external/xlsx/poi-65016.xlsx', import.meta.url));

async function round(context: BrowserContext, first: Page, seed: number): Promise<void> {
  const r = rng(seed);
  let page = first;
  // Three seeds in four on a slow disk (150 to 500 ms before each write starts).
  const slow = r() < 0.75 ? 150 + Math.floor(r() * 350) : 0;
  await slowDisk(context, slow);
  // One seed in five starts from a workbook over 256 KB.
  const large = r() < 0.2 && existsSync(LARGE);
  if (large) await installLaunchQueue(page);
  await openIsolated(page);
  if (large) {
    await page.evaluate(
      ([name, bytes]) => (window as unknown as LaunchHandle).__launch(name as string, bytes as number[]),
      ['Large.xlsx', Array.from(readFileSync(LARGE))] as const,
    );
    await expect(page).toHaveURL(/#\/grid\?doc=/, { timeout: CORE_START });
  } else {
    await page.goto('./#/grid');
  }
  await expect(bar(page)).toBeVisible({ timeout: CORE_START });
  formulas = r() < 0.5;
  const done: string[] = large ? ['open a workbook of 308 KB'] : [];
  if (formulas) {
    await tellMe(page, 'Show Formulas');
    done.push('show formulas');
  }
  let offered = false;
  let openText = '';
  let offeredText = '';
  // Two to four crashes a seed, each a round of its own.
  const crashes = 2 + Math.floor(r() * 3);
  for (let c = 0; c < crashes; c += 1) {
    // Put it back, when the last crash left an entry to offer.
    if (offered) {
      await page
        .getByTestId('storage-notice')
        .filter({ hasText: offeredText })
        .getByRole('button', { name: 'Put it back' })
        .click();
      await expect(bar(page)).toHaveValue(offeredText, { timeout: 15_000 });
      await page.locator('[data-kiln-grid-proxy]').focus();
      await page.keyboard.press('Enter');
      done.push('put it back');
      offered = false;
    }
    const steps = 2 + Math.floor(r() * 8);
    // The signal of the screen before the last action, once it had settled.
    let signalBefore: Signal | null = null;
    for (let s = 0; s < steps; s += 1) {
      const [, act] = pick(r, ACTIONS);
      if (s === steps - 1) {
        await answerDialogs(page);
        await closeEntry(page);
        await settled(page);
        signalBefore = await signal(page);
      }
      done.push(await act(page, r));
      if (s < steps - 1) {
        await page.waitForTimeout(150);
        await answerDialogs(page);
      }
    }
    // The last action reaches the screen (or changes nothing there).
    const last = done[done.length - 1] ?? '';
    const showedIn = signalBefore ? await seen(page, last, signalBefore) : null;
    // An entry left open, now and then: it is offered back, not reopened, and not compared.
    const open = r() < 0.25;
    if (open) {
      await closeEntry(page);
      await onGrid(page, at(3, 14));
      // Other words each round: an entry the same as what its cell holds is no entry to offer.
      openText = `left open ${c + 1}`;
      await page.keyboard.type(openText);
      done.push('an entry left open');
    }
    // 0 to 300 ms after the last action (at once, half the time), writes under way: what shows,
    // then the process goes, or the page is reloaded in the moment the cells are read.
    const reloaded = r() < 0.5;
    const wait = r() < 0.5 ? 0 : Math.floor(r() * 300);
    if (wait) await page.waitForTimeout(wait);
    const read = await shown(page, reloaded);
    const names = JSON.parse(read[' tabs'] ?? '[]') as string[];
    const active = read[' active'] ?? '';
    // An entry left open is offered back once its text has reached the screen.
    const typed = read[' bar'] === openText;
    const before = cells(read);
    const visibleBefore = new Set(JSON.parse(read[' visible'] ?? '[]') as string[]);
    if (open) {
      delete before.D14;
    }
    const what = `seed ${seed} (${formulas ? 'formulas' : 'values'}, disk +${slow} ms, ${reloaded ? 'reload' : 'crash'} after ${wait} ms): ${done.join('; ')}`;
    console.log(
      `CHAOS ${what}; ${Object.keys(before).length} cells shown; the last action ${showedIn === null ? 'changed nothing on screen' : `showed in ${showedIn} ms`}; compared strictly`,
    );
    const nothing = names.length === 1 && Object.keys(before).length === 0 && !large;
    const next = await crashAndRecover(context, page, nothing, what, reloaded);
    // Nothing was ever entered: there is nothing to recover, and nothing is listed.
    if (!next) return;
    page = next;
    // The sheets in their order; a sheet added as the process went may follow them.
    const back = await sheetNames(page);
    // Soft: a round that fails is counted, and the seed goes on to its next round.
    expect.soft(back.slice(0, names.length), `${what}: the sheets`).toEqual(names);
    expect.soft(back.length - names.length, `${what}: the sheets`).toBeLessThanOrEqual(1);
    await showSheet(page, active);
    // What came back is what showed, both ways, in every round: no cell lost, none come back that
    // showed empty. The last action had shown before the page went (its signal, above), so the
    // state after it is the one that counts; one whose signal never came changed nothing, and the
    // state before it is the same. A difference is named inside or outside that action's cells.
    // (Review 4's N6, an undo the journal never hears of, does not show here: a change no record
    // holds asks for a small checkpoint at once.)
    // The window scrolled as it was: the corner cells of what showed, last to first.
    const corners = [...visibleBefore];
    if (corners.length > 1) {
      await goTo(page, corners[corners.length - 1] as string);
      await goTo(page, corners[0] as string);
    }
    const cellsOf = footprint(last);
    let differ: Record<string, [string | undefined, string | undefined]> = {};
    await expect.soft
      .poll(
        async () => {
          const raw = await settled(page);
          const visibleAfter = new Set(JSON.parse(raw[' visible'] ?? '[]') as string[]);
          const after = cells(raw);
          if (open) delete after.D14;
          differ = {};
          // The cells on screen in both reads: a recovered window can be scrolled otherwise.
          for (const k of new Set([...Object.keys(before), ...Object.keys(after)]))
            if (visibleBefore.has(k) && visibleAfter.has(k) && after[k] !== before[k]) {
              const where = cellsOf === null || cellsOf.has(k) ? 'in the last action' : 'elsewhere';
              differ[`${k} (${where})`] = [before[k], after[k]];
            }
          return differ;
        },
        { timeout: 20_000, message: what },
      )
      .toEqual({});
    // No stop is named: every change on screen came back. (The large workbook's first changes
    // before its first copy is written are the manual's named case.)
    const stops = page.getByTestId('storage-notice').filter({ hasText: 'is back as it was before' });
    if (!large || c > 0) await expect.soft(stops, `${what}: a stop`).toHaveCount(0);
    if (open && typed) {
      await expect(page.getByTestId('storage-notice').filter({ hasText: openText }), what).toBeVisible();
      offered = true;
      offeredText = openText;
    }
  }
}

for (let seed = FIRST; seed < FIRST + SEEDS; seed += 1)
  test(`Grid: random actions and a crash at a random moment, seed ${seed}`, async ({ page, context }) => {
    await round(context, page, seed);
  });

// Seeds that failed once, kept as cases of their own.
for (const seed of [7, 8, 11, 25, 33, 36])
  test(`Grid: random actions and a crash, kept seed ${seed}`, async ({ page, context }) => {
    await round(context, page, seed);
  });
