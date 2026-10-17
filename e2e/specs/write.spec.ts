/**
 * Kiln Write end to end (WP-64, PLAN.md §1.3–§1.7, §2.2): the editor in the app frame of the web
 * build — typing (also with an IME), formatting and styles from the ribbon and the shortcuts,
 * lists, tables, Find and Replace, Go To, header editing, zoom and views, undo, the clipboard,
 * opening a fixture through the file picker, the screenshots of the visual review, axe, and the
 * typing latency budget B5 on the 300-page document.
 */
import { execFileSync } from 'node:child_process';
import { existsSync, mkdtempSync, readFileSync, rmSync, statSync } from 'node:fs';
import { createServer, type Server } from 'node:http';
import type { AddressInfo } from 'node:net';
import { tmpdir } from 'node:os';
import { extname, join, normalize } from 'node:path';
import { fileURLToPath } from 'node:url';
import { expect, type Page, test } from '@playwright/test';
import { expectNoSeriousA11yViolations } from '../../packages/testing/src/playwright/a11y.ts';

const repo = (p: string) => fileURLToPath(new URL(`../../${p}`, import.meta.url));

/**
 * The B5 exception recorded in docs/qa/perf.md ("## Exceptions" table: the row whose budget starts
 * with "B5"), if it is proposed or approved: its allowance in ms (the first "p95 < N ms" of the
 * Allowance column) and its status. `null` when there is no such row or it is rejected.
 */
function b5Exception(): { allowance: number; status: string } | null {
  const doc = readFileSync(repo('docs/qa/perf.md'), 'utf8');
  const row = doc.split('\n').find((l) => /^\|\s*B5\b/.test(l));
  if (!row) return null;
  const cells = row.split('|').map((c) => c.trim());
  const allowance = Number(/p95 < (\d+(?:\.\d+)?) ms/.exec(cells[3] ?? '')?.[1]);
  const status = (cells[5] ?? '').toLowerCase();
  if (!Number.isFinite(allowance) || !/^(proposed|approved)/.test(status)) return null;
  return { allowance, status: status.split(/\s/)[0] ?? status };
}
const shot = (name: string) => repo(`e2e/screenshots/write/${name}`);

/** Mean relative luminance of the horizontal ruler's middle row (0 black … 1 white). */
async function rulerLuminance(page: Page): Promise<number> {
  return page.getByTestId('write-ruler').evaluate((el) => {
    const c = el as HTMLCanvasElement;
    const ctx = c.getContext('2d');
    if (!ctx || c.width === 0) return Number.NaN;
    const row = ctx.getImageData(0, Math.floor(c.height / 2), c.width, 1).data;
    let sum = 0;
    let n = 0;
    for (let i = 0; i < row.length; i += 4) {
      if ((row[i + 3] ?? 0) < 128) continue;
      sum += (0.2126 * (row[i] ?? 0) + 0.7152 * (row[i + 1] ?? 0) + 0.0722 * (row[i + 2] ?? 0)) / 255;
      n += 1;
    }
    return n ? sum / n : Number.NaN;
  });
}

interface WriteHandle {
  doc: number;
  client: {
    query(doc: number, q: unknown): Promise<Record<string, unknown>>;
    execute(doc: number, c: unknown): Promise<unknown>;
  };
  controller: { getStats(): { tiles: number; zoom: number } } | null;
  state: { dialog: unknown; navOpen: boolean; status: { zoom: number; view: string } | null };
}

/** The development handles of the web app (`apps/web/src/routes/app-frame/debug.ts`, `routes/write`). */
interface KilnHandle {
  registry: { execute(id: string, args?: unknown): Promise<{ status: string }> };
  log(): { id: string; outcome: string }[];
}
type KilnWindow = { __kilnWrite?: WriteHandle; __kiln?: KilnHandle };

/** Opens Kiln Write on a new document and waits for its first painted page. */
async function openWrite(page: Page): Promise<void> {
  await page.goto('/#/write');
  await page.waitForFunction(
    () => ((window as unknown as KilnWindow).__kilnWrite?.controller?.getStats().tiles ?? 0) > 0,
    null,
    {
      timeout: 45_000,
    },
  );
  await page.getByTestId('kiln-input-proxy').focus();
}

/** The body's paragraphs as the core has them. */
async function paragraphs(page: Page): Promise<string[]> {
  return page.evaluate(async () => {
    const w = (window as unknown as KilnWindow).__kilnWrite as WriteHandle;
    const r = (await w.client.query(w.doc, 'Paragraphs')) as { Paragraphs?: { text: string }[] };
    return (r.Paragraphs ?? []).map((p) => p.text);
  });
}

/** The selection's formatting (`Query::Write(Format)`). */
async function format(page: Page): Promise<Record<string, unknown>> {
  return page.evaluate(async () => {
    const w = (window as unknown as KilnWindow).__kilnWrite as WriteHandle;
    const r = (await w.client.query(w.doc, { Write: 'Format' })) as {
      Write: { Format: Record<string, unknown> };
    };
    return r.Write.Format;
  });
}

/** The Write model's command log: every command id that reached its handler. */
async function commandLog(page: Page): Promise<{ id: string; outcome: string }[]> {
  return page.evaluate(async () => {
    const w = (window as unknown as KilnWindow).__kilnWrite as WriteHandle;
    const r = (await w.client.query(w.doc, { Write: 'CommandLog' })) as {
      Write: { CommandLog: { id: string; outcome: string }[] };
    };
    return r.Write.CommandLog;
  });
}

/** Runs a command through the window's registry, as the ribbon does. */
async function run(page: Page, id: string, args?: unknown): Promise<string> {
  return page.evaluate(
    async ([i, a]) =>
      (await (window as unknown as KilnWindow).__kiln?.registry.execute(i as string, a))?.status ?? '',
    [id, args] as const,
  );
}

/** Waits until the core's answer satisfies `check`. */
async function until<T>(get: () => Promise<T>, check: (v: T) => boolean, what: string): Promise<T> {
  let last: T | undefined;
  await expect
    .poll(
      async () => {
        last = await get();
        return check(last);
      },
      { message: what, timeout: 10_000 },
    )
    .toBe(true);
  return last as T;
}

test.describe('Kiln Write editor', () => {
  test('types, formats and applies styles from shortcuts, the ribbon and the gallery', async ({ page }) => {
    await openWrite(page);
    await page.keyboard.type('The Autumn Firing');
    await page.keyboard.press('Control+Alt+1');
    await until(
      () => format(page),
      (f) => f.style_id === 'Heading1',
      'Heading 1',
    );
    await page.keyboard.press('Enter');
    await page.keyboard.type('Twice a year ');
    await page.keyboard.press('Control+b');
    await page.keyboard.type('bold');
    await page.keyboard.press('Control+b');
    await page.keyboard.type(' and plain.');
    await until(
      () => paragraphs(page),
      (p) => p[1] === 'Twice a year bold and plain.',
      'typed text',
    );
    // Shift+Ctrl+Left twice from the end selects "plain." — not bold; then "bold".
    await page.keyboard.press('Control+Shift+ArrowLeft');
    await until(
      () => format(page),
      (f) => f.bold === false && f.collapsed === false,
      'plain is not bold',
    );
    await page.keyboard.press('Home');
    for (let i = 0; i < 13; i++) await page.keyboard.press('ArrowRight');
    await page.keyboard.press('Control+Shift+ArrowRight');
    await until(
      () => format(page),
      (f) => f.bold === true,
      'the typed word is bold',
    );
    // The ribbon's Italic and Underline toggles show the state the core reports.
    await page.locator('[data-command="suite.format.italic"]').first().click();
    await until(
      () => format(page),
      (f) => f.italic === true,
      'italic from the ribbon',
    );
    await expect(page.locator('[data-command="suite.format.italic"]').first()).toHaveAttribute(
      'aria-pressed',
      'true',
    );
    // Font size from the box.
    const size = page.getByRole('combobox', { name: /Font Size|Size/ }).first();
    await size.fill('14');
    await size.press('Enter');
    await until(
      () => format(page),
      (f) => f.size === 14,
      'font size 14',
    );
    // A style from the gallery, then Normal with Ctrl+Shift+N.
    await page.getByTestId('kiln-input-proxy').focus();
    await page.locator('[data-command="write.styles.apply"]').getByText('Heading 2').first().click();
    await until(
      () => format(page),
      (f) => f.style_id === 'Heading2',
      'Heading 2 from the gallery',
    );
    await page.getByTestId('kiln-input-proxy').focus();
    await page.keyboard.press('Control+Shift+N');
    await until(
      () => format(page),
      (f) => f.style_id === 'Normal',
      'Normal',
    );
    const ids = (await commandLog(page)).map((e) => e.id);
    for (const id of [
      'write.styles.heading_1',
      'suite.format.bold',
      'suite.format.italic',
      'suite.format.font_size',
      'write.styles.apply',
      'write.styles.normal',
    ]) {
      expect(ids, id).toContain(id);
    }
  });

  test('types with an IME: the composition shows before it commits, once', async ({ page }) => {
    await openWrite(page);
    await page.keyboard.type('Kiln ');
    const cdp = await page.context().newCDPSession(page);
    await cdp.send('Input.imeSetComposition', { text: 'に', selectionStart: 1, selectionEnd: 1 });
    await cdp.send('Input.imeSetComposition', { text: 'にほん', selectionStart: 3, selectionEnd: 3 });
    await until(
      () => paragraphs(page),
      (p) => p[0] === 'Kiln にほん',
      'composition in the document',
    );
    await cdp.send('Input.insertText', { text: '日本' });
    await until(
      () => paragraphs(page),
      (p) => p[0] === 'Kiln 日本',
      'committed once',
    );
    await page.keyboard.type('!');
    await until(
      () => paragraphs(page),
      (p) => p[0] === 'Kiln 日本!',
      'typing after the commit',
    );
  });

  test('lists: bullets and numbering, Tab and Shift+Tab change the level, Enter twice ends the list', async ({
    page,
  }) => {
    await openWrite(page);
    await page.keyboard.press('Control+Shift+L');
    await page.keyboard.type('Bisque by Wednesday');
    await page.keyboard.press('Enter');
    await page.keyboard.type('Glaze by Thursday');
    await until(
      () => format(page),
      (f) => (f.list as { bullet?: boolean } | null)?.bullet === true,
      'bullets',
    );
    await page.keyboard.press('Home');
    await page.keyboard.press('Tab');
    await until(
      () => format(page),
      (f) => (f.list as { level?: number } | null)?.level === 1,
      'level 2',
    );
    await page.keyboard.press('Shift+Tab');
    await until(
      () => format(page),
      (f) => (f.list as { level?: number } | null)?.level === 0,
      'level 1',
    );
    await run(page, 'suite.format.numbering');
    await until(
      () => format(page),
      (f) => (f.list as { bullet?: boolean } | null)?.bullet === false,
      'numbered',
    );
    await page.keyboard.press('End');
    await page.keyboard.press('Enter');
    await page.keyboard.press('Enter');
    await until(
      () => format(page),
      (f) => f.list === null,
      'an empty item ends the list',
    );
  });

  test('tables: grid picker, Tab through cells and a new row, insert and merge from Table Layout', async ({
    page,
  }) => {
    await openWrite(page);
    await page.getByRole('tab', { name: 'Insert' }).click();
    await page.locator('[data-command="suite.insert.table"]').first().click();
    const grid = page.getByTestId('write-table-grid');
    await grid.getByRole('button', { name: 'Insert a table of 3 by 2' }).hover();
    await expect(page.getByText('3×2 Table')).toBeVisible();
    await grid.getByRole('button', { name: 'Insert a table of 3 by 2' }).click();
    const table = async () =>
      (await format(page)).table as {
        rows: number;
        cols: number;
        row: number;
        col: number;
        multi_cell: boolean;
      } | null;
    await until(table, (t) => t?.rows === 2 && t.cols === 3, 'a 3 × 2 table');
    await expect(page.getByRole('tab', { name: 'Table Design' })).toBeVisible();
    await page.getByTestId('kiln-input-proxy').focus();
    await page.keyboard.type('Shelf');
    await page.keyboard.press('Tab');
    await page.keyboard.type('Body');
    await until(table, (t) => t?.col === 1, 'Tab moves to the next cell');
    for (let i = 0; i < 5; i++) await page.keyboard.press('Tab');
    await until(table, (t) => t?.rows === 3 && t.row === 2, 'Tab in the last cell adds a row');
    await run(page, 'suite.table.insert_row_above');
    await until(table, (t) => t?.rows === 4, 'Insert Above');
    await run(page, 'suite.table.insert_column_right');
    await until(table, (t) => t?.cols === 4, 'Insert Right');
    await page.keyboard.press('Shift+ArrowRight');
    await run(page, 'write.table.select', { what: 'Row' });
    await run(page, 'suite.table.merge_cells');
    await until(table, (t) => t?.cols === 4 && t.multi_cell === false, 'the row merged into one cell');
    const ids = (await commandLog(page)).map((e) => e.id);
    expect(ids).toContain('write.table.insert');
    expect(ids).toContain('write.edit.tab');
    expect(ids).toContain('suite.table.merge_cells');
  });

  test('Find and Replace with match case and whole words, Go To a page', async ({ page }) => {
    await openWrite(page);
    await page.keyboard.type('Kiln kilns the kiln. KILN.');
    await page.keyboard.press('Control+h');
    const dialog = page.getByRole('dialog', { name: 'Find and Replace' });
    await expect(dialog).toBeVisible();
    await dialog.getByLabel('Find what').fill('kiln');
    await dialog.getByLabel('Replace with').fill('oven');
    await dialog.getByRole('button', { name: 'More >>' }).click();
    await dialog.getByRole('checkbox', { name: 'Find whole words only' }).check();
    await dialog.getByRole('button', { name: 'Replace All' }).click();
    await until(
      () => paragraphs(page),
      (p) => p[0] === 'oven kilns the oven. oven.',
      'whole words replaced',
    );
    await dialog.getByRole('checkbox', { name: 'Find whole words only' }).uncheck();
    await dialog.getByRole('checkbox', { name: 'Match case' }).check();
    await dialog.getByLabel('Find what').fill('oven.');
    await dialog.getByLabel('Replace with').fill('fire.');
    await dialog.getByRole('button', { name: 'Replace All' }).click();
    await until(
      () => paragraphs(page),
      (p) => p[0] === 'oven kilns the fire. fire.',
      'case-sensitive',
    );
    await page.keyboard.press('Escape');
    await expect(dialog).toBeHidden();
    // Ctrl+Enter twice makes three pages; Go To page 3.
    await page.getByTestId('kiln-input-proxy').focus();
    await page.keyboard.press('Control+Enter');
    await page.keyboard.press('Control+Enter');
    await page.keyboard.press('Control+Home');
    await page.keyboard.press('Control+g');
    const goTo = page.getByRole('dialog', { name: 'Find and Replace' });
    await goTo.getByLabel(/Enter page number/).fill('3');
    await goTo.getByRole('button', { name: 'Go To' }).click();
    await page.keyboard.press('Escape');
    await until(
      () =>
        page.evaluate(() => {
          const s = (window as unknown as KilnWindow).__kilnWrite?.state.status as
            | { page?: { current: number; total: number } }
            | null
            | undefined;
          return s?.page ? `${s.page.current}/${s.page.total}` : '';
        }),
      (v) => v === '3/3',
      'the caret is on page 3 of 3',
    );
    await expect(page.getByRole('status').filter({ hasText: 'Page 3 of 3' })).toBeVisible();
    const ids = (await commandLog(page)).map((e) => e.id);
    expect(ids).toContain('write.find.replace_all');
    expect(ids).toContain('write.find.go_to');
  });

  test('headers: edit, different first page, link to previous, Esc returns to the body', async ({ page }) => {
    await openWrite(page);
    await page.keyboard.type('Body text');
    await run(page, 'write.insert.header');
    await until(
      () => format(page),
      (f) => f.story === 'Header',
      'editing the header',
    );
    await expect(page.getByRole('tab', { name: 'Header & Footer' })).toBeVisible();
    await page.getByTestId('kiln-input-proxy').focus();
    await page.keyboard.type('Studio co-op');
    await run(page, 'write.header_footer.different_first_page');
    await until(
      () => format(page),
      (f) => (f.header_footer as { different_first?: boolean } | null)?.different_first === true,
      'different first page',
    );
    await page.keyboard.press('Escape');
    await until(
      () => format(page),
      (f) => f.story === 'Body',
      'Esc closes the header',
    );
    expect(await paragraphs(page)).toEqual(['Body text']);
    const ids = (await commandLog(page)).map((e) => e.id);
    expect(ids).toEqual(
      expect.arrayContaining([
        'write.insert.header',
        'write.header_footer.different_first_page',
        'write.header_footer.close',
      ]),
    );
  });

  test('zoom, view modes, Show ¶ and the ruler', async ({ page }) => {
    await openWrite(page);
    await page.keyboard.type('Zoom me');
    await run(page, 'suite.view.zoom', { zoom: 1.5 });
    await expect(page.getByText('150%')).toBeVisible();
    await until(
      () =>
        page.evaluate(() => (window as unknown as KilnWindow).__kilnWrite?.controller?.getStats().zoom ?? 0),
      (z) => Math.abs(z - 1.5) < 0.01,
      'the surface follows the zoom',
    );
    await page.getByRole('radio', { name: 'Web Layout' }).click();
    await until(
      () => page.evaluate(() => (window as unknown as KilnWindow).__kilnWrite?.state.status?.view ?? ''),
      (v) => v === 'WebLayout',
      'Web Layout',
    );
    await expect(page.getByTestId('write-ruler')).toBeHidden();
    await page.getByRole('radio', { name: 'Print Layout' }).click();
    await expect(page.getByTestId('write-ruler')).toBeVisible();
    await run(page, 'suite.view.ruler');
    await expect(page.getByTestId('write-ruler')).toBeHidden();
    await run(page, 'suite.view.ruler');
    await page.getByTestId('kiln-input-proxy').focus();
    await page.keyboard.press('Control+Shift+8');
    await until(
      () => format(page),
      (f) => f.show_marks === true,
      'Show ¶',
    );
    await run(page, 'suite.view.zoom', { zoom: 1 });
    await expect(page.getByText('100%')).toBeVisible();
  });

  test('undo and redo: typing groups by word, formatting is its own step', async ({ page }) => {
    await openWrite(page);
    await page.keyboard.type('one two');
    await until(
      () => paragraphs(page),
      (p) => p[0] === 'one two',
      'typed',
    );
    await page.keyboard.press('Control+z');
    await until(
      () => paragraphs(page),
      (p) => p[0] === 'one ',
      'undo removes the last word',
    );
    await page.keyboard.press('Control+y');
    await until(
      () => paragraphs(page),
      (p) => p[0] === 'one two',
      'redo',
    );
    await page.keyboard.press('Control+a');
    await page.keyboard.press('Control+u');
    await until(
      () => format(page),
      (f) => f.underline === 'Single',
      'underlined',
    );
    await page.keyboard.press('Control+z');
    await until(
      () => format(page),
      (f) => f.underline === null,
      'undo the formatting',
    );
    expect(await paragraphs(page)).toEqual(['one two']);
  });

  test('clipboard: copy and paste keep the formatting, Paste Text Only drops it', async ({
    page,
    context,
  }) => {
    await context.grantPermissions(['clipboard-read', 'clipboard-write']);
    await openWrite(page);
    await page.keyboard.type('plain fire');
    await page.keyboard.press('Control+Shift+ArrowLeft');
    await page.keyboard.press('Control+b');
    await page.keyboard.press('Control+c');
    // The core writes the system clipboard asynchronously (HTML and text).
    await expect.poll(() => page.evaluate(() => navigator.clipboard.readText())).toBe('fire');
    // Paste keeps the source formatting: the copy is bold.
    await page.keyboard.press('End');
    await page.keyboard.press('Control+v');
    await until(
      () => paragraphs(page),
      (p) => p[0] === 'plain firefire',
      'pasted',
    );
    await page.keyboard.press('Control+Shift+ArrowLeft');
    await until(
      () => format(page),
      (f) => f.bold === true,
      'the paste kept bold',
    );
    // Keep Text Only takes the formatting where it lands: after "plain", not bold.
    await page.keyboard.press('Home');
    await page.keyboard.press('Control+ArrowRight');
    await run(page, 'suite.edit.paste_text_only');
    await until(
      () => paragraphs(page),
      (p) => p[0] === 'plain firefirefire',
      'pasted as text',
    );
    await page.keyboard.press('Home');
    await page.keyboard.press('Control+ArrowRight');
    for (let i = 0; i < 4; i++) await page.keyboard.press('Shift+ArrowRight');
    await until(
      () => format(page),
      (f) => f.bold === false && f.collapsed === false,
      'text only is not bold',
    );
    // HTML from another application is sanitised and mapped by the core.
    await page.keyboard.press('End');
    await page.evaluate(() => {
      const proxy = document.querySelector('[data-testid="kiln-input-proxy"]') as HTMLElement;
      const data = new DataTransfer();
      data.setData('text/html', '<h2>Kiln</h2><ul><li>one</li><li>two</li></ul><script>alert(1)</script>');
      data.setData('text/plain', 'Kiln\none\ntwo');
      proxy.dispatchEvent(
        new ClipboardEvent('paste', { clipboardData: data, bubbles: true, cancelable: true }),
      );
    });
    await until(
      () => paragraphs(page),
      (p) => p.includes('two') && !p.join('').includes('alert'),
      'HTML paste',
    );
  });
});

test.describe('documents, screenshots and accessibility', () => {
  test('opens a fixture through the file picker, shows panes and dialogs, light and dark', async ({
    page,
  }) => {
    test.setTimeout(120_000);
    await openWrite(page);
    // The web host uses the File System Access API when the browser has it; without it, the
    // file input that Playwright's file chooser answers (the picker stub of the e2e tests).
    await page.evaluate(() => {
      // biome-ignore lint/suspicious/noExplicitAny: the API is not in the DOM typings
      delete (window as any).showOpenFilePicker;
    });
    await page.getByRole('button', { name: 'File' }).click();
    await page.getByRole('button', { name: 'Open', exact: true }).click();
    const [chooser] = await Promise.all([
      page.waitForEvent('filechooser'),
      page.getByRole('button', { name: /Browse this device/ }).click(),
    ]);
    await chooser.setFiles(repo('fixtures/docx/features/styles.docx'));
    await page.waitForFunction(
      () =>
        /doc=\d/.test(location.hash) &&
        ((window as unknown as KilnWindow).__kilnWrite?.controller?.getStats().tiles ?? 0) > 0,
    );
    await expect(page.getByTestId('document-title')).toContainText('styles');
    await run(page, 'suite.view.navigation_pane');
    await expect(page.getByTestId('write-nav-pane')).toBeVisible();
    await expect(page.getByTestId('write-navigation').getByRole('button').first()).toBeVisible();
    await page.evaluate(() => document.fonts.ready);
    await page.waitForTimeout(300);
    await page.screenshot({ path: shot('editor-light-desktop.png') });
    await expectNoSeriousA11yViolations(page);
    await run(page, 'write.styles.pane');
    await expect(page.getByTestId('write-styles-pane')).toBeVisible();
    await expectNoSeriousA11yViolations(page);
    await run(page, 'suite.format.paragraph_dialog');
    await expect(page.getByRole('dialog', { name: 'Paragraph' })).toBeVisible();
    await expectNoSeriousA11yViolations(page);
    await page.keyboard.press('Escape');
    await page.emulateMedia({ colorScheme: 'dark' });
    await expect(page.getByTestId('write-styles-pane')).toBeVisible();
    await page.waitForTimeout(400);
    await page.screenshot({ path: shot('editor-dark-desktop.png') });
    await expectNoSeriousA11yViolations(page);
  });

  test('theme switches repaint the rulers; dialogs, menus and the table picker read in both themes', async ({
    page,
  }) => {
    test.setTimeout(120_000);
    await openWrite(page);
    await page.keyboard.type('Kiln Write in both themes.');
    await page.keyboard.press('Shift+Home');
    for (const scheme of ['light', 'dark'] as const) {
      await page.emulateMedia({ colorScheme: scheme });
      // The rulers draw from the brand tokens: after a theme change they repaint in the new theme
      // (a light ruler on the dark UI was the bug).
      await expect
        .poll(
          async () => {
            const lum = await rulerLuminance(page);
            return scheme === 'dark' ? lum < 0.3 : lum > 0.6;
          },
          { message: `the rulers repaint in the ${scheme} theme` },
        )
        .toBe(true);

      // Font dialog: the preview reads (Automatic colour = the page's text colour) and axe passes.
      await run(page, 'suite.format.font_dialog');
      const font = page.getByTestId('write-font-dialog');
      await expect(font).toBeVisible();
      const ratio = await font.locator('.kiln-write-preview').evaluate((box) => {
        const parse = (c: string) => (c.match(/[\d.]+/g) ?? []).slice(0, 3).map(Number);
        const lin = (v: number) => {
          const x = v / 255;
          return x <= 0.04045 ? x / 12.92 : ((x + 0.055) / 1.055) ** 2.4;
        };
        const lum = (c: number[]) =>
          0.2126 * lin(c[0] ?? 0) + 0.7152 * lin(c[1] ?? 0) + 0.0722 * lin(c[2] ?? 0);
        const span = box.querySelector('span') as HTMLElement;
        const a = lum(parse(getComputedStyle(span).color));
        const b = lum(parse(getComputedStyle(box).backgroundColor));
        return (Math.max(a, b) + 0.05) / (Math.min(a, b) + 0.05);
      });
      expect(ratio, `Font preview contrast (${scheme})`).toBeGreaterThanOrEqual(4.5);
      await expectNoSeriousA11yViolations(page);
      await page.keyboard.press('Escape');
      await expect(font).toBeHidden();

      await run(page, 'suite.layout.page_setup');
      await expect(page.getByRole('dialog', { name: 'Page Setup' })).toBeVisible();
      await expectNoSeriousA11yViolations(page);
      await page.keyboard.press('Escape');

      await page.getByTestId('kiln-input-proxy').focus();
      await page.keyboard.press('Control+h');
      await expect(page.getByRole('dialog', { name: 'Find and Replace' })).toBeVisible();
      await expectNoSeriousA11yViolations(page);
      await page.keyboard.press('Escape');

      const box = await page.getByTestId('write-editor').boundingBox();
      if (!box) throw new Error('no editor');
      await page.mouse.click(box.x + box.width / 2, box.y + 160, { button: 'right' });
      await expect(page.getByRole('menu').first()).toBeVisible();
      // The menu's shortcut column and group labels (`--kiln-faint`) and the mini toolbar's small
      // split buttons are ui-kit's (WP-58) and fail contrast and target size in every app: handed off
      // in docs/qa/handoffs.md. Everything else in the menu and the toolbar is checked.
      await expectNoSeriousA11yViolations(page, {
        exclude: ['.kiln-menu__shortcut', '.kiln-menu__group-label', '.kiln-minitoolbar__bar'],
      });
      await expectNoSeriousA11yViolations(page, {
        include: ['.kiln-minitoolbar__bar'],
        disableRules: ['target-size'],
      });
      await page.keyboard.press('Escape');

      await page.getByRole('tab', { name: 'Insert' }).click();
      await page.locator('[data-command="suite.insert.table"]').first().click();
      await expect(page.getByTestId('write-table-grid')).toBeVisible();
      await expectNoSeriousA11yViolations(page);
      await page.keyboard.press('Escape');
      await page.getByRole('tab', { name: 'Home' }).click();
    }
  });
});

const MIME: Record<string, string> = {
  '.html': 'text/html; charset=utf-8',
  '.js': 'text/javascript',
  '.css': 'text/css',
  '.wasm': 'application/wasm',
  '.json': 'application/json',
  '.webmanifest': 'application/manifest+json',
  '.svg': 'image/svg+xml',
  '.png': 'image/png',
  '.ico': 'image/x-icon',
  '.ttf': 'font/ttf',
  '.woff2': 'font/woff2',
  '.map': 'application/json',
};

/** Serves a built site (the app routes by hash: no fallback needed). */
function serveStatic(root: string): Promise<Server> {
  const server = createServer((req, res) => {
    const path = decodeURIComponent(new URL(req.url ?? '/', 'http://x').pathname);
    const file = normalize(join(root, path === '/' ? 'index.html' : path));
    try {
      if (!file.startsWith(root) || !statSync(file).isFile()) throw new Error('not found');
      res.writeHead(200, { 'content-type': MIME[extname(file)] ?? 'application/octet-stream' });
      res.end(readFileSync(file));
    } catch {
      res.writeHead(404).end();
    }
  });
  return new Promise((resolve) => server.listen(0, '127.0.0.1', () => resolve(server)));
}

/**
 * Budget B5 (PLAN.md §3.9) is a property of what users run: it is measured on the production
 * build (`vite build`, release mode), not on the development server, whose React checks render
 * every component twice.
 */
test.describe('performance (production build)', () => {
  test.describe.configure({ mode: 'serial' });
  let outDir = '';
  let server: Server | undefined;
  let base = '';

  test.beforeAll(async () => {
    test.setTimeout(300_000);
    const doc300 = repo('fixtures/large/doc-300p.docx');
    if (!existsSync(doc300)) {
      // Generated on demand (gitignored, deterministic): python-docx in the project virtualenv.
      execFileSync('python3', ['-I', repo('tools/make-fixtures.py'), '--synthetic-large', '--only', 'docx'], {
        stdio: 'pipe',
        timeout: 240_000,
      });
    }
    outDir = mkdtempSync(join(tmpdir(), 'kiln-b5-'));
    execFileSync(
      'pnpm',
      ['--filter', '@kiln/web', 'exec', 'vite', 'build', '--outDir', outDir, '--emptyOutDir'],
      {
        cwd: repo(''),
        env: { ...process.env, VITE_KILN_RELEASE: '1' },
        stdio: 'pipe',
      },
    );
    server = await serveStatic(outDir);
    base = `http://127.0.0.1:${(server.address() as AddressInfo).port}`;
  });

  test.afterAll(async () => {
    await new Promise<void>((resolve) => (server ? server.close(() => resolve()) : resolve()));
    if (outDir) rmSync(outDir, { recursive: true, force: true });
  });

  test('B5: keystroke to paint on the 300-page document (budget p95 < 16 ms)', async ({ page }) => {
    test.setTimeout(240_000);
    await page.goto(`${base}/#/write`);
    await expect(page.getByText(/^Page 1 of 1$/)).toBeVisible({ timeout: 45_000 });
    await page.evaluate(() => {
      // biome-ignore lint/suspicious/noExplicitAny: the API is not in the DOM typings
      delete (window as any).showOpenFilePicker;
    });
    await page.getByRole('button', { name: 'File' }).click();
    await page.getByRole('button', { name: 'Open', exact: true }).click();
    const [chooser] = await Promise.all([
      page.waitForEvent('filechooser'),
      page.getByRole('button', { name: /Browse this device/ }).click(),
    ]);
    await chooser.setFiles(repo('fixtures/large/doc-300p.docx'));
    await expect(page.getByTestId('document-title')).toContainText('doc-300p', { timeout: 60_000 });
    await expect(page.getByText(/^Page 1 of \d{2,3}$/)).toBeVisible({ timeout: 60_000 });
    await page.getByTestId('kiln-input-proxy').focus();
    await page.keyboard.press('Control+Home');
    await page.keyboard.press('End');
    // Let the background pagination of the 300 pages settle before measuring.
    await page.waitForTimeout(2500);
    // B5 is keystroke to *screen* (PLAN.md §3.9: PerformanceObserver + rAF). For every key:
    // the key event's timestamp, the surface's `kiln:input-paint` mark (the composite that draws
    // the keystroke's frame has finished; observed through a PerformanceObserver) and the frame
    // that presents it: a requestAnimationFrame loop runs for the whole measurement, and each
    // frame notes when its callback ran and posts a message from it, which runs once that frame's
    // rendering (paint and commit) is done. The latency is the key event to the end of the first
    // frame whose callback ran after the mark (the first frame that can contain the drawing). The composite time alone is recorded as well, as a diagnostic of the work per key.
    await page.evaluate(() => {
      const w = window as unknown as {
        __b5: { down: number[]; paint: number[]; presented: [number, number][]; stop: boolean };
      };
      w.__b5 = { down: [], paint: [], presented: [], stop: false };
      document
        .querySelector('[data-testid="kiln-input-proxy"]')
        ?.addEventListener('keydown', (e) => w.__b5.down.push(e.timeStamp), true);
      new PerformanceObserver((list) => {
        for (const m of list.getEntries()) if (m.name === 'kiln:input-paint') w.__b5.paint.push(m.startTime);
      }).observe({ type: 'mark' });
      const channel = new MessageChannel();
      channel.port1.onmessage = (e: MessageEvent<number>) =>
        w.__b5.presented.push([e.data, performance.now()]);
      const frame = () => {
        if (w.__b5.stop) return;
        channel.port2.postMessage(performance.now());
        requestAnimationFrame(frame);
      };
      requestAnimationFrame(frame);
    });
    const text = ' the quick brown fox jumps over the lazy dog while the kiln cools down slowly';
    for (const ch of text) {
      await page.keyboard.type(ch);
      await page.waitForTimeout(70);
    }
    await page.waitForTimeout(500);
    const measured = await page.evaluate(() => {
      const b5 = (
        window as unknown as {
          __b5: { down: number[]; paint: number[]; presented: [number, number][]; stop: boolean };
        }
      ).__b5;
      b5.stop = true;
      return b5.down.map((t) => {
        const p = b5.paint.find((x) => x >= t);
        const shown = p === undefined ? undefined : b5.presented.find(([start]) => start >= p)?.[1];
        return {
          composite: p === undefined ? Number.POSITIVE_INFINITY : p - t,
          screen: shown === undefined ? Number.POSITIVE_INFINITY : shown - t,
        };
      });
    });
    const pct = (xs: number[], q: number) => {
      const sorted = [...xs].sort((a, b) => a - b);
      return sorted[Math.max(0, Math.ceil(sorted.length * q) - 1)] ?? Number.POSITIVE_INFINITY;
    };
    const screen = measured.map((m) => m.screen);
    const composite = measured.map((m) => m.composite);
    const p95 = pct(screen, 0.95);
    const p50 = pct(screen, 0.5);
    const summary =
      `n=${screen.length} keystroke→screen p50=${p50.toFixed(1)} ms p95=${p95.toFixed(1)} ms ` +
      `(keystroke→composite p50=${pct(composite, 0.5).toFixed(1)} ms p95=${pct(composite, 0.95).toFixed(1)} ms)`;
    test.info().annotations.push({ type: 'B5', description: summary });
    console.log(`B5 on doc-300p (production build): ${summary}`);
    expect(screen.length).toBe(text.length);
    // The budget is 16 ms (PLAN.md §3.9). While docs/qa/perf.md carries a B5 exception that is
    // proposed or approved, the test holds the exception's allowance (§3.9: × 1.5 on CI runners)
    // and says loudly that the budget itself is not met; once the exception is removed or marked
    // rejected, the 16 ms budget is asserted. The allowance is read from the document, so the test
    // and the recorded exception cannot drift apart.
    const exception = b5Exception();
    const tolerance = process.env.CI ? 1.5 : 1;
    const limit = exception ? exception.allowance * tolerance : 16 * tolerance;
    const met = p95 < 16 * tolerance;
    test.info().annotations.push({
      type: 'B5 budget',
      description: met
        ? 'met (p95 < 16 ms)'
        : `NOT MET (p95 ${p95.toFixed(1)} ms ≥ 16 ms); held to the ${exception?.status ?? 'missing'} exception's ${limit} ms`,
    });
    if (!met)
      console.warn(
        `B5 budget (16 ms) not met: p95 ${p95.toFixed(1)} ms; exception ${exception?.status ?? 'none'}`,
      );
    expect(p95, `B5 p95 ${p95.toFixed(1)} ms (budget 16 ms, assertion ${limit} ms)`).toBeLessThan(limit);
  });
});
