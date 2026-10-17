/**
 * Kiln Write's advanced features end to end (WP-66, PLAN.md §2.2): comments with threads and
 * balloons, track changes with markup modes and accept/reject, tables of contents, footnotes and
 * endnotes, captions and cross-references, floating objects (wrap, position, drag, order, the
 * Selection pane), text boxes and shapes, Format Painter, Markdown shortcuts and smart
 * typography, Paste Markdown, code blocks and callouts, cover pages, focus mode, word goals,
 * spelling, hyphenation and document properties — through the ribbon, the shortcuts and the
 * panes of the web build, checked against what the core reports, with the screenshots of the
 * visual review (light and dark, desktop and phone).
 */
import { fileURLToPath } from 'node:url';
import { expect, type Page, test } from '@playwright/test';
import { expectNoSeriousA11yViolations } from '../../packages/testing/src/playwright/a11y.ts';

const repo = (p: string) => fileURLToPath(new URL(`../../${p}`, import.meta.url));
const shot = (name: string) => repo(`e2e/screenshots/write-features/${name}`);

interface WriteHandle {
  doc: number;
  client: {
    query(doc: number, q: unknown): Promise<Record<string, unknown>>;
    execute(doc: number, c: unknown): Promise<unknown>;
  };
  controller: {
    getStats(): { tiles: number; zoom: number };
    clientRectOf(r: { x: number; y: number; width: number; height: number }): {
      left: number;
      top: number;
      width: number;
      height: number;
    };
  } | null;
}
interface KilnHandle {
  registry: { execute(id: string, args?: unknown): Promise<{ status: string }> };
}
type KilnWindow = { __kilnWrite?: WriteHandle; __kiln?: KilnHandle };

type Json = Record<string, unknown>;

async function openWrite(page: Page): Promise<void> {
  await page.goto('/#/write');
  await page.waitForFunction(
    () => ((window as unknown as KilnWindow).__kilnWrite?.controller?.getStats().tiles ?? 0) > 0,
    null,
    { timeout: 45_000 },
  );
  await page.getByTestId('kiln-input-proxy').focus();
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
          return check(last);
        },
        { message: what, timeout: 15_000 },
      )
      .toBe(true);
  } catch (e) {
    throw new Error(`${what}: last value ${JSON.stringify(last)?.slice(0, 2000)}\n${String(e)}`);
  }
  return last as T;
}

/** Selects text of the body by paragraph index and char offsets. */
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

async function caretAt(page: Page, para: number, offset: number): Promise<void> {
  await select(page, para, offset, offset);
}

/** The screen point of a caret at a paragraph offset (its vertical middle). */
async function pointAt(page: Page, para: number, offset: number): Promise<{ x: number; y: number }> {
  const rect = await page.evaluate(
    async ([p, o]) => {
      const w = (window as unknown as KilnWindow).__kilnWrite as WriteHandle;
      const r = (await w.client.query(w.doc, 'Paragraphs')) as { Paragraphs: { id: number }[] };
      const block = r.Paragraphs[p as number]?.id ?? 0;
      await w.client.execute(w.doc, { Write: { SelectTo: { pos: { block, offset: o }, extend: false } } });
      const s = (await w.client.query(w.doc, 'Selection')) as {
        Selection: { caret: { rect: { x: number; y: number; width: number; height: number } } };
      };
      return w.controller?.clientRectOf(s.Selection.caret.rect) ?? null;
    },
    [para, offset] as const,
  );
  if (!rect) throw new Error('no caret');
  return { x: rect.left + 1, y: rect.top + rect.height / 2 };
}

/** Drags the mouse over text from one offset to another of a paragraph. */
async function dragOver(page: Page, para: number, from: number, to: number): Promise<void> {
  const a = await pointAt(page, para, from);
  const b = await pointAt(page, para, to);
  await page.mouse.move(a.x, a.y);
  await page.mouse.down();
  await page.mouse.move((a.x + b.x) / 2, b.y, { steps: 4 });
  await page.mouse.move(b.x, b.y, { steps: 4 });
  await page.mouse.up();
}

test.describe('Kiln Write features', () => {
  test.describe.configure({ timeout: 120_000 });

  test('comments thread: new from Ctrl+Alt+M, balloon, reply, resolve, pane, delete', async ({ page }) => {
    await openWrite(page);
    await page.keyboard.type('Please label anything glazed with the new tenmoku.');
    await select(page, 0, 7, 12);
    await page.keyboard.press('Control+Alt+M');
    const composer = page.getByTestId('write-comment-composer');
    await expect(composer).toBeVisible();
    await composer
      .getByRole('textbox', { name: 'New comment' })
      .fill('Can we keep the tenmoku pieces off shelf 1?');
    await composer.getByRole('button', { name: 'Comment' }).click();
    await until(
      () => writeQuery<Json[]>(page, 'Comments'),
      (c) => c.length === 1 && c[0]?.anchor_text === 'label',
      'a comment on "label"',
    );
    // The balloon beside the page (Simple Markup shows comments).
    const card = page.getByTestId('write-comment-card').first();
    await expect(card).toBeVisible();
    await expect(card).toContainText('Can we keep the tenmoku pieces off shelf 1?');
    // Reply and resolve on the card.
    await card.getByRole('button', { name: 'Reply' }).click();
    await card.getByRole('textbox', { name: 'Reply' }).fill('Yes, shelf 3.');
    await card.getByRole('textbox', { name: 'Reply' }).press('Control+Enter');
    await until(
      () => writeQuery<Json[]>(page, 'Comments'),
      (c) => c.length === 2 && c[1]?.parent === c[0]?.id,
      'a reply in the thread',
    );
    await expect(card).toContainText('Yes, shelf 3.');
    // The tab strip's Comments button counts the open thread.
    await expect(page.locator('.kiln-appframe__strip-btn[data-command="suite.review.show_comments"]')).toContainText(
      'Comments1',
    );
    // The Comments pane lists the thread; Resolve there.
    await run(page, 'ui.write.comments_pane');
    const pane = page.getByTestId('write-comments-pane');
    await expect(pane).toBeVisible();
    await expect(pane).toContainText('1 open · 0 resolved');
    await pane
      .getByRole('button', { name: /Resolve/ })
      .first()
      .click();
    await until(
      () => writeQuery<Json[]>(page, 'Comments'),
      (c) => c[0]?.resolved === true,
      'resolved',
    );
    await expect(pane).toContainText('0 open · 1 resolved');
    await expectNoSeriousA11yViolations(page);
    // Next Comment and Delete Comment from the Review tab.
    await caretAt(page, 0, 0);
    await run(page, 'suite.review.next_comment');
    await until(
      () => features(page),
      (f) => f.comment !== null,
      'Next Comment goes to the comment',
    );
    await run(page, 'suite.review.delete_comment');
    await until(
      () => writeQuery<Json[]>(page, 'Comments'),
      (c) => c.length === 0,
      'deleted',
    );
  });

  test('accept/reject: track changes, markup display modes and the Reviewing pane', async ({ page }) => {
    await openWrite(page);
    await page.keyboard.type('The kiln glows.');
    await page.keyboard.press('Control+Shift+E');
    await until(
      () => features(page),
      (f) => f.tracking === true,
      'Track Changes on (Ctrl+Shift+E)',
    );
    await page.getByRole('tab', { name: 'Review', exact: true }).click();
    await expect(page.locator('[data-command="write.review.track_changes"]').first()).toHaveAttribute(
      'aria-pressed',
      'true',
    );
    await page.getByTestId('kiln-input-proxy').focus();
    await page.keyboard.press('End');
    await page.keyboard.type(' Bright.');
    await page.keyboard.press('Home');
    await page.keyboard.press('Delete');
    await page.keyboard.press('Delete');
    await page.keyboard.press('Delete');
    await page.keyboard.press('Delete');
    await until(
      () => writeQuery<Json>(page, 'Revisions'),
      (r) => r.insertions === 1 && r.deletions === 1,
      'an insertion and a deletion',
    );
    // All Markup shows the deleted text struck through; No Markup and Original change the text.
    await run(page, 'write.review.markup_mode', { mode: 'All' });
    await until(
      () => features(page),
      (f) => f.markup === 'All',
      'All Markup',
    );
    await run(page, 'write.review.markup_mode', { mode: 'Original' });
    await until(
      () => features(page),
      (f) => f.markup === 'Original',
      'Original',
    );
    await run(page, 'write.review.markup_mode', { mode: 'All' });
    // The Reviewing pane lists both changes; reject the deletion there.
    await run(page, 'write.review.reviewing_pane');
    const pane = page.getByTestId('write-reviewing-pane');
    await expect(pane.getByTestId('write-revision')).toHaveCount(2);
    await pane.getByRole('button', { name: /^Reject deleted/ }).click();
    await until(
      () => paragraphs(page),
      (p) => p[0] === 'The kiln glows. Bright.',
      'the deletion rejected',
    );
    // Accept All Changes from the ribbon's split button.
    await run(page, 'write.review.accept', { scope: 'All' });
    await until(
      () => writeQuery<Json>(page, 'Revisions'),
      (r) => (r.entries as unknown[]).length === 0,
      'all accepted',
    );
    expect((await paragraphs(page))[0]).toBe('The kiln glows. Bright.');
    // Reject: a new insertion goes away.
    await page.keyboard.press('End');
    await page.keyboard.type(' Ash');
    await run(page, 'write.review.reject', { scope: 'All' });
    await until(
      () => paragraphs(page),
      (p) => p[0] === 'The kiln glows. Bright.',
      'rejected',
    );
    await page.screenshot({ path: shot('review-light-desktop.png') });
  });

  test('toc insert and update from the References tab, clickable entries', async ({ page }) => {
    await openWrite(page);
    for (const [text, style] of [
      ['What goes in the kiln', 'Heading1'],
      ['Shelves are loaded from the back.', 'Normal'],
      ['Loading order', 'Heading2'],
      ['Tall pieces first.', 'Normal'],
      ['The schedule', 'Heading1'],
    ] as const) {
      await page.keyboard.type(text);
      await run(page, 'write.styles.apply', { style_id: style });
      await page.keyboard.press('Enter');
    }
    await caretAt(page, 0, 0);
    await page.getByRole('tab', { name: 'References' }).click();
    await page.locator('[data-command="write.references.table_of_contents"]').first().click();
    await page.getByRole('menuitem', { name: /Automatic Table 1/ }).click();
    await until(
      () => features(page),
      (f) => f.has_toc === true,
      'a table of contents',
    );
    const paras = await paragraphs(page);
    expect(paras[0]).toBe('Contents');
    expect(paras.some((p) => p.startsWith('Loading order\t'))).toBe(true);
    // Update: a page break before "The schedule" (at the end of the paragraph above) moves it
    // to page 2.
    const schedule = paras.indexOf('The schedule');
    await caretAt(page, schedule - 1, paras[schedule - 1]?.length ?? 0);
    await page.getByTestId('kiln-input-proxy').focus();
    await page.keyboard.press('Control+Enter');
    await run(page, 'write.references.update_table', { entire: false });
    await until(
      () => paragraphs(page),
      (p) => p.some((x) => x === 'The schedule\t2'),
      'page numbers updated',
    );
    // Ctrl+click on an entry goes to its heading.
    const entry = (await paragraphs(page)).findIndex((p) => p.startsWith('Loading order\t'));
    await caretAt(page, entry, 2);
    await run(page, 'write.insert.follow_link');
    await until(
      () => format(page),
      (f) => f.style_id === 'Heading2',
      'the caret is in the heading',
    );
    // Update entire table picks up a new heading.
    await page.keyboard.press('Control+End');
    await page.keyboard.press('Enter');
    await page.keyboard.type('Unloading day');
    await run(page, 'write.styles.apply', { style_id: 'Heading1' });
    await run(page, 'write.references.update_table', { entire: true });
    await until(
      () => paragraphs(page),
      (p) => p.some((x) => x.startsWith('Unloading day\t')),
      'entire table updated',
    );
  });

  test('footnote and endnote: Ctrl+Alt+F, Show Notes, Next Footnote, Ctrl+Alt+D', async ({ page }) => {
    await openWrite(page);
    await page.keyboard.type('Cone 10 is about 1300 degrees.');
    await page.keyboard.press('Control+Alt+F');
    await until(
      () => features(page),
      (f) => f.in_note === true,
      'the caret is in the new footnote',
    );
    await page.keyboard.type('Orton cones, self-supporting.');
    await run(page, 'write.references.show_notes');
    await until(
      () => features(page),
      (f) => f.in_note === false,
      'Show Notes goes back to the reference',
    );
    await page.keyboard.press('Control+Home');
    await page.keyboard.press('Control+Alt+D');
    await until(
      () => features(page),
      (f) => f.in_note === true,
      'an endnote',
    );
    await page.keyboard.type('See the firing log.');
    await run(page, 'write.references.show_notes');
    await page.keyboard.press('Control+Home');
    await run(page, 'write.references.next_footnote');
    await until(
      () => format(page),
      (f) => f.story === 'Body',
      'Next Footnote stays in the text',
    );
    await page.waitForTimeout(300);
    await page.screenshot({ path: shot('footnotes-light-desktop.png') });
  });

  test('captions and cross-reference dialogs', async ({ page }) => {
    await openWrite(page);
    await page.keyboard.type('Glaze test tiles');
    await page.getByRole('tab', { name: 'References' }).click();
    await page.locator('[data-command="ui.write.caption_dialog"]').first().click();
    const dialog = page.getByTestId('write-caption-dialog');
    await expect(dialog).toBeVisible();
    await dialog.getByLabel('Caption text (after the number)').fill('Tiles after the firing');
    await dialog.getByRole('button', { name: 'OK' }).click();
    await until(
      () => paragraphs(page),
      (p) => p[1] === 'Figure 1: Tiles after the firing',
      'a numbered caption',
    );
    await page.keyboard.press('Control+End');
    await page.keyboard.press('Enter');
    await page.keyboard.type('As shown in ');
    await page.locator('[data-command="ui.write.cross_reference_dialog"]').first().click();
    const cr = page.getByTestId('write-crossref-dialog');
    await expect(cr).toBeVisible();
    await cr.getByRole('combobox').first().click();
    await page.getByRole('option', { name: 'Figure' }).click();
    await cr.getByRole('combobox').nth(1).click();
    await page.getByRole('option', { name: 'Only label and number' }).click();
    await cr.getByRole('option', { name: /Tiles after the firing/ }).click();
    await cr.getByRole('button', { name: 'Insert' }).click();
    await until(
      () => paragraphs(page),
      (p) => p[2] === 'As shown in Figure 1',
      'a cross-reference to the figure',
    );
  });

  test('float wrap drag: wrap, position presets, drag and resize the frame, order, Selection pane', async ({
    page,
  }) => {
    await openWrite(page);
    await page.keyboard.type('Pictures sit beside the text. '.repeat(12));
    await caretAt(page, 0, 0);
    // A picture from bytes (as Insert ▸ Pictures does after the host's file picker).
    const png = await page.evaluate(async () => {
      const c = document.createElement('canvas');
      c.width = 120;
      c.height = 80;
      const ctx = c.getContext('2d');
      if (ctx) {
        ctx.fillStyle = '#c2481c';
        ctx.fillRect(0, 0, 120, 80);
      }
      const blob: Blob = await new Promise((r) => c.toBlob((b) => r(b as Blob), 'image/png'));
      return Array.from(new Uint8Array(await blob.arrayBuffer()));
    });
    await run(page, 'write.insert.picture', { name: 'tiles.png', bytes: png, floating: false });
    const objects = () => writeQuery<Json[]>(page, 'Objects');
    const picture = (await until(objects, (o) => o.length === 1, 'one picture'))[0] as Json;
    await run(page, 'write.arrange.select', { id: picture.id });
    await until(
      () => features(page),
      (f) => (f.object as Json | null)?.id === picture.id,
      'the picture is selected',
    );
    await page.getByRole('tab', { name: 'Layout' }).click();
    await page.locator('[data-command="suite.arrange.wrap_text"]').first().click();
    await page.getByRole('menuitem', { name: 'Square' }).click();
    await until(
      () => features(page),
      (f) => (f.object as Json | null)?.wrap === 'Square',
      'square wrapping',
    );
    await page.locator('[data-command="suite.arrange.position"]').first().click();
    await page.getByRole('menuitem', { name: 'Top Right' }).click();
    // The frame and its handles over the picture; drag it down and to the left.
    const frame = page.getByTestId('write-object-frame');
    await expect(frame).toBeVisible();
    const before = await frame.boundingBox();
    if (!before) throw new Error('no frame');
    await page.mouse.move(before.x + before.width / 2, before.y + before.height / 2);
    await page.mouse.down();
    await page.mouse.move(before.x + before.width / 2 - 120, before.y + before.height / 2 + 60, { steps: 6 });
    await page.mouse.up();
    await until(
      async () => (await features(page)).object as Json | null,
      (o) => {
        const r = o?.rect as { x: number } | null;
        const r0 = (picture.rect as { x: number } | null)?.x ?? 0;
        return !!r && Math.abs(r.x - r0) > 20;
      },
      'dragged',
    );
    // Resize from the bottom-right handle, once the frame has followed the picture.
    await until(
      async () => (await frame.boundingBox())?.x ?? 0,
      (x) => Math.abs(x - (before.x - 120)) < 4,
      'the frame at the dropped place',
    );
    const handle = await frame.locator('[data-handle="se"]').boundingBox();
    if (!handle) throw new Error('no handle');
    const hx = handle.x + handle.width / 2;
    const hy = handle.y + handle.height / 2;
    await page.mouse.move(hx, hy);
    await page.mouse.down();
    await page.mouse.move(hx + 60, hy + 40, { steps: 5 });
    await page.mouse.up();
    await until(
      async () => (await features(page)).object as Json | null,
      (o) => (o?.width as number) > 100,
      'resized',
    );
    // Behind text, order and the Selection pane.
    await run(page, 'write.arrange.wrap_text', { wrap: 'Behind' });
    await run(page, 'write.insert.text_box', { text: 'Pull quote' });
    await run(page, 'suite.arrange.selection_pane');
    const pane = page.getByTestId('write-selection-pane');
    await expect(pane.getByTestId('write-selection-item')).toHaveCount(2);
    await pane.getByText(String(picture.name)).click();
    await run(page, 'write.arrange.order', { order: 'ToFront' });
    await until(
      () => objects(),
      (o) => o[0]?.id === picture.id,
      'brought to front',
    );
    await run(page, 'write.arrange.rotate', { op: 'Right90' });
    await until(
      () => features(page),
      (f) => (f.object as Json | null)?.rotation === 90,
      'rotated',
    );
    await page.waitForTimeout(300);
    await page.screenshot({ path: shot('objects-light-desktop.png') });
    // Alt text from the Layout dialog.
    await run(page, 'write.arrange.select', { id: picture.id });
    await run(page, 'ui.write.layout_dialog');
    const layout = page.getByTestId('write-layout-dialog');
    await layout.getByRole('tab', { name: 'Alt Text' }).click();
    await layout.getByRole('textbox').first().fill('Test tiles after the glaze firing');
    await layout.getByRole('button', { name: 'OK' }).click();
    await until(
      async () => ((await objects()).find((o) => o.id === picture.id) ?? null) as Json | null,
      (o) => o?.alt_text === 'Test tiles after the glaze firing',
      'alt text saved',
    );
  });

  test('format painter: single click paints once, double-click keeps painting', async ({ page }) => {
    await openWrite(page);
    await page.keyboard.type('Bold source and plain target words here');
    await select(page, 0, 0, 4);
    await page.keyboard.press('Control+b');
    await caretAt(page, 0, 2);
    await page.locator('[data-command="suite.edit.format_painter"]').first().click();
    await until(
      () => features(page),
      (f) => f.painter === false,
      'Format Painter armed',
    );
    // Painting follows a mouse selection: drag over "target".
    await dragOver(page, 0, 22, 28);
    await until(
      async () => {
        await caretAt(page, 0, 24);
        return (await format(page)).bold;
      },
      (b) => b === true,
      'target painted bold',
    );
    await until(
      () => features(page),
      (f) => f.painter === null,
      'a single click paints once',
    );
    // Double-click: the brush stays until Esc.
    await caretAt(page, 0, 2);
    await page.locator('[data-command="suite.edit.format_painter"]').first().dblclick();
    await until(
      () => features(page),
      (f) => f.painter === true,
      'sticky Format Painter',
    );
    await dragOver(page, 0, 29, 34);
    await dragOver(page, 0, 35, 39);
    await until(
      async () => {
        await caretAt(page, 0, 31);
        const a = (await format(page)).bold;
        await caretAt(page, 0, 37);
        return a === true && (await format(page)).bold === true;
      },
      (b) => b,
      'two words painted with one sticky brush',
    );
    expect((await features(page)).painter).toBe(true);
    await page.keyboard.press('Escape');
    await until(
      () => features(page),
      (f) => f.painter === null,
      'Esc puts the brush down',
    );
    // Undo the sticky painting to give the copy/paste below plain text.
    await page.keyboard.press('Control+z');
    await page.keyboard.press('Control+z');
    // Ctrl+Shift+C / Ctrl+Shift+V.
    await caretAt(page, 0, 1);
    await page.keyboard.press('Control+Shift+C');
    await until(
      () => features(page),
      (f) => f.copied_format === true,
      'formatting copied',
    );
    await select(page, 0, 34, 39);
    await page.keyboard.press('Control+Shift+V');
    await until(
      async () => {
        await caretAt(page, 0, 36);
        return (await format(page)).bold;
      },
      (b) => b === true,
      'pasted formatting',
    );
  });

  test('markdown shortcuts and smart typography as you type', async ({ page }) => {
    await openWrite(page);
    await page.keyboard.type('# Firing notes');
    await until(
      () => format(page),
      (f) => f.style_id === 'Heading1',
      '# makes Heading 1',
    );
    await page.keyboard.press('Enter');
    await page.keyboard.type('- bisque first');
    await until(
      () => format(page),
      (f) => (f.list as Json | null)?.bullet === true,
      '- makes a bulleted list',
    );
    await page.keyboard.press('Enter');
    await page.keyboard.press('Enter');
    await page.keyboard.type('Use **cone 10** and `slow cool`, "carefully" -- it\'s 1/2 done...');
    await until(
      () => paragraphs(page),
      (p) => p[2] === 'Use cone 10 and slow cool, “carefully” – it’s ½ done…',
      'inline Markdown and typography',
    );
    await caretAt(page, 2, 6);
    expect((await format(page)).bold).toBe(true);
    await page.keyboard.press('Control+End');
    await page.keyboard.press('Enter');
    await page.keyboard.type('---');
    await page.keyboard.press('Enter');
    await page.keyboard.type('> Fire is patient.');
    await until(
      () => format(page),
      (f) => f.style_id === 'Quote',
      '> makes a quote',
    );
    // Ctrl+Z undoes the AutoFormat, not the typing.
    await page.keyboard.press('Enter');
    await page.keyboard.type('1. ');
    await page.keyboard.press('Control+z');
    await until(
      () => paragraphs(page),
      (p) => p[p.length - 1] === '1. ',
      'undo restores the typed marker',
    );
    await page.waitForTimeout(300);
    await page.screenshot({ path: shot('markdown-light-desktop.png') });
  });

  test('paste markdown, code block and callout', async ({ page }) => {
    await openWrite(page);
    await run(page, 'write.insert.paste_markdown_table', {
      text: '## Shelves\n\n| Shelf | Glaze | Cone |\n|---|---|---|\n| 1–2 | Tenmoku | 10 |\n| 5 | Cobalt slip | 9 |\n\n```js\nconst cone = 10; // hot\n```\n',
    });
    await until(
      () => paragraphs(page),
      (p) => p.includes('Shelves') && p.includes('Tenmoku') && p.includes('const cone = 10; // hot'),
      'a heading, a table and code',
    );
    await page.keyboard.press('Control+End');
    await run(page, 'write.insert.callout', { kind: 'Tip' });
    await page.keyboard.type('Wax the feet before Friday.');
    await until(
      () => paragraphs(page),
      (p) => p.includes('Tip Wax the feet before Friday.'),
      'a tip callout',
    );
    await page.waitForTimeout(300);
    await page.screenshot({ path: shot('blocks-light-desktop.png') });
  });

  test('cover page, properties and word goal', async ({ page }) => {
    await openWrite(page);
    await page.keyboard.type('Twice a year we load the big catenary kiln together.');
    await run(page, 'write.file.properties', {
      props: { title: 'The Autumn Firing', author: 'Studio co-op' },
    });
    await page.getByRole('tab', { name: 'Insert' }).click();
    await page.locator('[data-command="write.insert.cover_page"]').first().click();
    await page.getByRole('menuitem', { name: /Slab/ }).click();
    await until(
      () => paragraphs(page),
      (p) => p.some((x) => x.includes('The Autumn Firing')) && p.includes('Studio co-op'),
      'a cover page from the properties',
    );
    await run(page, 'ui.write.word_goal_dialog');
    const goal = page.getByTestId('write-word-goal-dialog');
    await expect(goal).toBeVisible();
    await goal.getByRole('spinbutton', { name: 'Goal (words)' }).fill('20');
    await goal.getByRole('spinbutton', { name: 'Goal (words)' }).press('Enter');
    await goal.getByRole('button', { name: 'OK' }).click();
    await until(
      () => features(page),
      (f) => f.word_goal === 20,
      'goal saved',
    );
    await expect(page.getByTestId('write-word-goal')).toBeVisible();
    // The Navigation pane's card shows the goal's progress.
    await run(page, 'suite.view.navigation_pane');
    const card = page.getByRole('region', { name: 'Word goal' });
    await expect(card).toContainText('Word goal');
    await expect(card).toContainText('/ 20');
    await expect(card.getByRole('progressbar', { name: 'Word goal' })).toBeVisible();
    await page.getByTestId('kiln-input-proxy').focus();
    await page.keyboard.press('Control+Home');
    await page.waitForTimeout(500);
    await page.screenshot({ path: shot('cover-light-desktop.png') });
  });

  test('focus mode: the chrome fades and the line stays centred', async ({ page }) => {
    await openWrite(page);
    await page.keyboard.type('Focus on one paragraph at a time.');
    await page.getByRole('tab', { name: 'View', exact: true }).click();
    await page.locator('[data-command="write.view.focus_mode"]').first().click();
    await until(
      () => features(page),
      (f) => f.focus === true,
      'focus on',
    );
    await expect(page.locator('html')).toHaveAttribute('data-kiln-focus', 'on');
    // The chrome fades once the pointer leaves it (hovering brings it back).
    await page.mouse.move(640, 500);
    await page.waitForTimeout(700);
    const opacity = await page
      .locator('.kiln-ribbon')
      .first()
      .evaluate((el) => Number(getComputedStyle(el).opacity));
    expect(opacity).toBeLessThan(0.5);
    await page.waitForTimeout(600);
    await page.screenshot({ path: shot('focus-light-desktop.png') });
    await page.getByTestId('write-focus-button').click();
    await until(
      () => features(page),
      (f) => f.focus === false,
      'focus off from the status bar',
    );
  });

  test('spelling suggestion: squiggle, context menu, F7 pane, add to dictionary', async ({ page }) => {
    await openWrite(page);
    await until(
      () => features(page),
      (f) => f.dictionary === true,
      'the dictionary is loaded',
    );
    await page.keyboard.type('Fire teh kiln and the glaze sets. Zorblat ');
    await until(
      () => features(page),
      (f) => (f.misspellings as number) >= 2,
      'two misspellings',
    );
    await expect(page.getByTestId('write-proofing-status')).toContainText('2 spelling errors');
    // Right-click on "teh": the suggestions head the context menu.
    const rect = await page.evaluate(async () => {
      const w = (window as unknown as KilnWindow).__kilnWrite as WriteHandle;
      const r = (await w.client.query(w.doc, 'Paragraphs')) as { Paragraphs: { id: number }[] };
      const block = r.Paragraphs[0]?.id ?? 0;
      await w.client.execute(w.doc, { Write: { SelectTo: { pos: { block, offset: 6 }, extend: false } } });
      const s = (await w.client.query(w.doc, 'Selection')) as {
        Selection: { caret: { rect: { x: number; y: number; width: number; height: number } } };
      };
      return w.controller?.clientRectOf(s.Selection.caret.rect);
    });
    if (!rect) throw new Error('no caret');
    await page.mouse.click(rect.left + 1, rect.top + rect.height / 2, { button: 'right' });
    const fix = page.getByRole('menuitem', { name: 'the', exact: true });
    await expect(fix).toBeVisible();
    await page.waitForTimeout(200);
    await page.screenshot({ path: shot('spelling-menu-light-desktop.png') });
    await fix.click();
    await until(
      () => paragraphs(page),
      (p) => p[0]?.startsWith('Fire the kiln') ?? false,
      'corrected from the menu',
    );
    // F7: the Spelling pane shows the next word; Add to Dictionary.
    await page.keyboard.press('F7');
    const pane = page.getByTestId('write-spelling-pane');
    await expect(pane).toContainText('Zorblat');
    await pane.getByRole('button', { name: 'Add to Dictionary' }).click();
    await expect(pane).toContainText('The spelling check is complete.');
    await expect(page.getByTestId('write-proofing-status')).toContainText('No issues');
  });

  test('hyphenation and document properties', async ({ page }) => {
    await openWrite(page);
    await page.getByRole('tab', { name: 'Layout' }).click();
    await page.locator('[data-command="write.layout.hyphenation"]').first().click();
    await until(
      () => features(page),
      (f) => f.hyphenation === true,
      'automatic hyphenation',
    );
    // File ▸ Info ▸ Edit Properties.
    await page.getByRole('button', { name: 'File' }).click();
    await page.getByRole('button', { name: 'Info' }).click();
    await page.getByRole('button', { name: 'Edit Properties' }).click();
    const dialog = page.getByTestId('write-properties-dialog');
    await dialog.getByLabel('Title').fill('Firing log');
    await page.screenshot({ path: shot('properties-light-desktop.png') });
    await dialog.getByRole('button', { name: 'OK' }).click();
    await until(
      () => writeQuery<Json>(page, 'Properties'),
      (p) => p.title === 'Firing log',
      'title saved',
    );
  });

  test('screenshots: review features in light and dark, desktop and phone', async ({
    page,
    browser,
    baseURL,
  }) => {
    const build = async (p: Page) => {
      await openWrite(p);
      await p.keyboard.type('The Autumn Firing');
      await p.keyboard.press('Control+Alt+1');
      await p.keyboard.press('Enter');
      await p.keyboard.type(
        'We have room for roughly 240 pieces across six shelves. Please label anything glazed with the new tenmoku so we can track how it breaks over the rims.',
      );
      await select(p, 1, 78, 105);
      await run(p, 'write.comment.new', {
        text: 'Can we keep the tenmoku pieces off shelf 1? It ran last spring.',
      });
      await select(p, 1, 122, 149);
      await run(p, 'write.comment.new', { text: 'Added the cone 9 shelf for porcelain.' });
      await p.keyboard.press('Control+Shift+E');
      await p.keyboard.press('End');
      await p.keyboard.type(' Bring gloves.');
      await run(p, 'write.review.markup_mode', { mode: 'All' });
      await caretAt(p, 1, 80);
      await p.evaluate(() => document.fonts.ready);
    };
    await build(page);
    await page.waitForTimeout(600);
    await page.screenshot({ path: shot('features-light-desktop.png') });
    await run(page, 'write.review.reviewing_pane');
    await page.waitForTimeout(300);
    await page.screenshot({ path: shot('reviewing-pane-light-desktop.png') });
    await page.emulateMedia({ colorScheme: 'dark' });
    await page.waitForTimeout(500);
    await page.screenshot({ path: shot('reviewing-pane-dark-desktop.png') });
    await run(page, 'write.review.reviewing_pane');
    await page.waitForTimeout(400);
    await page.screenshot({ path: shot('features-dark-desktop.png') });
    await expectNoSeriousA11yViolations(page);
    // Phone: a fresh page at phone size (the layout of the editor differs from the start).
    const phoneContext = await browser.newContext({
      baseURL,
      viewport: { width: 412, height: 915 },
      colorScheme: 'dark',
    });
    const phone = await phoneContext.newPage();
    try {
      await build(phone);
      await phone.waitForTimeout(600);
      await phone.screenshot({ path: shot('features-dark-phone.png') });
      await phone.emulateMedia({ colorScheme: 'light' });
      await run(phone, 'ui.write.comments_pane');
      await phone.waitForTimeout(500);
      await phone.screenshot({ path: shot('features-light-phone.png') });
      // The phone app frame's icon-only buttons (search, strip, share) are the shell's (WP-62)
      // and have no names yet; this page's own parts are checked on the desktop above.
      await expectNoSeriousA11yViolations(phone, {
        exclude: ['.kiln-searchbox', '.kiln-appframe__strip-btn', '.kiln-appframe__share'],
      });
    } finally {
      await phoneContext.close();
    }
  });
});
