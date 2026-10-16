#!/usr/bin/env node
// Captures review screenshots of the chrome demo (AppFrame, ribbon, Backstage, palette, Key Tips,
// mini toolbar, context menu, Options) in light and dark, desktop and phone.
//
//   node packages/ui-kit/stories/chrome-shots.mjs [outDir]     (default: $TMPDIR/kiln-chrome)
import { spawn } from 'node:child_process';
import { mkdirSync } from 'node:fs';
import { tmpdir } from 'node:os';
import { join } from 'node:path';
import { fileURLToPath } from 'node:url';
import { chromium } from 'playwright';

const out = process.argv[2] ?? join(tmpdir(), 'kiln-chrome');
mkdirSync(out, { recursive: true });
const pkg = fileURLToPath(new URL('..', import.meta.url));
const PORT = 5180;
const base = `http://localhost:${PORT}/?view=chrome`;

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
const errors = [];
const shots = [];
try {
  for (const theme of ['light', 'dark']) {
    const open = async (viewport, query = '') => {
      const page = await browser.newPage({ viewport, deviceScaleFactor: 1 });
      page.on('pageerror', (e) => errors.push(`${theme}: ${e.message}`));
      page.on('console', (m) => m.type() === 'error' && errors.push(`${theme}: ${m.text()}`));
      await page.emulateMedia({ colorScheme: theme === 'dark' ? 'dark' : 'light' });
      await page.goto(`${base}&theme=${theme}${query}`);
      await page.waitForSelector('.kiln-ribbon');
      await page.evaluate(() => document.fonts.ready);
      return page;
    };
    const shot = async (page, name, opts = {}) => {
      const file = join(out, `${theme}-${name}.png`);
      await page.screenshot({ path: file, ...opts });
      shots.push(file);
    };

    const page = await open({ width: 1440, height: 900 });
    await shot(page, 'desktop');
    await shot(page, 'ribbon-home', { clip: { x: 0, y: 0, width: 1440, height: 170 } });
    for (const t of ['Insert', 'Layout', 'Review', 'View']) {
      await page.getByRole('tab', { name: t, exact: true }).click();
      await page.waitForTimeout(150);
      await shot(page, `ribbon-${t.toLowerCase()}`, { clip: { x: 0, y: 0, width: 1440, height: 170 } });
    }
    await page.getByRole('tab', { name: 'Home' }).click();

    // Tooltip.
    await page.locator('[data-command="format.bold"]').first().hover();
    await page.waitForTimeout(800);
    await shot(page, 'tooltip', { clip: { x: 0, y: 0, width: 900, height: 260 } });
    await page.mouse.move(700, 600);

    // Contextual tab: click into the table.
    await page.locator('.cx-table td').first().click();
    await page.waitForTimeout(150);
    await shot(page, 'contextual', { clip: { x: 0, y: 0, width: 1440, height: 170 } });

    // Key Tips.
    await page.locator('.cx-page p').first().click();
    await page.keyboard.press('Alt');
    await page.waitForSelector('.kiln-keytip');
    await shot(page, 'keytips-root', { clip: { x: 0, y: 0, width: 1440, height: 170 } });
    await page.keyboard.press('h');
    await page.waitForTimeout(200);
    await shot(page, 'keytips-home', { clip: { x: 0, y: 0, width: 1440, height: 190 } });
    await page.keyboard.press('Escape');
    await page.keyboard.press('Escape');

    // Mini toolbar on selection.
    const para = page.locator('.cx-page p').first();
    const box = await para.boundingBox();
    await page.mouse.move(box.x + 40, box.y + 8);
    await page.mouse.down();
    await page.mouse.move(box.x + 260, box.y + 8, { steps: 6 });
    await page.mouse.up();
    await page.waitForSelector('.kiln-minitoolbar');
    await page.waitForTimeout(150);
    await shot(page, 'minitoolbar', {
      clip: { x: Math.max(0, box.x - 40), y: Math.max(0, box.y - 90), width: 640, height: 200 },
    });

    // Context menu.
    await page.mouse.click(box.x + 120, box.y + 30, { button: 'right' });
    await page.waitForSelector('.kiln-menu');
    await shot(page, 'contextmenu', {
      clip: { x: Math.max(0, box.x - 20), y: Math.max(0, box.y - 60), width: 520, height: 420 },
    });
    await page.keyboard.press('Escape');

    // Palette.
    await page.keyboard.press('Control+Shift+P');
    await page.waitForSelector('.kiln-palette');
    await shot(page, 'palette-empty');
    await page.keyboard.type('merge');
    await page.waitForTimeout(100);
    await shot(page, 'palette-merge');
    await page.keyboard.press('Escape');

    // Task pane via Find.
    await page.keyboard.press('Control+f');
    await page.waitForSelector('.kiln-taskpane');
    await shot(page, 'taskpane');

    // Backstage.
    await page.getByRole('button', { name: 'File', exact: true }).click();
    await page.waitForSelector('.kiln-backstage');
    await page.waitForTimeout(250);
    await shot(page, 'backstage');
    await page.keyboard.press('Escape');

    // Options ▸ Keyboard.
    await page.keyboard.press('Control+Shift+P');
    await page.keyboard.type('options');
    await page.keyboard.press('Enter');
    await page.waitForSelector('.kiln-options');
    await shot(page, 'options-general');
    await page.getByRole('button', { name: 'Keyboard', exact: true }).click();
    await page.locator('[data-command-row="format.bold"]').click();
    await page.waitForTimeout(100);
    await shot(page, 'options-keyboard');
    await page.keyboard.press('Escape');

    // Zoom popover.
    await page.getByRole('button', { name: /Zoom level/ }).click();
    await page.waitForSelector('.kiln-statusbar__zoom-popover');
    await shot(page, 'zoom', { clip: { x: 1440 - 420, y: 900 - 360, width: 420, height: 360 } });
    await page.keyboard.press('Escape');
    await page.close();

    // Simplified (narrow desktop), collapsed, phone.
    const narrow = await open({ width: 1024, height: 720 });
    await shot(narrow, 'simplified');
    await narrow.keyboard.press('Control+F1');
    await narrow.waitForTimeout(150);
    await shot(narrow, 'collapsed', { clip: { x: 0, y: 0, width: 1024, height: 200 } });
    await narrow.close();

    const phone = await open({ width: 390, height: 844 });
    await shot(phone, 'phone');
    const overflow = await phone.evaluate(() => document.documentElement.scrollWidth > window.innerWidth);
    if (overflow) errors.push(`${theme}: horizontal overflow on phone`);
    await phone.close();
  }
} finally {
  await browser.close();
  server.kill();
}
console.log(shots.join('\n'));
if (errors.length) {
  console.error(`\n${errors.length} page error(s):\n${errors.join('\n')}`);
  process.exitCode = 1;
}
