/**
 * The app frame (WP-58, PLAN.md §1.3–1.5): every app's frame in light and dark (reviewed
 * screenshots in e2e/screenshots/chrome/), axe on each frame, the ribbon's keyboard model,
 * collapse (Ctrl+F1), Classic/Simplified, Key Tips (Alt → H → B reaches its catalog id), the
 * palette's Office synonyms, the theme switch and File ▸ Options.
 */
import { execFileSync } from 'node:child_process';
import { mkdtempSync, readFileSync, rmSync, statSync } from 'node:fs';
import { createServer, type Server } from 'node:http';
import type { AddressInfo } from 'node:net';
import { tmpdir } from 'node:os';
import { extname, join, normalize } from 'node:path';
import { fileURLToPath } from 'node:url';
import { expect, type Page, test } from '@playwright/test';
import { expectNoSeriousA11yViolations } from '../../packages/testing/src/playwright/a11y.ts';

declare global {
  interface Window {
    /** The app frame's development handle (apps/web/src/routes/app-frame/debug.ts). */
    __kiln?: {
      app: string;
      resolved: { key: string; command: string; args?: unknown }[];
      executed: { id: string; status: string }[];
      log(): { seq: number; id: string; outcome: string }[];
      registry: {
        setContext(patch: Record<string, unknown>): void;
        getState(id: string): { enabled: boolean; pending: boolean; visible: boolean };
      };
      keymap: {
        resolve(
          e: { key: string; ctrlKey: boolean; shiftKey: boolean; altKey: boolean },
          ctx: Record<string, unknown>,
          o?: { release?: boolean },
        ): { command: string; args?: unknown } | undefined;
      };
    };
  }
}

const shot = (name: string) => fileURLToPath(new URL(`../screenshots/chrome/${name}`, import.meta.url));
const APPS = ['write', 'grid', 'deck', 'notes'] as const;

/** Opens an app frame and waits until the core has answered (title known, ribbon drawn). */
async function openFrame(page: Page, app: string) {
  await page.goto(`/#/${app}`);
  await expect(page.getByRole('tablist', { name: 'Ribbon tabs' })).toBeVisible();
  await expect(page.getByTestId('document-title')).not.toHaveText('Opening…', { timeout: 30_000 });
  await page.evaluate(() => document.fonts.ready);
}

const executed = (page: Page) => page.evaluate(() => window.__kiln?.executed ?? []);

test.describe('chrome', () => {
  for (const app of APPS) {
    for (const scheme of ['light', 'dark'] as const) {
      test(`${app} frame (${scheme}, desktop) renders the brand frame and passes axe`, async ({ page }) => {
        await page.emulateMedia({ colorScheme: scheme });
        await openFrame(page, app);
        await expect(page.getByRole('button', { name: 'File' })).toBeVisible();
        await expect(page.getByRole('tab', { name: 'Home' })).toHaveAttribute('aria-selected', 'true');
        await expect(page.locator('.kiln-appframe')).toHaveAttribute('data-app', app);
        const overflow = await page.evaluate(() => document.documentElement.scrollWidth > window.innerWidth);
        expect(overflow).toBe(false);
        await page.screenshot({ path: shot(`${app}-frame-${scheme}-desktop.png`) });
        await expectNoSeriousA11yViolations(page);
      });
    }
  }

  test('the Write frame shows the core document: title, Undo state and status bar', async ({ page }) => {
    await openFrame(page, 'write');
    await expect(page.getByTestId('document-title')).toContainText('Document');
    await expect(page.getByTestId('save-status')).toHaveText('New document');
    // Nothing to undo yet: the core says so through DocSummary.can_undo.
    await expect(page.getByRole('button', { name: 'Undo' })).toHaveAttribute('aria-disabled', 'true');
    await expect(page.getByRole('radiogroup', { name: 'Views' })).toBeVisible();
  });

  test('ribbon tabs follow the WAI-ARIA tabs pattern and lead into the commands', async ({ page }) => {
    await openFrame(page, 'write');
    const home = page.getByRole('tab', { name: 'Home' });
    await home.focus();
    await page.keyboard.press('ArrowRight');
    await expect(page.getByRole('tab', { name: 'Insert' })).toBeFocused();
    await expect(page.getByRole('tab', { name: 'Insert' })).toHaveAttribute('aria-selected', 'true');
    await page.keyboard.press('End');
    await expect(page.getByRole('tab', { name: 'View', exact: true })).toBeFocused();
    await page.keyboard.press('Home');
    await expect(home).toBeFocused();
    await page.keyboard.press('ArrowDown');
    const toolbar = page.getByRole('toolbar', { name: 'Home commands' });
    await expect(toolbar.locator(':focus')).toHaveCount(1);
    await page.keyboard.press('ArrowRight');
    await expect(toolbar.locator(':focus')).toHaveCount(1);
    await page.keyboard.press('Escape');
    await expect(home).toBeFocused();
  });

  test('Ctrl+F1 collapses and restores the ribbon; the choice is a host preference', async ({ page }) => {
    await openFrame(page, 'write');
    await expect(page.getByRole('tabpanel')).toBeVisible();
    await page.getByRole('tab', { name: 'Home' }).focus();
    await page.keyboard.press('Control+F1');
    await expect(page.getByRole('tabpanel')).toHaveCount(0);
    expect(await page.evaluate(() => localStorage.getItem('kiln.pref.ui.ribbon.collapsed'))).toBe('1');
    // A tab click shows the commands floating over the document.
    await page.getByRole('tab', { name: 'Insert' }).click();
    await expect(page.getByRole('tabpanel')).toBeVisible();
    await page.keyboard.press('Escape');
    await page.keyboard.press('Control+F1');
    await expect(page.getByRole('tabpanel')).toBeVisible();
    expect(await page.evaluate(() => localStorage.getItem('kiln.pref.ui.ribbon.collapsed'))).toBe('0');
  });

  test('switches between the Classic and Simplified ribbon', async ({ page }) => {
    await openFrame(page, 'grid');
    const ribbon = page.locator('.kiln-ribbon');
    await expect(ribbon).toHaveAttribute('data-layout', 'classic');
    await page.getByRole('button', { name: 'Ribbon display options' }).click();
    await page.getByRole('menuitem', { name: 'Simplified ribbon' }).click();
    await expect(ribbon).toHaveAttribute('data-layout', 'simplified');
    await page.screenshot({ path: shot('grid-simplified-light-desktop.png') });
    expect(await page.evaluate(() => localStorage.getItem('kiln.pref.ui.ribbon.layout'))).toBe('simplified');
    await page.reload();
    await expect(page.locator('.kiln-ribbon')).toHaveAttribute('data-layout', 'simplified');
    await page.getByRole('button', { name: 'Ribbon display options' }).click();
    await page.getByRole('menuitem', { name: 'Classic ribbon' }).click();
    await expect(page.locator('.kiln-ribbon')).toHaveAttribute('data-layout', 'classic');
  });

  test('a narrow window collapses groups (Classic) or overflows into More (Simplified)', async ({ page }) => {
    await page.setViewportSize({ width: 1024, height: 768 });
    await openFrame(page, 'write');
    // Automatic layout: Simplified below 1100 px, with the rest of the commands under More.
    await expect(page.locator('.kiln-ribbon')).toHaveAttribute('data-layout', 'simplified');
    await expect(page.locator('.kiln-ribbon__more')).toBeVisible();
    await page.getByRole('button', { name: 'Ribbon display options' }).click();
    await page.getByRole('menuitem', { name: 'Classic ribbon' }).click();
    await expect(page.locator('[data-ribbon-group][data-collapsed="true"]').first()).toBeVisible();
    await page.screenshot({ path: shot('write-frame-light-tablet.png') });
    const overflow = await page.evaluate(() => document.documentElement.scrollWidth > window.innerWidth);
    expect(overflow).toBe(false);
    await page.getByRole('button', { name: 'Ribbon display options' }).click();
    await page.getByRole('menuitem', { name: /Automatic/ }).click();
    await page.setViewportSize({ width: 390, height: 844 });
    await expect(page.locator('.kiln-ribbon')).toHaveAttribute('data-layout', 'simplified');
    expect(await page.evaluate(() => document.documentElement.scrollWidth > window.innerWidth)).toBe(false);
    await page.screenshot({ path: shot('write-frame-light-phone.png') });
  });

  test('Key Tips: Alt → H → B reaches Borders, Alt → H → 1 reaches Bold', async ({ page }) => {
    await openFrame(page, 'write');
    await page.getByRole('main').click();
    await page.keyboard.press('Alt');
    await expect(page.getByTestId('keytips')).toBeVisible();
    await expect(page.locator('[data-keytip-badge="H"]')).toBeVisible();
    await page.screenshot({ path: shot('write-keytips-light-desktop.png') });
    await page.keyboard.press('h');
    await expect(page.locator('[data-keytip-badge="B"]')).toBeVisible();
    // Borders is a drop-down in Kiln Write (WP-64): its Key Tip takes the focus to it, and Enter
    // opens the borders menu.
    await page.keyboard.press('b');
    await expect(page.locator('[data-command="write.paragraph.borders"]').first()).toBeFocused();
    await page.keyboard.press('Enter');
    const borders = page.getByRole('menuitem', { name: 'Bottom Border' });
    await expect(borders).toBeVisible();
    await borders.click();
    await expect
      .poll(async () => (await executed(page)).map((e) => e.id))
      .toContain('write.paragraph.borders');
    await page.keyboard.press('Alt');
    await expect(page.locator('[data-keytip-badge="H"]')).toBeVisible();
    await page.keyboard.press('h');
    await expect(page.locator('[data-keytip-badge="1"]')).toBeVisible();
    await page.keyboard.press('1');
    await expect.poll(async () => (await executed(page)).map((e) => e.id)).toContain('suite.format.bold');
  });

  test("the palette finds 'Merge & Center' by its Office synonym and shows shortcuts", async ({ page }) => {
    await openFrame(page, 'grid');
    await page.keyboard.press('Control+Shift+P');
    const dialog = page.getByRole('dialog');
    await expect(dialog).toBeVisible();
    await dialog.getByRole('combobox').fill('combine cells');
    await expect(dialog.getByRole('option', { name: /Merge & Center/ }).first()).toBeVisible();
    await page.screenshot({ path: shot('grid-palette-light-desktop.png') });
    // (Bold is Kiln Write's core command since WP-64; Grid offers it when its core does, WP-71.)
    await dialog.getByRole('combobox').fill('save');
    await expect(dialog.getByRole('option', { name: /^Save/ }).first()).toContainText('Ctrl+S');
    await expectNoSeriousA11yViolations(page);
    await page.keyboard.press('Escape');
    await expect(dialog).toBeHidden();
  });

  test('View ▸ Theme switches to dark and keeps it through the host', async ({ page }) => {
    await page.emulateMedia({ colorScheme: 'light' });
    await openFrame(page, 'write');
    await page.getByRole('tab', { name: 'View', exact: true }).click();
    await page.getByRole('button', { name: 'Theme' }).click();
    await page.getByRole('menuitemcheckbox', { name: 'Dark' }).click();
    await expect(page.locator('html')).toHaveAttribute('data-theme', 'dark');
    const bg = await page.evaluate(() => getComputedStyle(document.body).backgroundColor);
    expect(bg).toBe('rgb(23, 17, 14)');
    expect(await page.evaluate(() => localStorage.getItem('kiln.pref.ui.theme'))).toBe('dark');
    await page.reload();
    await expect(page.locator('html')).toHaveAttribute('data-theme', 'dark');
    await page.getByRole('tab', { name: 'View', exact: true }).click();
    await page.getByRole('button', { name: 'Theme' }).click();
    await page.getByRole('menuitemcheckbox', { name: 'Use System Setting' }).click();
    await expect
      .poll(() => page.evaluate(() => getComputedStyle(document.body).backgroundColor))
      .toBe('rgb(246, 239, 230)');
  });

  test('File ▸ Options opens the Options dialog, which passes axe', async ({ page }) => {
    await openFrame(page, 'write');
    await page.getByRole('button', { name: 'File' }).click();
    await page.getByRole('button', { name: 'Options' }).click();
    const dialog = page.getByRole('dialog', { name: /Options/ });
    await expect(dialog).toBeVisible();
    await page.screenshot({ path: shot('write-options-light-desktop.png') });
    await expectNoSeriousA11yViolations(page);
    await page.keyboard.press('Escape');
    await expect(dialog).toBeHidden();
  });

  test('a planned command says it is not in this build rather than doing nothing', async ({ page }) => {
    // Format Painter arrives with WP-66 (Cut, the example before WP-64, is in the core now).
    await openFrame(page, 'write');
    await page.locator('[data-command="suite.edit.format_painter"]').first().click();
    await expect(page.getByText(/is not in this build yet/)).toBeVisible();
    expect((await executed(page)).at(-1)).toEqual({ id: 'suite.edit.format_painter', status: 'pending' });
  });
});

const MIME: Record<string, string> = {
  '.html': 'text/html; charset=utf-8',
  '.js': 'text/javascript',
  '.css': 'text/css',
  '.wasm': 'application/wasm',
  '.json': 'application/json',
  '.webmanifest': 'application/manifest+json',
  '.svg': 'image/svg+xml',
  '.png': 'image/png',
  '.ttf': 'font/ttf',
  '.woff2': 'font/woff2',
  '.map': 'application/json',
};

/** Serves a built site (no SPA fallback needed: the app routes by hash). */
function serveStatic(root: string): Promise<Server> {
  const server = createServer((req, res) => {
    const path = decodeURIComponent(new URL(req.url ?? '/', 'http://x').pathname);
    const file = normalize(join(root, path === '/' ? 'index.html' : path));
    try {
      if (!file.startsWith(root) || !statSync(file).isFile()) throw new Error('not found');
      res.writeHead(200, { 'content-type': MIME[extname(file)] ?? 'application/octet-stream' });
      res.end(readFileSync(file));
    } catch {
      res.writeHead(404).end();
    }
  });
  return new Promise((resolve) => server.listen(0, '127.0.0.1', () => resolve(server)));
}

/**
 * What production (and the Pages site) ships: `vite build` with VITE_KILN_RELEASE=1, where every
 * command the core does not implement yet is hidden. The chrome must still be whole: no group slab
 * without a command, no tab that opens onto nothing, the base styles present, the zoom control
 * laid out in one row.
 */
test.describe('chrome (release build)', () => {
  test.describe.configure({ mode: 'serial' });
  const repoRoot = fileURLToPath(new URL('../..', import.meta.url));
  let outDir = '';
  let server: Server | undefined;
  let base = '';

  test.beforeAll(async () => {
    test.setTimeout(240_000);
    outDir = mkdtempSync(join(tmpdir(), 'kiln-release-'));
    execFileSync(
      'pnpm',
      ['--filter', '@kiln/web', 'exec', 'vite', 'build', '--outDir', outDir, '--emptyOutDir'],
      {
        cwd: repoRoot,
        env: { ...process.env, VITE_KILN_RELEASE: '1' },
        stdio: 'pipe',
      },
    );
    server = await serveStatic(outDir);
    base = `http://127.0.0.1:${(server.address() as AddressInfo).port}`;
  });

  test.afterAll(async () => {
    await new Promise<void>((resolve) => (server ? server.close(() => resolve()) : resolve()));
    if (outDir) rmSync(outDir, { recursive: true, force: true });
  });

  async function openRelease(page: Page, app: string) {
    await page.goto(`${base}/#/${app}`);
    await expect(page.getByRole('tablist', { name: 'Ribbon tabs' })).toBeVisible();
    await expect(page.getByTestId('document-title')).not.toHaveText('Opening…', { timeout: 30_000 });
    await page.evaluate(() => document.fonts.ready);
  }

  /** Every group of the shown tab holds at least one visible control in its body. */
  async function expectNoEmptyGroups(page: Page, where: string) {
    const groups = page.locator('.kiln-ribbon__groups > [role="group"]');
    const count = await groups.count();
    expect(count, `${where}: a tab with no group should not exist`).toBeGreaterThan(0);
    for (let i = 0; i < count; i++) {
      const g = groups.nth(i);
      const label = await g.getAttribute('aria-label');
      const controls = g.locator(
        '.kiln-ribbon__group-body :is(button, input, [role="combobox"]):visible, :scope > button:visible',
      );
      expect(await controls.count(), `${where}: group "${label}" is an empty slab`).toBeGreaterThan(0);
    }
  }

  for (const app of ['write', 'grid'] as const) {
    for (const scheme of ['light', 'dark'] as const) {
      test(`${app} frame (${scheme}): no empty groups or tabs, styled controls, zoom in one row`, async ({
        page,
      }) => {
        await page.emulateMedia({ colorScheme: scheme });
        await openRelease(page, app);
        // The primitives' styles made it into the bundle: only their `.kiln-btn` rule sets `--btn-px`.
        const buttonBase = await page
          .locator('.kiln-ribbon .kiln-btn')
          .first()
          .evaluate((el) => getComputedStyle(el).getPropertyValue('--btn-px').trim());
        expect(buttonBase).not.toBe('');
        const tabs = page.getByRole('tab');
        const names = await tabs.allTextContents();
        expect(names.length).toBeGreaterThan(0);
        for (const name of names) {
          await page.getByRole('tab', { name, exact: true }).click();
          await expectNoEmptyGroups(page, `${app} ▸ ${name}`);
        }
        // Zoom: −, slider, + and the percentage centred on one line.
        const zoom = page.locator('.kiln-statusbar__zoom');
        await expect(zoom.getByRole('slider', { name: 'Zoom' })).toBeVisible();
        const centres = await zoom.locator('.kiln-zoom > *').evaluateAll((els) =>
          els.map((el) => {
            const r = el.getBoundingClientRect();
            return r.top + r.height / 2;
          }),
        );
        expect(centres.length).toBeGreaterThanOrEqual(4);
        for (const c of centres) expect(Math.abs(c - (centres[0] ?? 0))).toBeLessThan(3);
        await page.getByRole('tab', { name: names[0] ?? '', exact: true }).click();
        await page.mouse.move(720, 600); // no hover highlight in the reviewed screenshot
        await page.waitForTimeout(300); // let the tab's hover transition finish
        await page.screenshot({ path: shot(`release-${app}-frame-${scheme}-desktop.png`) });
        await expectNoSeriousA11yViolations(page);
      });
    }
  }

  for (const app of ['deck', 'notes'] as const) {
    test(`${app} frame (light): no document, so tabs only over the notice`, async ({ page }) => {
      await openRelease(page, app);
      await expect(page.getByText(/cannot be created in this build yet/)).toBeVisible();
      await expect(page.locator('.kiln-ribbon__groups')).toHaveCount(0);
      await page.screenshot({ path: shot(`release-${app}-frame-light-desktop.png`) });
      await expectNoSeriousA11yViolations(page);
    });
  }

  test('Home (light, desktop and phone) renders with its base styles', async ({ page }) => {
    await page.goto(`${base}/#/home`);
    await expect(page.getByRole('heading', { level: 1 })).toContainText('What are we making today?');
    await page.evaluate(() => document.fonts.ready);
    await page.screenshot({ path: shot('release-home-light-desktop.png') });
    await page.setViewportSize({ width: 390, height: 844 });
    expect(await page.evaluate(() => document.documentElement.scrollWidth > window.innerWidth)).toBe(false);
    await page.screenshot({ path: shot('release-home-light-phone.png') });
    await expectNoSeriousA11yViolations(page);
  });
});
