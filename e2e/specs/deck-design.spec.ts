/**
 * Kiln Deck's Design tab in the browser: the twelve themes from the gallery, Browse for
 * Themes with a PowerPoint presentation's master, the Variants gallery's
 * pictures, and Colors, Fonts, Effects and Background Styles, all kept through a save.
 * Screenshots go to e2e/screenshots/deck-design/.
 */
import { fileURLToPath } from 'node:url';
import { expect, type Page, test } from '@playwright/test';
import { newDeck, repoFile, ribbonTab, saveAndReopen, saved, state, zipEntry } from './deck-helpers.ts';

const shot = (name: string) => fileURLToPath(new URL(`../screenshots/deck-design/${name}`, import.meta.url));

/** Opens a gallery's full view (its More button). */
async function moreOf(page: Page, gallery: string) {
  await page.getByRole('button', { name: `More ${gallery}` }).click();
}

/** A menu item of the open gallery's menus, through its submenu. */
async function pick(page: Page, submenu: string, item: string) {
  await page.getByRole('button', { name: submenu, exact: true }).click();
  await page.getByRole('menuitem', { name: item, exact: true }).click();
}

test.describe('deck-design', () => {
  test('themes: the gallery applies the bundled themes and Browse for Themes takes a presentation’s master', async ({
    page,
  }) => {
    await newDeck(page);
    await ribbonTab(page, 'Design');
    for (const name of ['Cobalt', 'Jade', 'Kiln']) {
      await page.getByRole('option', { name, exact: true }).first().click();
      await expect.poll(async () => (await state(page)).theme).toBe(name);
    }
    await moreOf(page, 'Themes');
    await page.evaluate(() => Promise.all(document.getAnimations().map((a) => a.finished)));
    await page.screenshot({ path: shot('themes-gallery.png') });
    const [chooser] = await Promise.all([
      page.waitForEvent('filechooser'),
      page.getByRole('button', { name: 'Browse for Themes…' }).click(),
    ]);
    await chooser.setFiles(repoFile('fixtures/pptx/features/theme-colors.pptx'));
    await expect.poll(async () => (await state(page)).theme).not.toBe('Kiln');
    const theme = (await state(page)).theme;
    const layouts = (await state(page)).layouts as { name: string }[];
    expect(layouts.map((l) => l.name)).toContain('Title and Content');
    await saveAndReopen(page, 'Themed.pptx');
    expect((await state(page)).theme).toBe(theme);
  });

  test('variants: the gallery shows the theme’s variants; Colors, Fonts, Effects and Background Styles change it', async ({
    page,
  }) => {
    await newDeck(page);
    await ribbonTab(page, 'Design');
    // The variants are drawn from the theme.
    const variant = page.getByRole('option', { name: 'Variant 2', exact: true }).first();
    await expect(variant.locator('img')).toHaveAttribute('src', /^data:image\/png/);
    const scheme = async () =>
      /<a:clrScheme[\s\S]*?<\/a:clrScheme>/.exec(zipEntry(await saved(page), 'ppt/theme/theme1.xml'))?.[0];
    const before = await scheme();
    await variant.click();
    await expect.poll(scheme).not.toBe(before);
    await moreOf(page, 'Variants');
    await pick(page, 'Colors', 'Grayscale');
    await moreOf(page, 'Variants');
    await pick(page, 'Fonts', 'Office');
    await moreOf(page, 'Variants');
    await pick(page, 'Effects', 'Raku');
    await moreOf(page, 'Variants');
    await pick(page, 'Background Styles', 'Style 10');
    await page.getByTestId('deck-editor').screenshot({ path: shot('variants.png') });
    const bytes = await saved(page);
    const xml = zipEntry(bytes, 'ppt/theme/theme1.xml');
    expect(xml).toContain('<a:clrScheme name="Grayscale">');
    expect(xml).toContain('typeface="Calibri Light"');
    expect(zipEntry(bytes, 'ppt/slideMasters/slideMaster1.xml')).toMatch(
      /<p:bgRef idx="1001"><a:schemeClr val="bg2"/,
    );
    await saveAndReopen(page, 'Variants.pptx');
    const again = zipEntry(await saved(page), 'ppt/theme/theme1.xml');
    expect(again).toContain('<a:clrScheme name="Grayscale">');
  });
});
