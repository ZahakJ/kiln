/**
 * The site's screenshots and demo: every picture on the landing page and in the manual is
 * the real application, from the built Pages artefact (`tools/pages-build.sh`, served by
 * `tools/pages-serve.mjs`), so the pictures are what is deployed, with sample documents (the bundled templates saved
 * as Office files by `kiln-cli convert`, in e2e/fixtures/media/), at device scale 2 in light and dark. Writes docs/media/screenshots/*.png and docs/media/demo.gif;
 * `pnpm media:render --screenshots` then makes the WebP and AVIF sizes the page loads.
 *
 *   pnpm e2e --grep docs-media --workers=1
 */
import { spawnSync } from 'node:child_process';
import { mkdirSync, readFileSync, rmSync, statSync } from 'node:fs';
import { type Browser, expect, type Page, test } from '@playwright/test';
import { noFilePickers, repo } from './grid-helpers.ts';

/** The built web app; the suite's development server shows chrome the production build hides. */
const BASE = process.env.KILN_E2E_BASE ?? 'http://127.0.0.1:4180/kiln/app/';
const KILN_DEPLOYED = !!process.env.KILN_E2E_BASE;
const OUT = repo('docs/media/screenshots');
const SCRATCH = repo('e2e/screenshots/.docs-media');
const DESKTOP = { width: 1440, height: 900 };
const PHONE = { width: 390, height: 844 };
const SCHEMES = ['light', 'dark'] as const;
type Scheme = (typeof SCHEMES)[number];

async function contextFor(browser: Browser, scheme: Scheme, phone = false) {
  const context = await browser.newContext({
    viewport: phone ? PHONE : DESKTOP,
    deviceScaleFactor: 2,
    colorScheme: scheme,
    hasTouch: phone,
    isMobile: phone,
    serviceWorkers: 'block',
  });
  await context.addInitScript(() => {
    localStorage.setItem('kiln.pref.ui.home.tour_seen', '1');
    localStorage.setItem(
      'kiln.options',
      // Red squiggles under the people's names in a sample say nothing about Kiln.
      JSON.stringify({
        general: { userName: 'Tilly Okonjo', initials: 'TO' },
        proofing: { checkSpellingAsYouType: false },
      }),
    );
  });
  return context;
}

/** Opens a sample as a file, as File ▸ Open does. */
async function open(page: Page, file: string): Promise<void> {
  await page.goto(`${BASE}#/home`);
  await expect(page.getByRole('heading', { level: 1 }).first()).toBeVisible({ timeout: 60_000 });
  await noFilePickers(page);
  const [chooser] = await Promise.all([page.waitForEvent('filechooser'), page.keyboard.press('Control+o')]);
  await chooser.setFiles({
    name: file,
    mimeType: 'application/octet-stream',
    buffer: readFileSync(repo(`e2e/fixtures/media/${file}`)),
  });
  await expect(page.getByTestId('document-title').first()).toContainText(file.slice(0, 8), {
    timeout: 60_000,
  });
  await page.evaluate(() => document.fonts.ready);
  await page.waitForTimeout(1500);
}

/** Selects a cell through the Name Box, as a person would. */
async function goTo(page: Page, cell: string): Promise<void> {
  const nameBox = page.getByRole('textbox', { name: 'Name Box' });
  await nameBox.click();
  await nameBox.fill(cell);
  await nameBox.press('Enter');
}

async function shoot(page: Page, name: string, scheme: Scheme, clip?: { width: number; height: number }) {
  await page.mouse.move(2, 2);
  await page.waitForTimeout(900);
  await page.screenshot({
    path: `${OUT}/${name}-${scheme}.png`,
    ...(clip ? { clip: { x: 0, y: 0, ...clip } } : {}),
  });
}

/** The status bar has counted the pages. */
async function paginated(page: Page): Promise<void> {
  await expect(page.getByText(/^Page \d+ of [1-9]\d*$/).first()).toBeVisible({ timeout: 20_000 });
}

async function scroll(page: Page, by: number, x = DESKTOP.width / 2, y = 500): Promise<void> {
  await page.mouse.move(x, y);
  for (let left = by; left > 0; left -= 400) {
    await page.mouse.wheel(0, Math.min(left, 400));
    await page.waitForTimeout(60);
  }
  await page.waitForTimeout(800);
}

test.describe('docs-media', () => {
  test.describe.configure({ mode: 'serial', timeout: 240_000 });
  test.beforeAll(async () => {
    mkdirSync(OUT, { recursive: true });
    const built = await fetch(new URL('/__kiln/health', BASE)).then(
      (r) => r.text(),
      () => 'missing',
    );
    if (!KILN_DEPLOYED && !built.startsWith('built'))
      throw new Error('docs-media shoots the Pages artefact: run tools/pages-build.sh first');
  });

  for (const scheme of SCHEMES) {
    test(`Kiln Write, ${scheme}`, async ({ browser }) => {
      const context = await contextFor(browser, scheme);
      const page = await context.newPage();
      await open(page, 'Annual report 2025.docx');
      await scroll(page, scheme === 'light' ? 1180 : 1860);
      await paginated(page);
      await shoot(page, 'write-report', scheme);
      await shoot(page, 'ribbon', scheme, { width: DESKTOP.width, height: 196 });
      await open(page, 'The Allotment newsletter.docx');
      await paginated(page);
      await shoot(page, 'write-newsletter', scheme);

      await open(page, 'Repair cafe proposal.docx');
      await paginated(page);
      await shoot(page, 'write-proposal', scheme);
      // A real review: one replaced word, one added sentence and a comment on two words.
      await page.mouse.click(735, 669);
      await page.keyboard.press('Control+Shift+E');
      await page.keyboard.press('Home');
      for (let i = 0; i < 4; i++) await page.keyboard.press('Control+ArrowRight');
      await page.keyboard.press('Control+Delete');
      await page.keyboard.type('audited ');
      await page.mouse.click(1000, 711);
      await page.keyboard.press('End');
      await page.keyboard.type(' Most said they would use a free repair service.');
      await page.mouse.click(735, 669);
      await page.keyboard.press('Home');
      await page.keyboard.press('Control+Shift+ArrowRight');
      await page.keyboard.press('Control+Shift+ArrowRight');
      await page.keyboard.press('Control+Alt+M');
      const composer = page.getByTestId('write-comment-composer');
      await composer.getByRole('textbox', { name: 'New comment' }).fill('Which year is this survey from?');
      await composer.getByRole('button', { name: 'Comment' }).click();
      await page.getByRole('tab', { name: 'Review', exact: true }).click();
      await page.getByRole('button', { name: /Display for Review/ }).click();
      await page
        .getByRole('menuitemradio', { name: /All Markup/ })
        .or(page.getByRole('menuitem', { name: /All Markup/ }))
        .first()
        .click();
      // A change of markup leaves the status bar at "Page 1 of 0" until the next click in the page.
      await page.waitForTimeout(1000);
      await page.mouse.click(620, 760);
      await paginated(page);
      await shoot(page, 'write-review', scheme);
      await context.close();
    });

    test(`Kiln Grid, ${scheme}`, async ({ browser }) => {
      const context = await contextFor(browser, scheme);
      const page = await context.newPage();
      await open(page, 'Household budget.xlsx');
      await goTo(page, 'D16');
      await shoot(page, 'grid-budget', scheme);
      await page.keyboard.press('Control+Shift+P');
      const palette = page.getByRole('dialog');
      await palette.getByRole('combobox').fill('merge');
      await expect(palette.getByRole('option', { name: /Merge & Center/ }).first()).toBeVisible();
      await shoot(page, 'palette', scheme);
      await open(page, 'Expense report.xlsx');
      await goTo(page, 'F16');
      await shoot(page, 'grid-expenses', scheme);
      await context.close();
    });

    test(`Kiln Deck, ${scheme}`, async ({ browser }) => {
      const context = await contextFor(browser, scheme);
      const page = await context.newPage();
      await open(page, 'Tideline launch briefing.pptx');
      await page.keyboard.press('PageDown');
      await page.keyboard.press('PageDown');
      await page.keyboard.press('PageDown');
      await shoot(page, 'deck-launch', scheme);
      await open(page, 'Q3 business review.pptx');
      await page.keyboard.press('PageDown');
      await page.keyboard.press('PageDown');
      await shoot(page, 'deck-review', scheme);
      await page.getByRole('tab', { name: 'Slide Show' }).click();
      await page.getByRole('button', { name: 'Use Presenter View' }).click();
      const popup = context.waitForEvent('page');
      await page.getByRole('button', { name: 'From Beginning' }).click();
      const audience = await popup;
      await expect(page.getByTestId('deck-presenter')).toBeVisible({ timeout: 15_000 });
      await expect(audience.getByTestId('deck-show')).toBeVisible({ timeout: 60_000 });
      await page.keyboard.press('ArrowRight');
      await page.keyboard.press('ArrowRight');
      await shoot(page, 'deck-presenter', scheme);
      await context.close();
    });

    test(`Kiln Home, ${scheme}`, async ({ browser }) => {
      const context = await contextFor(browser, scheme);
      const page = await context.newPage();
      await page.goto(`${BASE}#/home`);
      await expect(page.getByRole('heading', { level: 1 }).first()).toBeVisible({ timeout: 60_000 });
      await page.waitForTimeout(2000);
      await shoot(page, 'home', scheme);
      await context.close();
    });

    test(`phones, ${scheme}`, async ({ browser }) => {
      const context = await contextFor(browser, scheme, true);
      const page = await context.newPage();
      await open(page, 'Repair cafe proposal.docx');
      await shoot(page, 'phone-write', scheme);
      await open(page, 'Household budget.xlsx');
      await shoot(page, 'phone-grid', scheme);
      await open(page, 'Tideline launch briefing.pptx');
      await shoot(page, 'phone-deck', scheme);
      await context.close();
    });
  }

  test('demo.gif', async ({ browser }) => {
    rmSync(SCRATCH, { recursive: true, force: true });
    mkdirSync(SCRATCH, { recursive: true });
    const context = await browser.newContext({
      viewport: { width: 1280, height: 800 },
      colorScheme: 'light',
      serviceWorkers: 'block',
      recordVideo: { dir: SCRATCH, size: { width: 1280, height: 800 } },
    });
    await context.addInitScript(() => localStorage.setItem('kiln.pref.ui.home.tour_seen', '1'));
    const page = await context.newPage();
    await page.goto(`${BASE}#/home`);
    await expect(page.getByRole('heading', { level: 1 }).first()).toBeVisible({ timeout: 60_000 });
    // The cores load once before the clip starts, so it shows work and not a first load.
    await open(page, 'Household budget.xlsx');
    await open(page, 'Tideline launch briefing.pptx');
    await page.goto(`${BASE}#/home`);
    await expect(page.getByRole('heading', { level: 1 }).first()).toBeVisible({ timeout: 60_000 });
    const started = Date.now();
    await page.waitForTimeout(2000);
    await open(page, 'Household budget.xlsx');
    await goTo(page, 'D13');
    await page.waitForTimeout(1000);
    await page.keyboard.type('4200', { delay: 180 });
    await page.keyboard.press('Enter');
    await page.waitForTimeout(2200);
    await page.keyboard.press('Control+Shift+P');
    await page.getByRole('dialog').getByRole('combobox').pressSequentially('merge', { delay: 140 });
    await page.waitForTimeout(1400);
    await page.keyboard.press('Escape');
    await open(page, 'Tideline launch briefing.pptx');
    await page.waitForTimeout(700);
    await page.keyboard.press('PageDown');
    await page.waitForTimeout(900);
    await page.keyboard.press('PageDown');
    await page.waitForTimeout(1500);
    const seconds = (Date.now() - started) / 1000;
    const video = await page.video()?.path();
    await context.close();
    expect(video).toBeTruthy();
    // ffmpeg's two-pass palette: one palette for the whole clip, no dither crawl on flat chrome.
    const skip = Math.max(0, (await durationOf(video as string)) - seconds);
    const filter =
      'fps=11,scale=880:-1:flags=lanczos,split[a][b];[a]palettegen=max_colors=128:stats_mode=diff[p];' +
      '[b][p]paletteuse=dither=bayer:bayer_scale=4:diff_mode=rectangle';
    const r = spawnSync(
      'ffmpeg',
      [
        '-y',
        '-v',
        'error',
        '-ss',
        skip.toFixed(2),
        '-i',
        video as string,
        '-vf',
        filter,
        repo('docs/media/demo.gif'),
      ],
      { encoding: 'utf8' },
    );
    expect(r.status, r.stderr).toBe(0);
    expect(statSync(repo('docs/media/demo.gif')).size).toBeLessThan(8 * 1024 * 1024);
    rmSync(SCRATCH, { recursive: true, force: true });
  });
});

/** Seconds of the recording, which starts before the page has anything to show. */
async function durationOf(file: string): Promise<number> {
  const r = spawnSync(
    'ffprobe',
    ['-v', 'error', '-show_entries', 'format=duration', '-of', 'csv=p=0', file],
    {
      encoding: 'utf8',
    },
  );
  const stated = Number(r.stdout.trim());
  if (Number.isFinite(stated) && stated > 0) return stated;
  // A browser's WebM may state no duration: read the last packet's time.
  const p = spawnSync(
    'ffprobe',
    ['-v', 'error', '-show_entries', 'packet=pts_time', '-of', 'csv=p=0', file],
    { encoding: 'utf8' },
  );
  return Number(p.stdout.trim().split('\n').pop()) || 0;
}
