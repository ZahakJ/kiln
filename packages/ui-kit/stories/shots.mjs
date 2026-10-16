#!/usr/bin/env node
// Review sheets of every ui-kit primitive in the light and dark themes (WP-51):
//
//   pnpm --filter @kiln/ui-kit shots
//     -> stories/out/primitives-light.png, stories/out/primitives-dark.png   (one sheet per theme)
//     -> stories/out/<theme>/<part>.png                                       (the individual captures)
//
// Starts the gallery's Vite server, drives Chromium through every primitive (including the ones that
// only exist while open: menus, popovers, select and combobox lists, the colour panel, the expanded
// Styles gallery, tooltips, the dialog and toasts), then lays the captures out on one sheet per theme.
// Fails when the page logs an error or a brand font does not load.
import { spawn } from 'node:child_process';
import { mkdirSync, readFileSync } from 'node:fs';
import { join } from 'node:path';
import { fileURLToPath } from 'node:url';
import { chromium } from 'playwright';

const pkg = fileURLToPath(new URL('..', import.meta.url));
const OUT = join(pkg, 'stories', 'out');
const PORT = 5181;
const base = `http://localhost:${PORT}/`;
const W = 1440;

const server = spawn(
  process.execPath,
  [
    'node_modules/vite/bin/vite.js',
    '--config',
    'stories/vite.config.ts',
    '--port',
    String(PORT),
    '--strictPort',
  ],
  { cwd: pkg, stdio: ['ignore', 'pipe', 'inherit'] },
);
const stop = () => server.exitCode === null && server.kill();
process.on('exit', stop);
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

async function clipAround(page, selector, pad) {
  const box = await page.locator(selector).last().boundingBox();
  if (!box) throw new Error(`${selector} not visible`);
  const vp = page.viewportSize();
  const x = Math.max(0, Math.floor(box.x - pad));
  const y = Math.max(0, Math.floor(box.y - pad));
  return {
    x,
    y,
    width: Math.min(vp.width - x, Math.ceil(box.width + 2 * pad)),
    height: Math.min(vp.height - y, Math.ceil(box.height + 2 * pad)),
  };
}

/** The box around every element matching `selector` (e.g. a menu and its open submenu), padded. */
async function clipAroundAll(page, selector, pad) {
  const boxes = (await page.locator(selector).all()).map((l) => l.boundingBox());
  const rects = (await Promise.all(boxes)).filter(Boolean);
  if (rects.length === 0) throw new Error(`${selector} not visible`);
  const vp = page.viewportSize();
  const x = Math.max(0, Math.floor(Math.min(...rects.map((r) => r.x)) - pad));
  const y = Math.max(0, Math.floor(Math.min(...rects.map((r) => r.y)) - pad));
  const right = Math.min(vp.width, Math.ceil(Math.max(...rects.map((r) => r.x + r.width)) + pad));
  const bottom = Math.min(vp.height, Math.ceil(Math.max(...rects.map((r) => r.y + r.height)) + pad));
  return { x, y, width: right - x, height: bottom - y };
}

/** Captures one theme; returns [{ name, title, file }]. */
async function captureTheme(browser, theme) {
  const dir = join(OUT, theme);
  mkdirSync(dir, { recursive: true });
  const page = await browser.newPage({ viewport: { width: W, height: 900 }, deviceScaleFactor: 1 });
  const errors = [];
  page.on('pageerror', (e) => errors.push(e.message));
  page.on('console', (m) => m.type() === 'error' && errors.push(m.text()));
  await page.goto(`${base}?theme=${theme}&app=write`);
  await page.waitForSelector('.gx-grid');
  const fonts = await page.evaluate(async () => {
    await document.fonts.ready;
    await Promise.all([document.fonts.load('600 13px Figtree'), document.fonts.load('700 20px Fraunces')]);
    return {
      figtree: document.fonts.check('600 13px Figtree'),
      fraunces: document.fonts.check('700 20px Fraunces'),
    };
  });
  if (!fonts.figtree || !fonts.fraunces)
    throw new Error(`[${theme}] brand fonts did not load: ${JSON.stringify(fonts)}`);
  // Wait for the tiles (<img>) to decode.
  await page.evaluate(() => Promise.all([...document.images].map((i) => i.decode().catch(() => undefined))));

  const parts = [];
  const shot = async (name, title, opts) => {
    const file = join(dir, `${name}.png`);
    await page.screenshot({ path: file, ...opts });
    parts.push({ name, title, file });
  };
  const sectionClip = async (label) => {
    const loc = page.locator(`section[aria-label^="${label}"]`);
    await loc.scrollIntoViewIfNeeded();
    return loc
      .screenshot({ path: join(dir, `${slug(label)}.png`) })
      .then(() =>
        parts.push({ name: slug(label), title: label, file: join(dir, `${slug(label)}.png`), section: true }),
      );
  };

  for (const label of [
    'Ribbon sample',
    'Buttons',
    'Inputs',
    'Tabs',
    'Brand glyph set',
    'Menus, popovers, tooltips, dialog',
    'Colour picker',
    'Small things',
  ]) {
    await sectionClip(label);
  }
  await page.evaluate(() => window.scrollTo(0, 0));

  // Menu with an open submenu.
  await page.getByRole('button', { name: 'Edit' }).click();
  await page.getByRole('menuitem', { name: 'Paste Options' }).hover();
  await page.getByRole('menuitemradio', { name: 'Merge Formatting' }).waitFor();
  await page.waitForTimeout(200);
  await shot('menu', 'Menu and submenu', { clip: await clipAroundAll(page, '.kiln-menu', 24) });
  await page.keyboard.press('Escape');
  await page.keyboard.press('Escape');

  // Split button menu (Paste).
  await page.getByRole('button', { name: 'More options for Paste' }).click();
  await page.getByRole('menuitem', { name: 'Keep Text Only' }).waitFor();
  await page.waitForTimeout(200);
  await shot('split-menu', 'Split button menu', { clip: await clipAround(page, '.kiln-menu', 24) });
  await page.keyboard.press('Escape');

  // Colour panel from the Font Colour split button.
  await page.getByRole('button', { name: 'More options for Font Colour' }).click();
  await page.locator('.kiln-popover .kiln-color').waitFor();
  await page.waitForTimeout(200);
  await shot('color-panel', 'Colour picker panel', { clip: await clipAround(page, '.kiln-popover', 24) });
  await page.keyboard.press('Escape');

  // Styles gallery expanded.
  await page.getByRole('button', { name: 'More Styles' }).click();
  await page.locator('.kiln-gallery__panel').waitFor();
  await page.waitForTimeout(200);
  await shot('gallery-open', 'Gallery, expanded', {
    clip: await clipAround(page, '.kiln-gallery__panel', 24),
  });
  await page.keyboard.press('Escape');

  // Font combobox, open and filtered.
  const fontBox = page.getByRole('combobox', { name: 'Font', exact: true });
  await fontBox.click();
  await page.keyboard.press('ControlOrMeta+a');
  await page.keyboard.type('F');
  await page.locator('.kiln-combobox__popup').waitFor();
  await page.waitForTimeout(150);
  await shot('combobox', 'Combobox (font list)', {
    clip: await clipAround(page, '.kiln-combobox__popup', 24),
  });
  await page.keyboard.press('Escape');
  await page.keyboard.press('Escape');

  // Select list.
  const view = page.getByRole('combobox', { name: 'View' });
  await view.scrollIntoViewIfNeeded();
  await view.click();
  await page.locator('.kiln-select__content').waitFor();
  await page.waitForTimeout(200);
  await shot('select', 'Select list', { clip: await clipAround(page, '.kiln-select__content', 24) });
  await page.keyboard.press('Escape');
  await page.evaluate(() => window.scrollTo(0, 0));

  // Popover (Insert Table grid).
  await page
    .locator('section[aria-label^="Menus"]')
    .getByRole('button', { name: 'Table', exact: true })
    .click();
  await page.locator('.kiln-popover').waitFor();
  await page.waitForTimeout(200);
  await shot('popover', 'Popover', { clip: await clipAround(page, '.kiln-popover', 24) });
  await page.keyboard.press('Escape');

  // Super tooltip.
  await page.getByRole('button', { name: 'Bold', exact: true }).first().hover();
  await page.locator('.kiln-tooltip').waitFor();
  await page.waitForTimeout(250);
  await shot('tooltip', 'Super tooltip', { clip: await clipAround(page, '.kiln-tooltip', 40) });
  await page.mouse.move(0, 0);

  // Dialog.
  await page.getByRole('button', { name: 'Paragraph…' }).click();
  await page.getByRole('dialog').waitFor();
  await page.waitForTimeout(300);
  await shot('dialog', 'Dialog', { clip: await clipAround(page, '.kiln-dialog', 32) });
  await page.keyboard.press('Escape');
  await page.waitForTimeout(250);

  // Toasts.
  await page.getByRole('button', { name: 'Success toast' }).click();
  await page.getByRole('button', { name: 'Toast with action' }).click();
  await page.getByRole('button', { name: 'Error toast' }).click();
  await page.waitForTimeout(350);
  await shot('toasts', 'Toasts', { clip: await clipAround(page, '.kiln-toast-list', 16) });

  if (errors.length) throw new Error(`[${theme}] page errors:\n  ${errors.join('\n  ')}`);
  await page.close();
  return parts;
}

const slug = (s) =>
  s
    .toLowerCase()
    .replace(/[^a-z0-9]+/g, '-')
    .replace(/^-|-$/g, '');

/** Lays the captures out on one sheet (a masonry of labelled panels) and screenshots it. */
async function composeSheet(browser, theme, parts) {
  const css = readFileSync(join(pkg, '..', 'tokens', 'tokens.css'), 'utf8');
  const figtree = readFileSync(
    join(pkg, '..', '..', 'assets', 'fonts', 'brand', 'Figtree', 'Figtree-VF.ttf'),
  ).toString('base64');
  const fraunces = readFileSync(
    join(pkg, '..', '..', 'assets', 'fonts', 'brand', 'Fraunces', 'Fraunces-VF.ttf'),
  ).toString('base64');
  const img = (p) => `data:image/png;base64,${readFileSync(p.file).toString('base64')}`;
  const wide = new Set(['ribbon-sample', 'brand-glyph-set']);
  const cards = parts
    .map((p) =>
      p.section
        ? `<figure class="bare ${wide.has(p.name) ? 'wide' : ''}"><img src="${img(p)}"></figure>`
        : `<figure class="${wide.has(p.name) ? 'wide' : ''}"><figcaption>${p.title}</figcaption><img src="${img(p)}"></figure>`,
    )
    .join('');
  const html = `<!doctype html><html data-theme="${theme}" data-app="write"><head><meta charset="utf-8"><style>
${css}
@font-face{font-family:Figtree;font-weight:300 900;src:url(data:font/ttf;base64,${figtree})}
@font-face{font-family:Fraunces;font-weight:100 900;src:url(data:font/ttf;base64,${fraunces})}
html,body{margin:0;background:var(--kiln-bg);color:var(--kiln-fg);font-family:var(--kiln-font-ui)}
header{display:flex;align-items:baseline;gap:16px;padding:28px 32px 8px}
h1{margin:0;font:800 34px/1 var(--kiln-font-display);font-variation-settings:var(--kiln-display-settings);letter-spacing:-.015em}
header p{margin:0;color:var(--kiln-muted);font-size:14px}
main{columns:3;column-gap:16px;padding:16px 32px 32px}
figure{break-inside:avoid;margin:0 0 16px;padding:12px;border:1px solid var(--kiln-line);border-radius:14px;background:var(--kiln-panel)}
figure.wide{column-span:all}
figure.bare{padding:0;border:0;background:none}
figure.bare img{border-radius:14px}
figcaption{margin:0 0 10px;color:var(--kiln-ember-text);font-size:10.5px;font-weight:800;letter-spacing:.12em;text-transform:uppercase}
img{display:block;max-width:100%;height:auto;border-radius:8px}
</style></head><body><header><h1>Kiln primitives</h1><p>${theme === 'dark' ? 'Dark' : 'Light'} theme · Kiln Write · @kiln/ui-kit</p></header><main>${cards}</main></body></html>`;
  const page = await browser.newPage({ viewport: { width: 1600, height: 1000 }, deviceScaleFactor: 1 });
  await page.setContent(html, { waitUntil: 'load' });
  await page.evaluate(async () => {
    await document.fonts.ready;
    await Promise.all([...document.images].map((i) => i.decode()));
  });
  const file = join(OUT, `primitives-${theme}.png`);
  await page.screenshot({ path: file, fullPage: true });
  await page.close();
  return file;
}

const browser = await chromium.launch();
const written = [];
try {
  for (const theme of ['light', 'dark']) {
    const parts = await captureTheme(browser, theme);
    written.push(await composeSheet(browser, theme, parts));
  }
} finally {
  await browser.close();
  stop();
}
for (const f of written) console.log(`ui-kit shots: wrote ${f}`);
