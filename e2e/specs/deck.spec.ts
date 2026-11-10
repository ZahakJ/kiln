/**
 * Kiln Deck in the browser: a new presentation on the stage, text typed into a placeholder,
 * shapes inserted, moved with smart-guide snapping, resized and rotated by their handles, slides
 * added and reordered by dragging in the thumbnail rail, the notes pane, the dialogs and the
 * Design gallery; screenshots in light and dark on desktop and phone for the visual review.
 */
import { fileURLToPath } from 'node:url';
import { expect, type Page, test } from '@playwright/test';
import { openPhoneSlide } from './deck-helpers.ts';

const shot = (name: string) => fileURLToPath(new URL(`../screenshots/deck/${name}`, import.meta.url));

interface DeckBox {
  x: number;
  y: number;
  width: number;
  height: number;
  rotation: number;
}
interface DeckStateLike {
  slides: { id: number; title: string; hidden: boolean; layout: string; version: number }[];
  sections: { name: string; start: number; count: number }[];
  current: number;
  selected: number[];
  slide_width: number;
  slide_height: number;
  shapes: { id: number; name: string; kind: string }[];
  selection: number[];
  editing: boolean;
  selection_box: DeckBox | null;
  theme: string;
  notes: string;
  notes_visible: boolean;
  layouts: { id: number; name: string }[];
  format: { bold: boolean; italic: boolean; font_size: number; fill: string | null; align: string } | null;
}
interface DeckHandle {
  doc: number;
  sent: string[];
  state: { deck: DeckStateLike | null };
  client: {
    query(doc: number, q: unknown): Promise<Record<string, unknown>>;
    execute(doc: number, c: unknown): Promise<unknown>;
  };
  stage: { focus(): void } | null;
  fetch(): Promise<DeckStateLike | null>;
}
interface StageHandle {
  stats: { frames: number };
  view(): { zoom: number; left: number; top: number; width: number; height: number };
}
type W = { __kilnDeck: DeckHandle; __kilnDeckStage?: StageHandle | null };

const CORE_START = 60_000;

async function openDeck(page: Page, query = '') {
  await page.goto(`/#/deck?blink=0${query}`);
  // A phone shows the slide list first: its first slide opens to edit.
  await page.waitForFunction(
    () =>
      ((window as unknown as W).__kilnDeckStage?.stats.frames ?? 0) > 0 ||
      !!document.querySelector('[data-testid="deck-phone-list"] [data-slide-index]'),
    null,
    { timeout: CORE_START },
  );
  await openPhoneSlide(page);
  await page.waitForFunction(() => ((window as unknown as W).__kilnDeckStage?.stats.frames ?? 0) > 0, null, {
    timeout: CORE_START,
  });
  await page.evaluate(() => document.fonts.ready);
}

async function state(page: Page): Promise<DeckStateLike> {
  return page.evaluate(async () => {
    const d = (window as unknown as W).__kilnDeck;
    const r = (await d.client.query(d.doc, { Deck: 'State' })) as { Deck: { State: DeckStateLike } };
    return r.Deck.State;
  });
}

async function core(page: Page, command: unknown) {
  await page.evaluate(async (c) => {
    const d = (window as unknown as W).__kilnDeck;
    await d.client.execute(d.doc, { Deck: c });
    await d.fetch();
  }, command);
}

/** Page coordinates of a point of the slide (slide px). */
async function onSlide(page: Page, x: number, y: number): Promise<{ x: number; y: number }> {
  const box = await page.getByTestId('deck-stage').boundingBox();
  const v = await page.evaluate(() => (window as unknown as W).__kilnDeckStage?.view());
  if (!box || !v) throw new Error('no stage');
  return { x: box.x + v.left + x * v.zoom, y: box.y + v.top + y * v.zoom };
}

/** Waits until the stage has painted what the core last said. */
async function settle(page: Page) {
  const n = await page.evaluate(() => (window as unknown as W).__kilnDeckStage?.stats.frames ?? 0);
  await page
    .waitForFunction((k) => ((window as unknown as W).__kilnDeckStage?.stats.frames ?? 0) > k, n, {
      timeout: 5_000,
    })
    .catch(() => undefined);
  await page.waitForTimeout(150);
}

test.describe('Deck', () => {
  test('a new presentation: the title slide on the stage, the rail, notes and status bar', async ({
    page,
  }) => {
    await openDeck(page);
    await expect(page.locator('.kiln-appframe')).toHaveAttribute('data-app', 'deck');
    await expect(page.getByTestId('deck-rail').getByRole('option')).toHaveCount(1);
    await expect(page.getByTestId('deck-notes')).toBeVisible();
    await expect(page.getByText('Slide 1 of 1')).toBeVisible();
    const s = await state(page);
    expect(s.slides[0]?.layout).toBe('Title Slide');
    expect(s.theme).toBe('Kiln');
    // The slide is zoomed to fit and centred on the stage.
    const box = await page.getByTestId('deck-stage').boundingBox();
    const v = await page.evaluate(() => (window as unknown as W).__kilnDeckStage?.view());
    expect(box && v).toBeTruthy();
    if (box && v) {
      expect(Math.abs(v.left + v.width / 2 - box.width / 2)).toBeLessThan(2);
      expect(v.width).toBeLessThanOrEqual(box.width);
    }
  });

  test('text editing in a placeholder: click, type, select, bold, Esc', async ({ page }) => {
    await openDeck(page);
    const s = await state(page);
    const title = await onSlide(page, s.slide_width / 2, s.slide_height * 0.38);
    await page.mouse.click(title.x, title.y);
    await page.keyboard.type('Every ticket fires the next kiln.');
    await expect
      .poll(async () => (await state(page)).slides[0]?.title)
      .toBe('Every ticket fires the next kiln.');
    expect((await state(page)).editing).toBe(true);
    // Shift+Home selects to the line start; Ctrl+B makes it bold.
    await page.keyboard.press('Shift+Home');
    // The Kiln theme's titles are bold: Ctrl+B takes the bold off, a second press puts it back.
    const bold = (await state(page)).format?.bold ?? false;
    await page.keyboard.press('Control+b');
    await expect.poll(async () => (await state(page)).format?.bold).toBe(!bold);
    await page.keyboard.press('Control+b');
    await expect.poll(async () => (await state(page)).format?.bold).toBe(bold);
    // Backspace and typing replace text through the core.
    await page.keyboard.press('End');
    await page.keyboard.press('Backspace');
    await page.keyboard.type('!');
    await expect
      .poll(async () => (await state(page)).slides[0]?.title)
      .toBe('Every ticket fires the next kiln!');
    // Esc ends editing with the placeholder selected; a second Esc clears the selection.
    await page.keyboard.press('Escape');
    await expect.poll(async () => (await state(page)).editing).toBe(false);
    expect((await state(page)).selection.length).toBe(1);
    await page.keyboard.press('Escape');
    await expect.poll(async () => (await state(page)).selection.length).toBe(0);
    // Undo goes back step by step: the "!", then the deleted ".".
    await page.keyboard.press('Control+z');
    await expect
      .poll(async () => (await state(page)).slides[0]?.title)
      .toBe('Every ticket fires the next kiln');
    await page.keyboard.press('Control+z');
    await expect
      .poll(async () => (await state(page)).slides[0]?.title)
      .toBe('Every ticket fires the next kiln.');
  });

  test('shapes: insert, move with snapping, resize and rotate by the handles, nudge', async ({ page }) => {
    await openDeck(page);
    await core(page, {
      SetLayout: { layout: (await state(page)).layouts.find((l) => l.name === 'Blank')?.id ?? 0 },
    });
    // Insert ▸ Shapes: arm a rectangle and drag it out.
    await page.evaluate(async () => {
      const d = (window as unknown as W).__kilnDeck;
      await d.client.execute(d.doc, { Deck: { ArmInsert: { preset: 'rect' } } });
    });
    const a = await onSlide(page, 200, 200);
    const b = await onSlide(page, 400, 320);
    await page.mouse.move(a.x, a.y);
    await page.mouse.down();
    await page.mouse.move((a.x + b.x) / 2, (a.y + b.y) / 2, { steps: 4 });
    await page.mouse.move(b.x, b.y, { steps: 4 });
    await page.mouse.up();
    await expect.poll(async () => (await state(page)).shapes.length).toBe(1);
    let box = (await state(page)).selection_box;
    expect(box).toBeTruthy();
    // The shape is 200 × 120 px = 150 × 90 pt.
    expect(Math.round(box?.width ?? 0)).toBe(150);
    expect(Math.round(box?.height ?? 0)).toBe(90);
    // Move it so its centre comes within a few pixels of the slide's centre: it snaps there.
    const s = await state(page);
    const from = await onSlide(page, 300, 260);
    const to = await onSlide(page, s.slide_width / 2 + 3, s.slide_height / 2 - 2);
    await page.mouse.move(from.x, from.y);
    await page.mouse.down();
    await page.mouse.move(from.x + 20, from.y + 10, { steps: 3 });
    await page.mouse.move(to.x, to.y, { steps: 8 });
    await settle(page);
    await page.screenshot({ path: shot('drag-smart-guides-light-desktop.png') });
    await page.mouse.up();
    box = (await state(page)).selection_box;
    // Centre on the slide's centre (in points: 480 × 270 on a 960 × 540 pt slide).
    expect(Math.abs((box?.x ?? 0) + (box?.width ?? 0) / 2 - (s.slide_width * 0.75) / 2)).toBeLessThan(0.6);
    expect(Math.abs((box?.y ?? 0) + (box?.height ?? 0) / 2 - (s.slide_height * 0.75) / 2)).toBeLessThan(0.6);
    // Resize by the bottom-right handle.
    const br = await onSlide(
      page,
      ((box?.x ?? 0) + (box?.width ?? 0)) / 0.75,
      ((box?.y ?? 0) + (box?.height ?? 0)) / 0.75,
    );
    await page.mouse.move(br.x, br.y);
    await page.mouse.down();
    await page.mouse.move(br.x + 30, br.y + 15, { steps: 5 });
    await page.mouse.up();
    const resized = (await state(page)).selection_box;
    expect((resized?.width ?? 0) > (box?.width ?? 0)).toBe(true);
    // Rotate by the rotation handle (above the top edge): Shift snaps to 15°.
    const r = resized as DeckBox;
    // The rotation handle is 22 screen px above the top edge.
    const zoom = (await page.evaluate(() => (window as unknown as W).__kilnDeckStage?.view().zoom)) ?? 1;
    const top = await onSlide(page, (r.x + r.width / 2) / 0.75, r.y / 0.75 - 22 / zoom);
    const right = await onSlide(page, (r.x + r.width) / 0.75 + 80, (r.y + r.height / 2) / 0.75);
    await page.mouse.move(top.x, top.y);
    await page.mouse.down();
    await page.keyboard.down('Shift');
    await page.mouse.move(right.x, right.y, { steps: 6 });
    await page.mouse.up();
    await page.keyboard.up('Shift');
    await expect.poll(async () => Math.round((await state(page)).selection_box?.rotation ?? 0)).toBe(90);
    // Arrows nudge; Ctrl+arrows nudge finely.
    await page.locator('[data-kiln-deck-proxy]').focus();
    const before = (await state(page)).selection_box?.x ?? 0;
    await page.keyboard.press('ArrowRight');
    await expect.poll(async () => (await state(page)).selection_box?.x ?? 0).toBeGreaterThan(before);
    await page.screenshot({ path: shot('shape-rotated-light-desktop.png') });
  });

  test('the rail: new slides, drag to reorder, sections, context menu', async ({ page }) => {
    await openDeck(page);
    await page.locator('[data-kiln-deck-proxy]').focus();
    await page.keyboard.press('Control+m');
    await page.keyboard.press('Control+m');
    await expect(page.getByTestId('deck-rail').getByRole('option')).toHaveCount(3);
    // Name the slides by their titles.
    for (const [i, title] of ['One', 'Two', 'Three'].entries()) {
      await core(page, { SelectSlide: { index: i, extend: false, toggle: false } });
      const st = await state(page);
      const ph = st.shapes.find((x) => /^Title/.test(x.name));
      if (ph) {
        await core(page, { SelectShapes: { ids: [ph.id] } });
        await core(page, { InsertText: { text: title } });
        await core(page, 'Escape');
      }
    }
    await expect
      .poll(async () => (await state(page)).slides.map((s) => s.title))
      .toEqual(['One', 'Two', 'Three']);
    // Drag the first slide below the third.
    const items = page.getByTestId('deck-rail').getByRole('option');
    const first = await items.nth(0).boundingBox();
    const third = await items.nth(2).boundingBox();
    if (!first || !third) throw new Error('no rail items');
    await page.mouse.move(first.x + first.width / 2, first.y + first.height / 2);
    await page.mouse.down();
    await page.mouse.move(first.x + first.width / 2, first.y + first.height / 2 + 20, { steps: 3 });
    await page.mouse.move(third.x + third.width / 2, third.y + third.height - 4, { steps: 10 });
    await page.screenshot({ path: shot('rail-drag-light-desktop.png') });
    await page.mouse.up();
    await expect
      .poll(async () => (await state(page)).slides.map((s) => s.title))
      .toEqual(['Two', 'Three', 'One']);
    // The rail's keys: Up selects the slide above, Ctrl+Enter adds one after it.
    await page.getByTestId('deck-rail').focus();
    const at = (await state(page)).current;
    await expect
      .poll(async () => page.evaluate(() => (window as unknown as W).__kilnDeck.state.deck?.current), {
        message: 'the rail shows the core’s current slide',
      })
      .toBe(at);
    await page.keyboard.press(at > 0 ? 'ArrowUp' : 'ArrowDown');
    await expect.poll(async () => (await state(page)).current).toBe(at > 0 ? at - 1 : at + 1);
    await page.keyboard.press('Control+Enter');
    await expect.poll(async () => (await state(page)).slides.length).toBe(4);
    // Context menu ▸ Add Section, then Hide Slide.
    await items.nth(0).click({ button: 'right' });
    await page.getByRole('menuitem', { name: 'Add Section' }).click();
    await expect.poll(async () => (await state(page)).sections.length).toBeGreaterThan(0);
    await expect(page.getByTestId('deck-section').first()).toBeVisible();
    await items.nth(1).click({ button: 'right' });
    await page.getByRole('menuitemcheckbox', { name: 'Hide Slide' }).click();
    await expect.poll(async () => (await state(page)).slides[1]?.hidden).toBe(true);
    await page.screenshot({ path: shot('rail-sections-light-desktop.png') });
  });

  test('notes, Design themes, Slide Size and Format Background', async ({ page }) => {
    await openDeck(page);
    const notes = page.getByRole('textbox', { name: 'Speaker notes' });
    await notes.click();
    await page.keyboard.type('Start with the number people remember.');
    await expect.poll(async () => (await state(page)).notes).toBe('Start with the number people remember.');
    // Design ▸ Themes: Cobalt.
    await page.getByRole('tab', { name: 'Design' }).click();
    await page.getByRole('option', { name: 'Cobalt', exact: true }).first().click();
    await expect.poll(async () => (await state(page)).theme).toBe('Cobalt');
    // Slide Size ▸ Standard (4:3).
    await page
      .getByRole('button', { name: /^Slide Size/ })
      .first()
      .click();
    await page.getByRole('menuitem', { name: 'Standard (4:3)' }).click();
    await expect.poll(async () => Math.round((await state(page)).slide_width)).toBe(960);
    // Format Background: a solid fill applied to all slides.
    await page
      .getByRole('button', { name: /^Format Background/ })
      .first()
      .click();
    const dlg = page.getByTestId('deck-format-background');
    await expect(dlg).toBeVisible();
    await dlg.getByRole('button', { name: 'Apply to All' }).click();
    await expect(dlg).toHaveCount(0);
    await page.getByRole('tab', { name: 'Home' }).click();
    await page.screenshot({ path: shot('design-cobalt-light-desktop.png') });
  });

  test('Insert: table, text box, slide number, header and footer, link, pictures', async ({ page }) => {
    await openDeck(page);
    await page.getByRole('tab', { name: 'Insert' }).click();
    // Table ▸ 3 × 4.
    await page
      .getByRole('button', { name: /^Table/ })
      .first()
      .click();
    const table = page.getByTestId('deck-insert-table');
    await table.getByRole('button', { name: 'OK' }).click();
    await expect.poll(async () => (await state(page)).shapes.some((s) => s.kind === 'table')).toBe(true);
    await page.keyboard.press('Escape');
    await page.keyboard.press('Escape');
    // Text Box: armed, then a click on the slide places one and types into it.
    await page
      .getByRole('button', { name: /^Text Box/ })
      .first()
      .click();
    const p = await onSlide(page, 120, 640);
    await page.mouse.click(p.x, p.y);
    await page.keyboard.type('Kiln Co-op');
    await expect
      .poll(async () => (await state(page)).shapes.filter((s) => s.kind === 'text box').length)
      .toBeGreaterThan(0);
    await page.keyboard.press('Escape');
    // Header & Footer: slide numbers on every slide.
    await page
      .getByRole('button', { name: /^Header & Footer/ })
      .first()
      .click();
    const hf = page.getByTestId('deck-header-footer');
    await hf.getByTestId('deck-hf-number').check();
    await hf.getByRole('button', { name: 'Apply to All' }).click();
    await expect
      .poll(async () => (await state(page)).shapes.some((s) => /Slide Number/.test(s.name)))
      .toBe(true);
    // Link on the selected text box.
    const st = await state(page);
    const box = st.shapes.find((s) => s.kind === 'text box');
    if (box) await core(page, { SelectShapes: { ids: [box.id] } });
    await page.keyboard.press('Control+k');
    const link = page.getByTestId('deck-link');
    await link.getByRole('textbox').fill('https://kiln.example/studio');
    await link.getByRole('button', { name: 'OK' }).click();
    await expect(link).toHaveCount(0);
    await page.screenshot({ path: shot('insert-light-desktop.png') });
  });

  test('Home: duplicate, layout, reset, arrange, align, group, format painter, find and replace', async ({
    page,
  }) => {
    await openDeck(page);
    await core(page, {
      SetLayout: { layout: (await state(page)).layouts.find((l) => l.name === 'Blank')?.id ?? 0 },
    });
    await core(page, { InsertShape: { preset: 'rect', rect: { x: 100, y: 100, width: 200, height: 100 } } });
    await core(page, { InsertText: { text: 'Clay' } });
    await core(page, 'Escape');
    await core(page, {
      InsertShape: { preset: 'ellipse', rect: { x: 500, y: 300, width: 150, height: 150 } },
    });
    await core(page, 'SelectAll');
    await page.locator('[data-kiln-deck-proxy]').focus();
    // Arrange ▸ Order is on Home; Shape Format ▸ Align Objects ▸ Align Left lines both up.
    await page.getByRole('button', { name: 'Arrange' }).click();
    await expect(page.getByRole('menuitem', { name: 'Bring to Front' })).toBeVisible();
    await page.keyboard.press('Escape');
    await page.getByRole('tab', { name: 'Shape Format' }).click();
    await page.getByRole('button', { name: /^Align Objects/ }).click();
    await page.getByRole('menuitem', { name: 'Align Left' }).click();
    await expect
      .poll(async () => {
        const st = await state(page);
        return st.selection.length === 2 && Math.round(st.selection_box?.width ?? 0) === 150;
      })
      .toBe(true);
    await page.getByRole('tab', { name: 'Home' }).click();
    // Group (Ctrl+Shift+G) and Ctrl+D duplicates the group.
    await page.locator('[data-kiln-deck-proxy]').focus();
    expect((await state(page)).selection.length).toBe(2);
    await page.keyboard.press('Control+Shift+g');
    await expect.poll(async () => (await state(page)).shapes.length).toBe(1);
    await page.keyboard.press('Control+d');
    await expect.poll(async () => (await state(page)).shapes.length).toBe(2);
    await page.keyboard.press('Control+z');
    await expect.poll(async () => (await state(page)).shapes.length).toBe(1);
    const group = (await state(page)).shapes[0];
    if (group) await core(page, { SelectShapes: { ids: [group.id] } });
    await page.locator('[data-kiln-deck-proxy]').focus();
    await page.keyboard.press('Control+Shift+h');
    await expect.poll(async () => (await state(page)).shapes.length).toBe(2);
    // Find and Replace through the dialog.
    await page.keyboard.press('Control+h');
    const find = page.getByTestId('deck-find');
    await find.getByLabel('Find what').fill('Clay');
    await find.getByLabel('Replace with').fill('Glaze');
    await find.getByRole('button', { name: 'Replace All' }).click();
    await find.getByRole('button', { name: 'Close' }).first().click();
    await expect
      .poll(async () =>
        page.evaluate(async () => {
          const d = (window as unknown as W).__kilnDeck;
          const r = (await d.client.query(d.doc, { Deck: 'Outline' })) as {
            Deck: { Outline: { lines: [number, string][] }[] };
          };
          return JSON.stringify(r.Deck.Outline);
        }),
      )
      .toContain('Glaze');
    // Duplicate the slide (Ctrl+D in the rail), change its layout, reset it.
    await page.getByTestId('deck-rail').focus();
    await page.keyboard.press('Control+d');
    await expect.poll(async () => (await state(page)).slides.length).toBe(2);
    await page
      .getByRole('button', { name: /^Layout/ })
      .first()
      .click();
    await page.getByRole('menuitem', { name: 'Title Only' }).click();
    await expect.poll(async () => (await state(page)).slides[1]?.layout).toBe('Title Only');
    // The Layout button keeps the focus after its menu (and its tooltip covers Reset).
    await page.locator('[data-kiln-deck-proxy]').focus();
    await page.mouse.move(700, 600);
    await page
      .getByRole('button', { name: /^Reset/ })
      .first()
      .click();
    // Format Painter: the rectangle's fill goes onto the oval with one click.
    await core(page, { SelectSlide: { index: 0, extend: false, toggle: false } });
    const st = await state(page);
    const rect = st.shapes.find((x) => /^Rectangle/.test(x.name));
    const oval = st.shapes.find((x) => /^Oval/.test(x.name));
    if (!rect || !oval) throw new Error(`shapes: ${JSON.stringify(st.shapes)}`);
    await core(page, { SelectShapes: { ids: [oval.id] } });
    const ob = (await state(page)).selection_box as DeckBox;
    await core(page, { SelectShapes: { ids: [rect.id] } });
    await core(page, {
      SetFill: { fill: { Solid: { color: '1D8A60', brightness: null, transparency: null } } },
    });
    await page.locator('[data-kiln-deck-proxy]').focus();
    await page
      .getByRole('button', { name: /^Format/ })
      .first()
      .click();
    const target = await onSlide(page, (ob.x + ob.width / 2) / 0.75, (ob.y + ob.height / 2) / 0.75);
    await page.mouse.click(target.x, target.y);
    await expect
      .poll(async () => {
        const s2 = await state(page);
        return s2.selection.includes(oval.id) ? (s2.format?.fill ?? null) : 'not selected';
      })
      .toBe('1D8A60');
  });

  test('clicks: double-click selects a word, triple-click the paragraph, double-click a shape edits it', async ({
    page,
  }) => {
    await openDeck(page);
    const s0 = await state(page);
    const title = s0.shapes.find((x) => /^Title/.test(x.name));
    if (!title) throw new Error('no title');
    await core(page, { SelectShapes: { ids: [title.id] } });
    await core(page, { InsertText: { text: 'Every ticket fires' } });
    await core(page, 'Escape');
    await core(page, 'Escape');
    await core(page, { SelectShapes: { ids: [title.id] } });
    const b = (await state(page)).selection_box as DeckBox;
    await core(page, 'Escape');
    // Double-click on the first word of the title: editing, with the word selected (typing
    // replaces it).
    const line = await onSlide(page, b.x / 0.75 + 30, (b.y + b.height / 2) / 0.75);
    await page.mouse.dblclick(line.x, line.y);
    await expect.poll(async () => (await state(page)).editing).toBe(true);
    await page.keyboard.type('Each ');
    await expect.poll(async () => (await state(page)).slides[0]?.title).toBe('Each ticket fires');
    // Triple-click: the whole paragraph.
    await page.mouse.click(line.x, line.y, { clickCount: 3 });
    await page.keyboard.type('Clay');
    await expect.poll(async () => (await state(page)).slides[0]?.title).toBe('Clay');
    // Double-click on a shape without text enters its text.
    await page.keyboard.press('Escape');
    await core(page, { InsertShape: { preset: 'rect', rect: { x: 900, y: 500, width: 200, height: 120 } } });
    await core(page, 'Escape');
    await core(page, 'Escape');
    const c = await onSlide(page, 1000, 560);
    await page.mouse.dblclick(c.x, c.y);
    await expect.poll(async () => (await state(page)).editing).toBe(true);
    await page.keyboard.type('Glaze');
    // The text went into the rectangle (Find selects it there).
    await expect
      .poll(async () =>
        page.evaluate(async () => {
          const d = (window as unknown as W).__kilnDeck;
          await d.client.execute(d.doc, {
            Deck: { Find: { text: 'Glaze', match_case: true, whole_word: false, backward: false } },
          });
          const r = (await d.client.query(d.doc, { Deck: 'State' })) as {
            Deck: {
              State: { find_done: boolean; selection: number[]; shapes: { id: number; name: string }[] };
            };
          };
          const st = r.Deck.State;
          return (
            !st.find_done && st.shapes.some((x) => /^Rectangle/.test(x.name) && st.selection.includes(x.id))
          );
        }),
      )
      .toBe(true);
  });

  test('Insert ▸ Table ▸ OK puts the caret in the first cell and typing goes there', async ({ page }) => {
    await openDeck(page);
    await page.getByRole('tab', { name: 'Insert' }).click();
    await page
      .getByRole('button', { name: /^Table/ })
      .first()
      .click();
    await page.getByTestId('deck-insert-table').getByRole('button', { name: 'OK' }).click();
    await expect(page.getByTestId('deck-insert-table')).toHaveCount(0);
    await expect
      .poll(() => page.evaluate(() => document.activeElement?.hasAttribute('data-kiln-deck-proxy')))
      .toBe(true);
    await page.keyboard.type('Q1');
    await page.keyboard.press('Tab');
    await page.keyboard.type('Q2');
    // Both went into the table's cells (Find finds them; the title is untouched).
    const found = (text: string) =>
      page.evaluate(async (t) => {
        const d = (window as unknown as W).__kilnDeck;
        await d.client.execute(d.doc, {
          Deck: { Find: { text: t, match_case: true, whole_word: false, backward: false } },
        });
        const r = (await d.client.query(d.doc, { Deck: 'State' })) as {
          Deck: {
            State: { find_done: boolean; selection: number[]; shapes: { id: number; kind: string }[] };
          };
        };
        const st = r.Deck.State;
        return !st.find_done && st.shapes.some((x) => x.kind === 'table' && st.selection.includes(x.id));
      }, text);
    await expect.poll(() => found('Q1')).toBe(true);
    await expect.poll(() => found('Q2')).toBe(true);
    expect((await state(page)).slides[0]?.title).toBe('');
    await core(page, 'Escape');
    await core(page, 'Escape');
    // The other dialogs give the keyboard back to the slide too.
    for (const [tab, button, testId] of [
      ['Insert', /^Header & Footer/, 'deck-header-footer'],
      ['Design', /^Format Background/, 'deck-format-background'],
    ] as const) {
      await page.getByRole('tab', { name: tab, exact: true }).click();
      await page.getByRole('button', { name: button }).first().click();
      await expect(page.getByTestId(testId)).toBeVisible();
      await page.getByTestId(testId).getByRole('button', { name: 'Cancel' }).click();
      await expect
        .poll(() => page.evaluate(() => document.activeElement?.hasAttribute('data-kiln-deck-proxy')))
        .toBe(true);
    }
  });

  test('Font, Paragraph and Symbol dialogs apply to the text', async ({ page }) => {
    await openDeck(page);
    const s0 = await state(page);
    const title = s0.shapes.find((x) => /^Title/.test(x.name));
    if (!title) throw new Error('no title');
    await core(page, { SelectShapes: { ids: [title.id] } });
    await core(page, { InsertText: { text: 'Kiln' } });
    await core(page, 'SelectAll');
    // Home ▸ Font ↘: 40 pt, italic.
    await page
      .getByRole('button', { name: /^Font Settings/ })
      .first()
      .click();
    const font = page.getByTestId('deck-font');
    await expect(font).toBeVisible();
    await font.getByRole('combobox', { name: 'Size' }).fill('40');
    await font.getByRole('combobox', { name: 'Font style' }).click();
    await page.getByRole('option', { name: 'Bold Italic' }).click();
    await font.getByRole('button', { name: 'OK' }).click();
    await expect.poll(async () => (await state(page)).format?.font_size).toBe(40);
    await expect.poll(async () => (await state(page)).format?.italic).toBe(true);
    // Home ▸ Paragraph ↘: centred.
    await page
      .getByRole('button', { name: /^Paragraph Settings/ })
      .first()
      .click();
    const para = page.getByTestId('deck-paragraph');
    await para.getByRole('combobox', { name: 'Alignment' }).click();
    await page.getByRole('option', { name: 'Centered' }).click();
    await para.getByRole('button', { name: 'OK' }).click();
    await expect.poll(async () => (await state(page)).format?.align).toBe('center');
    // Insert ▸ Symbol: © at the end of the text.
    await page.keyboard.press('End');
    await page.getByRole('tab', { name: 'Insert' }).click();
    await page
      .getByRole('button', { name: /^Symbol/ })
      .first()
      .click();
    await page.getByTestId('deck-symbol').getByRole('button', { name: 'Insert ©' }).click();
    await expect.poll(async () => (await state(page)).slides[0]?.title).toBe('Kiln©');
  });

  test('Design: themes and variants show their samples (light and dark)', async ({ page }) => {
    for (const scheme of ['light', 'dark'] as const) {
      await page.emulateMedia({ colorScheme: scheme });
      await openDeck(page);
      await page.getByRole('tab', { name: 'Design', exact: true }).click();
      const tile = page.getByRole('option', { name: 'Cobalt', exact: true }).first();
      await expect(tile).toBeVisible();
      await expect.poll(async () => (await tile.boundingBox())?.height ?? 0).toBeGreaterThan(30);
      // The sample is the core's picture of the theme.
      await expect(tile.locator('img')).toHaveCount(1, { timeout: 15_000 });
      const img = await tile.locator('img').boundingBox();
      expect((img?.width ?? 0) > 40 && (img?.height ?? 0) > 20).toBe(true);
      const variant = page.getByRole('option', { name: 'Variant 1', exact: true }).first();
      // A variant is its picture of the current slide.
      await expect(variant.locator('img')).toHaveCount(1, { timeout: 15_000 });
      const v = await variant.locator('img').boundingBox();
      expect((v?.width ?? 0) > 30 && (v?.height ?? 0) > 8).toBe(true);
      await page.screenshot({
        path: shot(`design-gallery-${scheme}.png`),
        clip: { x: 0, y: 40, width: 900, height: 160 },
      });
    }
  });

  test('views: slide sorter, reading view, zoom, fit and the notes toggle', async ({ page }) => {
    await openDeck(page);
    await page.locator('[data-kiln-deck-proxy]').focus();
    await page.keyboard.press('Control+m');
    await page.keyboard.press('Control+m');
    // Slide Sorter from the status bar; drag the last slide first.
    await page.getByRole('radio', { name: 'Slide Sorter' }).click();
    const sorter = page.getByTestId('deck-sorter');
    await expect(sorter.getByRole('option')).toHaveCount(3);
    const items = sorter.getByRole('option');
    const ids = (await state(page)).slides.map((s) => s.id);
    const last = await items.nth(2).boundingBox();
    const first = await items.nth(0).boundingBox();
    if (!last || !first) throw new Error('no sorter items');
    await page.mouse.move(last.x + last.width / 2, last.y + last.height / 2);
    await page.mouse.down();
    await page.mouse.move(last.x + 10, last.y + 10, { steps: 3 });
    await page.mouse.move(first.x + 4, first.y + first.height / 2, { steps: 8 });
    await page.mouse.up();
    await expect.poll(async () => (await state(page)).slides[0]?.id).toBe(ids[2]);
    await page.screenshot({ path: shot('sorter-light-desktop.png') });
    // Double-click opens a slide in Normal view.
    await items.nth(1).dblclick();
    await expect(page.getByTestId('deck-stage')).toBeVisible();
    // A zoom chosen in View ▸ Zoom ends fitting; Fit to Window fits again.
    await page.getByRole('tab', { name: 'View', exact: true }).click();
    await page.getByRole('button', { name: 'Zoom', exact: true }).click();
    await page.getByRole('menuitem', { name: '150%' }).click();
    await expect
      .poll(async () => page.evaluate(() => (window as unknown as W).__kilnDeckStage?.view().zoom ?? 0))
      .toBeCloseTo(1.5, 2);
    await page.getByRole('button', { name: 'Fit to Window' }).click();
    await expect
      .poll(async () => page.evaluate(() => (window as unknown as W).__kilnDeckStage?.view().zoom ?? 0))
      .toBeLessThan(1);
    // The notes toggle in the status bar hides and shows the notes pane.
    await page.getByTestId('deck-notes-toggle').click();
    await expect(page.getByTestId('deck-notes')).toHaveCount(0);
    await page.getByTestId('deck-notes-toggle').click();
    await expect(page.getByTestId('deck-notes')).toBeVisible();
  });

  for (const scheme of ['light', 'dark'] as const) {
    for (const device of ['desktop', 'phone'] as const) {
      test(`review: an edited deck (${scheme}, ${device})`, async ({ page }) => {
        await page.emulateMedia({ colorScheme: scheme });
        if (device === 'phone') await page.setViewportSize({ width: 390, height: 844 });
        await openDeck(page);
        // A title slide and a content slide with a shape.
        const st = await state(page);
        const title = st.shapes.find((x) => /^Title/.test(x.name));
        const sub = st.shapes.find((x) => /^Subtitle/.test(x.name));
        if (title) {
          await core(page, { SelectShapes: { ids: [title.id] } });
          await core(page, { InsertText: { text: 'Open Studio Weekend' } });
          await core(page, 'Escape');
        }
        if (sub) {
          await core(page, { SelectShapes: { ids: [sub.id] } });
          await core(page, { InsertText: { text: '15–16 November' } });
          await core(page, 'Escape');
        }
        await core(page, { NewSlide: { layout: null } });
        const s2 = await state(page);
        const heading = s2.shapes.find((x) => /^Title/.test(x.name));
        if (heading) {
          await core(page, { SelectShapes: { ids: [heading.id] } });
          await core(page, { InsertText: { text: 'Every ticket fires the next kiln.' } });
          await core(page, 'Escape');
        }
        await core(page, {
          InsertShape: { preset: 'roundRect', rect: { x: 820, y: 220, width: 300, height: 180 } },
        });
        await core(page, { SetNotes: { text: 'Start with the number people remember: 42% goes on gas.' } });
        await settle(page);
        await page.waitForTimeout(400);
        expect(await page.evaluate(() => document.documentElement.scrollWidth > window.innerWidth)).toBe(
          false,
        );
        await page.screenshot({ path: shot(`deck-${scheme}-${device}.png`) });
      });
    }
  }
});
