/**
 * Kiln Deck's Review tab in the browser: comments on an object and on the slide with
 * replies, resolving and markers, kept through a save; Previous, Next and Delete across the
 * slides; Spelling (F7) through the slides and their notes; Version History of a shared
 * presentation: each version's slides as thumbnails with the changed ones marked, a version
 * shown read-only and restored. Screenshots go to e2e/screenshots/deck-review/.
 */
import { fileURLToPath } from 'node:url';
import { expect, type Page, test } from '@playwright/test';
import { join, share, startServer } from './collab-helpers.ts';
import { deck, newDeck, ribbonTab, saveAndReopen, state, waitForStage } from './deck-helpers.ts';

const shot = (name: string) => fileURLToPath(new URL(`../screenshots/deck-review/${name}`, import.meta.url));

/** Types `text` into the shape `index` of the current slide. */
async function typeInto(page: Page, index: number, text: string) {
  const st = await state(page);
  const shape = st.shapes[st.shapes.length - 1 - index];
  if (!shape) throw new Error(`no shape ${index}`);
  await deck(page, { SelectShapes: { ids: [shape.id] } });
  await deck(page, 'EditText');
  await deck(page, { InsertText: { text } });
  await deck(page, 'Escape');
  await deck(page, 'Escape');
}

test.describe('deck-review', () => {
  test('comments: on an object and on the slide, with a reply, resolved, shown as markers and saved', async ({
    page,
  }) => {
    await newDeck(page);
    const title = (await state(page)).shapes.find((s) => /^Title/.test(s.name));
    if (!title) throw new Error('no title');
    await deck(page, { SelectShapes: { ids: [title.id] } });
    // Ctrl+Alt+M opens the pane with a box for the comment.
    await page.getByTestId('deck-stage').focus();
    await page.keyboard.press('Control+Alt+m');
    const pane = page.getByTestId('deck-comments-pane');
    await expect(pane).toBeVisible();
    await pane.getByTestId('deck-comment-composer').fill('Can the title be bigger?');
    await pane.getByTestId('deck-comment-composer').press('Control+Enter');
    await expect(pane.getByTestId('deck-comment')).toHaveCount(1);
    await expect(pane.getByText(/^On Title/)).toBeVisible();
    // A reply, then resolved.
    await pane.getByRole('button', { name: 'Reply', exact: true }).click();
    await pane.getByTestId('deck-comment-reply-box').fill('Made it 60 pt.');
    await pane.getByRole('button', { name: 'Reply', exact: true }).click();
    await expect(pane.getByTestId('deck-comment-reply')).toHaveCount(1);
    // A comment on the slide itself.
    await deck(page, { SelectShapes: { ids: [] } });
    await pane.getByTestId('deck-comment-new').click();
    await pane.getByTestId('deck-comment-composer').fill('Add the date of the open day');
    await pane.getByRole('button', { name: 'Post' }).click();
    await expect(pane.getByTestId('deck-comment')).toHaveCount(2);
    await expect(page.getByTestId('deck-comment-marker')).toHaveCount(2);
    await pane.getByRole('button', { name: 'Resolve thread' }).first().click();
    await expect
      .poll(async () => (await state(page)).comments as unknown)
      .toMatchObject([{ resolved: true, replies: [{ text: 'Made it 60 pt.' }] }, { resolved: false }]);
    await page.getByTestId('deck-editor').screenshot({ path: shot('comments-pane.png') });
    // Show Comments closes the pane; the markers stay; one opens its thread.
    await ribbonTab(page, 'Review');
    await page.getByRole('button', { name: 'Show Comments' }).click();
    await expect(pane).toBeHidden();
    await page.getByTestId('deck-comment-marker').last().click();
    await expect(pane).toBeVisible();
    await saveAndReopen(page, 'Commented.pptx');
    const st = await state(page);
    expect(st.comment_total).toBe(2);
    expect(st.comments).toMatchObject([
      { text: 'Can the title be bigger?', resolved: true, shape_name: expect.stringMatching(/^Title/) },
      { text: 'Add the date of the open day', shape: null },
    ]);
  });

  test('comments: Previous and Next go across the slides, Delete removes threads', async ({ page }) => {
    await newDeck(page);
    await deck(page, { NewComment: { text: 'First', author: 'Ana', initials: 'A' } });
    await deck(page, { NewSlide: { layout: null } });
    await deck(page, { NewSlide: { layout: null } });
    await deck(page, { NewComment: { text: 'Third', author: 'Ben', initials: 'B' } });
    await deck(page, { SelectSlide: { index: 1, extend: false, toggle: false } });
    await ribbonTab(page, 'Review');
    await page.getByRole('button', { name: 'Next Comment' }).click();
    await expect.poll(async () => (await state(page)).current).toBe(2);
    await expect(page.getByTestId('deck-comments-pane').getByText('Third')).toBeVisible();
    await page.getByRole('button', { name: 'Next Comment' }).click();
    await expect.poll(async () => (await state(page)).current).toBe(0);
    await page.getByRole('button', { name: 'Previous Comment' }).click();
    await expect.poll(async () => (await state(page)).current).toBe(2);
    // Delete ▸ Delete All Comments in This Presentation.
    await page.getByRole('button', { name: 'Delete Comment', exact: true }).click();
    await expect.poll(async () => (await state(page)).comment_total).toBe(1);
    await page.getByRole('button', { name: 'More options for Delete Comment' }).click();
    await page.getByRole('menuitem', { name: 'Delete All Comments in This Presentation' }).click();
    await expect.poll(async () => (await state(page)).comment_total).toBe(0);
  });

  test('spelling: F7 goes through the slides and their notes, changing and ignoring words', async ({
    page,
  }) => {
    await newDeck(page);
    await deck(page, { NewSlide: { layout: null } });
    await typeInto(page, 0, 'Firing schedule');
    await typeInto(page, 1, 'Bisque then glazze');
    await deck(page, { SetNotes: { text: 'Remmember the cones' } });
    await deck(page, { SelectSlide: { index: 0, extend: false, toggle: false } });
    await page.getByTestId('deck-stage').focus();
    await page.keyboard.press('F7');
    const pane = page.getByTestId('deck-spelling');
    await expect(pane.getByTestId('deck-spell-word')).toHaveText('glazze', { timeout: 30_000 });
    await expect(pane.getByRole('option', { name: 'glaze', exact: true })).toBeVisible();
    await page.getByTestId('deck-editor').screenshot({ path: shot('spelling.png') });
    await pane.getByRole('option', { name: 'glaze', exact: true }).click();
    await pane.getByTestId('deck-spell-change').click();
    await expect(pane.getByTestId('deck-spell-word')).toHaveText('Remmember');
    await expect(pane.getByText('in the notes of slide 2')).toBeVisible();
    await pane.getByRole('button', { name: 'Ignore Once' }).click();
    await expect(pane.getByText("Spell check complete. You're good to go!")).toBeVisible();
    const st = await state(page);
    expect(st.spell_done).toBe(true);
    await deck(page, { SelectSlide: { index: 1, extend: false, toggle: false } });
    const body = (await state(page)).shapes.find((s) => !/^Title/.test(s.name));
    expect(body).toBeTruthy();
    const outline = await page.evaluate(async () => {
      const d = (
        window as unknown as {
          __kilnDeck: { doc: number; client: { query(doc: number, q: unknown): Promise<unknown> } };
        }
      ).__kilnDeck;
      return d.client.query(d.doc, { Deck: 'Outline' });
    });
    expect(JSON.stringify(outline)).toContain('Bisque then glaze');
  });

  // the parity row deck.review.version-history.
  test('version history', async ({ browser }) => {
    test.setTimeout(900_000);
    const server = await startServer();
    const ca = await browser.newContext({ viewport: { width: 1440, height: 900 } });
    const cb = await browser.newContext({ viewport: { width: 1280, height: 800 } });
    try {
      const a = await ca.newPage();
      const b = await cb.newPage();
      await newDeck(a);
      const link = await share(a, server, 'Ana Martín');
      await join(b, link, 'Ben Okafor');
      await waitForStage(b);
      await expect.poll(async () => (await state(b)).slides.length).toBe(1);
      // Ben adds a slide; Ana saves, which is a version.
      await deck(b, { NewSlide: { layout: null } });
      await expect.poll(async () => (await state(a)).slides.length, { timeout: 20_000 }).toBe(2);
      await a.evaluate(async () => {
        const d = (
          window as unknown as {
            __kilnDeck: { doc: number; client: { execute(doc: number, c: unknown): Promise<unknown> } };
          }
        ).__kilnDeck;
        await d.client.execute(d.doc, {
          Suite: { SaveAs: { path: 'opfs:documents/Versions.pptx', format: 'Pptx' } },
        });
      });
      await a.evaluate(async () => {
        await (
          window as unknown as { __kiln?: { registry: { execute(id: string): Promise<unknown> } } }
        ).__kiln?.registry.execute('suite.file.version_history');
      });
      const pane = a.getByTestId('version-history');
      await expect(pane).toBeVisible();
      const saved = pane.getByRole('button', { name: /Saved.*Ana Martín/ });
      await expect(saved).toBeVisible({ timeout: 20_000 });
      // The version's slides, the new one marked.
      await saved.click();
      const slides = pane.getByTestId('version-slide');
      await expect(slides).toHaveCount(2);
      await expect(slides.nth(1)).toHaveAttribute('data-mark', 'Added');
      await expect(slides.nth(0)).toHaveAttribute('data-mark', 'Same');
      await expect(slides.nth(1).getByText('New')).toBeVisible();
      await expect(a.getByTestId('version-bar')).toBeVisible();
      await a.screenshot({ path: shot('version-history.png') });
      // Ben goes on; Ana's view stays on the version until she restores it.
      await deck(b, { NewSlide: { layout: null } });
      await expect.poll(async () => (await state(b)).slides.length).toBe(3);
      expect((await state(a)).slides.length).toBe(2);
      await pane.getByTestId('version-restore').click();
      await expect(a.getByTestId('version-bar')).toBeHidden();
      await expect.poll(async () => (await state(a)).slides.length).toBe(2);
      await expect.poll(async () => (await state(b)).slides.length, { timeout: 20_000 }).toBe(2);
      await expect(pane.getByRole('button', { name: /Restored from/ })).toBeVisible();
    } finally {
      await ca.close();
      await cb.close();
      await server.stop();
    }
  });
});
