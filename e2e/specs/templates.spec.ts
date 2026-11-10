/**
 * Templates: new from a template opens the right app on a new, untitled document with the
 * template's content, from Kiln Home's gallery and from File ▸ New; the template itself is never
 * the document's file. Reviewed screenshots of File ▸ New (light, dark, phone, Arabic) in
 * e2e/screenshots/templates/.
 */
import { readFileSync } from 'node:fs';
import { fileURLToPath } from 'node:url';
import { expect, type Page, test } from '@playwright/test';
import { expectNoSeriousA11yViolations } from '../../packages/testing/src/playwright/a11y.ts';

const repo = (p: string) => fileURLToPath(new URL(`../../${p}`, import.meta.url));
const shot = (name: string) => repo(`e2e/screenshots/templates/${name}`);
const CORE_START = 45_000;

type Kind = 'Write' | 'Grid' | 'Deck';
interface IndexEntry {
  id: string;
  kind: Kind;
  name: string;
  file: string;
  featured?: boolean;
}
const TEMPLATES: IndexEntry[] = JSON.parse(
  readFileSync(repo('assets/templates/index.json'), 'utf8'),
).templates;
const AR: Record<string, string> = JSON.parse(readFileSync(repo('packages/i18n/locales/ar.json'), 'utf8'));
const UNTITLED: Record<Kind, string> = { Write: 'Document', Grid: 'Book', Deck: 'Presentation' };

interface DocInfo {
  title: string;
  path: string | null;
  format: string | null;
  dirty: boolean;
  /** Every text the document shows (paragraphs, cells, slides), joined. */
  text: string;
}

/** The open document as the core reports it (through the development handle `window.__kiln`). */
async function docInfo(page: Page): Promise<DocInfo> {
  const doc = Number(new URL(page.url()).hash.match(/doc=(\d+)/)?.[1]);
  return page.evaluate(async (d) => {
    // biome-ignore lint/suspicious/noExplicitAny: the development handle
    const client = (window as any).__kiln.client;
    const summary = (await client.query(d, 'Summary')).Summary;
    const texts: string[] = [];
    const walk = (v: unknown) => {
      if (typeof v === 'string') texts.push(v);
      else if (Array.isArray(v)) v.forEach(walk);
      else if (v && typeof v === 'object') Object.values(v).forEach(walk);
    };
    walk(await client.query(d, 'A11y').catch(() => null));
    if (summary.kind === 'Write') walk(await client.query(d, 'Paragraphs'));
    return {
      title: summary.title,
      path: summary.path,
      format: summary.format,
      dirty: summary.dirty,
      text: texts.join(' '),
    };
  }, doc);
}

/** The first words of a template's content that every template of the kind shows. */
async function expectContent(page: Page, kind: Kind) {
  await expect(page).toHaveURL(new RegExp(`#/${kind.toLowerCase()}\\?doc=\\d+$`), { timeout: CORE_START });
  await expect(page.getByTestId('document-title')).toContainText(`${UNTITLED[kind]}1`, {
    timeout: CORE_START,
  });
  const info = await docInfo(page);
  expect(info.title).toBe(`${UNTITLED[kind]}1`);
  // A new document: the template is not its file.
  expect(info.path).toBeNull();
  expect(info.format).toBeNull();
  expect(info.dirty).toBe(false);
  return info;
}

const first = (kind: Kind) => {
  const t = TEMPLATES.find((e) => e.kind === kind && e.featured) ?? TEMPLATES.find((e) => e.kind === kind);
  if (!t) throw new Error(`no ${kind} template`);
  return t;
};

/** Kiln Home ▸ Templates ▸ the first featured template of `kind`. */
async function fromHome(page: Page, kind: Kind) {
  const tpl = first(kind);
  await page.goto('/#/home');
  await page.getByRole('button', { name: /^Browse templates:/ }).click();
  await page.locator(`[data-template="${tpl.id}"]`).click();
  const info = await expectContent(page, kind);
  expect(info.text.length, `${tpl.id} has content`).toBeGreaterThan(200);
  await expect(page.locator('.kiln-appframe')).toHaveAttribute('data-app', kind.toLowerCase());
  await page.evaluate(() => document.fonts.ready);
  await page.waitForTimeout(800);
  await page.screenshot({ path: shot(`opened-${tpl.id}.png`) });
}

test.describe('templates', () => {
  test('the build has twelve templates per app, each with a thumbnail', () => {
    for (const kind of ['Write', 'Grid', 'Deck'] as const) {
      const list = TEMPLATES.filter((t) => t.kind === kind);
      expect(list, kind).toHaveLength(12);
      for (const t of list) {
        expect(readFileSync(repo(`assets/templates/${t.file}`)).length).toBeGreaterThan(500);
        expect(
          readFileSync(repo(`assets/templates/${t.file.replace(/\.k\w+$/, '.png')}`)).length,
        ).toBeGreaterThan(2000);
      }
    }
  });

  test('new from a Write template on Home opens Kiln Write with its content', async ({ page }) => {
    await fromHome(page, 'Write');
  });

  test('new from a Grid template on Home opens Kiln Grid with its content', async ({ page }) => {
    await fromHome(page, 'Grid');
  });

  test('new from a Deck template on Home opens Kiln Deck with its content', async ({ page }) => {
    await fromHome(page, 'Deck');
  });

  test('File ▸ New: search and filter the gallery, then open a template of another app', async ({ page }) => {
    const deck = first('Deck');
    await page.goto('/#/write');
    await expect(page.getByTestId('document-title')).toContainText('Document', { timeout: CORE_START });
    await page.getByRole('button', { name: 'File' }).click();
    const dialog = page.getByRole('dialog', { name: /File menu/ });
    // File ▸ Home: a blank document and featured templates above the recent files.
    await expect(
      dialog.getByRole('region', { name: 'New' }).locator('[data-template]').first(),
    ).toBeVisible();
    await dialog.getByRole('button', { name: 'New', exact: true }).click();
    const gallery = dialog.locator('.kiln-tgallery');
    await expect(gallery.locator('[data-template]')).toHaveCount(TEMPLATES.length);
    // This window's app comes first.
    await expect(gallery.locator('[data-blank]').first()).toHaveAttribute('data-blank', 'write');
    await gallery.getByRole('button', { name: /Presentations/ }).click();
    await expect(gallery.locator('[data-template]')).toHaveCount(
      TEMPLATES.filter((t) => t.kind === 'Deck').length,
    );
    await gallery.getByRole('searchbox', { name: 'Search templates' }).fill(deck.name);
    await expect(gallery.locator(`[data-template="${deck.id}"]`)).toBeVisible();
    await page.screenshot({ path: shot('backstage-new-search.png') });
    await gallery.locator(`[data-template="${deck.id}"]`).click();
    const info = await expectContent(page, 'Deck');
    expect(info.text.length).toBeGreaterThan(100);
  });

  for (const scheme of ['light', 'dark'] as const) {
    test(`File ▸ New gallery (${scheme}, desktop and phone) passes axe`, async ({ page }) => {
      await page.emulateMedia({ colorScheme: scheme });
      await page.goto('/#/grid');
      await expect(page.getByTestId('document-title')).toContainText('Book', { timeout: CORE_START });
      await page.getByRole('button', { name: 'File' }).click();
      const dialog = page.getByRole('dialog', { name: /File menu/ });
      await page.evaluate(() =>
        Promise.all([...document.images].map((i) => i.decode().catch(() => undefined))),
      );
      await page.screenshot({ path: shot(`backstage-home-${scheme}-desktop.png`) });
      await dialog.getByRole('button', { name: 'New', exact: true }).click();
      await expect(dialog.locator('[data-blank]').first()).toHaveAttribute('data-blank', 'grid');
      await page.evaluate(() =>
        Promise.all([...document.images].map((i) => i.decode().catch(() => undefined))),
      );
      await page.screenshot({ path: shot(`backstage-new-${scheme}-desktop.png`) });
      await expectNoSeriousA11yViolations(page);
      await page.setViewportSize({ width: 390, height: 844 });
      await page.waitForTimeout(300);
      expect(await page.evaluate(() => document.documentElement.scrollWidth > window.innerWidth)).toBe(false);
      await page.screenshot({ path: shot(`backstage-new-${scheme}-phone.png`) });
    });
  }

  test('File ▸ New in Arabic mirrors and translates the template names', async ({ page }) => {
    await page.addInitScript(() =>
      localStorage.setItem('kiln.options', JSON.stringify({ language: { ui: 'ar' } })),
    );
    const tpl = first('Write');
    await page.goto('/#/write');
    await expect(page.locator('html')).toHaveAttribute('dir', 'rtl');
    await expect(page.getByTestId('document-title')).not.toHaveText(/…/, { timeout: CORE_START });
    await page.getByRole('button', { name: AR.File ?? 'File' }).click();
    const dialog = page.getByRole('dialog');
    await dialog.getByRole('button', { name: AR.New ?? 'New', exact: true }).click();
    const card = dialog.locator(`[data-template="${tpl.id}"]`);
    await expect(card).toContainText(AR[tpl.name] ?? '');
    await page.evaluate(() =>
      Promise.all([...document.images].map((i) => i.decode().catch(() => undefined))),
    );
    await page.screenshot({ path: shot('backstage-new-ar-desktop.png') });
    await expectNoSeriousA11yViolations(page);
  });
});
