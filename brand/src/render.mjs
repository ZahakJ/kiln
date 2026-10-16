// usage: node render.mjs jobs.json   (jobs: [{in, out, w, h, scale?, scheme?, transparent?}])
//    or: node render.mjs in out w h [scale] [scheme]
import { createRequire } from 'module';
import fs from 'fs';
import path from 'path';
const require = createRequire('/home/muth/OfficeSuite/node_modules/.pnpm/playwright@1.64.1/node_modules/playwright/package.json');
const { chromium } = require('playwright');
let jobs;
const a = process.argv.slice(2);
if (a.length === 1) jobs = JSON.parse(fs.readFileSync(a[0], 'utf8'));
else jobs = [{ in: a[0], out: a[1], w: +a[2], h: +a[3], scale: +(a[4] || 1), scheme: a[5] || 'light' }];
const browser = await chromium.launch();
for (const j of jobs) {
  const page = await browser.newPage({ viewport: { width: j.w, height: j.h }, deviceScaleFactor: j.scale || 1, colorScheme: j.scheme || 'light' });
  await page.goto('file://' + path.resolve(j.in));
  await page.evaluate(() => document.fonts.ready);
  await page.waitForTimeout(120);
  await page.screenshot({ path: j.out, omitBackground: j.transparent !== false, clip: { x: 0, y: 0, width: j.w, height: j.h } });
  await page.close();
}
await browser.close();
