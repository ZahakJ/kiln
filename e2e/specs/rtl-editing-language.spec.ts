/**
 * the editing language follows the display language until the user picks one, as in
 * Office. With only the interface set to Arabic, a new document, workbook and presentation are
 * مستند1, مصنف1 (its sheet ورقة1, right to left, column A at the right) and عرض تقديمي1; in German
 * Dokument1, Mappe1 (Tabelle1) and Präsentation1. A language picked in Options ▸ Language wins,
 * the first run that takes the interface from the system follows the system, and a file that is
 * opened keeps its own names and direction. An Arabic presentation is right to left with its
 * master mirrored, and its placeholders prompt in the interface language ("انقر لإضافة عنوان"),
 * which is never saved. Screenshots of the new documents and of the Home tab ribbons in Arabic
 * and German, light and dark.
 */
import { readFileSync } from 'node:fs';
import { fileURLToPath } from 'node:url';
import { expect, type Page, test } from '@playwright/test';
import { type DeckWindow, deck, query as deckQuery, onSlide, saved, zipEntry } from './deck-helpers.ts';
import { loaded, settle } from './i18n-helpers.ts';
import { decodePng } from './surface-helpers.ts';

const repo = (p: string) => fileURLToPath(new URL(`../../${p}`, import.meta.url));
const shot = (name: string) => repo(`e2e/screenshots/rtl-ui/${name}`);
const ar = JSON.parse(readFileSync(repo('packages/i18n/locales/ar.json'), 'utf8')) as Record<string, string>;
const de = JSON.parse(readFileSync(repo('packages/i18n/locales/de.json'), 'utf8')) as Record<string, string>;
const MESSAGES = { ar, de, en: {} as Record<string, string> };

type App = 'write' | 'grid' | 'deck';
type Lang = keyof typeof MESSAGES;
type Json = Record<string, unknown>;

const DESKTOP = { width: 1440, height: 900 };

/** Opens an app on a new document with these stored options (`null`: nothing stored). */
async function open(page: Page, app: App, options: Json | null, lang: Lang | 'en' = 'en'): Promise<void> {
  await page.setViewportSize(DESKTOP);
  if (options)
    await page.addInitScript((o) => localStorage.setItem('kiln.options', JSON.stringify(o)), options);
  await page.goto(`/#/${app}`);
  await loaded(page, MESSAGES[lang]);
}

async function query<T = Json>(page: Page, q: unknown): Promise<T> {
  return page.evaluate(async (q) => {
    // biome-ignore lint/suspicious/noExplicitAny: the development handle
    const k = (window as any).__kiln;
    return k.client.query(k.registry.context.core.summary.doc, q);
  }, q) as Promise<T>;
}

async function sheet(page: Page): Promise<{ name: string; rtl: boolean }> {
  const st = await query<{ Grid: { State: { sheets: { name: string }[]; active?: number } } }>(page, {
    Grid: 'State',
  });
  const s = st.Grid.State;
  return {
    name: s.sheets[s.active ?? 0]?.name ?? '',
    rtl: /"rtl":true/.test(JSON.stringify(s)),
  };
}

async function writeRtl(page: Page): Promise<boolean | null> {
  const f = await query<{ Write?: { Format?: { rtl?: boolean } } }>(page, { Write: 'Format' });
  return f.Write?.Format?.rtl ?? null;
}

const title = (page: Page) => page.getByTestId('document-title');

/** Waits until Deck's stage has painted. */
async function stage(page: Page): Promise<void> {
  await page.waitForFunction(
    () => ((window as unknown as DeckWindow).__kilnDeckStage?.stats.frames ?? 0) > 0,
  );
}

/** The new presentation's title placeholder: its alignment and where its prompt's ink is. */
async function deckTitle(page: Page): Promise<{ align: string; inkFrom: number; inkTo: number }> {
  type St = { shapes: { id: number; name: string }[]; format: { align: string } | null };
  const shape = (await deckQuery<St>(page, 'State')).shapes.find((s) => /title/i.test(s.name));
  if (!shape) throw new Error('no title placeholder');
  await deck(page, { SelectShapes: { ids: [shape.id] } });
  const align = (await deckQuery<St>(page, 'State')).format?.align ?? '';
  await deck(page, { SelectShapes: { ids: [] } });
  await page.waitForTimeout(300);
  // The Kiln theme's title box, mirrored: 31.75 %–93.75 % across, 26 %–62 % down a 1280 × 720 slide.
  const a = await onSlide(page, 0.3175 * 1280, 0.26 * 720);
  const b = await onSlide(page, 0.9375 * 1280, 0.62 * 720);
  const clip = {
    x: Math.round(a.x),
    y: Math.round(a.y),
    width: Math.round(b.x - a.x),
    height: Math.round(b.y - a.y),
  };
  const img = await decodePng(page, await page.screenshot({ clip }));
  let from = img.width;
  let to = -1;
  for (let y = 0; y < img.height; y++)
    for (let x = 0; x < img.width; x++) {
      const i = (y * img.width + x) * 4;
      // The prompt is cream on the dark title slide (the dashed outline is grey).
      if ((img.data[i] ?? 0) > 220 && (img.data[i + 1] ?? 0) > 210 && (img.data[i + 2] ?? 0) > 190) {
        from = Math.min(from, x);
        to = Math.max(to, x);
      }
    }
  return { align, inkFrom: from / img.width, inkTo: to / img.width };
}

/** The sheet tab strip's tab of a name. */
const sheetTab = (page: Page, name: string) =>
  page.locator('.kiln-sheettabs__list').getByRole('tab', { name, exact: true });

test.describe('rtl editing language follows the display language', () => {
  for (const scheme of ['light', 'dark'] as const) {
    test(`rtl ar: a new workbook is مصنف1 with ورقة1, right to left (${scheme})`, async ({ page }) => {
      await page.emulateMedia({ colorScheme: scheme });
      await open(page, 'grid', { language: { ui: 'ar' } }, 'ar');
      await expect(title(page)).toContainText('مصنف1');
      await expect.poll(() => sheet(page)).toEqual({ name: 'ورقة1', rtl: true });
      await expect(sheetTab(page, 'ورقة1')).toBeVisible();
      await settle(page);
      await page.screenshot({ path: shot(`grid-new-ar-desktop-${scheme}.png`) });
    });

    test(`rtl ar: a new document is مستند1 and a presentation عرض تقديمي1 (${scheme})`, async ({ page }) => {
      await page.emulateMedia({ colorScheme: scheme });
      await open(page, 'write', { language: { ui: 'ar' } }, 'ar');
      await expect(title(page)).toContainText('مستند1');
      await expect.poll(() => writeRtl(page)).toBe(true);
      await settle(page);
      await page.screenshot({ path: shot(`write-new-ar-desktop-${scheme}.png`) });

      await open(page, 'deck', null, 'ar');
      await expect(title(page)).toContainText('عرض تقديمي1');
      await stage(page);
      // Right to left: the title is right-aligned and its Arabic prompt ends at the box's right.
      const t = await deckTitle(page);
      expect(t.align).toBe('right');
      expect(t.inkTo).toBeGreaterThan(0.85);
      expect(t.inkFrom).toBeGreaterThan(0.25);
      // PowerPoint's right-to-left master, and no prompt in the saved slide.
      const pptx = await saved(page);
      const master = zipEntry(pptx, 'ppt/slideMasters/slideMaster1.xml');
      expect(master).toMatch(/<p:titleStyle><a:lvl1pPr[^>]*rtl="1"/);
      expect(master).toMatch(/<p:titleStyle><a:lvl1pPr[^>]*algn="r"/);
      const slide = zipEntry(pptx, 'ppt/slides/slide1.xml');
      expect(slide).not.toContain('انقر');
      await settle(page);
      await page.screenshot({ path: shot(`deck-new-ar-desktop-${scheme}.png`) });
    });

    test(`rtl de: Mappe1 with Tabelle1, Dokument1 and Präsentation1 (${scheme})`, async ({ page }) => {
      await page.emulateMedia({ colorScheme: scheme });
      await open(page, 'grid', { language: { ui: 'de' } }, 'de');
      await expect(title(page)).toContainText('Mappe1');
      await expect.poll(() => sheet(page)).toEqual({ name: 'Tabelle1', rtl: false });
      await expect(sheetTab(page, 'Tabelle1')).toBeVisible();
      await settle(page);
      await page.screenshot({ path: shot(`grid-new-de-desktop-${scheme}.png`) });
      if (scheme === 'dark') return;
      await open(page, 'write', null, 'de');
      await expect(title(page)).toContainText('Dokument1');
      await expect.poll(() => writeRtl(page)).toBe(false);
      await open(page, 'deck', null, 'de');
      await expect(title(page)).toContainText('Präsentation1');
      await stage(page);
      const t = await deckTitle(page);
      expect(t.align).toBe('left');
      await settle(page);
      await page.screenshot({ path: shot('deck-new-de-desktop-light.png') });
    });

    for (const lang of ['ar', 'de'] as const) {
      for (const app of ['grid', 'write', 'deck'] as const) {
        test(`rtl ribbon ${app} ${lang}: the Home tab shows its labels whole (${scheme})`, async ({
          page,
        }) => {
          await page.emulateMedia({ colorScheme: scheme });
          await open(page, app, { language: { ui: lang } }, lang);
          await settle(page);
          const ribbon = page.locator('.kiln-ribbon').first();
          const box = await ribbon.boundingBox();
          if (!box) throw new Error('no ribbon');
          // The title bar and the ribbon, as the window shows them.
          await page.screenshot({
            path: shot(`ribbon-${app}-${lang}-${scheme}.png`),
            clip: { x: 0, y: 0, width: DESKTOP.width, height: Math.ceil(box.y + box.height + 8) },
          });
        });
      }
    }
  }

  test('rtl: an editing language picked in Options wins over the display language', async ({ page }) => {
    await open(page, 'grid', { language: { ui: 'ar', editing: 'en-US' } }, 'ar');
    await expect(title(page)).toContainText('Book1');
    await expect.poll(() => sheet(page)).toEqual({ name: 'Sheet1', rtl: false });
    // Options ▸ Language shows the choice; Match display language goes back to Arabic.
    await page.getByRole('button', { name: ar.File, exact: true }).click();
    await page.getByRole('button', { name: ar.Options, exact: true }).click();
    const options = page.getByRole('dialog');
    await options.getByRole('button', { name: ar.Language, exact: true }).click();
    const select = options.getByTestId('options-editing-language');
    await expect(select).toHaveValue('en-US');
    await select.selectOption('auto');
    await expect(select.locator('option:checked')).toContainText('العربية');
    await settle(page);
    await page.screenshot({ path: shot('options-language-ar-desktop-light.png') });
    expect(
      await page.evaluate(() => JSON.parse(localStorage.getItem('kiln.options') ?? '{}').language.editing),
    ).toBe('auto');
    await page.keyboard.press('Escape');
    await page.locator('[data-kiln-grid-proxy]').focus();
    await page.keyboard.press('Control+n');
    await expect(title(page)).toContainText('مصنف', { timeout: 30_000 });
    await expect.poll(() => sheet(page)).toEqual({ name: 'ورقة1', rtl: true });
  });

  test('rtl: a German interface taken from the system makes German documents', async ({ browser }) => {
    const context = await browser.newContext({ locale: 'de-DE', viewport: DESKTOP });
    const page = await context.newPage();
    await open(page, 'grid', { language: { ui: 'system' } }, 'de');
    await expect(page.locator('html')).toHaveAttribute('lang', 'de');
    await expect(title(page)).toContainText('Mappe1');
    await expect.poll(() => sheet(page)).toEqual({ name: 'Tabelle1', rtl: false });
    await context.close();
  });

  test('rtl: a file that is opened keeps its own names and direction', async ({ page }) => {
    await open(page, 'grid', { language: { ui: 'ar' } }, 'ar');
    const doc = await page.evaluate(
      async ([b, file]) => {
        // biome-ignore lint/suspicious/noExplicitAny: the development handle
        const client = (window as any).__kiln.client;
        return (
          await client.open(Uint8Array.from(b as number[]), { name: file, path: null, read_only: false })
        ).doc as number;
      },
      [Array.from(readFileSync(repo('fixtures/xlsx/features/styles.xlsx'))), 'styles.xlsx'] as const,
    );
    await page.goto(`/#/grid?doc=${doc}`);
    await loaded(page, ar);
    await expect(title(page)).toContainText('styles');
    const s = await sheet(page);
    expect(s.rtl).toBe(false);
    expect(s.name).not.toBe('ورقة1');
  });
});
