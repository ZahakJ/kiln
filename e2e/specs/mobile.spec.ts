/**
 * The phone and tablet frame, run by the `pixel-7` (Chromium, 412 × 839,
 * touch) and `ipad` (WebKit, 810 × 1080, touch) projects: the top app bar, the bottom toolbar and
 * the ribbon sheet from the ribbon's own registry, typing, keyboard docking (the on-screen
 * keyboard emulated by shrinking the visual viewport, and by resizing the window as the Android
 * app does), Back (the browser's and the app bar's: a sheet first, then the document, then Kiln
 * Home), Share in the overflow menu, Write's reflow view, the file name in the title bar, the
 * 44 × 44 px hit-target audit, axe, and the reviewed screenshots in e2e/screenshots/mobile/.
 */
import { readFileSync } from 'node:fs';
import { fileURLToPath } from 'node:url';
import { expect, type Page, test } from '@playwright/test';
import { expectNoSeriousA11yViolations } from '../../packages/testing/src/playwright/a11y.ts';
import { cellPoint, gridState, value } from './grid-helpers.ts';
import { doubleTap, drag, longPress, menusOffScreen, pinch } from './touch.ts';

const shot = (name: string) => fileURLToPath(new URL(`../screenshots/mobile/${name}`, import.meta.url));
const fixture = (path: string) =>
  Array.from(readFileSync(fileURLToPath(new URL(`../../${path}`, import.meta.url))));

type App = 'write' | 'grid' | 'deck';
const APPS: readonly App[] = ['write', 'grid', 'deck'];
/** A template with real content for each app's screenshots. */
const TEMPLATE: Record<App, string> = {
  write: 'write/letter.kwrite',
  grid: 'grid/budget.kgrid',
  deck: 'deck/pitch.kdeck',
};

interface KilnClientHandle {
  query(doc: number, q: unknown): Promise<unknown>;
  execute(doc: number | null, c: unknown): Promise<unknown>;
  open(bytes: Uint8Array, hint: object): Promise<{ doc: number }>;
}
type Win = {
  __kiln?: {
    client?: KilnClientHandle | null;
    registry: { getState(id: string): { value: unknown; checked?: boolean; enabled: boolean } };
  };
  __kilnWrite?: { doc: number };
};

const isPhone = (page: Page) => (page.viewportSize()?.width ?? 1440) < 640;
const formFactor = (page: Page) => (isPhone(page) ? 'phone' : 'tablet');

/** Opens an app's frame (a template's content, or a blank document) and waits for the core. */
async function openApp(page: Page, app: App, opts: { template?: boolean } = {}): Promise<void> {
  const query = opts.template ? `?template=${TEMPLATE[app]}&new=${Date.now()}` : `?new=${Date.now()}`;
  await page.goto(`/#/${app}${query}`);
  await expect(page.getByTestId('document-title')).not.toHaveText('Opening…', { timeout: 45_000 });
  await page.evaluate(() => document.fonts.ready);
}

/** Opens a .docx fixture in the web core and shows it in Kiln Write (name and extension known). */
async function openDocx(page: Page, path: string, name: string): Promise<void> {
  await openApp(page, 'write');
  await page.waitForFunction(() => !!(window as unknown as Win).__kiln?.client);
  const doc = await page.evaluate(
    async ([bytes, file]) => {
      const client = (window as unknown as Win).__kiln?.client;
      if (!client) throw new Error('window.__kiln.client is missing (development build only)');
      const s = await client.open(Uint8Array.from(bytes as number[]), {
        name: file as string,
        path: null,
        format: 'Docx',
        read_only: false,
      });
      return s.doc;
    },
    [fixture(path), name] as const,
  );
  await page.goto(`/#/write?doc=${doc}`);
  await expect(page.getByTestId('document-title')).toContainText(name.replace(/\.docx$/, ''), {
    timeout: 30_000,
  });
}

/** The Write document's paragraphs, from the core. */
async function writeParagraphs(page: Page): Promise<string[]> {
  return page.evaluate(async () => {
    const w = window as unknown as Win;
    const client = w.__kiln?.client;
    const doc = w.__kilnWrite?.doc;
    if (!client || doc === undefined) return [];
    const r = (await client.query(doc, 'Paragraphs')) as { Paragraphs?: { text: string }[] };
    return r.Paragraphs?.map((p) => p.text) ?? [];
  });
}

type DeckWin = {
  __kilnDeck?: {
    state: {
      deck: {
        current: number;
        slides: unknown[];
        selection: number[];
        editing: boolean;
        notes_visible: boolean;
        selection_box: { rotation: number } | null;
      } | null;
    };
  };
  __kilnDeckStage?: {
    view(): { zoom: number; left: number; top: number; width: number; height: number };
  } | null;
};
const deckState = (page: Page) =>
  page.evaluate(() => (window as unknown as DeckWin).__kilnDeck?.state.deck ?? null);
const deckView = (page: Page) =>
  page.evaluate(() => (window as unknown as DeckWin).__kilnDeckStage?.view() ?? null);

/** The selected text of the Write document. */
async function writeSelected(page: Page): Promise<string> {
  return page.evaluate(async () => {
    const w = window as unknown as Win;
    const client = w.__kiln?.client;
    const doc = w.__kilnWrite?.doc;
    if (!client || doc === undefined) return '';
    const r = (await client.query(doc, 'SelectedText')) as { Text?: string };
    return r.Text ?? '';
  });
}

/** The ribbon sheet's chips cut at the strip's start edge (there should be none). */
async function chipsCutAtStart(page: Page): Promise<string[]> {
  return page.evaluate(() => {
    const strip = document.querySelector('.kiln-ribbon-sheet__chips');
    if (!strip) return ['no strip'];
    const s = strip.getBoundingClientRect();
    const rtl = getComputedStyle(strip).direction === 'rtl';
    return [...strip.querySelectorAll<HTMLElement>('[role="tab"]')]
      .filter((c) => {
        const r = c.getBoundingClientRect();
        return rtl
          ? r.right > s.right + 0.5 && r.left < s.right - 0.5
          : r.left < s.left - 0.5 && r.right > s.left + 0.5;
      })
      .map((c) => c.textContent ?? '');
  });
}

async function waitPainted(page: Page): Promise<void> {
  await page.waitForTimeout(900);
}

/** The pixels a screenshot is taken at: CSS pixels, so phone and tablet files stay small. */
async function snap(page: Page, name: string): Promise<void> {
  await page.screenshot({ path: shot(name), scale: 'css', animations: 'disabled', caret: 'hide' });
}

/**
 * Every control a finger can use in `scope` that is under 44 × 44 CSS px: buttons, links, fields,
 * tabs, menu items, options and anything focusable, visible and not inert. The document canvas
 * itself (Write's page, Grid's sheet, Deck's slide) is not a control: a finger selects there, and
 * its handles are 44 px targets of their own.
 */
async function smallTargets(page: Page, scopes: readonly string[]): Promise<string[]> {
  return page.evaluate((selectors) => {
    const INTERACTIVE =
      'button, a[href], input:not([type="hidden"]), select, textarea, [role="button"], [role="tab"], [role="menuitem"], [role="menuitemcheckbox"], [role="menuitemradio"], [role="option"], [role="switch"], [role="checkbox"], [role="radio"], [role="slider"], [role="combobox"], [tabindex]:not([tabindex="-1"])';
    /** The document canvas itself: the page, the sheet and the slide (see the doc comment). */
    const EXCLUDED = '.kiln-write__surface, [data-kiln-grid-surface], [data-kiln-deck-stage]';
    const out: string[] = [];
    const seen = new Set<Element>();
    for (const sel of selectors)
      for (const scope of document.querySelectorAll(sel))
        for (const el of scope.querySelectorAll<HTMLElement>(INTERACTIVE)) {
          if (seen.has(el)) continue;
          seen.add(el);
          if (el.closest(EXCLUDED)) continue;
          if (el.closest('[inert], [aria-hidden="true"]') && !el.closest('.kiln-number__spin')) continue;
          const style = getComputedStyle(el);
          if (style.visibility === 'hidden' || style.display === 'none' || style.pointerEvents === 'none')
            continue;
          const r = el.getBoundingClientRect();
          if (r.width === 0 || r.height === 0) continue;
          // Visually hidden (the clip-path 1 px pattern) and scrolled-off parts are not targets.
          if (r.width <= 2 && r.height <= 2) continue;
          if (r.bottom <= 0 || r.top >= window.innerHeight) continue;
          if (r.width < 43.5 || r.height < 43.5) {
            const name =
              el.getAttribute('aria-label') ||
              el.getAttribute('data-command') ||
              el.textContent?.trim().slice(0, 30) ||
              el.className;
            out.push(`${name} (${Math.round(r.width)} × ${Math.round(r.height)}) in ${sel}`);
          }
        }
    return out;
  }, scopes);
}

test.describe('mobile', () => {
  test('mobile: the frame of each app on a phone and a tablet, light and dark, passes axe', async ({
    page,
  }) => {
    for (const app of APPS) {
      for (const scheme of isPhone(page) ? (['light', 'dark'] as const) : (['light'] as const)) {
        await page.emulateMedia({ colorScheme: scheme });
        await openApp(page, app, { template: true });
        await expect(page.locator('.kiln-appframe')).toHaveAttribute('data-form-factor', formFactor(page));
        if (isPhone(page)) {
          await expect(page.getByTestId('app-bar')).toBeVisible();
          await expect(page.getByTestId('bottom-toolbar')).toBeVisible();
          await expect(page.locator('.kiln-ribbon')).toHaveCount(0);
          await expect(page.locator('.kiln-appframe > .kiln-statusbar')).toBeHidden();
        } else {
          // A tablet keeps the desktop chrome with the Simplified ribbon.
          await expect(page.locator('.kiln-ribbon')).toHaveAttribute('data-layout', 'simplified');
          await expect(page.getByTestId('bottom-toolbar')).toHaveCount(0);
        }
        expect(await page.evaluate(() => document.documentElement.scrollWidth > window.innerWidth)).toBe(
          false,
        );
        await waitPainted(page);
        await snap(page, `${app}-${scheme}-${formFactor(page)}.png`);
        await expectNoSeriousA11yViolations(page);
      }
    }
  });

  test('mobile: Kiln Home on a phone and a tablet (FAB New and the tab bar on a phone)', async ({ page }) => {
    for (const scheme of isPhone(page) ? (['light', 'dark'] as const) : (['light'] as const)) {
      await page.emulateMedia({ colorScheme: scheme });
      await page.goto('/#/home');
      await expect(page.getByRole('heading', { level: 1 })).toBeVisible({ timeout: 30_000 });
      await page.evaluate(() => document.fonts.ready);
      if (isPhone(page)) {
        await expect(page.getByRole('navigation', { name: 'Kiln Home sections' })).toBeVisible();
        const fab = page.locator('.kiln-home__new-btn');
        await expect(fab).toBeVisible();
        // The floating button sits above the tab bar, at the end edge.
        const box = await fab.boundingBox();
        const bar = await page.getByRole('navigation', { name: 'Kiln Home sections' }).boundingBox();
        expect(box && bar && box.y + box.height <= bar.y).toBe(true);
      }
      if (isPhone(page)) {
        // In every section, scrolled to its end, nothing a finger uses is under the New button's
        // band or the tab bar.
        for (const section of ['Recent', 'Pinned', 'Shared with me']) {
          const tab = page.getByRole('tab', { name: section, exact: true });
          if (await tab.count()) await tab.tap();
          await page.evaluate(() => window.scrollTo(0, document.documentElement.scrollHeight));
          const under = await page.evaluate(() => {
            const band = document.querySelector('.kiln-home__rail')?.getBoundingClientRect();
            const bar = document.querySelector('.kiln-home__tabbar')?.getBoundingClientRect();
            const top = Math.min(band?.top ?? Number.POSITIVE_INFINITY, bar?.top ?? Number.POSITIVE_INFINITY);
            return [
              ...document.querySelectorAll<HTMLElement>(
                '.kiln-home__main button, .kiln-home__main a[href], .kiln-home__main [role="tab"]',
              ),
            ]
              .filter((el) => !el.closest('.kiln-home__rail, .kiln-home__tabbar'))
              .filter((el) => {
                const r = el.getBoundingClientRect();
                return r.width > 2 && r.height > 2 && r.bottom > top + 0.5 && r.top < window.innerHeight;
              })
              .map(
                (el) => el.getAttribute('aria-label') || el.textContent?.trim().slice(0, 30) || el.className,
              );
          });
          expect(under, section).toEqual([]);
        }
        await page.getByRole('tab', { name: 'Recent', exact: true }).tap();
        await page.evaluate(() => window.scrollTo(0, 0));
      }
      // Door names never clip on a tablet.
      for (const noun of await page.locator('.kiln-door__noun').all())
        expect(await noun.evaluate((el) => el.scrollWidth <= el.clientWidth + 1)).toBe(true);
      expect(await page.evaluate(() => document.documentElement.scrollWidth > window.innerWidth)).toBe(false);
      await waitPainted(page);
      await snap(page, `home-${scheme}-${formFactor(page)}.png`);
      await expectNoSeriousA11yViolations(page);
    }
  });

  test('mobile: typing in Write and Grid on a phone', async ({ page }) => {
    test.skip(!isPhone(page), 'the phone layout');
    await openApp(page, 'write');
    // Write opens reflowed to the screen on a phone (Web Layout).
    await expect
      .poll(() =>
        page.evaluate(() => (window as unknown as Win).__kiln?.registry.getState('suite.view.mode').value),
      )
      .toBe('WebLayout');
    await page.locator('.kiln-write__surface').tap({ position: { x: 60, y: 40 } });
    await page.keyboard.type('Typed on a phone');
    await expect.poll(() => writeParagraphs(page)).toContain('Typed on a phone');
    // Bold from the bottom toolbar applies to what is typed next.
    const bold = page.getByTestId('bottom-toolbar').locator('[data-command="suite.format.bold"]');
    await bold.tap();
    await expect(bold).toHaveAttribute('aria-pressed', 'true');
    // Undo in the app bar.
    await expect(page.getByTestId('app-bar').getByRole('button', { name: 'Undo' })).not.toHaveAttribute(
      'aria-disabled',
      'true',
    );

    await openApp(page, 'grid');
    await page.waitForFunction(
      () =>
        !!(window as unknown as { __kilnGrid?: unknown }).__kilnGrid &&
        Number(document.querySelector<HTMLElement>('[data-kiln-grid-surface]')?.dataset.kilnFrames ?? 0) > 0,
    );
    // Tap B2, type, Enter.
    const b2 = await cellPoint(page, 1, 1);
    await page.touchscreen.tap(b2.x, b2.y);
    await expect
      .poll(async () => ((await gridState(page)) as unknown as { name_box: string }).name_box)
      .toBe('B2');
    await page.keyboard.type('412');
    await page.keyboard.press('Enter');
    await expect.poll(() => value(page, 1, 1)).toBe('412');
    // The bottom toolbar shows the cell's quick commands.
    await expect(page.getByTestId('bottom-toolbar')).toHaveAttribute('data-context', 'cell');
  });

  test('mobile: the ribbon sheet lists the desktop tabs as chips and groups as tiles, from the same registry', async ({
    page,
  }) => {
    test.skip(!isPhone(page), 'the phone layout');
    await openApp(page, 'write');
    const opener = page.getByTestId('ribbon-sheet-button');
    await expect(opener).toHaveText(/Home/);
    await opener.tap();
    const sheet = page.getByTestId('ribbon-sheet');
    await expect(sheet).toBeVisible();
    await expect(sheet).toHaveAttribute('role', 'dialog');
    const chips = sheet.getByRole('tablist', { name: 'Ribbon tabs' }).getByRole('tab');
    await expect(chips.first()).toHaveText('Home');
    await expect(chips.first()).toHaveAttribute('aria-selected', 'true');
    // The desktop group names, as tiles.
    for (const group of ['Clipboard', 'Font', 'Paragraph', 'Styles'])
      await expect(sheet.getByRole('region', { name: group })).toBeVisible();
    // Bold from the sheet runs the registry's command and shows its state.
    const bold = sheet.locator('[data-command="suite.format.bold"]');
    await bold.tap();
    await expect(bold).toHaveAttribute('aria-pressed', 'true');
    // Insert: its own groups; the opener then names it.
    await sheet.getByRole('tab', { name: 'Insert' }).tap();
    await expect(sheet.getByRole('tab', { name: 'Insert' })).toHaveAttribute('aria-selected', 'true');
    await expect(sheet.getByRole('region', { name: 'Tables' })).toBeVisible();
    // The chips follow the arrow keys (a hardware keyboard).
    await sheet.getByRole('tab', { name: 'Insert' }).focus();
    await page.keyboard.press('ArrowRight');
    await expect(sheet.getByRole('tab', { name: 'Layout' })).toBeFocused();
    await expect(sheet.getByRole('tab', { name: 'Layout' })).toHaveAttribute('aria-selected', 'true');
    // No chip is cut at the strip's start; the strip fades where it scrolls on.
    await expect.poll(() => chipsCutAtStart(page)).toEqual([]);
    await snap(page, 'write-ribbon-sheet-light-phone.png');
    await expectNoSeriousA11yViolations(page);
    await page.keyboard.press('Escape');
    await expect(sheet).toBeHidden();
    await expect(opener).toHaveText(/Layout/);
  });

  test('mobile: the bottom toolbar docks on the on-screen keyboard', async ({ page }) => {
    // The keyboard shrinks the visual viewport (a browser), emulated here: `visualViewport` is
    // replaced by one whose height the test sets.
    await page.addInitScript(() => {
      const target = new EventTarget();
      const vv = {
        height: window.innerHeight,
        width: window.innerWidth,
        offsetTop: 0,
        offsetLeft: 0,
        pageTop: 0,
        pageLeft: 0,
        scale: 1,
        onresize: null,
        onscroll: null,
        addEventListener: target.addEventListener.bind(target),
        removeEventListener: target.removeEventListener.bind(target),
        dispatchEvent: target.dispatchEvent.bind(target),
      };
      Object.defineProperty(window, 'visualViewport', { configurable: true, get: () => vv });
      (window as unknown as { __keyboard(h: number): void }).__keyboard = (h: number) => {
        vv.height = window.innerHeight - h;
        target.dispatchEvent(new Event('resize'));
      };
    });
    await openApp(page, 'write');
    const inner = await page.evaluate(() => window.innerHeight);
    const keyboard = 300;
    await page.evaluate(
      (h) => (window as unknown as { __keyboard(h: number): void }).__keyboard(h),
      keyboard,
    );
    await expect(page.locator('.kiln-appframe')).toHaveAttribute('data-keyboard', 'open');
    if (isPhone(page)) {
      // The toolbar's bottom edge is the keyboard's top edge.
      await expect
        .poll(async () => {
          const b = await page.getByTestId('bottom-toolbar').boundingBox();
          return b ? Math.round(b.y + b.height) : -1;
        })
        .toBe(inner - keyboard);
      await snap(page, 'write-keyboard-light-phone.png');
    } else {
      // A tablet: the status bar stays above the keyboard.
      await expect
        .poll(async () => {
          const b = await page.locator('.kiln-statusbar').first().boundingBox();
          return b ? Math.round(b.y + b.height) : -1;
        })
        .toBe(inner - keyboard);
    }
    await page.evaluate(() => (window as unknown as { __keyboard(h: number): void }).__keyboard(0));
    await expect(page.locator('.kiln-appframe')).not.toHaveAttribute('data-keyboard', 'open');

    // The Android app resizes the whole web view (adjustResize): the toolbar follows the bottom.
    if (isPhone(page)) {
      const size = page.viewportSize() as { width: number; height: number };
      await page.setViewportSize({ width: size.width, height: size.height - keyboard });
      await expect
        .poll(async () => {
          const b = await page.getByTestId('bottom-toolbar').boundingBox();
          return b ? Math.round(b.y + b.height) : -1;
        })
        .toBe(size.height - keyboard);
      await page.setViewportSize(size);
    }
  });

  test('mobile: Back on a tablet closes the File menu, then the document, then shows Kiln Home', async ({
    page,
  }) => {
    test.skip(isPhone(page), 'the tablet layout');
    await page.goto('/#/home');
    await expect(page.getByRole('heading', { level: 1 })).toBeVisible({ timeout: 30_000 });
    await openApp(page, 'grid');
    await page.getByRole('button', { name: 'File', exact: true }).click();
    const backstage = page.getByRole('dialog', { name: /File menu/ });
    await expect(backstage).toBeVisible();
    await page.evaluate(() => history.back());
    await expect(backstage).toBeHidden();
    await expect(page.locator('.kiln-ribbon')).toBeVisible();
    await page.evaluate(() => history.back());
    await expect(page.getByRole('heading', { level: 1 })).toBeVisible({ timeout: 15_000 });
  });

  test('mobile: Back closes the sheet, then the document (asking about changes), then shows Kiln Home', async ({
    page,
  }) => {
    test.skip(!isPhone(page), 'the phone layout');
    await page.goto('/#/home');
    await expect(page.getByRole('heading', { level: 1 })).toBeVisible({ timeout: 30_000 });
    await openApp(page, 'write');
    // The browser's Back: the ribbon sheet first.
    await page.getByTestId('ribbon-sheet-button').tap();
    await expect(page.getByTestId('ribbon-sheet')).toBeVisible();
    await page.evaluate(() => history.back());
    await expect(page.getByTestId('ribbon-sheet')).toBeHidden();
    await expect(page.getByTestId('app-bar')).toBeVisible();
    // Unsaved changes: Back asks, and Back again cancels the question and keeps the document.
    await page.locator('.kiln-write__surface').tap({ position: { x: 60, y: 40 } });
    await page.keyboard.type('Keep me');
    await expect.poll(() => writeParagraphs(page)).toContain('Keep me');
    await page.evaluate(() => history.back());
    const ask = page.getByTestId('save-changes');
    await expect(ask).toBeVisible();
    await page.evaluate(() => history.back());
    await expect(ask).toBeHidden();
    await expect(page.getByTestId('app-bar')).toBeVisible();
    expect(await writeParagraphs(page)).toContain('Keep me');
    // The app bar's Back: the question again; Don't Save closes, and Kiln Home shows.
    await page.getByTestId('app-bar-back').tap();
    await expect(ask).toBeVisible();
    await page.getByTestId('save-changes-discard').tap();
    await expect(page.getByRole('heading', { level: 1 })).toBeVisible({ timeout: 15_000 });
    await expect(page).toHaveURL(/#\/home|#\/?$|\/$/);

    // A document with nothing to save closes at once on the browser's Back.
    await openApp(page, 'deck');
    await page.evaluate(() => history.back());
    await expect(page.getByRole('heading', { level: 1 })).toBeVisible({ timeout: 15_000 });
  });

  test('mobile: Share in the overflow menu sends a PDF, and explains why an unsaved file cannot go', async ({
    page,
  }) => {
    test.skip(!isPhone(page), 'the phone layout');
    await openDocx(page, 'fixtures/docx/features/paragraphs.docx', 'paragraphs.docx');
    await page.getByTestId('app-bar-overflow').tap();
    const menu = page.getByRole('menu');
    for (const item of ['Share', 'Save', 'Save As', 'Print', 'Find and Replace', 'File', 'Settings'])
      await expect(menu.getByRole('menuitem', { name: item, exact: true })).toBeVisible();
    await expect(menu.getByRole('menuitemcheckbox', { name: 'Print layout' })).toBeVisible();
    await snap(page, 'write-overflow-light-phone.png');
    await menu.getByRole('menuitem', { name: 'Share', exact: true }).tap();
    const sheet = page.getByTestId('share-sheet');
    await expect(sheet).toBeVisible();
    await expect(sheet.getByRole('heading', { name: 'Share' })).toBeVisible();
    await snap(page, 'write-share-light-phone.png');
    await expectNoSeriousA11yViolations(page);
    // The browser here cannot share files: the copy is downloaded.
    const [download] = await Promise.all([
      page.waitForEvent('download'),
      sheet.getByTestId('share-pdf').tap(),
    ]);
    expect(download.suggestedFilename()).toBe('paragraphs.pdf');
    await expect(sheet).toBeHidden();
    // The file as last saved: never saved, so the sheet says what to do, in a designed toast.
    await page.getByTestId('app-bar-overflow').tap();
    await page.getByRole('menuitem', { name: 'Share', exact: true }).tap();
    await page.getByTestId('share-file').tap();
    const toast = page.locator('.kiln-toast').filter({ hasText: 'Save the document first, then share it.' });
    await expect(toast).toBeVisible();
    await expect(page.getByTestId('share-sheet')).toBeVisible();
  });

  test('mobile: Write reflows on a phone, and Print layout shows the pages', async ({ page }) => {
    test.skip(!isPhone(page), 'the phone layout');
    await openApp(page, 'write', { template: true });
    const mode = () =>
      page.evaluate(() => (window as unknown as Win).__kiln?.registry.getState('suite.view.mode').value);
    await expect.poll(mode).toBe('WebLayout');
    await page.getByTestId('app-bar-overflow').tap();
    const toggle = page.getByRole('menuitemcheckbox', { name: 'Print layout' });
    await expect(toggle).toHaveAttribute('aria-checked', 'false');
    await toggle.tap();
    await expect.poll(mode).toBe('PrintLayout');
    await waitPainted(page);
    await snap(page, 'write-print-layout-light-phone.png');
    await page.getByTestId('app-bar-overflow').tap();
    await expect(page.getByRole('menuitemcheckbox', { name: 'Print layout' })).toHaveAttribute(
      'aria-checked',
      'true',
    );
    await page.getByRole('menuitemcheckbox', { name: 'Print layout' }).tap();
    await expect.poll(mode).toBe('WebLayout');
  });

  for (const language of ['en', 'ar'] as const)
    test(`mobile: the title bar shows the name and extension as one file name (${language})`, async ({
      page,
    }) => {
      if (language === 'ar')
        await page.addInitScript(() => {
          localStorage.setItem('kiln.options', JSON.stringify({ language: { ui: 'ar' } }));
        });
      await openDocx(page, 'fixtures/docx/features/paragraphs.docx', 'paragraphs.docx');
      await expect(page.locator('html')).toHaveAttribute('dir', language === 'ar' ? 'rtl' : 'ltr');
      // "paragraphs.docx" with no gap, left to right in either interface direction.
      await expect(page.getByTestId('document-title')).toHaveText('paragraphs.docx');
      const gap = await page.getByTestId('document-title').evaluate((el) => {
        const text = el.querySelector('.kiln-titlebar__title-text')?.getBoundingClientRect();
        const ext = el.querySelector('.kiln-titlebar__ext')?.getBoundingClientRect();
        return text && ext ? Math.round(ext.left - text.right) : null;
      });
      expect(gap).toBe(0);
      if (language === 'en') return;
      await page.evaluate(() => document.fonts.ready);
      await waitPainted(page);
      await snap(page, `write-light-${formFactor(page)}-ar.png`);
      if (isPhone(page)) {
        // The bottom toolbar mirrors: the ribbon sheet's button at the right edge.
        const box = await page.getByTestId('ribbon-sheet-button').boundingBox();
        const width = page.viewportSize()?.width ?? 0;
        expect(box && box.x + box.width > width - 60).toBe(true);
        await page.getByTestId('ribbon-sheet-button').tap();
        await waitPainted(page);
        await page.getByTestId('ribbon-sheet').getByRole('tab').nth(3).tap();
        await expect.poll(() => chipsCutAtStart(page)).toEqual([]);
        await snap(page, 'write-ribbon-sheet-light-phone-ar.png');
        await expectNoSeriousA11yViolations(page);
      }
    });

  test('mobile: Write under a finger: long press, the teardrop handles, the context toolbar, pinch and double tap', async ({
    page,
  }) => {
    await openApp(page, 'write', { template: true });
    await waitPainted(page);
    const surface = page.locator('.kiln-write__surface');
    const box = await surface.boundingBox();
    if (!box) throw new Error('no surface');
    // The page zooms itself; the chrome keeps the browser's zoom.
    for (const sel of ['.kiln-appbar', '.kiln-bottombar', '.kiln-appframe'])
      if (await page.locator(sel).count())
        expect(
          await page
            .locator(sel)
            .first()
            .evaluate((el) => getComputedStyle(el).touchAction),
          sel,
        ).toBe('auto');
    expect(
      await page
        .locator('.kiln-write__surface .kiln-surface-scroller')
        .evaluate((el) => getComputedStyle(el).touchAction),
    ).toBe('pan-x pan-y');
    // A long press on a word: the word is selected and the context menu opens there.
    const at = { x: box.x + 90, y: box.y + 330 };
    await longPress(page, at);
    const menu = page.getByRole('menu');
    await expect(menu).toBeVisible();
    await expect(menu.getByRole('menuitem', { name: 'Copy', exact: true })).toBeVisible();
    await expect.poll(() => writeSelected(page)).not.toBe('');
    const word = await writeSelected(page);
    await snap(page, `write-longpress-light-${formFactor(page)}.png`);
    await page.keyboard.press('Escape');
    await expect(menu).toBeHidden();
    // The teardrop handles: dragging the end one down a line selects more.
    const handles = page.locator('[data-kiln-handle][data-side="End"]');
    await expect(handles).toHaveCount(1);
    const hb = await handles.boundingBox();
    if (!hb) throw new Error('no handle');
    expect(hb.width).toBeGreaterThanOrEqual(44);
    await drag(page, { x: hb.x + 22, y: hb.y + 22 }, { x: hb.x + 140, y: hb.y + 64 }, 10);
    await expect.poll(async () => (await writeSelected(page)).length).toBeGreaterThan(word.length + 5);
    // The floating context toolbar over the selection.
    const bar = page.getByTestId('context-toolbar');
    await expect(bar).toBeVisible();
    await expect(bar).toHaveAttribute('data-kind', 'text');
    await expect(bar.getByTestId('context-comment')).toBeVisible();
    await snap(page, `write-handles-light-${formFactor(page)}.png`);
    await expectNoSeriousA11yViolations(page);
    // Copy from the toolbar runs the registry's command; the toolbar goes.
    await bar.getByTestId('context-copy').tap();
    await expect(bar).toBeHidden();
    // Pinch zooms the page; a double tap gives the width back.
    const zoom = () =>
      page.evaluate(
        () =>
          (
            window as unknown as { __kiln?: { registry: { getState(id: string): { value: unknown } } } }
          ).__kiln?.registry.getState('suite.view.zoom').value as number,
      );
    const z0 = await zoom();
    await pinch(page, { x: box.x + box.width / 2, y: box.y + 220 }, 80, 200);
    await expect.poll(zoom).toBeGreaterThan(z0 * 1.6);
    await doubleTap(page, { x: box.x + box.width / 2, y: box.y + 400 });
    await expect.poll(zoom).toBeCloseTo(isPhone(page) ? 1 : z0, 1);
  });

  test('mobile: Deck on a phone: the slide list, + for a slide, a slide full width, Back to the list, Notes and Present', async ({
    page,
  }) => {
    test.skip(!isPhone(page), 'the phone layout');
    await page.goto('/#/home');
    await expect(page.getByRole('heading', { level: 1 })).toBeVisible({ timeout: 30_000 });
    for (const scheme of ['light', 'dark'] as const) {
      await page.emulateMedia({ colorScheme: scheme });
      await openApp(page, 'deck', { template: true });
      const list = page.getByTestId('deck-phone-list');
      await expect(list).toBeVisible();
      await expect(page.getByTestId('deck-stage')).toBeHidden();
      await expect(list.getByRole('listitem')).toHaveCount((await deckState(page))?.slides.length ?? 0);
      await waitPainted(page);
      await snap(page, `deck-list-${scheme}-phone.png`);
      if (scheme === 'light') await expectNoSeriousA11yViolations(page);
    }
    await page.emulateMedia({ colorScheme: 'light' });
    const list = page.getByTestId('deck-phone-list');
    const count = (await deckState(page))?.slides.length ?? 0;
    // The FAB adds a slide after the current one.
    await page.getByTestId('deck-phone-new-slide').tap();
    await expect.poll(async () => (await deckState(page))?.slides.length).toBe(count + 1);
    // A tap on slide 2 opens it full width, with the slide bar under it.
    await list.getByRole('button', { name: /^Slide 2/ }).tap();
    await expect(page.getByTestId('deck-stage')).toBeVisible();
    await expect(list).toBeHidden();
    await expect.poll(async () => (await deckState(page))?.current).toBe(1);
    const stage = await page.getByTestId('deck-stage').boundingBox();
    const width = page.viewportSize()?.width ?? 0;
    expect(stage && stage.width >= width - 2).toBe(true);
    const bar = page.getByTestId('deck-phone-bar');
    await expect(bar).toContainText('Slide 2 of');
    // Notes: off gives the slide the screen, on shows them under the bar.
    const notes = bar.getByTestId('deck-phone-notes');
    await expect(notes).toHaveAttribute('aria-pressed', 'true');
    await expect(page.locator('.kiln-decknotes')).toBeVisible();
    await waitPainted(page);
    await snap(page, 'deck-slide-light-phone.png');
    await expectNoSeriousA11yViolations(page);
    await notes.tap();
    await expect(page.locator('.kiln-decknotes')).toHaveCount(0);
    await expect(notes).toHaveAttribute('aria-pressed', 'false');
    await notes.tap();
    await expect(page.locator('.kiln-decknotes')).toBeVisible();
    // Next slide from the bar.
    await bar.getByRole('button', { name: 'Next Slide' }).tap();
    await expect.poll(async () => (await deckState(page))?.current).toBe(2);
    // Back: the list first, then the presentation closes (asking about the new slide).
    await page.evaluate(() => history.back());
    await expect(list).toBeVisible();
    await expect(page.getByTestId('deck-stage')).toBeHidden();
    await list.getByRole('button', { name: /^Slide 1/ }).tap();
    await page.getByTestId('app-bar-back').tap();
    await expect(list).toBeVisible();
    // Present from the slide bar: full screen, a swipe goes on, Notes shows the speaker notes.
    await list.getByRole('button', { name: /^Slide 1/ }).tap();
    await page.getByTestId('deck-phone-present').tap();
    const show = page.getByTestId('deck-show');
    await expect(show).toBeVisible({ timeout: 15_000 });
    await expect(show).toHaveAttribute('data-slide', '0');
    const sb = await show.boundingBox();
    if (!sb) throw new Error('no show');
    await drag(
      page,
      { x: sb.x + sb.width * 0.8, y: sb.y + sb.height / 2 },
      { x: sb.x + sb.width * 0.2, y: sb.y + sb.height / 2 },
      6,
    );
    await expect(show).toHaveAttribute('data-slide', '1');
    await drag(
      page,
      { x: sb.x + sb.width * 0.2, y: sb.y + sb.height / 2 },
      { x: sb.x + sb.width * 0.8, y: sb.y + sb.height / 2 },
      6,
    );
    await expect(show).toHaveAttribute('data-slide', '0');
    // A tap on the right of the slide goes forward, on the left third back.
    await page.touchscreen.tap(sb.x + sb.width * 0.8, sb.y + sb.height / 2);
    await expect(show).toHaveAttribute('data-slide', '1');
    await page.touchscreen.tap(sb.x + sb.width * 0.1, sb.y + sb.height / 2);
    await expect(show).toHaveAttribute('data-slide', '0');
    await page.getByTestId('deck-show-notes-toggle').tap();
    await expect(page.getByTestId('deck-show-notes')).toBeVisible();
    await expect(page.getByTestId('deck-show-notes')).toContainText('Thank them for the time');
    await waitPainted(page);
    await snap(page, 'deck-present-light-phone.png');
    await page.keyboard.press('Escape');
    await expect(show).toBeHidden({ timeout: 15_000 });
  });

  test('mobile: Deck under a finger: 28 px shape handles with rotation, pinch and double tap', async ({
    page,
  }) => {
    await openApp(page, 'deck', { template: true });
    if (isPhone(page))
      await page
        .getByTestId('deck-phone-list')
        .getByRole('button', { name: /^Slide 1/ })
        .tap();
    const stage = page.getByTestId('deck-stage');
    await expect(stage).toBeVisible();
    await waitPainted(page);
    // Tap the title: it is selected, with 28 px handles a finger can take.
    const v = await deckView(page);
    const sb = await stage.boundingBox();
    if (!v || !sb) throw new Error('no stage');
    await page.touchscreen.tap(sb.x + v.left + v.width / 2, sb.y + v.top + v.height * 0.36);
    await expect.poll(async () => (await deckState(page))?.selection.length).toBe(1);
    const se = page.getByTestId('deck-handle-se');
    await expect(se).toBeVisible();
    const seBox = await se.boundingBox();
    expect(seBox && seBox.width >= 44 && seBox.height >= 44).toBe(true);
    const dot = await se.evaluate((el) => getComputedStyle(el, '::after').width);
    expect(dot).toBe('28px');
    const rotate = page.getByTestId('deck-handle-rotate');
    await expect(rotate).toBeVisible();
    // The floating toolbar over the shape (a text box tapped is edited at once: its text has the keys).
    if (!(await deckState(page))?.editing)
      await expect(page.getByTestId('context-toolbar')).toHaveAttribute('data-kind', 'object');
    await snap(page, `deck-handles-light-${formFactor(page)}.png`);
    // The rotation handle turns the shape.
    const rb = await rotate.boundingBox();
    if (!rb) throw new Error('no rotation handle');
    const c = { x: rb.x + 22, y: rb.y + 22 };
    await drag(page, c, { x: c.x + 120, y: c.y + 60 }, 10);
    await expect
      .poll(async () => Math.round((await deckState(page))?.selection_box?.rotation ?? 0))
      .not.toBe(0);
    // Pinch zooms the slide; a double tap on the background fits it again.
    await page.touchscreen.tap(sb.x + sb.width - 4, sb.y + sb.height - 4);
    // The tap clears the selection through the core; until it has, the shape's floating toolbar
    // sits where the fingers land, and a pinch that starts on its buttons is not the slide's.
    await expect.poll(async () => (await deckState(page))?.selection.length).toBe(0);
    await expect(page.getByTestId('context-toolbar')).toBeHidden();
    const z0 = (await deckView(page))?.zoom ?? 1;
    await pinch(page, { x: sb.x + sb.width / 2, y: sb.y + sb.height / 2 }, 60, 160);
    await expect.poll(async () => (await deckView(page))?.zoom ?? 0).toBeGreaterThan(z0 * 1.8);
    await pinch(page, { x: sb.x + sb.width / 2, y: sb.y + sb.height / 2 }, 200, 60);
    await expect.poll(async () => (await deckView(page))?.zoom ?? 0).toBeLessThan(z0 * 1.2);
    // A double tap on the background: the slide fits; again: closer; again: it fits.
    const corner = { x: sb.x + sb.width - 3, y: sb.y + sb.height - 3 };
    await doubleTap(page, corner);
    await expect.poll(async () => (await deckView(page))?.zoom ?? 0).toBeCloseTo(z0, 2);
    await page.waitForTimeout(400);
    await doubleTap(page, corner);
    await expect.poll(async () => (await deckView(page))?.zoom ?? 0).toBeGreaterThan(z0 * 1.5);
    await page.waitForTimeout(400);
    await doubleTap(page, corner);
    await expect.poll(async () => (await deckView(page))?.zoom ?? 0).toBeCloseTo(z0, 2);
  });

  test('mobile: every control of the phone layout is at least 44 × 44 CSS px', async ({ page }) => {
    test.skip(!isPhone(page), 'the phone layout');
    const small: string[] = [];
    // The on-screen keyboard (Grid's formula bar pinned on it), as in the docking test.
    await page.addInitScript(() => {
      const target = new EventTarget();
      const vv = {
        height: window.innerHeight,
        width: window.innerWidth,
        offsetTop: 0,
        offsetLeft: 0,
        pageTop: 0,
        pageLeft: 0,
        scale: 1,
        onresize: null,
        onscroll: null,
        addEventListener: target.addEventListener.bind(target),
        removeEventListener: target.removeEventListener.bind(target),
        dispatchEvent: target.dispatchEvent.bind(target),
      };
      Object.defineProperty(window, 'visualViewport', { configurable: true, get: () => vv });
      (window as unknown as { __keyboard(h: number): void }).__keyboard = (h: number) => {
        vv.height = window.innerHeight - h;
        target.dispatchEvent(new Event('resize'));
      };
    });
    const keyboard = (h: number) =>
      page.evaluate((x) => (window as unknown as { __keyboard(h: number): void }).__keyboard(x), h);
    await page.goto('/#/home');
    await expect(page.getByRole('heading', { level: 1 })).toBeVisible({ timeout: 30_000 });
    small.push(...(await smallTargets(page, ['.kiln-home'])));
    for (const app of APPS) {
      await openApp(page, app, { template: true });
      // The whole frame: the app bar, the editor's own phone parts, the bottom toolbar.
      small.push(...(await smallTargets(page, ['.kiln-appframe'])));
      if (app === 'grid') {
        // A range: the handles, the statistics chip, the floating toolbar.
        const b3 = await cellPoint(page, 2, 1);
        await page.touchscreen.tap(b3.x, b3.y);
        const end = page.getByTestId('grid-range-handle-end');
        await expect(end).toBeVisible();
        const hb = await end.boundingBox();
        if (hb) await drag(page, { x: hb.x + 22, y: hb.y + 22 }, { x: hb.x + 90, y: hb.y + 80 }, 6);
        await expect(page.getByTestId('context-toolbar')).toBeVisible();
        small.push(...(await smallTargets(page, ['.kiln-appframe'])));
        // The formula bar on the keyboard, with the quick keys.
        await page.getByRole('textbox', { name: 'Formula Bar' }).tap();
        await keyboard(300);
        await expect(page.getByTestId('grid-quick-keys')).toBeVisible();
        small.push(...(await smallTargets(page, ['.kiln-appframe'])));
        await page.keyboard.press('Escape');
        await keyboard(0);
        // A long press: the cell's menu.
        const d6 = await cellPoint(page, 5, 3);
        await longPress(page, d6);
        await expect(page.getByRole('menu')).toBeVisible();
        small.push(...(await smallTargets(page, ['[role="menu"]'])));
        small.push(...(await menusOffScreen(page)));
        await page.keyboard.press('Escape');
      }
      if (app === 'deck') {
        // A slide open: the slide bar and the notes.
        await page
          .getByTestId('deck-phone-list')
          .getByRole('button', { name: /^Slide 1/ })
          .tap();
        await expect(page.getByTestId('deck-phone-bar')).toBeVisible();
        small.push(...(await smallTargets(page, ['.kiln-appframe'])));
        // Present: the show's toolbar, its pen tools, and Notes.
        await page.getByTestId('deck-phone-present').tap();
        const show = page.getByTestId('deck-show');
        await expect(show).toBeVisible({ timeout: 15_000 });
        await show.getByRole('button', { name: 'Pen and laser pointer tools' }).tap();
        await page.getByTestId('deck-show-notes-toggle').tap();
        await expect(page.getByTestId('deck-show-notes')).toBeVisible();
        small.push(...(await smallTargets(page, ['.kiln-show'])));
        await page.keyboard.press('Escape');
        await expect(show).toBeHidden({ timeout: 15_000 });
        if (await page.getByTestId('app-bar-back').isVisible()) await page.getByTestId('app-bar-back').tap();
      }
      // The overflow menu.
      await page.getByTestId('app-bar-overflow').tap();
      small.push(...(await smallTargets(page, ['[role="menu"]'])));
      small.push(...(await menusOffScreen(page)));
      await page.keyboard.press('Escape');
      // The ribbon sheet, every tab of it.
      await page.getByTestId('ribbon-sheet-button').tap();
      const sheet = page.getByTestId('ribbon-sheet');
      const count = await sheet.getByRole('tab').count();
      for (let i = 0; i < count; i++) {
        await sheet.getByRole('tab').nth(i).tap();
        // The whole tab, scrolled through.
        const steps = await sheet
          .locator('.kiln-sheet__body')
          .evaluate((el) => Math.ceil(el.scrollHeight / el.clientHeight));
        for (let s = 0; s < steps; s++) {
          await sheet
            .locator('.kiln-sheet__body')
            .evaluate((el, n) => el.scrollTo({ top: n * el.clientHeight }), s);
          small.push(...(await smallTargets(page, ['[data-testid="ribbon-sheet"]'])));
        }
      }
      await page.keyboard.press('Escape');
      await expect(sheet).toBeHidden();
    }
    expect([...new Set(small)]).toEqual([]);
  });
});
