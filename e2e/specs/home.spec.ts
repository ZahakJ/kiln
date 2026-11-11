/**
 * Kiln Home: the start screen in light and dark on desktop and phone,
 * and in Arabic (reviewed screenshots in e2e/screenshots/home/), axe, creating a document through
 * the core, search across recent files and templates, pinned files, the empty states, Shared with
 * me, keyboard navigation, the welcome tour (shown once) and no trace of Kiln Notes.
 */
import { readFileSync } from 'node:fs';
import { fileURLToPath } from 'node:url';
import { expect, type Page, test } from '@playwright/test';
import { expectNoSeriousA11yViolations } from '../../packages/testing/src/playwright/a11y.ts';

const repo = (p: string) => fileURLToPath(new URL(`../../${p}`, import.meta.url));
const shot = (name: string) => repo(`e2e/screenshots/home/${name}`);
/** Starting the wasm core (first document) can take well over the 5 s expect default under CPU load. */
const CORE_START = 30_000;

interface IndexEntry {
  id: string;
  kind: 'Write' | 'Grid' | 'Deck';
  name: string;
  file: string;
  featured?: boolean;
}
const TEMPLATES: IndexEntry[] = JSON.parse(
  readFileSync(repo('assets/templates/index.json'), 'utf8'),
).templates;
const AR: Record<string, string> = JSON.parse(readFileSync(repo('packages/i18n/locales/ar.json'), 'utf8'));
/** Kiln Notes named as an app (speaker notes and a cell's notes are other apps' features). */
const NOTES_APP = /Kiln Notes|notebook|OneNote|Notizbuch/i;

async function openHome(page: Page) {
  await page.goto('/#/home');
  await expect(page.getByRole('heading', { level: 1 })).toContainText('What are we making today?');
  await expect(page.getByText('Your recent files will gather here')).toBeVisible({ timeout: CORE_START });
  await page.evaluate(() => document.fonts.ready);
}

function inArabic(page: Page) {
  return page.addInitScript(() =>
    localStorage.setItem('kiln.options', JSON.stringify({ language: { ui: 'ar' } })),
  );
}

/** Lets the CSS animations finish before a screenshot. */
async function settle(page: Page) {
  await page.evaluate(() =>
    Promise.all(
      document
        .getAnimations()
        .filter((a) => a.effect?.getTiming().iterations !== Number.POSITIVE_INFINITY)
        .map((a) => a.finished.catch(() => undefined)),
    ),
  );
}

test.describe('home', () => {
  for (const scheme of ['light', 'dark'] as const) {
    test(`Home (${scheme}, desktop) matches the brand layout and passes axe`, async ({ page }) => {
      await page.emulateMedia({ colorScheme: scheme });
      await openHome(page);
      for (const name of [
        /^New document in Kiln Write:/,
        /^New workbook in Kiln Grid:/,
        /^New deck in Kiln Deck:/,
      ]) {
        await expect(page.getByRole('button', { name })).toBeVisible();
      }
      await expect(page.getByRole('button', { name: /^Browse templates:/ })).toBeVisible();
      await expect(page.getByRole('navigation', { name: 'Kiln Home' })).toBeVisible();
      await expect(page.getByText('Coming from Microsoft Office?')).toBeVisible();
      await expect(
        page.getByRole('region', { name: 'Fresh from the kiln' }).locator('.kiln-tcard'),
      ).toHaveCount(4);
      expect(await page.evaluate(() => document.documentElement.scrollWidth > window.innerWidth)).toBe(false);
      await page.screenshot({ path: shot(`home-${scheme}-desktop.png`) });
      await expectNoSeriousA11yViolations(page);
    });

    test(`Home (${scheme}, phone) shows the doors two by two and the tab bar`, async ({ page }) => {
      await page.setViewportSize({ width: 390, height: 844 });
      await page.emulateMedia({ colorScheme: scheme });
      await openHome(page);
      await expect(page.getByRole('navigation', { name: 'Kiln Home sections' })).toBeVisible();
      await expect(page.locator('.kiln-home__new-btn')).toBeVisible();
      await expect(page.locator('.kiln-door')).toHaveCount(4);
      expect(await page.evaluate(() => document.documentElement.scrollWidth > window.innerWidth)).toBe(false);
      await page.screenshot({ path: shot(`home-${scheme}-phone.png`) });
      await expectNoSeriousA11yViolations(page);
    });

    test(`the template gallery (${scheme}, desktop and phone) lists every template by app`, async ({
      page,
    }) => {
      await page.emulateMedia({ colorScheme: scheme });
      await openHome(page);
      await page.getByRole('button', { name: /^Browse templates:/ }).click();
      await expect(page.getByRole('heading', { level: 1 })).toContainText('Start from a template');
      await expect(page.locator('[data-template]')).toHaveCount(TEMPLATES.length);
      await expect(page.locator('[data-blank]')).toHaveCount(3);
      await expect(page.getByText(`${TEMPLATES.length} templates`)).toBeVisible();
      await page.evaluate(() =>
        Promise.all([...document.images].map((i) => i.decode().catch(() => undefined))),
      );
      await page.screenshot({ path: shot(`gallery-${scheme}-desktop.png`) });
      await expectNoSeriousA11yViolations(page);
      await page.setViewportSize({ width: 390, height: 844 });
      expect(await page.evaluate(() => document.documentElement.scrollWidth > window.innerWidth)).toBe(false);
      await page.screenshot({ path: shot(`gallery-${scheme}-phone.png`) });
    });
  }

  test('Home and the gallery in Arabic mirror and pass axe', async ({ page }) => {
    await inArabic(page);
    await page.goto('/#/home');
    await expect(page.locator('html')).toHaveAttribute('dir', 'rtl');
    await expect(page.getByRole('heading', { level: 1 })).toContainText(
      AR['What are we making today?'] ?? '',
    );
    await page.evaluate(() => document.fonts.ready);
    // The rail is on the right.
    const rail = await page.getByRole('navigation', { name: AR['Kiln Home'] }).boundingBox();
    expect((rail?.x ?? 0) > 1000).toBe(true);
    await page.screenshot({ path: shot('home-ar-desktop.png') });
    await expectNoSeriousA11yViolations(page);
    await page.locator('.kiln-door--templates').click();
    await expect(page.getByRole('heading', { level: 1 })).toContainText(AR['Start from a template'] ?? '');
    await page.screenshot({ path: shot('gallery-ar-desktop.png') });
    await expectNoSeriousA11yViolations(page);
  });

  test('the Write door asks the core for a new document and opens the Write frame', async ({ page }) => {
    await openHome(page);
    await page.getByRole('button', { name: /^New document in Kiln Write:/ }).click();
    await expect(page).toHaveURL(/#\/write(\?doc=\d+)?$/);
    await expect(page.getByTestId('document-title')).toContainText('Document', { timeout: CORE_START });
    await expect(page.locator('.kiln-appframe')).toHaveAttribute('data-app', 'write');
  });

  test('New lists Write, Grid and Deck; Blank workbook opens Grid', async ({ page }) => {
    await openHome(page);
    await page.locator('.kiln-home__new-btn').click();
    const items = page.getByRole('menuitem');
    await expect(items).toHaveCount(3);
    await expect(page.getByRole('menuitem', { name: /Blank deck/ })).toBeEnabled();
    await page.getByRole('menuitem', { name: /Blank workbook/ }).click();
    await expect(page).toHaveURL(/#\/grid(\?doc=\d+)?$/);
    await expect(page.getByTestId('document-title')).toContainText('Book', { timeout: CORE_START });
  });

  test('no Kiln Notes anywhere on Home: doors, New, filters, gallery, its address', async ({ page }) => {
    await openHome(page);
    await page.locator('.kiln-home__new-btn').click();
    expect(await page.locator('body').innerText()).not.toMatch(NOTES_APP);
    await page.keyboard.press('Escape');
    await expect(page.locator('[data-filter]')).toHaveCount(3);
    await expect(page.locator('[data-door="notes"], [data-filter="notes"], [data-app="notes"]')).toHaveCount(
      0,
    );
    await page.getByRole('button', { name: /^Browse templates:/ }).click();
    expect(await page.locator('body').innerText()).not.toMatch(NOTES_APP);
    await page.goto('/#/notes');
    await page.reload();
    await expect(page.getByRole('heading', { level: 1 })).toContainText('What are we making today?');
    await expect(page.locator('.kiln-appframe')).toHaveCount(0);
    expect(await page.locator('body').innerText()).not.toMatch(NOTES_APP);
  });

  test('search looks through recent files and every template; Escape clears it', async ({ page }) => {
    await openHome(page);
    const target = TEMPLATES.find((t) => !t.featured) ?? TEMPLATES[0];
    if (!target) throw new Error('no templates');
    await page.keyboard.press('Control+K');
    await expect(page.getByRole('searchbox')).toBeFocused();
    await page.keyboard.type(target.name);
    // No file matches; the template does, though it is not featured.
    await expect(page.getByText('No matches')).toBeVisible();
    const found = page.getByRole('region', { name: 'Templates' });
    await expect(found.locator(`[data-template="${target.id}"]`)).toBeVisible();
    await page.screenshot({ path: shot('home-search.png') });
    await page.keyboard.press('Escape');
    await expect(page.getByRole('searchbox')).toHaveValue('');
    await expect(page.getByRole('region', { name: 'Fresh from the kiln' })).toBeVisible();
  });

  test('a recent file is found by search and pinned to the top', async ({ page }) => {
    await page.addInitScript(() => {
      // biome-ignore lint/suspicious/noExplicitAny: the API is not in the DOM typings
      const w = window as any;
      delete w.showOpenFilePicker;
      delete w.showSaveFilePicker;
    });
    await openHome(page);
    const [chooser] = await Promise.all([page.waitForEvent('filechooser'), page.keyboard.press('Control+o')]);
    await chooser.setFiles({
      name: 'Kiln tables.docx',
      mimeType: 'application/octet-stream',
      buffer: readFileSync(repo('fixtures/docx/features/tables.docx')),
    });
    await expect(page).toHaveURL(/#\/write\?doc=/, { timeout: CORE_START });
    await expect(page.getByTestId('document-title')).toContainText('Kiln tables', { timeout: CORE_START });
    await page.goto('/#/home');
    const row = page.getByRole('row', { name: /Kiln tables/ });
    await expect(row).toBeVisible({ timeout: CORE_START });
    await page.getByRole('searchbox').fill('tables');
    await expect(page.getByRole('row', { name: /Kiln tables/ })).toBeVisible();
    await page.getByRole('searchbox').fill('');
    await row.getByRole('button', { name: 'Pin Kiln tables.docx' }).click();
    await page.getByRole('tab', { name: 'Pinned' }).click();
    await expect(page.getByRole('row', { name: /Kiln tables/ })).toBeVisible();
    await page.screenshot({ path: shot('home-pinned.png') });
    await page.reload();
    await page.getByRole('tab', { name: 'Pinned' }).click();
    await expect(page.getByRole('row', { name: /Kiln tables/ })).toBeVisible({ timeout: CORE_START });
  });

  test('the empty states say what will appear and offer a way forward', async ({ page }) => {
    await openHome(page);
    const empty = page.getByRole('status').filter({ hasText: 'Your recent files will gather here' });
    await expect(empty.getByRole('button', { name: 'Open file' })).toBeVisible();
    await expect(empty.getByRole('button', { name: 'Browse templates' })).toBeVisible();
    await page.getByRole('tab', { name: 'Pinned' }).click();
    await expect(page.getByText('Nothing pinned yet')).toBeVisible();
    await page.getByRole('tab', { name: 'Shared with me' }).click();
    await expect(page.getByText('Nothing shared yet')).toBeVisible();
    await page.screenshot({ path: shot('home-empty-shared.png') });
    await page.getByRole('button', { name: 'Grid', exact: true }).click();
    await page.getByRole('searchbox').fill('zzz');
    await page.getByRole('tab', { name: 'Recent' }).click();
    await expect(page.getByText('No matches')).toBeVisible();
    await page.getByRole('button', { name: 'Clear the search and filter' }).click();
    await expect(page.getByText('Your recent files will gather here')).toBeVisible();
    await expectNoSeriousA11yViolations(page);
  });

  test('Shared with me lists the shared documents joined from links and opens them', async ({ page }) => {
    const link = '#/write?doc=k7p2q9xw&server=ws%3A%2F%2F127.0.0.1%3A9&token=t';
    await page.addInitScript((l) => {
      localStorage.setItem(
        'kiln.pref.ui.home.shared',
        JSON.stringify([{ link: l, name: 'Studio roadmap', kind: 'Write', at: Date.now() - 600_000 }]),
      );
    }, link);
    await openHome(page);
    await page.getByRole('button', { name: /^Shared with me/ }).click();
    const row = page.getByRole('row', { name: /Studio roadmap/ });
    await expect(row).toContainText('Shared through 127.0.0.1:9');
    await expect(row).toContainText('10 minutes ago');
    await page.screenshot({ path: shot('home-shared.png') });
    await row.getByRole('button', { name: /^Studio roadmap/ }).click();
    await expect(page).toHaveURL(/#\/write\?doc=k7p2q9xw&server=/);
  });

  test('Home is driven by the keyboard: Tab through, arrows in lists and the gallery, Enter opens', async ({
    page,
  }) => {
    await openHome(page);
    // Tab from the top reaches New, the rail, the search, the doors and the templates.
    const reached = new Set<string>();
    for (let i = 0; i < 40; i++) {
      await page.keyboard.press('Tab');
      const what = await page.evaluate(() => {
        const el = document.activeElement as HTMLElement | null;
        return el?.dataset.door ?? el?.dataset.template ?? el?.className.split(' ')[0] ?? '';
      });
      reached.add(what);
    }
    for (const want of ['kiln-home__new-btn', 'kiln-home__nav-item', 'write', 'grid', 'deck', 'templates'])
      expect([...reached], want).toContain(want);
    // Enter on the templates door opens the gallery; the arrows move between the cards.
    await page.locator('[data-door="templates"]').focus();
    await page.keyboard.press('Enter');
    await expect(page.getByRole('heading', { level: 1 })).toContainText('Start from a template');
    await page.locator('.kiln-tcard').first().focus();
    await page.keyboard.press('ArrowRight');
    await expect(page.locator('.kiln-tcard').nth(1)).toBeFocused();
    await page.keyboard.press('ArrowDown');
    const below = await page.evaluate(() => document.activeElement?.getBoundingClientRect().top ?? 0);
    const first = await page
      .locator('.kiln-tcard')
      .first()
      .evaluate((el) => el.getBoundingClientRect().top);
    expect(below).toBeGreaterThan(first);
    await page.keyboard.press('End');
    await expect(page.locator('.kiln-tcard').last()).toBeFocused();
    // Enter on a template opens its app with the template's content.
    const last = TEMPLATES.at(-1);
    await page.keyboard.press('Enter');
    await expect(page).toHaveURL(new RegExp(`#/${last?.kind.toLowerCase()}\\?doc=\\d+`), {
      timeout: CORE_START,
    });
  });

  test('Ctrl+K searches, the app filters narrow the lists, Settings opens Options', async ({ page }) => {
    await openHome(page);
    await page.keyboard.press('Control+K');
    await expect(page.getByRole('searchbox')).toBeFocused();
    await page.keyboard.type('zzzz');
    await expect(page.getByText('No matches')).toBeVisible();
    await page.getByRole('searchbox').fill('');
    await page.getByRole('button', { name: 'Grid', exact: true }).click();
    await expect(page.getByRole('button', { name: /Grid only/ })).toBeVisible();
    await page.getByRole('button', { name: 'Settings', exact: true }).first().click();
    await expect(page.getByRole('dialog', { name: /Options/ })).toBeVisible();
    await expectNoSeriousA11yViolations(page);
  });

  test('dismissing the Office banner is remembered through the host', async ({ page }) => {
    await openHome(page);
    await page.getByRole('button', { name: 'Dismiss' }).click();
    await expect(page.getByText('Coming from Microsoft Office?')).toHaveCount(0);
    expect(await page.evaluate(() => localStorage.getItem('kiln.pref.ui.home.switcher_dismissed'))).toBe('1');
    await page.reload();
    await expect(page.getByRole('heading', { level: 1 })).toBeVisible();
    await expect(page.getByText('Coming from Microsoft Office?')).toHaveCount(0);
  });
});

test.describe('home: the welcome tour', () => {
  // A fresh profile: the tour has not been seen yet.
  test.use({ storageState: { cookies: [], origins: [] } });

  test('the welcome tour shows on the first visit, in light and dark, on desktop and phone', async ({
    page,
  }) => {
    for (const viewport of [
      { width: 1440, height: 900, name: 'desktop' },
      { width: 390, height: 844, name: 'phone' },
    ]) {
      for (const scheme of ['light', 'dark'] as const) {
        await page.setViewportSize(viewport);
        await page.emulateMedia({ colorScheme: scheme });
        await page.goto('/#/home');
        await page.reload();
        const tour = page.getByRole('dialog', { name: 'Your Office files, as they are' });
        await expect(tour).toBeVisible();
        await expect(tour).toHaveAttribute('aria-modal', 'false');
        // It docks at the bottom without covering the page's heading.
        await expect(page.getByRole('heading', { level: 1 })).toBeVisible();
        await page.evaluate(() => document.fonts.ready);
        await settle(page);
        await page.screenshot({ path: shot(`tour-${scheme}-${viewport.name}.png`) });
        await expectNoSeriousA11yViolations(page);
      }
    }
  });

  test('the tour mirrors in Arabic, docked bottom-left', async ({ page }) => {
    await inArabic(page);
    await page.goto('/#/home');
    const tour = page.getByRole('dialog', { name: AR['Your Office files, as they are'] });
    await expect(tour).toBeVisible();
    await settle(page);
    expect(((await tour.boundingBox())?.x ?? 999) < 100).toBe(true);
    await page.evaluate(() => document.fonts.ready);
    await page.screenshot({ path: shot('tour-ar-desktop.png') });
    await expectNoSeriousA11yViolations(page);
  });

  test('the tour steps by keyboard, is skipped for good and can be taken again', async ({ page }) => {
    await page.goto('/#/home');
    await expect(page.getByRole('dialog', { name: 'Your Office files, as they are' })).toBeVisible();
    // The tour is the first thing Tab reaches; the arrow keys step through its cards.
    await page.keyboard.press('Tab');
    await expect(page.getByRole('button', { name: 'Skip the tour' })).toBeFocused();
    await page.keyboard.press('ArrowRight');
    await expect(page.getByRole('dialog', { name: 'The ribbon you already know' })).toContainText('2 of 3');
    await settle(page);
    await page.screenshot({ path: shot('tour-step-2.png') });
    await page.getByRole('button', { name: 'Next' }).click();
    await expect(page.getByRole('dialog', { name: 'Start from something finished' })).toBeVisible();
    await page.screenshot({ path: shot('tour-step-3.png') });
    await page.getByRole('button', { name: 'Skip tour' }).click();
    await expect(page.getByRole('dialog')).toHaveCount(0);
    expect(await page.evaluate(() => localStorage.getItem('kiln.pref.ui.home.tour_seen'))).toBe('1');
    // Shown once: not again after a reload.
    await page.reload();
    await expect(page.getByRole('heading', { level: 1 })).toBeVisible();
    await page.waitForTimeout(500);
    await expect(page.getByRole('dialog')).toHaveCount(0);
    // It can be taken again from the banner; finishing it opens the templates.
    await page.getByRole('button', { name: 'Take the tour' }).click();
    for (let i = 0; i < 2; i++) await page.getByRole('button', { name: 'Next' }).click();
    await page.getByRole('dialog').getByRole('button', { name: 'Browse templates' }).click();
    await expect(page.getByRole('dialog')).toHaveCount(0);
    await expect(page.getByRole('heading', { level: 1 })).toContainText('Start from a template');
  });
});

test.describe('home: Office names', () => {
  test('a ribbon ScreenTip names the command as Office calls it', async ({ page }) => {
    await page.goto('/#/write');
    await expect(page.getByTestId('document-title')).toContainText('Document', { timeout: CORE_START });
    // A visible ribbon command whose Office name differs from Kiln's title.
    const id = await page.evaluate(() => {
      // biome-ignore lint/suspicious/noExplicitAny: the development handle
      const registry = (window as any).__kiln.registry;
      const els = [...document.querySelectorAll<HTMLElement>('.kiln-ribbon [data-command]')].filter(
        (el) => el.offsetParent !== null,
      );
      for (const el of els) {
        const meta = registry.get(el.dataset.command);
        if (meta?.officeName && meta.officeName.toLowerCase() !== meta.title.toLowerCase())
          return el.dataset.command;
      }
      return null;
    });
    expect(id).not.toBeNull();
    await page.locator(`.kiln-ribbon [data-command="${id}"]`).first().hover();
    await expect(page.getByRole('tooltip')).toContainText('In Microsoft Office:');
  });
});
