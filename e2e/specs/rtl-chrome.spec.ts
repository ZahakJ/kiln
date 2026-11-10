/**
 * The interface in Arabic, right to left: the root is `dir="rtl" lang="ar"`; the ribbon,
 * panes, status bar, title bar, Backstage and Home are mirrored; icons that point along the line
 * (undo, redo, lists, indents, arrows) are flipped and the others are not; the document canvas
 * keeps its own direction. Screenshots of Home, Write, Grid and Deck at desktop and phone widths,
 * light and dark, and axe on each (0 serious or critical).
 */
import { readFileSync } from 'node:fs';
import { fileURLToPath } from 'node:url';
import { expect, type Locator, type Page, test } from '@playwright/test';
import { expectNoSeriousA11yViolations } from '../../packages/testing/src/playwright/a11y.ts';
import { loaded, settle } from './i18n-helpers.ts';

const repo = (p: string) => fileURLToPath(new URL(`../../${p}`, import.meta.url));
const shot = (name: string) => repo(`e2e/screenshots/rtl-ui/${name}`);
const ar = JSON.parse(readFileSync(repo('packages/i18n/locales/ar.json'), 'utf8')) as Record<string, string>;

const VIEWPORTS = {
  desktop: { width: 1440, height: 900 },
  phone: { width: 390, height: 844 },
} as const;

async function inArabic(page: Page): Promise<void> {
  await page.addInitScript(() => {
    localStorage.setItem('kiln.options', JSON.stringify({ language: { ui: 'ar' } }));
  });
}

async function open(page: Page, app: 'home' | 'write' | 'grid' | 'deck'): Promise<void> {
  await page.goto(`/#/${app}`);
  if (app === 'home') {
    await expect(page.getByRole('region', { name: ar.Create, exact: true })).toBeVisible({ timeout: 45_000 });
  } else {
    await loaded(page, ar);
  }
  await page.evaluate(() => document.fonts.ready);
}

async function box(l: Locator) {
  const b = await l.boundingBox();
  if (!b) throw new Error('no box');
  return b;
}

async function run(page: Page, id: string): Promise<void> {
  await page.evaluate(
    // biome-ignore lint/suspicious/noExplicitAny: the window handle is not typed here
    async (i) => (window as any).__kiln?.registry.execute(i),
    id,
  );
}

/** The horizontal flip an element is drawn with (-1 mirrored, 1 as drawn). */
async function flipOf(l: Locator): Promise<number> {
  return l.evaluate((el) => new DOMMatrixReadOnly(getComputedStyle(el).transform).a);
}

test.describe('RTL interface in Arabic', () => {
  for (const app of ['home', 'write', 'grid', 'deck'] as const) {
    for (const [vp, size] of Object.entries(VIEWPORTS)) {
      for (const scheme of ['light', 'dark'] as const) {
        test(`rtl ${app}: ${vp}, ${scheme}, screenshot and axe`, async ({ page }) => {
          await page.setViewportSize(size);
          await page.emulateMedia({ colorScheme: scheme });
          await inArabic(page);
          await open(page, app);
          await expect(page.locator('html')).toHaveAttribute('dir', 'rtl');
          await expect(page.locator('html')).toHaveAttribute('lang', 'ar');
          // Arabic text is drawn in a face that covers it (not a fallback box).
          const covered = await page.evaluate(() =>
            [...document.fonts].some(
              (f) => f.family.replace(/"/g, '') === 'Kiln Arabic Sans' && f.status === 'loaded',
            ),
          );
          expect(covered).toBe(true);
          const overflow = await page.evaluate(
            () => document.documentElement.scrollWidth > window.innerWidth + 1,
          );
          expect(overflow).toBe(false);
          await settle(page);
          await page.screenshot({ path: shot(`${app}-ar-${vp}-${scheme}.png`) });
          await expectNoSeriousA11yViolations(page);
        });
      }
    }
  }

  test('rtl write: ribbon, title bar, panes and status bar are mirrored; the canvas is not', async ({
    page,
  }) => {
    await page.setViewportSize(VIEWPORTS.desktop);
    await inArabic(page);
    await open(page, 'write');
    const width = VIEWPORTS.desktop.width;

    // The ribbon's tabs run from the right: File first, at the right edge.
    const tabs = page.getByRole('tablist', { name: ar['Ribbon tabs'] }).getByRole('tab');
    const first = await box(tabs.first());
    const last = await box(tabs.last());
    expect(first.x).toBeGreaterThan(last.x);
    const file = await box(page.getByRole('button', { name: ar.File, exact: true }));
    expect(file.x + file.width / 2).toBeGreaterThan(width / 2);

    // The title bar: Undo at the right, its arrow flipped; a symmetric icon stays.
    const undo = page.getByRole('button', { name: new RegExp(`^${ar.Undo}`) }).first();
    expect((await box(undo)).x).toBeGreaterThan(width / 2);
    expect(await flipOf(undo.locator('svg'))).toBe(-1);
    expect(
      await flipOf(
        page
          .getByRole('button', { name: new RegExp(`^${ar.Redo}`) })
          .first()
          .locator('svg'),
      ),
    ).toBe(-1);
    expect(
      await flipOf(
        page
          .getByRole('button', { name: new RegExp(`^${ar.Save}`) })
          .first()
          .locator('svg'),
      ),
    ).toBe(1);
    // Bullets and indents in the ribbon are flipped too.
    const bullets = page.locator('.kiln-ribbon .kiln-icon[data-mirror]');
    expect(await bullets.count()).toBeGreaterThan(2);
    expect(await flipOf(bullets.first())).toBe(-1);

    // The status bar: the page count at the right, the zoom at the left.
    const start = await box(page.locator('.kiln-statusbar__left'));
    const end = await box(page.locator('.kiln-statusbar__right'));
    expect(start.x).toBeGreaterThan(end.x);

    // The Navigation pane, at the start of the window, docks at the right.
    await run(page, 'suite.view.navigation_pane');
    const pane = page.locator('.kiln-write__pane').filter({ has: page.getByTestId('write-navigation') });
    await expect(pane).toBeVisible();
    const p = await box(pane);
    expect(p.x).toBeGreaterThan(width / 2);

    // The document canvas keeps its own direction.
    await expect(page.getByTestId('kiln-surface')).toHaveAttribute('dir', 'ltr');
    await page.evaluate(() => document.fonts.ready);
    await settle(page);
    await page.screenshot({ path: shot('write-ar-mirrored-pane.png') });
    await expectNoSeriousA11yViolations(page);
  });

  test('rtl grid and deck: ribbon and status bar mirrored, Backstage from the right', async ({ page }) => {
    await page.setViewportSize(VIEWPORTS.desktop);
    await inArabic(page);
    const width = VIEWPORTS.desktop.width;
    for (const app of ['grid', 'deck'] as const) {
      await open(page, app);
      const tabs = page.getByRole('tablist', { name: ar['Ribbon tabs'] }).getByRole('tab');
      expect((await box(tabs.first())).x).toBeGreaterThan((await box(tabs.last())).x);
      const start = await box(page.locator('.kiln-statusbar__left'));
      const end = await box(page.locator('.kiln-statusbar__right'));
      expect(start.x).toBeGreaterThan(end.x);
    }
    await page.getByRole('button', { name: ar.File, exact: true }).click();
    const rail = page.getByRole('navigation', { name: ar['File menu'] });
    await expect(rail).toBeVisible();
    const r = await box(rail);
    expect(r.x + r.width / 2).toBeGreaterThan(width / 2);
    const back = page.getByRole('button', { name: ar['Back to the document'] });
    expect(await flipOf(back.locator('svg'))).toBe(-1);
    await page.evaluate(() => document.fonts.ready);
    await settle(page);
    await page.screenshot({ path: shot('backstage-ar-desktop.png') });
    await expectNoSeriousA11yViolations(page);
  });

  test('rtl: the arrow keys follow the mirrored ribbon and Key Tips show on it', async ({ page }) => {
    await page.setViewportSize(VIEWPORTS.desktop);
    await inArabic(page);
    await open(page, 'write');
    const tabs = page.getByRole('tablist', { name: ar['Ribbon tabs'] });
    const home = tabs.getByRole('tab', { name: ar.Home, exact: true });
    await home.focus();
    // ArrowLeft moves on to the next tab, which sits to the left.
    await page.keyboard.press('ArrowLeft');
    const next = tabs.getByRole('tab', { name: ar.Insert, exact: true });
    await expect(next).toBeFocused();
    await page.keyboard.press('ArrowRight');
    await expect(home).toBeFocused();
    // Key Tips sit on the mirrored ribbon.
    await page.keyboard.press('Escape');
    await page.keyboard.press('Alt');
    const tip = page.locator('.kiln-keytip').first();
    await expect(tip).toBeVisible();
    await settle(page);
    await page.screenshot({ path: shot('write-ar-keytips.png') });
    await page.keyboard.press('Escape');
  });
});
