/**
 * The Kiln Write integration gate: the rest of Word's ribbon that the gate's P0 rows
 * name — Paste Special, the Symbol dialog, the Selection pane, aligning, distributing and
 * grouping objects, footnotes continued across pages, Show Markup's balloons, the views, page
 * movement, the Zoom dialog, Split, the Macros button, Table Properties, nested tables, crop to
 * shape, shape styles and text direction, the header and footer position, AutoCorrect, drag and
 * drop of text, keep rules, sections and bidirectional text — through the ribbon, the dialogs and
 * the keyboard of the web build, checked against what the core reports; then the Office file
 * loop over the feature fixtures (open, edit like a Word user, save, reopen) and the screenshots
 * of the visual review (light and dark, desktop and phone).
 */
import { readdirSync, readFileSync } from 'node:fs';
import { fileURLToPath } from 'node:url';
import { deflateSync, inflateRawSync } from 'node:zlib';
import { type BrowserContext, expect, type Page, test } from '@playwright/test';
import { expectNoSeriousA11yViolations } from '../../packages/testing/src/playwright/a11y.ts';

const repo = (p: string) => fileURLToPath(new URL(`../../${p}`, import.meta.url));
const shot = (name: string) => repo(`e2e/screenshots/write-gate/${name}`);

interface WriteHandle {
  doc: number;
  client: {
    query(doc: number, q: unknown): Promise<Record<string, unknown>>;
    execute(doc: number, c: unknown): Promise<unknown>;
  };
  controller: {
    getStats(): { tiles: number; zoom: number; scrollY: number };
    clientRectOf(r: Rect): { left: number; top: number; width: number; height: number };
  } | null;
}
interface KilnHandle {
  registry: { execute(id: string, args?: unknown): Promise<{ status: string }> };
}
type KilnWindow = { __kilnWrite?: WriteHandle; __kiln?: KilnHandle };
type Json = Record<string, unknown>;
type Rect = { x: number; y: number; width: number; height: number };

async function ready(page: Page): Promise<void> {
  await page.waitForFunction(
    () => ((window as unknown as KilnWindow).__kilnWrite?.controller?.getStats().tiles ?? 0) > 0,
    null,
    { timeout: 45_000 },
  );
}

async function openWrite(page: Page): Promise<void> {
  await page.goto('/#/write');
  await ready(page);
  await page.getByTestId('kiln-input-proxy').focus();
}

/** Opens a file through File ▸ Open ▸ Browse (the file input the picker falls back to). */
async function openFile(page: Page, file: { name: string; buffer: Buffer }): Promise<void> {
  await page.goto('/#/home');
  await expect(
    page.getByText('Your recent files will gather here').or(page.locator('.kiln-home__table')).first(),
  ).toBeVisible({ timeout: 45_000 });
  await noFilePickers(page);
  const [chooser] = await Promise.all([page.waitForEvent('filechooser'), page.keyboard.press('Control+o')]);
  await chooser.setFiles({ mimeType: 'application/octet-stream', ...file });
  await ready(page);
  await noFilePickers(page);
}

async function noFilePickers(page: Page): Promise<void> {
  await page.evaluate(() => {
    // biome-ignore lint/suspicious/noExplicitAny: the API is not in the DOM typings
    const w = window as any;
    delete w.showOpenFilePicker;
    delete w.showSaveFilePicker;
  });
}

async function paragraphs(page: Page): Promise<string[]> {
  return page.evaluate(async () => {
    const w = (window as unknown as KilnWindow).__kilnWrite as WriteHandle;
    const r = (await w.client.query(w.doc, 'Paragraphs')) as { Paragraphs?: { text: string }[] };
    return (r.Paragraphs ?? []).map((p) => p.text);
  });
}

async function writeQuery<T = Json>(page: Page, q: unknown): Promise<T> {
  return page.evaluate(async (query) => {
    const w = (window as unknown as KilnWindow).__kilnWrite as WriteHandle;
    const r = (await w.client.query(w.doc, { Write: query })) as { Write: Record<string, unknown> };
    return Object.values(r.Write)[0] as never;
  }, q);
}

const format = (page: Page) => writeQuery<Json>(page, 'Format');
const features = async (page: Page) => (await format(page)).features as Json;
const objects = (page: Page) => writeQuery<Json[]>(page, 'Objects');

async function run(page: Page, id: string, args?: unknown): Promise<string> {
  return page.evaluate(
    async ([i, a]) =>
      (await (window as unknown as KilnWindow).__kiln?.registry.execute(i as string, a))?.status ?? '',
    [id, args] as const,
  );
}

async function until<T>(get: () => Promise<T>, check: (v: T) => boolean, what: string): Promise<T> {
  let last: T | undefined;
  try {
    await expect
      .poll(
        async () => {
          last = await get();
          try {
            return check(last);
          } catch {
            return false;
          }
        },
        { message: what, timeout: 15_000 },
      )
      .toBe(true);
  } catch (e) {
    throw new Error(`${what}: last value ${JSON.stringify(last)?.slice(0, 2000)}\n${String(e)}`);
  }
  return last as T;
}

async function select(page: Page, para: number, from: number, to: number): Promise<void> {
  await page.evaluate(
    async ([p, a, b]) => {
      const w = (window as unknown as KilnWindow).__kilnWrite as WriteHandle;
      const r = (await w.client.query(w.doc, 'Paragraphs')) as { Paragraphs: { id: number }[] };
      const block = r.Paragraphs[p as number]?.id ?? 0;
      await w.client.execute(w.doc, { Write: { SelectTo: { pos: { block, offset: a }, extend: false } } });
      await w.client.execute(w.doc, { Write: { SelectTo: { pos: { block, offset: b }, extend: true } } });
    },
    [para, from, to] as const,
  );
}

const caretAt = (page: Page, para: number, offset: number) => select(page, para, offset, offset);

/** The screen point of a caret at a paragraph offset (its vertical middle). */
async function pointAt(page: Page, para: number, offset: number): Promise<{ x: number; y: number }> {
  const rect = await page.evaluate(
    async ([p, o]) => {
      const w = (window as unknown as KilnWindow).__kilnWrite as WriteHandle;
      const r = (await w.client.query(w.doc, 'Paragraphs')) as { Paragraphs: { id: number }[] };
      const block = r.Paragraphs[p as number]?.id ?? 0;
      await w.client.execute(w.doc, { Write: { SelectTo: { pos: { block, offset: o }, extend: false } } });
      const s = (await w.client.query(w.doc, 'Selection')) as { Selection: { caret: { rect: Rect } } };
      return w.controller?.clientRectOf(s.Selection.caret.rect) ?? null;
    },
    [para, offset] as const,
  );
  if (!rect) throw new Error('no caret');
  return { x: rect.left + 1, y: rect.top + rect.height / 2 };
}

/** The screen rectangle of a document rectangle. */
async function screenRect(page: Page, r: Rect) {
  return page.evaluate(
    (rect) => (window as unknown as KilnWindow).__kilnWrite?.controller?.clientRectOf(rect) ?? null,
    r,
  );
}

/** A `w` × `h` PNG in one colour, as bytes. */
function png(w = 48, h = 32, rgb: [number, number, number] = [40, 90, 200]): number[] {
  const crc = (buf: Buffer) => {
    let c = ~0;
    for (const b of buf) {
      c ^= b;
      for (let k = 0; k < 8; k++) c = (c >>> 1) ^ (0xedb88320 & -(c & 1));
    }
    return ~c >>> 0;
  };
  const chunk = (type: string, data: Buffer) => {
    const len = Buffer.alloc(4);
    len.writeUInt32BE(data.length);
    const td = Buffer.concat([Buffer.from(type, 'ascii'), data]);
    const sum = Buffer.alloc(4);
    sum.writeUInt32BE(crc(td));
    return Buffer.concat([len, td, sum]);
  };
  const ihdr = Buffer.alloc(13);
  ihdr.writeUInt32BE(w, 0);
  ihdr.writeUInt32BE(h, 4);
  ihdr[8] = 8;
  ihdr[9] = 2;
  const row = Buffer.concat([Buffer.from([0]), Buffer.from(Array.from({ length: w }, () => rgb).flat())]);
  const raw = Buffer.concat(Array.from({ length: h }, () => row));
  return Array.from(
    Buffer.concat([
      Buffer.from([0x89, 0x50, 0x4e, 0x47, 0x0d, 0x0a, 0x1a, 0x0a]),
      chunk('IHDR', ihdr),
      chunk('IDAT', deflateSync(raw)),
      chunk('IEND', Buffer.alloc(0)),
    ]),
  );
}

/** Inserts `n` floating pictures (In Front of Text) on paragraphs 0..n-1; returns their ids. */
async function floatingPictures(page: Page, n: number): Promise<number[]> {
  const ids: number[] = [];
  for (let i = 0; i < n; i++) {
    await caretAt(page, i, 0);
    await run(page, 'write.insert.picture', { name: `p${i}.png`, bytes: png(), floating: false });
    await run(page, 'write.arrange.wrap_text', { wrap: 'InFront' });
    const o = await until(
      async () => (await features(page)).object as Json | null,
      (x) => !!x && !ids.includes(x.id as number) && x.wrap === 'InFront',
      'a floating picture',
    );
    ids.push(o?.id as number);
  }
  return ids;
}

/** One entry of a zip package, inflated. */
function zipEntry(zip: Buffer, name: string): Buffer | null {
  const eocd = zip.lastIndexOf(Buffer.from([0x50, 0x4b, 0x05, 0x06]));
  if (eocd < 0) return null;
  const count = zip.readUInt16LE(eocd + 10);
  let at = zip.readUInt32LE(eocd + 16);
  for (let i = 0; i < count; i++) {
    const method = zip.readUInt16LE(at + 10);
    const size = zip.readUInt32LE(at + 20);
    const nameLen = zip.readUInt16LE(at + 28);
    const extra = zip.readUInt16LE(at + 30);
    const comment = zip.readUInt16LE(at + 32);
    const local = zip.readUInt32LE(at + 42);
    const entry = zip.subarray(at + 46, at + 46 + nameLen).toString('utf8');
    if (entry === name) {
      const start = local + 30 + zip.readUInt16LE(local + 26) + zip.readUInt16LE(local + 28);
      const data = zip.subarray(start, start + size);
      return method === 0 ? Buffer.from(data) : inflateRawSync(data);
    }
    at += 46 + nameLen + extra + comment;
  }
  return null;
}

/** The bytes of a file in the origin's private file system (`documents/<name>`). */
async function savedBytes(page: Page, name: string): Promise<Buffer> {
  let bytes: number[] | null = null;
  await expect
    .poll(
      async () => {
        bytes = await page.evaluate(async (n) => {
          try {
            const root = await navigator.storage.getDirectory();
            const dir = await root.getDirectoryHandle('documents');
            const file = await (await dir.getFileHandle(n)).getFile();
            return Array.from(new Uint8Array(await file.arrayBuffer()));
          } catch {
            return null;
          }
        }, name);
        return bytes !== null && bytes.length > 0;
      },
      { message: `${name} is saved`, timeout: 20_000 },
    )
    .toBe(true);
  return Buffer.from(bytes ?? []);
}

async function clearSaved(page: Page): Promise<void> {
  await page.evaluate(async () => {
    const root = await navigator.storage.getDirectory();
    await root.removeEntry('documents', { recursive: true }).catch(() => undefined);
  });
}

async function grantClipboard(context: BrowserContext, baseURL: string | undefined): Promise<void> {
  await context.grantPermissions(['clipboard-read', 'clipboard-write'], {
    origin: new URL(baseURL ?? 'http://localhost:5173').origin,
  });
}

/** File ▸ Save As ▸ a format button (the browser's private file system takes the file). */
async function saveAs(page: Page, format: RegExp): Promise<void> {
  await page.getByRole('button', { name: 'File' }).click();
  const menu = page.getByRole('dialog', { name: /File menu/ });
  await menu.getByRole('navigation', { name: 'File menu' }).getByRole('button', { name: 'Save As' }).click();
  await menu.getByRole('button', { name: format }).click();
}

/** Saves a new document as .docx and returns its document.xml. */
async function savedXml(page: Page, name = 'Document1.docx'): Promise<string> {
  await clearSaved(page);
  await noFilePickers(page);
  await saveAs(page, /^\.docx Word Document/);
  const zip = await savedBytes(page, name);
  return zipEntry(zip, 'word/document.xml')?.toString('utf8') ?? '';
}

/** The page (index) of a document point, from the Pages query. */
async function pageOf(page: Page, y: number, x?: number): Promise<number> {
  const pages = await writeQuery<{ rects: Rect[] }>(page, 'Pages');
  return pages.rects.findIndex(
    (r) => y >= r.y && y <= r.y + r.height && (x === undefined || (x >= r.x && x <= r.x + r.width)),
  );
}

async function caretRect(page: Page): Promise<Rect> {
  return page.evaluate(async () => {
    const w = (window as unknown as KilnWindow).__kilnWrite as WriteHandle;
    const s = (await w.client.query(w.doc, 'Selection')) as { Selection: { caret: { rect: Rect } } };
    return s.Selection.caret.rect;
  });
}

const dialog = (page: Page, id: string) => page.getByTestId(id);

/** A screenshot without the pointer's tooltip or a focused button's. */
async function snap(page: Page, name: string): Promise<void> {
  await page.mouse.move(2, 600);
  await page.evaluate(() => {
    const a = document.activeElement as HTMLElement | null;
    if (a?.closest('.kiln-ribbon, .kiln-statusbar')) a.blur();
  });
  await page.waitForTimeout(250);
  await page.screenshot({ path: shot(name) });
}

/**
 * How soon the keys must be back in the document after a picker, menu or dialog closes: at once on
 * a development machine; a CI runner, several times slower, gets three times as long.
 */
const KEYS_BACK = process.env.CI ? 3000 : 1000;

test.describe('Kiln Write gate', () => {
  test.describe.configure({ timeout: 150_000 });

  test('paste special: formatted, unformatted, HTML, picture and Markdown', async ({
    page,
    context,
    baseURL,
  }) => {
    await grantClipboard(context, baseURL);
    await openWrite(page);
    await page.keyboard.type('Glaze notes');
    await page.keyboard.press('Enter');
    // HTML on the system clipboard (what a browser or Word puts there).
    await page.evaluate(async () => {
      const html = '<p><b>Bold</b> tenmoku</p>';
      await navigator.clipboard.write([
        new ClipboardItem({
          'text/html': new Blob([html], { type: 'text/html' }),
          'text/plain': new Blob(['Bold tenmoku'], { type: 'text/plain' }),
        }),
      ]);
    });
    // Ctrl+Alt+V opens Paste Special; Unformatted Unicode Text pastes the words only.
    await page.keyboard.press('Control+Alt+V');
    const ps = dialog(page, 'write-paste-special-dialog');
    await expect(ps).toBeVisible();
    for (const label of [
      'Kiln formatted text',
      'HTML Format',
      'Unformatted Unicode Text',
      'Markdown (as styled text)',
    ])
      await expect(ps.getByRole('radio', { name: label })).toBeVisible();
    await ps.getByRole('radio', { name: 'Unformatted Unicode Text' }).check();
    await ps.getByRole('button', { name: 'OK' }).click();
    await until(
      () => paragraphs(page),
      (p) => p[1] === 'Bold tenmoku',
      'unformatted text',
    );
    const bold = async () => {
      await select(page, 1, 0, 4);
      return ((await format(page)).bold as boolean | null) ?? false;
    };
    expect(await bold()).toBe(false);
    // HTML Format keeps the bold.
    await caretAt(page, 1, 12);
    await page.getByTestId('kiln-input-proxy').focus();
    await page.keyboard.press('Enter');
    await page.keyboard.press('Control+Alt+V');
    await ps.getByRole('radio', { name: 'HTML Format' }).check();
    await ps.getByRole('button', { name: 'OK' }).click();
    await until(
      () => paragraphs(page),
      (p) => p.filter((t) => t === 'Bold tenmoku').length === 2,
      'HTML',
    );
    await select(page, 2, 0, 4);
    await until(
      async () => (await format(page)).bold,
      (b) => b === true,
      'HTML keeps bold',
    );
    // Markdown on the clipboard: Paste Special ▸ Markdown makes a heading and a list.
    await page.evaluate(async () => {
      await navigator.clipboard.writeText('# Firing log\n\n- cone 6\n- cone 10');
    });
    const count = (await paragraphs(page)).length;
    await caretAt(page, count - 1, (await paragraphs(page))[count - 1]?.length ?? 0);
    await page.getByTestId('kiln-input-proxy').focus();
    await page.keyboard.press('Enter');
    await page.keyboard.press('Control+Alt+V');
    await ps.getByRole('radio', { name: 'Markdown (as styled text)' }).check();
    await ps.getByRole('button', { name: 'OK' }).click();
    const p = await until(
      () => paragraphs(page),
      (t) => t.includes('Firing log') && t.includes('cone 10'),
      'Markdown',
    );
    const heading = p.indexOf('Firing log');
    await caretAt(page, heading, 1);
    await until(
      async () => (await format(page)).style_id,
      (s) => s === 'Heading1',
      'a heading',
    );
    // A picture on the clipboard: Paste Special offers Picture (PNG).
    await page.evaluate(async (bytes) => {
      const blob = new Blob([new Uint8Array(bytes)], { type: 'image/png' });
      await navigator.clipboard.write([new ClipboardItem({ 'image/png': blob })]);
    }, png());
    await page.getByTestId('kiln-input-proxy').focus();
    await page.keyboard.press('Control+Alt+V');
    await expect(ps.getByRole('radio', { name: 'Picture (PNG)' })).toBeChecked();
    await ps.getByRole('button', { name: 'OK' }).click();
    await until(
      async () => (await objects(page)).filter((o) => o.kind === 'Picture').length,
      (n) => n === 1,
      'a pasted picture',
    );
  });

  test('symbol dialog: symbol fonts, recent symbols, character codes and Alt+X', async ({ page }) => {
    await openWrite(page);
    await page.keyboard.type('Ticks ');
    await page.getByRole('tab', { name: 'Insert' }).click();
    await page.locator('[data-command="suite.insert.symbol"]').first().click();
    const sym = dialog(page, 'write-symbol-dialog');
    await expect(sym).toBeVisible();
    // Wingdings: the font's own characters, inserted in the font (Word's symbol fonts).
    await sym.getByRole('combobox').first().click();
    await page.getByRole('option', { name: 'Wingdings', exact: true }).click();
    const tick = sym.locator('button[title="Wingdings: F0FC"]').first();
    await expect(tick).toBeVisible();
    await tick.click();
    await until(
      () => paragraphs(page),
      (p) => (p[0]?.length ?? 0) > 'Ticks '.length,
      'a Wingdings tick',
    );
    // Recently used symbols list it; the character code field inserts any character.
    await expect(
      sym.getByRole('group', { name: 'Recently used' }).locator('button[title="Wingdings: F0FC"]'),
    ).toBeVisible();
    await sym.getByRole('textbox', { name: /Character code/ }).fill('263A');
    await sym.getByRole('button', { name: 'Insert ☺', exact: true }).click();
    await until(
      () => paragraphs(page),
      (p) => p[0]?.endsWith('☺') ?? false,
      'by code',
    );
    await sym.getByRole('button', { name: 'Close', exact: true }).last().click();
    // The tick shows as its Unicode equivalent (and saves as Word's w:sym in Wingdings).
    expect((await paragraphs(page))[0]).toMatch(/^Ticks [\u2713\u2714\u{1F5F8}]☺$/u);
    // A character code: 2603 then Alt+X makes a snowman; Alt+X again turns it back into the code.
    await page.getByTestId('kiln-input-proxy').focus();
    await page.keyboard.press('End');
    await page.keyboard.type(' 2603');
    await page.keyboard.press('Alt+x');
    await until(
      () => paragraphs(page),
      (p) => p[0]?.endsWith(' ☃') ?? false,
      'Alt+X makes ☃',
    );
    await page.keyboard.press('Alt+x');
    await until(
      () => paragraphs(page),
      (p) => p[0]?.endsWith(' 2603') ?? false,
      'and back',
    );
    // Alt+X does not open Key Tips.
    await expect(page.locator('.kiln-keytip')).toHaveCount(0);
  });

  test('selection pane: list, rename, hide and reorder objects', async ({ page }) => {
    await openWrite(page);
    await page.keyboard.type('One\nTwo\nThree');
    const ids = await floatingPictures(page, 2);
    await page.getByRole('tab', { name: 'Layout', exact: true }).click();
    await page.locator('[data-command="suite.arrange.selection_pane"]').first().click();
    const pane = page.getByTestId('write-selection-pane');
    await expect(pane).toBeVisible();
    await expect(pane.getByTestId('write-selection-item')).toHaveCount(2);
    // Rename the first object in the list.
    await pane.locator('.kiln-selection-pane__name').first().dblclick();
    const name = pane.getByRole('textbox', { name: 'Object name' });
    await name.fill('Kiln photo');
    await name.press('Enter');
    await until(
      async () => (await objects(page)).map((o) => o.name),
      (n) => n.includes('Kiln photo'),
      'renamed',
    );
    // Hide it with the eye: not painted, still listed, saved as hidden.
    await pane.getByRole('button', { name: 'Hide Kiln photo' }).click();
    await until(
      async () => (await objects(page)).find((o) => o.name === 'Kiln photo')?.hidden,
      (h) => h === true,
      'hidden',
    );
    await expect(pane.getByRole('button', { name: 'Show Kiln photo' })).toBeVisible();
    // Show All and Hide All.
    await pane.getByRole('button', { name: 'Hide All' }).click();
    await until(
      async () => (await objects(page)).every((o) => o.hidden === true),
      (v) => v,
      'all hidden',
    );
    await pane.getByRole('button', { name: 'Show All' }).click();
    await until(
      async () => (await objects(page)).every((o) => o.hidden !== true),
      (v) => v,
      'all shown',
    );
    // Reorder: send the front object backward.
    const front = async () => {
      const o = await objects(page);
      return [...o].sort((a, b) => (b.z as number) - (a.z as number))[0]?.id;
    };
    const before = await front();
    const frontName = (await objects(page)).find((o) => o.id === before)?.name as string;
    await pane.getByRole('button', { name: `Send ${frontName} backward` }).click();
    await until(front, (f) => f !== before, 'reordered');
    expect(ids).toContain(before);
    await expectNoSeriousA11yViolations(page);
  });

  test('align and distribute objects relative to the page, the margins and each other', async ({ page }) => {
    await openWrite(page);
    await page.keyboard.type('One\nTwo\nThree\nFour');
    const ids = await floatingPictures(page, 3);
    // Spread them out, then Shift+click them all.
    for (const [i, id] of ids.entries()) {
      await run(page, 'write.arrange.transform', {
        id,
        rect: { x: 200 + 130 * i + (i === 1 ? 50 : 0), y: 180 + 60 * i, width: 48, height: 32 },
      });
    }
    const rectOf = async (id: number) => (await objects(page)).find((o) => o.id === id)?.rect as Rect;
    // Laid out where they were moved before clicking them.
    await until(
      async () => Promise.all(ids.map(async (id) => (await rectOf(id))?.y ?? 0)),
      (ys) => ys.every((y, i) => Math.abs(y - (180 + 60 * i)) < 1.5),
      'moved apart',
    );
    await run(page, 'write.arrange.select', { id: ids[0] });
    await until(
      async () => (await objects(page)).find((o) => o.id === ids[0])?.selected,
      (v) => v === true,
      'the first selected',
    );
    await expect(page.locator('.kiln-object-frame').first()).toBeVisible();
    for (const id of ids.slice(1)) {
      const r = await screenRect(page, await rectOf(id));
      if (!r) throw new Error('no rect');
      await page.keyboard.down('Shift');
      await page.mouse.click(r.left + r.width / 2, r.top + r.height / 2);
      await page.keyboard.up('Shift');
    }
    await until(
      async () => (await objects(page)).filter((o) => o.in_selection).length,
      (n) => n === 3,
      'three selected',
    );
    await expect(page.getByTestId('write-multi-frame')).toHaveCount(2);
    // Arrange ▸ Align ▾ ▸ Align Top (to each other: Align Selected Objects).
    await page.getByRole('tab', { name: 'Layout', exact: true }).click();
    await page.locator('[data-command="suite.arrange.align"]').first().click();
    await page.getByRole('menuitem', { name: 'Align Top' }).click();
    const tops = await until(
      async () => Promise.all(ids.map(async (id) => (await rectOf(id)).y)),
      (t) => t.every((y) => Math.abs(y - (t[0] as number)) < 1),
      'tops aligned',
    );
    expect(tops[0]).toBeLessThan(190);
    // Distribute Horizontally: equal gaps.
    await page.locator('[data-command="suite.arrange.align"]').first().click();
    await page.getByRole('menuitem', { name: 'Distribute Horizontally' }).click();
    await until(
      async () => {
        const xs = await Promise.all(ids.map(async (id) => (await rectOf(id)).x));
        return Math.abs(xs[1] - xs[0] - (xs[2] - xs[1]));
      },
      (d) => d < 1.5,
      'distributed',
    );
    // Align to the margin: Align Left puts each selected object at the left margin.
    await run(page, 'ui.write.align_to', { to: 'Margin' });
    await run(page, 'ui.write.align', { align: 'Left' });
    const pages = await writeQuery<{ rects: Rect[] }>(page, 'Pages');
    await until(
      async () => Promise.all(ids.map(async (id) => (await rectOf(id)).x)),
      (xs) => xs.every((x) => Math.abs(x - ((pages.rects[0]?.x ?? 0) + 96)) < 1.5),
      'each at the left margin',
    );
    // And to the page edge.
    await run(page, 'ui.write.align_to', { to: 'Page' });
    await run(page, 'ui.write.align', { align: 'Right' });
    await until(
      async () => Promise.all(ids.map(async (id) => (await rectOf(id)).x + (await rectOf(id)).width)),
      (xs) => xs.every((x) => Math.abs(x - ((pages.rects[0]?.x ?? 0) + (pages.rects[0]?.width ?? 0))) < 1.5),
      'each at the page edge',
    );
  });

  test('arrange group: group and ungroup objects', async ({ page }) => {
    await openWrite(page);
    await page.keyboard.type('One\nTwo\nThree');
    const ids = await floatingPictures(page, 2);
    await run(page, 'write.insert.shape', {
      preset: 'ellipse',
      text: 'Kiln',
      rect: { x: 400, y: 300, width: 120, height: 60 },
    });
    const shape = (await until(
      async () => (await features(page)).object as Json | null,
      (o) => o?.kind === 'TextBox' || o?.kind === 'Shape',
      'a shape',
    )) as Json;
    const all = [...ids, shape.id as number];
    await run(page, 'write.arrange.select', { id: all[0] });
    for (const id of all.slice(1)) await run(page, 'write.arrange.select', { id, add: true });
    // Layout ▸ Arrange ▸ Group ▾ ▸ Group.
    await page.getByRole('tab', { name: 'Layout', exact: true }).click();
    await page.locator('.kiln-ribbon').getByRole('button', { name: 'Group', exact: true }).click();
    await page.getByRole('menuitem', { name: 'Group', exact: true }).click();
    const group = await until(
      () => objects(page),
      (o) => o.length === 1 && o[0]?.kind === 'Group',
      'one group',
    );
    // The group wraps as the first selected object: the In Front pictures keep it In Front.
    expect((group[0] as Json).wrap).toBe('InFront');
    await expect(page.getByTestId('write-selection-pane')).toHaveCount(0);
    // The group moves as one.
    const g = group[0] as Json;
    const r = g.rect as Rect;
    await run(page, 'write.arrange.transform', { id: g.id, rect: { ...r, x: r.x + 40 } });
    await until(
      async () => ((await objects(page))[0]?.rect as Rect | undefined)?.x ?? 0,
      (x) => Math.abs(x - (r.x + 40)) < 1.5,
      'moved',
    );
    await page.waitForTimeout(300);
    await snap(page, 'group-light-desktop.png');
    // Ungroup: the three objects again, all selected.
    await run(page, 'ui.write.ungroup');
    await until(
      () => objects(page),
      (o) => o.length === 3 && o.filter((x) => x.in_selection).length === 3,
      'ungrouped',
    );
    // Each gets its own wrapping back.
    const back = await objects(page);
    for (const id of ids) expect(back.find((o) => o.id === id)?.wrap).toBe('InFront');
    expect(back.find((o) => o.id === shape.id)?.wrap).toBe('Square');
    // Undo regroups, redo ungroups again.
    await page.keyboard.press('Control+z');
    await until(
      () => objects(page),
      (o) => o.length === 1,
      'undo',
    );
  });

  test('phone: every ribbon tab is a whole chip in the ribbon sheet; a narrow tablet keeps More tabs', async ({
    page,
  }) => {
    await openWrite(page);
    const list = page.getByRole('tablist', { name: 'Ribbon tabs' });
    const cut = async () =>
      list.evaluate((l) => {
        const r = l.getBoundingClientRect();
        return [...l.querySelectorAll<HTMLElement>('[role="tab"]')]
          .filter((t) => t.offsetWidth > 0)
          .some((t) => {
            const b = t.getBoundingClientRect();
            return b.left < r.left - 0.5 || b.right > r.right + 0.5;
          });
      });
    // Wide: every tab, no menu.
    await expect(page.getByRole('button', { name: 'More tabs' })).toHaveCount(0);
    const tabs = await list.getByRole('tab').allTextContents();
    // A tablet: tabs that do not fit wait in More tabs, none is cut mid-label.
    await page.setViewportSize({ width: 640, height: 900 });
    await expect(page.getByRole('button', { name: 'More tabs' })).toBeVisible();
    await expect.poll(cut).toBe(false);
    // A phone: the ribbon sheet lists the same tabs as chips, each label whole; the chip
    // row scrolls, and a chosen chip scrolls fully into view.
    for (const width of [360, 390, 430]) {
      await page.setViewportSize({ width, height: 844 });
      await page.getByTestId('ribbon-sheet-button').click();
      const chips = page.getByTestId('ribbon-sheet').getByRole('tab');
      await expect(chips).toHaveText(tabs);
      for (const chip of await chips.all())
        expect(await chip.evaluate((c) => c.scrollWidth <= c.clientWidth + 1)).toBe(true);
      await page.getByTestId('ribbon-sheet').getByRole('tab', { name: 'Review' }).click();
      const review = page.getByTestId('ribbon-sheet').getByRole('tab', { name: 'Review' });
      await expect(review).toHaveAttribute('aria-selected', 'true');
      await expect(review).toBeInViewport({ ratio: 1 });
      await page.keyboard.press('Escape');
      await expect(page.getByTestId('ribbon-sheet')).toBeHidden();
      expect(await page.evaluate(() => document.documentElement.scrollWidth > window.innerWidth)).toBe(false);
    }
  });

  test('measurement units: the ribbon, Table Properties and Options agree; Row options stand apart', async ({
    page,
  }) => {
    await openWrite(page);
    await page.keyboard.type('Indented');
    // en-US measures in inches, in the ribbon as in the dialogs.
    await page.getByRole('tab', { name: 'Layout', exact: true }).click();
    const indent = page
      .getByRole('spinbutton', { name: 'Indent Left' })
      .or(page.getByLabel('Indent Left'))
      .first();
    await expect(indent).toHaveValue(/^0 in$/);
    await indent.fill('0.5');
    await indent.press('Enter');
    await until(
      async () => (await format(page)).indent_left as number,
      (pt) => Math.abs(pt - 36) < 0.2,
      'half an inch',
    );
    await page.getByTestId('kiln-input-proxy').focus();
    await page.keyboard.press('End');
    await page.keyboard.press('Enter');
    await run(page, 'write.table.insert', { rows: 2, cols: 2 });
    await run(page, 'ui.write.table_properties_dialog');
    const dialog = page.getByTestId('write-table-properties-dialog');
    await dialog.getByRole('tab', { name: 'Row' }).click();
    await expect(dialog.getByLabel('Height', { exact: true })).toHaveValue(/ in$/);
    // The two Row options are one under the other, not run together.
    const a = await dialog.getByText('Allow row to break across pages').boundingBox();
    const b = await dialog.getByText('Repeat as header row at the top of each page').boundingBox();
    expect(a && b && b.y >= a.y + a.height).toBe(true);
    await dialog.getByRole('button', { name: 'Cancel' }).click();
    // File ▸ Options ▸ Advanced (Word's place): centimetres everywhere.
    await run(page, 'ui.options.open');
    const options = page.getByRole('dialog', { name: /Options/ });
    await options
      .getByRole('navigation', { name: 'Options sections' })
      .getByRole('button', { name: 'Advanced' })
      .click();
    await options.getByLabel('Show measurements in units of').selectOption('cm');
    await options.getByRole('button', { name: 'Close' }).last().click();
    await caretAt(page, 0, 1);
    await page.getByRole('tab', { name: 'Layout', exact: true }).first().click();
    await expect(indent).toHaveValue(/^1\.27 cm$/);
    await caretAt(page, 1, 0);
    await run(page, 'ui.write.table_properties_dialog');
    await page.getByTestId('write-table-properties-dialog').getByRole('tab', { name: 'Row' }).click();
    await expect(
      page.getByTestId('write-table-properties-dialog').getByLabel('Height', { exact: true }),
    ).toHaveValue(/ cm$/);
  });

  test('pickers hand the keys back: Insert ▸ Table 3×2, then type Month, Tab, Gas', async ({ page }) => {
    await openWrite(page);
    await page.getByRole('tab', { name: 'Insert' }).click();
    await page.locator('[data-command="suite.insert.table"]').first().click();
    await page.getByRole('button', { name: 'Insert a table of 3 by 2' }).click();
    // No click in the document: the keys are back in it (at once, not on the Table button), and
    // the next ones type in the new table's first cell.
    await expect(page.getByTestId('kiln-input-proxy')).toBeFocused({ timeout: KEYS_BACK });
    await page.keyboard.type('Month');
    await page.keyboard.press('Tab');
    await page.keyboard.type('Gas');
    await until(
      () => paragraphs(page),
      (p) => p.includes('Month') && p.includes('Gas'),
      'typed in the cells',
    );
    const t = (await format(page)).table as Json;
    expect([t.rows, t.cols, t.row, t.col]).toEqual([2, 3, 0, 1]);
    // A colour from the font colour picker, then typing goes on in the cell.
    await page.getByRole('tab', { name: 'Home' }).click();
    await page.keyboard.press('Shift+Home');
    await page
      .locator('[data-command="suite.format.font_color"] button[aria-label^="More options"]')
      .first()
      .click();
    await page.locator('.kiln-popover button[aria-label]').nth(5).click();
    await expect(page.getByTestId('kiln-input-proxy')).toBeFocused({ timeout: KEYS_BACK });
    await page.keyboard.press('End');
    await page.keyboard.type(' bill');
    await until(
      () => paragraphs(page),
      (p) => p.includes('Gas bill'),
      'typed after the colour',
    );
    // Borders ▾ (a menu): the keys come back too.
    await page.getByRole('tab', { name: 'Home', exact: true }).click();
    await page.locator('[data-command="write.paragraph.borders"]').first().click();
    await page.getByRole('menuitem', { name: 'Bottom Border' }).click();
    await expect(page.getByTestId('kiln-input-proxy')).toBeFocused({ timeout: KEYS_BACK });
    await page.keyboard.type('!');
    await until(
      () => paragraphs(page),
      (p) => p.includes('Gas bill!'),
      'typed after the borders',
    );
    // Insert ▸ Symbol: inserting from the dialog and closing it gives the keys back.
    await page.getByRole('tab', { name: 'Insert', exact: true }).click();
    await page.locator('[data-command="suite.insert.symbol"]').first().click();
    const symbols = page.getByRole('dialog', { name: /Symbol/ });
    await expect(symbols).toBeVisible();
    await page.keyboard.press('Escape');
    await expect(symbols).toHaveCount(0);
    await expect(page.getByTestId('kiln-input-proxy')).toBeFocused({ timeout: KEYS_BACK });
    // Insert ▸ Shapes ▾: choosing a shape arms drawing, with the keys in the document.
    await page.locator('[data-command="suite.insert.shapes"]').first().click();
    await page.getByRole('menuitem', { name: 'Rectangle' }).first().click();
    await expect(page.getByTestId('kiln-input-proxy')).toBeFocused({ timeout: KEYS_BACK });
  });

  test('font dialog previews the selected text, cut to its first line', async ({ page }) => {
    await openWrite(page);
    await page.keyboard.type(
      'Tenmoku glaze, iron-saturated and glossy, breaking brown at the rims of every bowl in the kiln',
    );
    await select(page, 0, 0, 13);
    await page.keyboard.press('Control+d');
    const preview = page.getByTestId('write-font-preview');
    await expect(preview).toHaveText('Tenmoku glaze');
    await page.keyboard.press('Escape');
    // A long selection: its first 60 characters, shown on one line.
    await select(page, 0, 0, 90);
    await page.keyboard.press('Control+d');
    await expect(preview).toHaveText(/^Tenmoku glaze, iron-saturated/);
    const box = await preview.locator('span').boundingBox();
    const outer = await preview.boundingBox();
    expect(box && outer && box.width <= outer.width).toBe(true);
    await page.keyboard.press('Escape');
  });

  test('autocorrect: typos, capitals, symbols and the replace as you type list', async ({ page }) => {
    await openWrite(page);
    await page.keyboard.type('teh kiln is hot. it glows on monday (c) ');
    await until(
      () => paragraphs(page),
      (p) => p[0] === 'The kiln is hot. It glows on Monday © ',
      'corrected as typed',
    );
    // Ctrl+Z right after a correction undoes it (Word).
    await page.keyboard.press('Enter');
    await page.keyboard.type('adn ');
    await until(
      () => paragraphs(page),
      (p) => p[1] === 'And ',
      'corrected',
    );
    await page.keyboard.press('Control+z');
    await until(
      () => paragraphs(page),
      (p) => p[1] === 'adn ' || p[1] === 'Adn ',
      'undone',
    );
    // File ▸ Options ▸ Proofing ▸ AutoCorrect: a new entry, then used as you type.
    await page.keyboard.press('Enter');
    await run(page, 'ui.options.open');
    const options = page.getByRole('dialog', { name: /Options/ });
    await expect(options).toBeVisible();
    await options
      .getByRole('navigation', { name: 'Options sections' })
      .getByRole('button', { name: 'Proofing' })
      .click();
    await options.getByRole('textbox', { name: 'Replace' }).fill('kln');
    await options.getByRole('textbox', { name: 'With' }).fill('Kiln');
    await options.getByRole('button', { name: 'Add' }).click();
    await expect(options.getByRole('list', { name: 'AutoCorrect entries' })).toContainText('kln');
    await options.getByRole('button', { name: 'Close' }).last().click();
    await page.getByTestId('kiln-input-proxy').focus();
    await page.keyboard.type('the kln ');
    await until(
      () => paragraphs(page),
      (p) => p[p.length - 1] === 'The Kiln ',
      'the new entry',
    );
  });

  test('drag and drop text: move a selection and copy it with Ctrl', async ({ page }) => {
    await openWrite(page);
    await page.keyboard.type('Alpha beta gamma');
    await select(page, 0, 0, 6);
    const from = await pointAt(page, 0, 2);
    const to = await pointAt(page, 0, 16);
    await select(page, 0, 0, 6);
    await page.mouse.move(from.x + 4, from.y);
    await page.mouse.down();
    await page.mouse.move((from.x + to.x) / 2, to.y, { steps: 5 });
    await page.mouse.move(to.x, to.y, { steps: 5 });
    await expect(page.getByTestId('write-drop-caret')).toBeVisible();
    await page.mouse.up();
    await until(
      () => paragraphs(page),
      (p) => p[0] === 'beta gammaAlpha ',
      'moved',
    );
    // Ctrl copies.
    await select(page, 0, 0, 5);
    const a = await pointAt(page, 0, 2);
    const b = await pointAt(page, 0, 16);
    await select(page, 0, 0, 5);
    await page.mouse.move(a.x + 3, a.y);
    await page.mouse.down();
    await page.keyboard.down('Control');
    await page.mouse.move((a.x + b.x) / 2, b.y, { steps: 5 });
    await page.mouse.move(b.x, b.y, { steps: 5 });
    await page.mouse.up();
    await page.keyboard.up('Control');
    await until(
      () => paragraphs(page),
      (p) => p[0] === 'beta gammaAlpha beta ',
      'copied',
    );
    // One undo step.
    await page.keyboard.press('Control+z');
    await until(
      () => paragraphs(page),
      (p) => p[0] === 'beta gammaAlpha ',
      'undo the copy',
    );
  });

  // The Office file loop: every feature fixture opens, takes a Word user's edits (a new
  // paragraph styled Heading 1, a comment on it, a table row), saves and reopens with them.
  for (const name of readdirSync(repo('fixtures/docx/features')).filter((f) => f.endsWith('.docx'))) {
    test(`office file loop: ${name} opens, is edited, saves and reopens`, async ({ page }) => {
      // The search for the first table's last cell visits every paragraph, one query each: the
      // large table's file takes about two minutes here and over two and a half on a CI runner.
      if (name === 'large-table.docx') test.slow();
      await openFile(page, { name, buffer: readFileSync(repo(`fixtures/docx/features/${name}`)) });
      await clearSaved(page);
      const before = await paragraphs(page);
      expect(before.length).toBeGreaterThan(0);
      const comments0 = (await writeQuery<Json[]>(page, 'Comments')).length;
      // A paragraph at the end, styled Heading 1, with a comment.
      await page.getByTestId('kiln-input-proxy').focus();
      await page.keyboard.press('Control+End');
      await page.keyboard.press('Enter');
      await page.keyboard.type('Kiln round trip');
      await page.keyboard.press('Control+Alt+1');
      const last = (await paragraphs(page)).lastIndexOf('Kiln round trip');
      expect(last).toBeGreaterThanOrEqual(0);
      await select(page, last, 0, 4);
      await run(page, 'write.comment.new', { text: 'Checked in Kiln.' });
      // A table row, as a Word user adds one: Tab in the last cell of the first table (a
      // fixture without a table gets one first).
      const tableAt = async (i: number) => {
        await caretAt(page, i, 0);
        return (await format(page)).table as Json | null;
      };
      const paras = await paragraphs(page);
      let lastCell = -1;
      let rows = 0;
      for (let i = 0; i < paras.length; i++) {
        const t = await tableAt(i);
        if (!t) {
          if (lastCell >= 0) break;
          continue;
        }
        rows = t.rows as number;
        if (t.row === rows - 1 && t.col === (t.cols as number) - 1) lastCell = i;
      }
      // The fixture with tables gets its row in the first of them.
      if (name === 'tables.docx') expect(lastCell).toBeGreaterThanOrEqual(0);
      if (lastCell < 0) {
        const end = (await paragraphs(page)).lastIndexOf('Kiln round trip');
        await caretAt(page, end, 0);
        await page.getByTestId('kiln-input-proxy').focus();
        await page.keyboard.press('End');
        await page.keyboard.press('Enter');
        await run(page, 'write.table.insert', { rows: 2, cols: 2 });
        rows = 2;
        await until(
          async () => ((await format(page)).table as Json | null)?.rows,
          (n) => n === 2,
          'the new table',
        );
        await page.getByTestId('kiln-input-proxy').focus();
        await page.keyboard.press('Tab');
        await page.keyboard.press('Tab');
        await page.keyboard.press('Tab');
      } else {
        await caretAt(page, lastCell, 0);
        await page.getByTestId('kiln-input-proxy').focus();
        await page.keyboard.press('End');
      }
      await page.keyboard.press('Tab');
      await page.keyboard.type('Kiln row');
      rows += 1;
      await until(
        async () => {
          const t = (await format(page)).table as Json | null;
          return t ? [t.rows, t.row, t.col] : null;
        },
        (v) => JSON.stringify(v) === JSON.stringify([rows, rows - 1, 0]),
        'typing in the new last row',
      );
      // Save (Ctrl+S keeps the name), then open the saved file.
      await noFilePickers(page);
      await page.keyboard.press('Control+s');
      const saved = await savedBytes(page, name);
      await openFile(page, { name, buffer: saved });
      const after = await paragraphs(page);
      expect(after).toContain('Kiln round trip');
      const at = after.lastIndexOf('Kiln round trip');
      await caretAt(page, at, 1);
      await until(
        async () => (await format(page)).style_id,
        (st) => st === 'Heading1',
        'Heading 1 kept',
      );
      const comments = await writeQuery<Json[]>(page, 'Comments');
      expect(comments.length).toBe(comments0 + 1);
      expect(
        comments.some((c) => c.text === 'Checked in Kiln.' || JSON.stringify(c).includes('Checked in Kiln.')),
      ).toBe(true);
      // The new row is there, with its text, in a table of one row more.
      const rowAt = after.indexOf('Kiln row');
      expect(rowAt).toBeGreaterThanOrEqual(0);
      const t = await tableAt(rowAt);
      expect(t ? [t.rows, t.row, t.col] : null).toEqual([rows, rows - 1, 0]);
      // Every paragraph that was there is still there, in order.
      const kept = before.filter((t) => t.trim() !== '');
      let k = 0;
      for (const t of after) if (k < kept.length && t === kept[k]) k++;
      expect(k).toBe(kept.length);
    });
  }

  test('footnote continued across pages: a long note starts under its reference and goes on', async ({
    page,
  }) => {
    await openWrite(page);
    // Fill most of the first page, then a footnote too long for what is left.
    for (let i = 0; i < 27; i++) {
      await page.keyboard.type(`Line ${i + 1} of the firing record.`);
      await page.keyboard.press('Enter');
    }
    await page.keyboard.type('The reference');
    const ref = await caretRect(page);
    await page.keyboard.press('Control+Alt+F');
    await until(
      async () => (await features(page)).in_note,
      (n) => n === true,
      'in a footnote',
    );
    for (let i = 0; i < 18; i++) {
      await page.keyboard.type(`Note line ${i + 1}: cone, atmosphere, shelf and the time of the firing.`);
      if (i < 17) await page.keyboard.press('Enter');
    }
    // The reference stays on page 1; the end of the note is on page 2.
    expect(await pageOf(page, ref.y + ref.height / 2)).toBe(0);
    const end = await until(
      () => caretRect(page),
      () => true,
      'the caret at the end of the note',
    );
    await until(
      () => pageOf(page, end.y + end.height / 2),
      (p) => p === 1,
      'the note continues on page 2',
    );
    await page.waitForTimeout(400);
    await snap(page, 'footnote-continued-light-desktop.png');
  });

  test('show markup balloons: revisions in balloons, all inline, accept from a balloon', async ({ page }) => {
    await openWrite(page);
    await page.keyboard.type('The glaze ran on shelf two.');
    await page.keyboard.press('Control+Shift+E');
    await select(page, 0, 10, 14);
    await page.keyboard.press('Delete');
    await run(page, 'write.review.markup_mode', { mode: 'All' });
    // Review ▸ Show Markup ▾ ▸ Balloons ▸ Show Revisions in Balloons.
    await page.getByRole('tab', { name: 'Review' }).click();
    await page.locator('[data-command="write.review.show_markup"]').first().click();
    // The submenu from the keyboard (Right opens it; its first item is the one wanted).
    await page.getByRole('menuitem', { name: 'Balloons', exact: true }).focus();
    await page.keyboard.press('ArrowRight');
    await expect(page.getByRole('menuitem', { name: 'Show Revisions in Balloons' })).toBeFocused();
    await page.keyboard.press('Enter');
    const balloon = page.getByTestId('write-revision-balloon');
    await expect(balloon).toHaveCount(1);
    await expect(balloon).toContainText('Deleted');
    await expect(balloon).toContainText('ran ');
    await page.waitForTimeout(300);
    await snap(page, 'revision-balloons-light-desktop.png');
    // Show All Revisions Inline: no balloons.
    await run(page, 'write.review.balloons', { mode: 'Inline' });
    await expect(balloon).toHaveCount(0);
    // Back in balloons; Accept on the balloon removes the deletion.
    await run(page, 'write.review.balloons', { mode: 'Revisions' });
    await expect(balloon).toHaveCount(1);
    await balloon.getByRole('button', { name: 'Accept' }).click();
    await until(
      () => paragraphs(page),
      (p) => p[0] === 'The glaze on shelf two.',
      'accepted',
    );
    await expect(balloon).toHaveCount(0);
  });

  test('views: print layout with page shadows, margins and the header area', async ({ page }) => {
    await openFile(page, {
      name: 'headers-footers.docx',
      buffer: readFileSync(repo('fixtures/docx/features/headers-footers.docx')),
    });
    await page.getByRole('radio', { name: 'Print Layout' }).click();
    const pages = await writeQuery<{ rects: Rect[] }>(page, 'Pages');
    expect(pages.rects.length).toBeGreaterThan(0);
    // Pages stand apart (the gap shows the shadow) and keep their paper size.
    const p0 = pages.rects[0] as Rect;
    expect(Math.round(p0.width)).toBe(816);
    if (pages.rects[1]) expect((pages.rects[1] as Rect).y).toBeGreaterThan(p0.y + p0.height + 8);
    // Double-click in the header area opens the header.
    const top = await screenRect(page, { x: p0.x + p0.width / 2, y: p0.y + 30, width: 1, height: 1 });
    if (!top) throw new Error('no page');
    await page.mouse.dblclick(top.left, top.top);
    await expect(page.getByRole('tab', { name: 'Header & Footer' })).toBeVisible();
    await page.keyboard.press('Escape');
    await page.waitForTimeout(300);
    await snap(page, 'print-layout-light-desktop.png');
  });

  test('views: web layout reflows to the window width', async ({ page }) => {
    await openWrite(page);
    await page.keyboard.type('A paragraph long enough to show where Web Layout wraps it. '.repeat(6));
    await page.getByRole('radio', { name: 'Web Layout' }).click();
    const width = async () => (await writeQuery<{ rects: Rect[] }>(page, 'Pages')).rects[0]?.width ?? 0;
    const wide = await until(width, (w) => w > 1000, 'the window width');
    await page.setViewportSize({ width: 1000, height: 800 });
    await until(width, (w) => w < wide - 200, 'narrower with the window');
    await page.getByRole('radio', { name: 'Print Layout' }).click();
    await until(width, (w) => Math.round(w) === 816, 'the paper again');
  });

  test('views: read mode with columns, page turning and zoomed pictures', async ({ page }) => {
    await openWrite(page);
    for (let i = 0; i < 80; i++) {
      await page.keyboard.type(
        `Paragraph ${i + 1} about the kiln, the glaze and the long cooling after the firing.`,
      );
      await page.keyboard.press('Enter');
    }
    await caretAt(page, 0, 0);
    await run(page, 'write.insert.picture', { name: 'kiln.png', bytes: png(1600, 1200), floating: false });
    await page.getByRole('radio', { name: 'Read Mode' }).click();
    // Columns: screen-sized pages, two side by side.
    const pages = await until(
      () => writeQuery<{ rects: Rect[] }>(page, 'Pages'),
      (p) => p.rects.length > 2 && (p.rects[1]?.x ?? 0) > (p.rects[0]?.x ?? 0),
      'pages side by side',
    );
    expect(pages.rects[0]?.width ?? 0).toBeLessThan(760);
    // The picture shrinks to the column.
    const pic = (await objects(page))[0] as Json;
    expect((pic.rect as Rect).width).toBeLessThanOrEqual((pages.rects[0]?.width ?? 0) - 40);
    const where = page.getByTestId('write-read-screen');
    await expect(where).toHaveText(/^Screen 1 of [2-9]$/);
    await page.keyboard.press('PageDown');
    await expect(where).toHaveText(/^Screen 2 of/);
    await page.getByTestId('write-read-next').click();
    await expect(where).toHaveText(/^Screen 3 of/);
    await page.getByTestId('write-read-prev').click();
    await expect(where).toHaveText(/^Screen 2 of/);
    await page.keyboard.press('Home');
    await expect(where).toHaveText(/^Screen 1 of/);
    // A double-click on the picture zooms it; Esc closes the zoom, then leaves Read Mode.
    await page.waitForTimeout(600);
    const r = await screenRect(page, pic.rect as Rect);
    if (!r) throw new Error('no picture');
    await page.mouse.dblclick(r.left + r.width / 2, r.top + r.height / 2);
    const zoom = page.getByTestId('write-read-zoom');
    await expect(zoom).toBeVisible();
    await expect(zoom.getByRole('img')).toHaveAttribute('alt', 'kiln.png');
    await page.waitForTimeout(300);
    await snap(page, 'read-mode-zoom-light-desktop.png');
    await page.keyboard.press('Escape');
    await expect(zoom).toHaveCount(0);
    await page.waitForTimeout(300);
    await snap(page, 'read-mode-light-desktop.png');
    await expectNoSeriousA11yViolations(page);
    await page.keyboard.press('Escape');
    await until(
      () =>
        page.evaluate(
          () =>
            (window as unknown as { __kilnWrite?: { state: { status?: { view?: string } } } }).__kilnWrite
              ?.state.status?.view ?? '',
        ),
      (v) => v === 'PrintLayout',
      'Esc leaves Read Mode',
    );
  });

  test('page movement: vertical by default, side to side from the View tab', async ({ page }) => {
    await openWrite(page);
    for (let i = 0; i < 60; i++) {
      await page.keyboard.type(`Line ${i + 1}`);
      await page.keyboard.press('Enter');
    }
    const rects = async () => (await writeQuery<{ rects: Rect[] }>(page, 'Pages')).rects;
    const v = await until(rects, (r) => r.length >= 2, 'two pages');
    // Vertical: one under the other, same left edge.
    expect(Math.abs((v[1] as Rect).x - (v[0] as Rect).x)).toBeLessThan(1);
    expect((v[1] as Rect).y).toBeGreaterThan((v[0] as Rect).y);
    await page.getByRole('tab', { name: 'View', exact: true }).click();
    await page.locator('[data-command="suite.view.side_to_side"]').first().click();
    await until(
      rects,
      (r) =>
        r.length >= 2 &&
        Math.abs((r[1] as Rect).y - (r[0] as Rect).y) < 1 &&
        (r[1] as Rect).x > (r[0] as Rect).x,
      'side by side',
    );
    await page.waitForTimeout(300);
    await snap(page, 'side-to-side-light-desktop.png');
    await page.locator('[data-command="suite.view.vertical_pages"]').first().click();
    await until(rects, (r) => r.length >= 2 && (r[1] as Rect).y > (r[0] as Rect).y, 'vertical again');
  });

  test('zoom dialog and the zoom slider: presets, percent from 10 to 500, page width', async ({ page }) => {
    await openWrite(page);
    await page.keyboard.type('Zoom');
    const zoom = () =>
      page.evaluate(() => (window as unknown as KilnWindow).__kilnWrite?.controller?.getStats().zoom ?? 0);
    await page.getByRole('tab', { name: 'View', exact: true }).click();
    await page.locator('[data-command="ui.write.zoom_dialog"]').first().click();
    const z = dialog(page, 'write-zoom-dialog');
    await expect(z).toBeVisible();
    await z.getByRole('radio', { name: '200%' }).check();
    await z.getByRole('button', { name: 'OK' }).click();
    await until(zoom, (v) => Math.abs(v - 2) < 0.01, '200%');
    await page.locator('[data-command="ui.write.zoom_dialog"]').first().click();
    await z.getByRole('spinbutton', { name: 'Percent' }).fill('35');
    await z.getByRole('button', { name: 'OK' }).click();
    await until(zoom, (v) => Math.abs(v - 0.35) < 0.01, '35%');
    // Out of range: clamped to 500%.
    await page.locator('[data-command="ui.write.zoom_dialog"]').first().click();
    await z.getByRole('spinbutton', { name: 'Percent' }).fill('900');
    await z.getByRole('button', { name: 'OK' }).click();
    await until(zoom, (v) => Math.abs(v - 5) < 0.01, 'at most 500%');
    // Page width fits the page to the window.
    await page.locator('[data-command="ui.write.zoom_dialog"]').first().click();
    await z.getByRole('radio', { name: 'Page width' }).check();
    await z.getByRole('button', { name: 'OK' }).click();
    await until(zoom, (v) => v > 1 && v < 2, 'page width');
    // The status bar's slider and its + button.
    const slider = page.getByRole('slider', { name: /Zoom/ });
    await slider.focus();
    await page.keyboard.press('Home');
    await until(zoom, (v) => Math.abs(v - 0.1) < 0.01, '10% from the slider');
    await page.keyboard.press('End');
    await until(zoom, (v) => Math.abs(v - 5) < 0.01, '500% from the slider');
  });

  test('window split: two panes of the same document, typing in the second', async ({ page }) => {
    await openWrite(page);
    for (let i = 0; i < 60; i++) {
      await page.keyboard.type(`Paragraph ${i + 1} about the kiln and its glaze.`);
      await page.keyboard.press('Enter');
    }
    await page.getByRole('tab', { name: 'View', exact: true }).click();
    await page.locator('[data-command="suite.view.split"]').first().click();
    const pane = page.getByTestId('write-split-pane');
    await expect(pane).toBeVisible();
    // The second pane scrolls on its own; the first stays where it is.
    const sc = page.getByTestId('write-split-scroller');
    // The pane's height comes with its first frame: scroll once it can.
    await until(
      () =>
        sc.evaluate((e) => {
          e.scrollTop = 1700;
          return e.scrollTop;
        }),
      (t) => t > 1600,
      'the second pane scrolled',
    );
    await page.waitForTimeout(800);
    const before = await page.evaluate(
      () => (window as unknown as KilnWindow).__kilnWrite?.controller?.getStats().scrollY ?? 0,
    );
    const box = await sc.boundingBox();
    if (!box) throw new Error('no pane');
    // Click on a line of the second pane and type there.
    await page.mouse.click(box.x + box.width / 2 - 80, box.y + 60);
    await until(
      async () => {
        const sel = await page.evaluate(async () => {
          const w = (window as unknown as KilnWindow).__kilnWrite as WriteHandle;
          return (await w.client.query(w.doc, 'Selection')) as {
            Selection: { kind: { Text?: { head: { block: number } } } };
          };
        });
        const ids = await page.evaluate(async () => {
          const w = (window as unknown as KilnWindow).__kilnWrite as WriteHandle;
          const r = (await w.client.query(w.doc, 'Paragraphs')) as { Paragraphs: { id: number }[] };
          return r.Paragraphs.map((p) => p.id);
        });
        return ids.indexOf(sel.Selection.kind.Text?.head.block ?? -1);
      },
      (i) => i > 20,
      'the caret in the second pane',
    );
    await page.keyboard.press('Home');
    await page.keyboard.type('Split here: ');
    const typed = await until(
      () => paragraphs(page),
      (p) => p.some((t) => t.startsWith('Split here: Paragraph')),
      'typed in the second pane',
    );
    const index = typed.findIndex((t) => t.startsWith('Split here: '));
    expect(index).toBeGreaterThan(20);
    const after = await page.evaluate(
      () => (window as unknown as KilnWindow).__kilnWrite?.controller?.getStats().scrollY ?? 0,
    );
    expect(Math.abs(after - before)).toBeLessThan(2);
    await page.waitForTimeout(500);
    await snap(page, 'split-light-desktop.png');
    // View ▸ Split again removes it.
    await page.locator('[data-command="suite.view.split"]').first().click();
    await expect(pane).toHaveCount(0);
  });

  test('macros explained: View ▸ Macros says macros are kept and never run', async ({ page }) => {
    await openWrite(page);
    await page.getByRole('tab', { name: 'View', exact: true }).click();
    await page.locator('[data-command="ui.macros.explain"]').first().click();
    const toast = page.getByText('Kiln never runs macros');
    await expect(toast).toBeVisible();
    await expect(page.getByText(/kept exactly as they are/)).toBeVisible();
  });

  test('table properties: table, row, column and cell from Table Layout ▸ Properties', async ({ page }) => {
    await openWrite(page);
    await page.keyboard.type('Firing schedule');
    await page.keyboard.press('Enter');
    await run(page, 'write.table.insert', { rows: 3, cols: 3 });
    await page.keyboard.type('Cone');
    await page.locator('[id$="-tab-tableLayout"]').click();
    await page.locator('[data-command="ui.write.table_properties_dialog"]').first().click();
    const d = dialog(page, 'write-table-properties-dialog');
    await expect(d).toBeVisible();
    // Table: centred, preferred width 80 %.
    await d.getByRole('tab', { name: 'Table' }).click();
    await d.getByRole('radio', { name: 'Center' }).check();
    await d.getByRole('button', { name: 'OK' }).click();
    await until(
      () => writeQuery<Json | null>(page, 'TableProperties'),
      (t) => t?.alignment === 'Center',
      'centred',
    );
    // Row: at least 30 pt, repeat as header row; Cell: centred vertically.
    await page.locator('[data-command="ui.write.table_properties_dialog"]').first().click();
    await d.getByRole('tab', { name: 'Row' }).click();
    await d.getByRole('checkbox', { name: /Repeat as header row/ }).check();
    await d.getByRole('tab', { name: 'Cell' }).click();
    await d.getByRole('radio', { name: 'Center' }).check();
    await d.getByRole('button', { name: 'OK' }).click();
    await until(
      () => writeQuery<Json | null>(page, 'TableProperties'),
      (t) => t?.row_header === true && t?.cell_v_align === 'Center',
      'row and cell',
    );
    const xml = await savedXml(page);
    expect(xml).toContain('<w:jc w:val="center"/>');
    expect(xml).toContain('<w:tblHeader/>');
    expect(xml).toContain('<w:vAlign w:val="center"/>');
    await expectNoSeriousA11yViolations(page);
  });

  test('nested tables: a table inside a cell, displayed and edited', async ({ page }) => {
    await openFile(page, {
      name: 'tables.docx',
      buffer: readFileSync(repo('fixtures/docx/features/tables.docx')),
    });
    // The first paragraph in a table cell: a 2×2 table goes in there.
    const count = (await paragraphs(page)).length;
    let cell = -1;
    for (let i = 0; i < count && cell < 0; i++) {
      await caretAt(page, i, 0);
      if ((await format(page)).table) cell = i;
    }
    expect(cell).toBeGreaterThanOrEqual(0);
    await page.getByTestId('kiln-input-proxy').focus();
    await run(page, 'write.table.insert', { rows: 2, cols: 2 });
    await page.keyboard.type('Nested cell');
    const t = await until(
      async () => (await format(page)).table as Json | null,
      (x) => ((x?.depth as number | undefined) ?? 1) >= 2,
      'a nested table',
    );
    expect(t?.depth).toBe(2);
    // Tab moves through the nested cells.
    await page.keyboard.press('Tab');
    await page.keyboard.type('next');
    const xml = await savedXml(page, 'tables.docx');
    // The new text sits inside two open tables (a w:tbl in a w:tc).
    const before = xml.slice(0, xml.indexOf('Nested cell'));
    const open = (before.match(/<w:tbl>/g)?.length ?? 0) - (before.match(/<\/w:tbl>/g)?.length ?? 0);
    expect(xml.indexOf('Nested cell')).toBeGreaterThan(0);
    expect(open).toBe(2);
    await page.waitForTimeout(300);
    await snap(page, 'nested-table-light-desktop.png');
  });

  test('crop to shape, picture corrections and the aspect ratio crop', async ({ page }) => {
    await openWrite(page);
    await run(page, 'write.insert.picture', {
      name: 'glaze.png',
      bytes: png(240, 160, [200, 90, 40]),
      floating: false,
    });
    await page.getByRole('tab', { name: 'Picture Format' }).click();
    await page
      .locator('[data-command="ui.write.crop_dialog"] button[aria-label^="More options"]')
      .first()
      .click();
    await page.getByRole('menuitem', { name: 'Crop to Shape' }).click();
    await page.getByRole('menuitem', { name: 'Oval' }).click();
    await page.waitForTimeout(300);
    await snap(page, 'crop-to-shape-light-desktop.png');
    // Corrections ▾ ▸ Brightness +20 %.
    await page.locator('.kiln-ribbon').getByRole('button', { name: 'Corrections' }).click();
    await page
      .getByRole('menuitem', { name: /Brightness: \+20%/ })
      .first()
      .click();
    const xml = await savedXml(page);
    expect(xml).toMatch(/<pic:spPr[^>]*>.*?<a:prstGeom prst="ellipse"/s);
    expect(xml).toMatch(/<a:lum bright="20000"/);
  });

  test('shape styles: the gallery, gradients, effects and text direction', async ({ page }) => {
    await openWrite(page);
    await page.keyboard.type('A shape for the firing schedule.');
    await run(page, 'write.insert.shape', {
      preset: 'roundRect',
      text: 'Bisque',
      rect: { x: 360, y: 260, width: 200, height: 110 },
    });
    await expect(page.getByRole('tab', { name: 'Shape Format' })).toBeVisible();
    await page.getByRole('tab', { name: 'Shape Format' }).click();
    // Shape Styles gallery: Intense Effect, Accent 2.
    await page
      .locator(
        '[data-command="write.shape.style"] [role="option"], [data-command="write.shape.style"] button',
      )
      .nth(9)
      .click();
    // Gradient ▾ ▸ Dark Variations ▸ Linear Down.
    await page.locator('.kiln-ribbon').getByRole('button', { name: 'Gradient' }).click();
    await page.getByRole('menuitem', { name: 'Linear Down' }).last().click();
    // Shape Effects ▾ ▸ Shadow and Text Direction ▾ ▸ Rotate All Text 270° (the Gradient
    // button's tooltip would cover Shape Effects: move away first).
    await page.mouse.move(700, 700);
    await page.evaluate(() => (document.activeElement as HTMLElement | null)?.blur());
    await page.locator('.kiln-ribbon').getByRole('button', { name: 'Shape Effects' }).click();
    await page.getByRole('menuitem', { name: /^Shadow/ }).click();
    await page.mouse.move(700, 700);
    await page.locator('.kiln-ribbon').getByRole('button', { name: 'Text Direction' }).click();
    await page.getByRole('menuitem', { name: 'Rotate All Text 270°' }).click();
    // No tooltip from the focused ribbon button in the screenshots.
    await page.mouse.move(700, 700);
    await page.evaluate(() => (document.activeElement as HTMLElement | null)?.blur());
    await page.waitForTimeout(400);
    await snap(page, 'shape-styles-light-desktop.png');
    await page.emulateMedia({ colorScheme: 'dark' });
    await page.waitForTimeout(500);
    await snap(page, 'shape-styles-dark-desktop.png');
    await page.emulateMedia({ colorScheme: 'light' });
    const xml = await savedXml(page);
    expect(xml).toContain('<a:gradFill');
    expect(xml).toContain('<a:outerShdw');
    expect(xml).toMatch(/<wps:bodyPr[^>]*vert="vert270"/);
  });

  test('header and footer position: distances from the edge in the Header & Footer tab', async ({ page }) => {
    await openWrite(page);
    await page.keyboard.type('Body text under the header.');
    await page.getByRole('tab', { name: 'Insert' }).click();
    await page.locator('[data-command="write.insert.header"]').first().click();
    await expect(page.getByRole('tab', { name: 'Header & Footer' })).toBeVisible();
    await page.getByRole('tab', { name: 'Header & Footer' }).click();
    await page.keyboard.type('Kiln log');
    const header = page.locator('[data-command="ui.write.header_distance"] input');
    // Typed with its unit: the box shows the user's unit (inches under en-US).
    await header.fill('2.5 cm');
    await header.press('Enter');
    const footer = page.locator('[data-command="ui.write.footer_distance"] input');
    await footer.fill('1.8 cm');
    await footer.press('Enter');
    await until(
      async () => (await format(page)).section as Json,
      (s) => Math.abs((s.header as number) - 70.87) < 0.5 && Math.abs((s.footer as number) - 51.02) < 0.5,
      'distances in points',
    );
    await page.keyboard.press('Escape');
    const xml = await savedXml(page);
    expect(xml).toMatch(/<w:pgMar [^>]*w:header="1417"/);
    expect(xml).toMatch(/<w:pgMar [^>]*w:footer="1020"/);
  });

  test('keep rules: keep with next and page break before from the Paragraph dialog', async ({ page }) => {
    await openWrite(page);
    for (let i = 0; i < 33; i++) {
      await page.keyboard.type(`Line ${i + 1}`);
      await page.keyboard.press('Enter');
    }
    await page.keyboard.type('Heading that must stay with its text');
    await page.keyboard.press('Enter');
    await page.keyboard.type(
      'The text it belongs to, long enough to need two lines on a page of this width. '.repeat(3),
    );
    const n = (await paragraphs(page)).length;
    const lineOf = async (para: number) => {
      await caretAt(page, para, 0);
      const r = await caretRect(page);
      return pageOf(page, r.y + r.height / 2);
    };
    // Page break before (Paragraph dialog ▸ Line and Page Breaks).
    await caretAt(page, 5, 0);
    await run(page, 'suite.format.paragraph_dialog');
    const pd = page.getByRole('dialog', { name: 'Paragraph' });
    await pd.getByRole('tab', { name: 'Line and Page Breaks' }).click();
    await pd.getByRole('checkbox', { name: 'Page break before' }).check();
    await pd.getByRole('button', { name: 'OK' }).click();
    await until(
      () => lineOf(5),
      (p) => p === 1,
      'line 6 starts page 2',
    );
    await until(
      () => lineOf(4),
      (p) => p === 0,
      'line 5 stays on page 1',
    );
    // Keep with next: the heading moves with its text.
    await caretAt(page, n - 2, 0);
    await run(page, 'suite.format.paragraph_dialog');
    await pd.getByRole('tab', { name: 'Line and Page Breaks' }).click();
    await pd.getByRole('checkbox', { name: 'Keep with next' }).check();
    await pd.getByRole('button', { name: 'OK' }).click();
    await caretAt(page, n - 2, 0);
    await until(
      async () => (await format(page)).keep_next,
      (k) => k === true,
      'keep with next set',
    );
    const h = await lineOf(n - 2);
    const body = await lineOf(n - 1);
    expect(h).toBe(body);
  });

  test('sections: a landscape section with two columns after a next page break', async ({ page }) => {
    await openWrite(page);
    await page.keyboard.type('Portrait, one column.');
    await page.getByRole('tab', { name: 'Layout', exact: true }).click();
    await page.locator('[data-command="suite.layout.breaks"]').first().click();
    await page.getByRole('menuitem', { name: /Next Page/ }).click();
    // The keys are back in the document after a ribbon command (Office).
    await expect(page.getByTestId('kiln-input-proxy')).toBeFocused();
    await page.keyboard.type('Landscape, two columns. '.repeat(40));
    await until(
      async () => (await paragraphs(page)).some((t) => t.startsWith('Landscape, two columns.')),
      (v) => v,
      'typed on the new page',
    );
    await page.locator('[data-command="suite.layout.orientation"]').first().click();
    await page.getByRole('menuitem', { name: 'Landscape' }).click();
    await page.locator('[data-command="write.layout.columns"]').first().click();
    await page.getByRole('menuitem', { name: 'Two' }).click();
    const rects = await until(
      async () => (await writeQuery<{ rects: Rect[] }>(page, 'Pages')).rects,
      (r) =>
        r.length >= 2 &&
        (r[1] as Rect).width > (r[1] as Rect).height &&
        (r[0] as Rect).width < (r[0] as Rect).height,
      'a portrait page, then a landscape one',
    );
    expect(rects.length).toBeGreaterThanOrEqual(2);
    await until(
      async () => ((await format(page)).section as Json).columns,
      (c) => c === 2,
      'two columns',
    );
    await page.mouse.move(700, 700);
    await page.evaluate(() => (document.activeElement as HTMLElement | null)?.blur());
    await page.waitForTimeout(400);
    await snap(page, 'sections-light-desktop.png');
    const xml = await savedXml(page);
    expect(xml.match(/<w:sectPr/g)?.length).toBe(2);
    expect(xml).toMatch(/<w:pgSz [^>]*w:orient="landscape"/);
    expect(xml).toMatch(/<w:cols [^>]*w:num="2"/);
  });

  test('bidi text: Arabic and Hebrew right to left, typed, with the caret moving in visual order', async ({
    page,
  }) => {
    await openFile(page, { name: 'rtl.docx', buffer: readFileSync(repo('fixtures/docx/features/rtl.docx')) });
    const p = await paragraphs(page);
    const arabic = p.findIndex((t) => /[\u0600-\u06FF]/.test(t) && !/[A-Za-z]/.test(t));
    const hebrew = p.findIndex((t) => /[\u0590-\u05FF]/.test(t));
    expect(arabic).toBeGreaterThanOrEqual(0);
    expect(hebrew).toBeGreaterThanOrEqual(0);
    for (const i of [arabic, hebrew]) {
      // Right to left: the caret at the start of the paragraph is at its right edge; Left moves
      // it on in the text (offset 1), which is to the left on the screen.
      await caretAt(page, i, 0);
      const a = await caretRect(page);
      const pages = await writeQuery<{ rects: Rect[] }>(page, 'Pages');
      const pg = pages.rects[0] as Rect;
      expect(a.x).toBeGreaterThan(pg.x + pg.width / 2);
      await page.getByTestId('kiln-input-proxy').focus();
      await page.keyboard.press('ArrowLeft');
      const b = await until(
        () => caretRect(page),
        (r) => r.x !== a.x,
        'the caret moved',
      );
      expect(b.x).toBeLessThan(a.x);
      const sel = await page.evaluate(async () => {
        const w = (window as unknown as KilnWindow).__kilnWrite as WriteHandle;
        return (await w.client.query(w.doc, 'Selection')) as {
          Selection: { kind: { Text: { head: { offset: number } } } };
        };
      });
      expect(sel.Selection.kind.Text.head.offset).toBe(1);
    }
    // Typing Arabic in the Arabic paragraph shapes it (joined letters, the Arabic font).
    await caretAt(page, arabic, 0);
    await page.keyboard.insertText('كيلن ');
    await until(
      () => paragraphs(page),
      (t) => t[arabic]?.startsWith('كيلن ') ?? false,
      'typed Arabic',
    );
    await page.waitForTimeout(400);
    await snap(page, 'bidi-light-desktop.png');
  });

  test('screenshots: the Write chrome in light and dark, desktop and phone', async ({
    page,
    browser,
    baseURL,
  }) => {
    const file = {
      name: 'images-anchored.docx',
      buffer: readFileSync(repo('fixtures/docx/features/images-anchored.docx')),
    };
    await openFile(page, file);
    await page.evaluate(() => document.fonts.ready);
    await page.waitForTimeout(600);
    await snap(page, 'chrome-light-desktop.png');
    await page.getByRole('tab', { name: 'Insert' }).click();
    await page.waitForTimeout(300);
    await snap(page, 'insert-tab-light-desktop.png');
    await page.emulateMedia({ colorScheme: 'dark' });
    await page.waitForTimeout(600);
    await snap(page, 'insert-tab-dark-desktop.png');
    await page.getByRole('tab', { name: 'View', exact: true }).click();
    await page.waitForTimeout(300);
    await snap(page, 'view-tab-dark-desktop.png');
    await expectNoSeriousA11yViolations(page);
    for (const scheme of ['light', 'dark'] as const) {
      const ctx = await browser.newContext({
        baseURL,
        viewport: { width: 412, height: 915 },
        colorScheme: scheme,
      });
      const phone = await ctx.newPage();
      try {
        await openFile(phone, file);
        await phone.evaluate(() => document.fonts.ready);
        await phone.waitForTimeout(800);
        await snap(phone, `chrome-${scheme}-phone.png`);
        // Nothing overflows the phone's width.
        const wide = await phone.evaluate(() => document.documentElement.scrollWidth);
        expect(wide).toBeLessThanOrEqual(412);
      } finally {
        await ctx.close();
      }
    }
  });
});
