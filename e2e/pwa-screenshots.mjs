#!/usr/bin/env node
// Takes the web app manifest's screenshots (`apps/web/public/screenshots/`): real windows of
// the app, as the install dialogs show them. Run against the dev server or the Pages emulation:
//
//   node e2e/pwa-screenshots.mjs [base URL]   (default http://localhost:5173/)
//
// Wide shots are 1280 × 800, narrow ones 412 × 915 (the sizes the manifest states).
import { mkdirSync } from 'node:fs';
import { fileURLToPath } from 'node:url';
import { chromium } from '@playwright/test';

const base = process.argv[2] ?? 'http://localhost:5173/';
const out = fileURLToPath(new URL('../apps/web/public/screenshots/', import.meta.url));
mkdirSync(out, { recursive: true });

const SHOTS = [
  { file: 'home-wide.png', hash: '#/home', size: [1280, 800], scheme: 'light' },
  {
    file: 'grid-wide.png',
    hash: '#/grid?template=grid/budget.kgrid&new=1',
    size: [1280, 800],
    scheme: 'light',
  },
  {
    file: 'write-wide-dark.png',
    hash: '#/write?template=write/report.kwrite&new=1',
    size: [1280, 800],
    scheme: 'dark',
  },
  { file: 'home-narrow.png', hash: '#/home', size: [412, 915], scheme: 'light', phone: true },
  {
    file: 'deck-narrow-dark.png',
    hash: '#/deck?template=deck/launch.kdeck&new=1',
    size: [412, 915],
    scheme: 'dark',
    phone: true,
  },
];

const browser = await chromium.launch();
for (const s of SHOTS) {
  const context = await browser.newContext({
    viewport: { width: s.size[0], height: s.size[1] },
    deviceScaleFactor: 1,
    colorScheme: s.scheme,
    hasTouch: !!s.phone,
    isMobile: !!s.phone,
    serviceWorkers: 'block',
  });
  const page = await context.newPage();
  await page.addInitScript(() => localStorage.setItem('kiln.pref.ui.home.tour_seen', '1'));
  await page.goto(new URL(s.hash, base).href);
  if (s.hash === '#/home') await page.getByRole('heading', { level: 1 }).waitFor({ timeout: 60_000 });
  else
    await page
      .getByTestId('document-title')
      .filter({ hasNotText: 'Opening' })
      .first()
      .waitFor({ timeout: 60_000 });
  await page.evaluate(() => document.fonts.ready);
  // The first frames of the document and the theme's fade.
  await page.waitForTimeout(2500);
  await page.screenshot({ path: `${out}${s.file}` });
  console.log(`pwa-screenshots: ${s.file}`);
  await context.close();
}
await browser.close();
