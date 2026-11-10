/**
 * Kiln Deck's View tab in the browser: Slide Master view — a layout edited with its
 * slides following, a custom layout inserted, renamed and given a picture placeholder, then used
 * for a new slide and kept through a save; Notes Page view typed into; Reading View; the ruler
 * with its indent markers and the guides dragged, copied and removed, kept through a save.
 * Screenshots go to e2e/screenshots/deck-view/.
 */
import { fileURLToPath } from 'node:url';
import { expect, test } from '@playwright/test';
import { deck, newDeck, ribbonTab, saveAndReopen, state } from './deck-helpers.ts';

const shot = (name: string) => fileURLToPath(new URL(`../screenshots/deck-view/${name}`, import.meta.url));

interface MasterView {
  current: number;
  title: boolean;
  footers: boolean;
  items: { id: number; name: string; master: boolean; used: number }[];
}

test.describe('deck-view', () => {
  test('slide master: layouts are edited, inserted, renamed and given placeholders; slides follow', async ({
    page,
  }) => {
    await newDeck(page);
    await deck(page, { NewSlide: { layout: null } });
    await ribbonTab(page, 'View');
    await page.getByRole('button', { name: 'Slide Master', exact: true }).click();
    await expect(page.getByTestId('deck-master-rail')).toBeVisible();
    await expect(page.getByRole('tab', { name: 'Slide Master' })).toHaveAttribute('aria-selected', 'true');
    await expect(page.locator('.kiln-statusbar').getByText('Slide Master')).toBeVisible();
    const view = (await state(page)).master_view as MasterView;
    expect(view.items[0]?.master).toBe(true);
    expect(view.items.find((i) => i.id === view.current)?.name).toBe('Title and Content');
    // The layout's title placeholder, filled: the slide on it shows the fill.
    const title = (await state(page)).shapes.find((s) => /^Title/.test(s.name));
    if (!title) throw new Error('no title on the layout');
    await deck(page, { SelectShapes: { ids: [title.id] } });
    await deck(page, {
      SetFill: { fill: { Solid: { color: 'accent1', brightness: 40, transparency: null } } },
    });
    // Insert Layout, Rename, Insert Placeholder ▸ Picture, Footers off.
    await page.getByRole('button', { name: 'Insert Layout' }).click();
    await expect
      .poll(async () => ((await state(page)).master_view as MasterView).items.length)
      .toBe(view.items.length + 1);
    await page.getByRole('button', { name: 'Rename', exact: true }).click();
    const rename = page.getByTestId('deck-rename-master');
    await rename.getByRole('textbox').fill('Quote');
    await rename.getByRole('button', { name: 'Rename' }).click();
    await page.getByRole('button', { name: 'Insert Placeholder' }).click();
    await page.getByRole('menuitem', { name: 'Picture', exact: true }).click();
    await page.locator('[data-command="deck.master.footers"]').click();
    await expect.poll(async () => ((await state(page)).master_view as MasterView).footers).toBe(false);
    const quote = (await state(page)).master_view as MasterView;
    expect(quote.items.find((i) => i.id === quote.current)?.name).toBe('Quote');
    await page.getByTestId('deck-editor').screenshot({ path: shot('slide-master.png') });
    // Close Master View: the Quote layout is in New Slide ▾.
    await page.getByRole('button', { name: 'Close Master View' }).click();
    await expect(page.getByTestId('deck-master-rail')).toBeHidden();
    await expect.poll(async () => (await state(page)).master_view).toBeNull();
    await ribbonTab(page, 'Home');
    await page.getByRole('button', { name: 'More options for New Slide' }).click();
    await page.getByRole('menuitem', { name: 'Quote', exact: true }).click();
    await expect.poll(async () => (await state(page)).slides.at(-1)?.layout).toBe('Quote');
    const st = await state(page);
    expect(st.shapes.some((s) => /^Picture/.test(s.name))).toBe(true);
    await page.getByTestId('deck-editor').screenshot({ path: shot('quote-slide.png') });
  });

  test('custom layouts from an imported master are listed and used for new slides', async ({ page }) => {
    await newDeck(page);
    await deck(page, { MasterView: { on: true } });
    await deck(page, 'InsertLayout');
    await deck(page, { RenameMasterItem: { name: 'Big Number' } });
    await deck(page, { InsertPlaceholder: { kind: 'Text' } });
    await deck(page, { MasterView: { on: false } });
    // The file is opened again: its master brings the custom layout.
    await saveAndReopen(page, 'Custom layouts.pptx');
    const layouts = (await state(page)).layouts as { id: number; name: string }[];
    const big = layouts.find((l) => l.name === 'Big Number');
    expect(big).toBeTruthy();
    await ribbonTab(page, 'Home');
    await page.getByRole('button', { name: 'Layout', exact: true }).click();
    await expect(page.getByRole('menuitem', { name: 'Big Number' })).toBeVisible();
    await page.getByRole('menuitem', { name: 'Big Number' }).click();
    await expect.poll(async () => (await state(page)).slides[0]?.layout).toBe('Big Number');
    await deck(page, { NewSlide: { layout: big?.id ?? null } });
    expect((await state(page)).slides.at(-1)?.layout).toBe('Big Number');
  });

  test('notes page: View ▸ Notes Page shows the slide above its notes, typed into in place', async ({
    page,
  }) => {
    await newDeck(page);
    await deck(page, { SetNotes: { text: 'Welcome everyone' } });
    await deck(page, { NewSlide: { layout: null } });
    await deck(page, { SetNotes: { text: 'The numbers' } });
    await deck(page, { SelectSlide: { index: 0, extend: false, toggle: false } });
    await ribbonTab(page, 'View');
    await page.getByRole('button', { name: 'Notes Page', exact: true }).click();
    const notesPage = page.getByTestId('deck-notes-page');
    await expect(notesPage).toBeVisible();
    const box = notesPage.getByRole('textbox', { name: 'Speaker notes' });
    await expect(box).toHaveValue('Welcome everyone');
    await box.click();
    await box.press('End');
    await box.type(' to the open day');
    await box.blur();
    await expect.poll(async () => (await state(page)).notes).toBe('Welcome everyone to the open day');
    await page.getByTestId('deck-editor').screenshot({ path: shot('notes-page.png') });
    // Page Down: the next slide's notes page.
    await notesPage.focus();
    await page.keyboard.press('PageDown');
    await expect.poll(async () => (await state(page)).current).toBe(1);
    await expect(box).toHaveValue('The numbers');
    await page.getByRole('button', { name: 'Normal', exact: true }).first().click();
    await expect(notesPage).toBeHidden();
  });

  test('reading view: the show plays in the window from the current slide; Esc comes back', async ({
    page,
  }) => {
    await newDeck(page);
    await deck(page, { NewSlide: { layout: null } });
    await deck(page, { NewSlide: { layout: null } });
    await deck(page, { SelectSlide: { index: 1, extend: false, toggle: false } });
    await page.locator('.kiln-statusbar').getByRole('radio', { name: 'Reading View' }).click();
    const reading = page.getByTestId('deck-reading-view');
    await expect(reading).toBeVisible();
    // The window stays: title and status bars.
    await expect(page.getByTestId('document-title')).toBeVisible();
    const show = reading.getByTestId('deck-show');
    await expect(show).toHaveAttribute('aria-label', 'Slide 2 of 3');
    await page.getByTestId('deck-reading-next').click();
    await expect(show).toHaveAttribute('aria-label', 'Slide 3 of 3');
    await page.keyboard.press('ArrowLeft');
    await expect(show).toHaveAttribute('aria-label', 'Slide 2 of 3');
    await page.getByTestId('deck-editor').screenshot({ path: shot('reading-view.png') });
    await page.keyboard.press('Escape');
    await expect(reading).toBeHidden();
    await expect(page.getByTestId('deck-editor')).toHaveAttribute('data-view', 'SlideNormal');
  });

  test('ruler and guides: the ruler sets indents; guides are dragged, copied and removed, and saved', async ({
    page,
  }) => {
    await newDeck(page);
    await deck(page, { NewSlide: { layout: null } });
    await ribbonTab(page, 'View');
    await page.locator('[data-command="suite.view.ruler"]').first().click();
    await expect(page.getByTestId('deck-ruler')).toBeVisible();
    // Typing in the body: the first-line marker dragged right indents the first line.
    const body = (await state(page)).shapes.find((s) => !/^Title/.test(s.name) && s.kind === 'placeholder');
    if (!body) throw new Error('no body');
    await deck(page, { SelectShapes: { ids: [body.id] } });
    await deck(page, 'EditText');
    await deck(page, { InsertText: { text: 'Kiln temperatures' } });
    const first = page.getByTestId('deck-ruler-first');
    await expect(first).toBeVisible();
    const before = (await state(page)).format as { indent_first: number; indent_left: number };
    const m = await first.boundingBox();
    if (!m) throw new Error('no marker');
    await page.mouse.move(m.x + m.width / 2, m.y + 3);
    await page.mouse.down();
    await page.mouse.move(m.x + m.width / 2 + 40, m.y + 3, { steps: 4 });
    await page.mouse.up();
    await expect
      .poll(async () => ((await state(page)).format as { indent_first: number }).indent_first)
      .toBeGreaterThan(before.indent_first + 10);
    await deck(page, 'Escape');
    await deck(page, 'Escape');
    // Guides: shown, the vertical one dragged, a copy with Ctrl, the copy dragged off.
    await page.locator('[data-command="deck.view.guides"]').first().click();
    const guides = page.getByTestId('deck-guide');
    await expect(guides).toHaveCount(2);
    const vertical = guides.nth(1);
    const g = await vertical.boundingBox();
    if (!g) throw new Error('no guide');
    const gy = g.y + g.height / 3;
    await page.mouse.move(g.x + 4, gy);
    await page.mouse.down();
    await page.mouse.move(g.x + 104, gy, { steps: 5 });
    await expect(page.locator('.kiln-deckguide__label')).toBeVisible();
    await page.screenshot({ path: shot('guides-drag.png') });
    await page.mouse.up();
    await expect
      .poll(async () => ((await state(page)).guide_list as { pos: number }[])[1]?.pos ?? 0)
      .toBeGreaterThan(700);
    // The handle has followed the guide.
    await expect.poll(async () => (await vertical.boundingBox())?.x ?? 0).toBeGreaterThan(g.x + 50);
    const moved = await vertical.boundingBox();
    if (!moved) throw new Error('no guide');
    await page.keyboard.down('Control');
    await page.mouse.move(moved.x + 4, gy);
    await page.mouse.down();
    await page.mouse.move(moved.x - 200, gy, { steps: 5 });
    await page.mouse.up();
    await page.keyboard.up('Control');
    await expect(guides).toHaveCount(3);
    await expect.poll(async () => (await guides.nth(2).boundingBox())?.x ?? 0).toBeLessThan(moved.x - 100);
    const copy = await guides.nth(2).boundingBox();
    if (!copy) throw new Error('no copy');
    await page.mouse.move(copy.x + 4, gy);
    await page.mouse.down();
    await page.mouse.move(copy.x - 2000, gy, { steps: 8 });
    await page.mouse.up();
    await expect(guides).toHaveCount(2);
    await page.getByTestId('deck-editor').screenshot({ path: shot('ruler-guides.png') });
    const list = (await state(page)).guide_list;
    await saveAndReopen(page, 'Guides.pptx');
    expect((await state(page)).guide_list).toEqual(list);
  });
});
