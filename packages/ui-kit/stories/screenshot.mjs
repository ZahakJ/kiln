#!/usr/bin/env node
// Captures review screenshots of the ui-kit gallery (light, dark, high contrast; desktop and phone;
// open menus, panels and dialogs). Starts the gallery's Vite server, drives Chromium, writes PNGs.
//
//   node packages/ui-kit/stories/screenshot.mjs [outDir]     (default: $TMPDIR/kiln-ui-gallery)
import { spawn } from 'node:child_process';
import { mkdirSync } from 'node:fs';
import { tmpdir } from 'node:os';
import { join } from 'node:path';
import { fileURLToPath } from 'node:url';
import { chromium } from 'playwright';

const out = process.argv[2] ?? join(tmpdir(), 'kiln-ui-gallery');
mkdirSync(out, { recursive: true });
const pkg = fileURLToPath(new URL('..', import.meta.url));
const PORT = 5180;
const base = `http://localhost:${PORT}/`;

const server = spawn(
  process.execPath,
  ['node_modules/vite/bin/vite.js', '--config', 'stories/vite.config.ts'],
  {
    cwd: pkg,
    stdio: ['ignore', 'pipe', 'inherit'],
  },
);
await new Promise((resolve, reject) => {
  const timer = setTimeout(() => reject(new Error('vite did not start in 60 s')), 60_000);
  server.stdout.on('data', (d) => {
    if (String(d).includes('Local:')) {
      clearTimeout(timer);
      resolve();
    }
  });
  server.on('exit', (code) => reject(new Error(`vite exited with ${code}`)));
});

const browser = await chromium.launch();
const shots = [];
try {
  for (const theme of ['light', 'dark', 'hc']) {
    const page = await browser.newPage({ viewport: { width: 1440, height: 900 }, deviceScaleFactor: 1 });
    const errors = [];
    page.on('pageerror', (e) => errors.push(e.message));
    page.on('console', (m) => m.type() === 'error' && errors.push(m.text()));
    await page.goto(`${base}?theme=${theme}`);
    await page.waitForSelector('.gx-grid');
    await page.evaluate(() => document.fonts.ready);
    const shot = async (name, opts = {}) => {
      const file = join(out, `${theme}-${name}.png`);
      await page.screenshot({ path: file, ...opts });
      shots.push(file);
    };
    await shot('full', { fullPage: true });

    // Menu with submenu.
    await page.getByRole('button', { name: 'Edit' }).click();
    await page.getByRole('menuitem', { name: 'Paste Options' }).hover();
    await page.getByRole('menuitemradio', { name: 'Merge Formatting' }).waitFor();
    await shot('menu', { clip: await clipAround(page, '.kiln-menu', 40) });
    await page.keyboard.press('Escape');
    await page.keyboard.press('Escape');

    // Font colour panel from the ribbon split button.
    await page.getByRole('button', { name: 'More options for Font Colour' }).click();
    await page.locator('.kiln-popover .kiln-color').waitFor();
    await shot('colorpanel', { clip: await clipAround(page, '.kiln-popover', 60) });
    await page.keyboard.press('Escape');

    // Paste split menu.
    await page.getByRole('button', { name: 'More options for Paste' }).click();
    await page.getByRole('menuitem', { name: 'Keep Text Only' }).waitFor();
    await shot('paste', { clip: await clipAround(page, '.kiln-menu', 60) });
    await page.keyboard.press('Escape');

    // Styles gallery expanded.
    await page.getByRole('button', { name: 'More Styles' }).click();
    await page.locator('.kiln-gallery__panel').waitFor();
    await shot('gallery', { clip: await clipAround(page, '.kiln-gallery__panel', 40) });
    await page.keyboard.press('Escape');

    // Font combobox open and filtered.
    const fontBox = page.getByRole('combobox', { name: 'Font', exact: true });
    await fontBox.click();
    await page.keyboard.type('Lib');
    await page.locator('.kiln-combobox__popup').waitFor();
    await shot('combobox', { clip: await clipAround(page, '.kiln-combobox__popup', 60) });
    await page.keyboard.press('Escape');
    await page.keyboard.press('Escape');

    // Super tooltip.
    await page.getByRole('button', { name: 'Bold', exact: true }).first().hover();
    await page.locator('.kiln-tooltip').waitFor();
    await shot('tooltip', { clip: await clipAround(page, '.kiln-tooltip', 80) });
    await page.mouse.move(0, 0);

    // Dialog.
    await page.getByRole('button', { name: 'Paragraph…' }).click();
    await page.getByRole('dialog').waitFor();
    await page.waitForTimeout(250);
    await shot('dialog');
    await page.keyboard.press('Escape');

    // Toasts.
    await page.getByRole('button', { name: 'Success toast' }).click();
    await page.getByRole('button', { name: 'Toast with action' }).click();
    await page.getByRole('button', { name: 'Error toast' }).click();
    await page.waitForTimeout(250);
    await shot('toasts', { clip: { x: 1440 - 420, y: 900 - 300, width: 420, height: 300 } });

    if (errors.length) console.warn(`[${theme}] page errors:\n  ${errors.join('\n  ')}`);
    await page.close();
  }

  // Theme matrix and phone width.
  const matrix = await browser.newPage({ viewport: { width: 1440, height: 640 } });
  await matrix.goto(`${base}?view=matrix&app=grid`);
  await matrix.waitForSelector('.gx-matrix');
  await matrix.screenshot({ path: join(out, 'matrix-grid.png'), fullPage: true });
  shots.push(join(out, 'matrix-grid.png'));
  await matrix.goto(`${base}?view=matrix&app=deck`);
  await matrix.waitForSelector('.gx-matrix');
  await matrix.screenshot({ path: join(out, 'matrix-deck.png'), fullPage: true });
  shots.push(join(out, 'matrix-deck.png'));
  await matrix.close();

  for (const theme of ['light', 'dark']) {
    const phone = await browser.newPage({
      viewport: { width: 390, height: 844 },
      deviceScaleFactor: 2,
      isMobile: true,
      hasTouch: true,
    });
    await phone.goto(`${base}?theme=${theme}`);
    await phone.waitForSelector('.gx-grid');
    const file = join(out, `phone-${theme}.png`);
    await phone.screenshot({ path: file });
    shots.push(file);
    const overflow = await phone.evaluate(() => document.documentElement.scrollWidth - window.innerWidth);
    if (overflow > 0) console.warn(`phone-${theme}: page overflows horizontally by ${overflow}px`);
    await phone.close();
  }

  const colorful = await browser.newPage({ viewport: { width: 1440, height: 900 } });
  await colorful.goto(`${base}?theme=light&icons=colorful&app=deck`);
  await colorful.waitForSelector('.gx-glyphs');
  const glyphs = colorful.locator('.gx-glyphs');
  await glyphs.screenshot({ path: join(out, 'glyphs-colorful.png') });
  shots.push(join(out, 'glyphs-colorful.png'));
  await colorful.close();
} finally {
  await browser.close();
  server.kill();
}
console.log(`ui-kit gallery: ${shots.length} screenshots in ${out}`);

async function clipAround(page, selector, pad) {
  const box = await page.locator(selector).last().boundingBox();
  if (!box) throw new Error(`${selector} not visible`);
  const vp = page.viewportSize();
  const x = Math.max(0, box.x - pad);
  const y = Math.max(0, box.y - pad);
  return {
    x,
    y,
    width: Math.min(vp.width - x, box.width + 2 * pad),
    height: Math.min(vp.height - y, box.height + 2 * pad),
  };
}
