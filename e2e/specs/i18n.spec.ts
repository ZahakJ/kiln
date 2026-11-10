/**
 * Interface languages: File ▸ Options ▸ Language switches the interface live and keeps
 * the choice; Home, every app's ribbon, Backstage and a dialog read in German and Arabic; the
 * Grid status bar formats its numbers for the language; German's long words are not clipped at
 * desktop or phone widths. English stays the default.
 */
import { readFileSync } from 'node:fs';
import { fileURLToPath } from 'node:url';
import { expect, type Page, test } from '@playwright/test';
import { fill, focusSheet, select } from './grid-helpers.ts';
import { loaded, settle } from './i18n-helpers.ts';

const repo = (p: string) => fileURLToPath(new URL(`../../${p}`, import.meta.url));
const shot = (name: string) => repo(`e2e/screenshots/i18n/${name}`);
const locale = (l: string) =>
  JSON.parse(readFileSync(repo(`packages/i18n/locales/${l}.json`), 'utf8')) as Record<string, string>;
const de = locale('de');
const ar = locale('ar');
const MESSAGES = { de, ar } as const;

/** Starts the window in `ui` (the stored option, as Options leaves it). */
async function startIn(page: Page, ui: string): Promise<void> {
  await page.addInitScript((lang) => {
    if (!sessionStorage.getItem('kiln.e2e.lang')) {
      localStorage.setItem('kiln.options', JSON.stringify({ language: { ui: lang } }));
      sessionStorage.setItem('kiln.e2e.lang', '1');
    }
  }, ui);
}

async function openApp(
  page: Page,
  app: 'write' | 'grid' | 'deck',
  m: Record<string, string> = {},
): Promise<void> {
  await page.goto(`/#/${app}`);
  await loaded(page, m);
}

/** Text that does not fit the box that shows it (clipped, with no ellipsis to say so). */
async function clipped(page: Page, scope: string): Promise<string[]> {
  return page.evaluate((sel) => {
    const out: string[] = [];
    for (const root of document.querySelectorAll(sel)) {
      for (const el of root.querySelectorAll<HTMLElement>('*')) {
        if (!el.offsetParent && getComputedStyle(el).position !== 'fixed') continue;
        const own = [...el.childNodes].some((n) => n.nodeType === 3 && n.textContent?.trim());
        if (!own) continue;
        const s = getComputedStyle(el);
        if (s.textOverflow === 'ellipsis' || s.visibility === 'hidden') continue;
        if (
          el.closest(
            '.kiln-visually-hidden, .visually-hidden, [aria-hidden="true"] .kiln-keytip, .kiln-surface, canvas',
          )
        )
          continue;
        const clipsX = s.overflowX !== 'visible' && el.scrollWidth > el.clientWidth + 1;
        const clipsY =
          s.overflowY !== 'visible' && el.scrollHeight > el.clientHeight + 2 && s.overflowY !== 'auto';
        if (clipsX || clipsY)
          out.push(`${el.className || el.tagName}: ${el.textContent?.trim().slice(0, 40)}`);
      }
    }
    return out;
  }, scope);
}

test.describe('i18n: interface languages', () => {
  test('English is the default; Options ▸ Language switches live and the choice persists', async ({
    page,
  }) => {
    await openApp(page, 'write');
    await expect(page.locator('html')).toHaveAttribute('lang', 'en');
    await expect(page.getByRole('tab', { name: 'Insert', exact: true })).toBeVisible();
    await page.getByRole('button', { name: 'File' }).click();
    await page.getByRole('button', { name: 'Options' }).click();
    // The dialog's own name changes with the language.
    const options = page.getByRole('dialog', { name: /Options|Optionen/ });
    await options.getByRole('button', { name: 'Language', exact: true }).click();
    const select = options.getByTestId('options-ui-language');
    await expect(select.locator('option')).toHaveText(['System', 'English', 'Deutsch', 'العربية']);
    await select.selectOption('de');
    // Live: the dialog and the ribbon behind it change at once.
    await expect(options.getByRole('button', { name: de.Language, exact: true })).toBeVisible();
    await expect(page.locator('html')).toHaveAttribute('lang', 'de');
    await page.keyboard.press('Escape');
    await expect(page.getByRole('tab', { name: de.Insert, exact: true })).toBeVisible();
    await page.reload();
    await expect(page.getByRole('tab', { name: de.Insert, exact: true })).toBeVisible({ timeout: 45_000 });
    await expect(page.locator('html')).toHaveAttribute('lang', 'de');
    expect(
      await page.evaluate(() => JSON.parse(localStorage.getItem('kiln.options') ?? '{}').language.ui),
    ).toBe('de');
    // Back to English through the same switch.
    await page.getByRole('button', { name: de.File }).click();
    await page.getByRole('button', { name: de.Options }).click();
    const again = page.getByRole('dialog');
    await again.getByRole('button', { name: de.Language, exact: true }).click();
    await again.getByTestId('options-ui-language').selectOption('en');
    await expect(page.locator('html')).toHaveAttribute('lang', 'en');
    await expect(again.getByRole('button', { name: 'Language', exact: true })).toBeVisible();
  });

  for (const lang of ['de', 'ar'] as const) {
    const m = MESSAGES[lang];
    test(`${lang}: Home, every ribbon, Backstage and a dialog read in the language`, async ({ page }) => {
      await startIn(page, lang);
      await page.goto('/#/home');
      await expect(page.locator('html')).toHaveAttribute('lang', lang);
      await expect(page.locator('html')).toHaveAttribute('dir', lang === 'ar' ? 'rtl' : 'ltr');
      await expect(page.getByRole('navigation', { name: m['Kiln Home'] })).toBeVisible({ timeout: 45_000 });
      await expect(page.getByRole('region', { name: m.Create, exact: true })).toBeVisible();

      const tabs = {
        write: ['Home', 'Insert', 'Layout', 'References', 'Review', 'View'],
        grid: ['Home', 'Insert', 'Page Layout', 'Formulas', 'Data', 'Review', 'View'],
        deck: ['Home', 'Insert', 'Design', 'Transitions', 'Animations', 'Slide Show', 'Review', 'View'],
      } as const;
      for (const app of ['write', 'grid', 'deck'] as const) {
        await openApp(page, app, m);
        const tablist = page.getByRole('tablist', { name: m['Ribbon tabs'] });
        for (const name of tabs[app]) {
          await expect(tablist.getByRole('tab', { name: m[name], exact: true })).toBeVisible();
        }
        // A ribbon command's own name, keyed by its command id.
        await expect(
          page.getByRole('button', { name: new RegExp(`^${m['cmd.suite.edit.paste']}`) }).first(),
        ).toBeVisible();
      }

      await openApp(page, 'write', m);
      await page.getByRole('button', { name: m.File }).click();
      const nav = page.getByRole('navigation', { name: m['File menu'] });
      for (const name of ['Info', 'Save As', 'Print', 'Export', 'Options']) {
        await expect(nav.getByRole('button', { name: m[name], exact: true })).toBeVisible();
      }
      await page.keyboard.press('Escape');
      await page.keyboard.press('Control+h');
      const find = page.getByRole('dialog', { name: m['Find and Replace'] });
      await expect(find).toBeVisible();
      await page.evaluate(() => document.fonts.ready);
      await settle(page);
      await page.screenshot({ path: shot(`find-${lang}.png`) });
      await page.keyboard.press('Escape');
    });
  }

  test('Arabic as the default editing language: a new Write document starts right to left', async ({
    page,
  }) => {
    type Handle = {
      doc: number;
      controller: unknown;
      client: { query(doc: number, q: unknown): Promise<Record<string, unknown>> };
    };
    type Win = { __kilnWrite?: Handle };
    const docOf = () => page.evaluate(() => (window as unknown as Win).__kilnWrite?.doc ?? null);
    /** The core's state of the paragraph at the caret: its direction and text. */
    const paragraph = () =>
      page.evaluate(async () => {
        const w = (window as unknown as Win).__kilnWrite;
        if (!w) return null;
        const f = (await w.client.query(w.doc, { Write: 'Format' })) as {
          Write?: { Format?: { rtl?: boolean } };
        };
        const p = (await w.client.query(w.doc, 'Paragraphs')) as { Paragraphs?: { text: string }[] };
        return { rtl: f.Write?.Format?.rtl ?? null, text: p.Paragraphs?.map((x) => x.text) ?? [] };
      });

    await openApp(page, 'write');
    await page.waitForFunction(() => (window as unknown as Win).__kilnWrite?.controller != null, null, {
      timeout: 45_000,
    });
    // The document already open is left as it was.
    expect((await paragraph())?.rtl).toBe(false);
    const first = await docOf();

    await page.getByRole('button', { name: 'File' }).click();
    await page.getByRole('button', { name: 'Options' }).click();
    const options = page.getByRole('dialog', { name: 'Options' });
    await options.getByRole('button', { name: 'Language', exact: true }).click();
    await options.getByTestId('options-editing-language').selectOption('ar-SA');
    await page.keyboard.press('Escape');
    await expect(options).toBeHidden();
    expect(
      await page.evaluate(() => JSON.parse(localStorage.getItem('kiln.options') ?? '{}').language.editing),
    ).toBe('ar-SA');
    expect((await paragraph())?.rtl).toBe(false);

    // File ▸ New ▸ Blank document.
    await page.getByRole('button', { name: 'File' }).click();
    await page
      .getByRole('navigation', { name: 'File menu' })
      .getByRole('button', { name: 'New', exact: true })
      .click();
    await page.locator('[data-blank="write"]').click();
    await loaded(page, {});
    await page.waitForFunction(
      (before) => {
        const w = (window as unknown as Win).__kilnWrite;
        return !!w && w.controller != null && w.doc !== before;
      },
      first,
      { timeout: 45_000 },
    );
    await page.getByTestId('kiln-input-proxy').focus();
    await page.keyboard.type('مرحبا بالعالم');
    await expect.poll(paragraph, { timeout: 20_000 }).toEqual({ rtl: true, text: ['مرحبا بالعالم'] });
  });

  test('de and ar: the Grid status bar formats the sum for the language', async ({ page }) => {
    for (const [lang, sum] of [
      ['de', de.Sum],
      ['ar', ar.Sum],
    ] as const) {
      await page.goto('/#/home');
      await page.evaluate(
        (l) => localStorage.setItem('kiln.options', JSON.stringify({ language: { ui: l } })),
        lang,
      );
      await page.goto('/#/grid?blink=0');
      await page.reload();
      await page.waitForFunction(
        () =>
          Number(document.querySelector<HTMLElement>('[data-kiln-grid-surface]')?.dataset.kilnFrames ?? 0) >
          0,
        null,
        { timeout: 60_000 },
      );
      await focusSheet(page);
      await fill(page, [['1234.5'], ['1000']]);
      await select(page, 0, 0, 1, 0);
      // The core formats the sum like the cell (General: no grouping); the chrome gives it the
      // language's decimal separator.
      const decimal = await page.evaluate(
        (l) =>
          new Intl.NumberFormat(l === 'de' ? 'de-DE' : 'ar')
            .formatToParts(1.5)
            .find((p) => p.type === 'decimal')?.value ?? '.',
        lang,
      );
      if (lang === 'de') expect(decimal).toBe(',');
      const expected = `2234${decimal}5`;
      await expect(page.locator('.kiln-statusbar')).toContainText(sum);
      await expect(page.locator('.kiln-statusbar')).toContainText(expected);
    }
  });

  for (const [name, viewport] of [
    ['desktop', { width: 1366, height: 860 }],
    ['phone', { width: 390, height: 844 }],
  ] as const) {
    test(`de: long German labels are not clipped (${name})`, async ({ page }) => {
      await page.setViewportSize(viewport);
      await startIn(page, 'de');
      await page.goto('/#/home');
      await expect(
        page
          .getByRole('navigation', { name: de['Kiln Home sections'] })
          .or(page.getByRole('navigation', { name: de['Kiln Home'] }))
          .first(),
      ).toBeVisible({ timeout: 45_000 });
      await page.evaluate(() => document.fonts.ready);
      await settle(page);
      await page.screenshot({ path: shot(`home-de-${name}.png`) });
      expect(await clipped(page, '.kiln-home')).toEqual([]);
      for (const app of ['write', 'grid', 'deck'] as const) {
        await openApp(page, app, de);
        await page.evaluate(() => document.fonts.ready);
        await settle(page);
        await page.screenshot({ path: shot(`${app}-de-${name}.png`) });
        expect(await clipped(page, '.kiln-ribbon, .kiln-statusbar, .kiln-titlebar, .kiln-bottombar')).toEqual(
          [],
        );
      }
      if (name === 'phone') {
        // A phone: File is in the app bar's overflow menu.
        await page.getByRole('button', { name: de['More options'], exact: true }).click();
        await page.getByRole('menuitem', { name: de['cmd.ui.backstage.open'] }).click();
      } else {
        await page.getByRole('button', { name: de.File }).click();
      }
      await expect(page.getByRole('navigation', { name: de['File menu'] })).toBeVisible();
      await settle(page);
      await page.screenshot({ path: shot(`backstage-de-${name}.png`) });
      expect(await clipped(page, '.kiln-backstage')).toEqual([]);
      await page.getByRole('button', { name: de.Options, exact: true }).click();
      await page.evaluate(() => document.fonts.ready);
      await page.evaluate(() =>
        Promise.all(document.getAnimations().map((a) => a.finished.catch(() => undefined))),
      );
      await settle(page);
      await page.screenshot({ path: shot(`options-de-${name}.png`) });
      expect(await clipped(page, '[role="dialog"]')).toEqual([]);
    });
  }
});
