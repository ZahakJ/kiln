#!/usr/bin/env node
// Checks the GitHub Pages artefact that tools/pages-build.sh assembled:
//
//   chrome JS   the scripts a window loads to become interactive, gzipped, from Vite's manifest:
//               the page's entry and its static imports (with the installed app's notices); each
//               app window adds the editors' chunk. The first must stay under 450 KB (B12), and
//               an app window's total under a ceiling of 820 KB so it cannot grow unseen.
//   wasm        every core module under 4,500 KB gzipped (B12), the collaboration module under
//               2,000 KB; the threads core is among them.
//   manifest    installable: name, start URL, scope, standalone display, theme colour, PNG icons
//               of 192 and 512 px (any and maskable), screenshots of the stated sizes, file
//               handlers with their icons, the share target; every file it names exists.
//   worker      sw.js exists, carries the precache list, and index.html links the manifest under
//               the Pages base path; its scope is the app, so the landing page and the manual stay
//               outside it.
//   site        the landing page names the apps, every image it loads is in the artefact, each is
//               small enough for a phone, demo.gif is under 8 MB, and the manual has its pages.
//
// Writes the numbers to target/pages/report.json. Usage: node tools/pages-check.mjs [site dir]
import { existsSync, readdirSync, readFileSync, statSync, writeFileSync } from 'node:fs';
import { join, resolve } from 'node:path';
import { fileURLToPath } from 'node:url';
import { gzipSync } from 'node:zlib';

const repo = fileURLToPath(new URL('..', import.meta.url));
const site = resolve(repo, process.argv[2] ?? 'target/pages/site');
const app = join(site, 'app');
const BASE = '/kiln/app/';
const CHROME_BUDGET_KB = 450;
const APP_WINDOW_CEILING_KB = 820;
const WASM_BUDGET_KB = 4500;
const COLLAB_BUDGET_KB = 2000;

const failures = [];
const fail = (message) => failures.push(message);
const kb = (bytes) => Math.ceil(bytes / 1000);
const gz = (file) => gzipSync(readFileSync(file), { level: 9 }).length;

// ── Chrome JS, from Vite's manifest (`build.manifest` in the pages mode).
const manifestFile = join(app, '.vite', 'manifest.json');
if (!existsSync(manifestFile)) {
  console.error(`pages-check: ${manifestFile} is missing (build with vite build --mode pages)`);
  process.exit(1);
}
const vite = JSON.parse(readFileSync(manifestFile, 'utf8'));
/** The files of a chunk and its static imports. */
function closure(keys) {
  const seen = new Set();
  const files = new Set();
  const visit = (key) => {
    if (seen.has(key) || !vite[key]) return;
    seen.add(key);
    const chunk = vite[key];
    if (chunk.file.endsWith('.js')) files.add(chunk.file);
    for (const imp of chunk.imports ?? []) visit(imp);
  };
  for (const key of keys) visit(key);
  return files;
}
const sum = (files) => [...files].reduce((n, f) => n + gz(join(app, f)), 0);
const entry = Object.keys(vite).find((k) => vite[k].isEntry && k.endsWith('index.html'));
if (!entry) fail('no entry chunk for index.html in the Vite manifest');
const pwaKeys = Object.keys(vite).filter((k) => /src\/pwa\/(index\.ts|PwaLayer\.tsx)$/.test(k));
if (pwaKeys.length < 2) fail('the Pages build has no service worker client (src/pwa)');
const initial = closure([entry, ...pwaKeys]);
const frameKey = Object.keys(vite).find((k) => k.endsWith('src/routes/app-frame/index.tsx'));
const windowFiles = closure([entry, ...pwaKeys, ...(frameKey ? [frameKey] : [])]);
const chrome = { initialKb: kb(sum(initial)), appWindowKb: kb(sum(windowFiles)), files: [...initial] };
if (chrome.initialKb >= CHROME_BUDGET_KB)
  fail(
    `B12: the page's first scripts are ${chrome.initialKb} KB gzipped, the limit is ${CHROME_BUDGET_KB} KB`,
  );
if (chrome.appWindowKb >= APP_WINDOW_CEILING_KB)
  fail(
    `an app window's scripts are ${chrome.appWindowKb} KB gzipped, the ceiling is ${APP_WINDOW_CEILING_KB} KB`,
  );

// ── WebAssembly modules.
const assets = join(app, 'assets');
const wasm = readdirSync(assets)
  .filter((f) => f.endsWith('.wasm'))
  .map((f) => ({ file: f, gzipKb: kb(gz(join(assets, f))) }));
if (wasm.filter((w) => w.file.startsWith('kiln_bridge_wasm_bg')).length < 4)
  fail('expected four core modules (Write, Grid, Grid with threads, Deck)');
for (const w of wasm) {
  const collab = w.file.startsWith('kiln_collab_wasm_bg');
  const budget = collab ? COLLAB_BUDGET_KB : WASM_BUDGET_KB;
  if (w.gzipKb >= budget) fail(`${w.file}: ${w.gzipKb} KB gzipped, the limit is ${budget} KB`);
}

// ── The web app manifest.
const pngSize = (file) => {
  const b = readFileSync(file);
  return b.toString('ascii', 1, 4) === 'PNG' ? `${b.readUInt32BE(16)}x${b.readUInt32BE(20)}` : null;
};
const exists = (rel) => existsSync(join(app, rel));
const webManifest = JSON.parse(readFileSync(join(app, 'manifest.webmanifest'), 'utf8'));
for (const key of ['name', 'short_name', 'start_url', 'scope', 'id', 'theme_color', 'background_color'])
  if (!webManifest[key]) fail(`manifest: no ${key}`);
if (webManifest.display !== 'standalone') fail('manifest: display is not standalone');
for (const [size, purpose] of [
  ['192x192', 'any'],
  ['512x512', 'any'],
  ['192x192', 'maskable'],
  ['512x512', 'maskable'],
]) {
  const icon = webManifest.icons.find(
    (i) => i.sizes === size && i.purpose === purpose && i.type === 'image/png',
  );
  if (!icon) fail(`manifest: no ${purpose} PNG icon of ${size}`);
  else if (!exists(icon.src) || pngSize(join(app, icon.src)) !== size)
    fail(`manifest: ${icon.src} is not a ${size} PNG`);
}
for (const shot of webManifest.screenshots ?? []) {
  if (!exists(shot.src)) fail(`manifest: screenshot ${shot.src} is missing`);
  else if (pngSize(join(app, shot.src)) !== shot.sizes) fail(`manifest: ${shot.src} is not ${shot.sizes}`);
}
const forms = new Set((webManifest.screenshots ?? []).map((s) => s.form_factor));
if (!forms.has('wide') || !forms.has('narrow')) fail('manifest: screenshots need a wide and a narrow one');
for (const handler of webManifest.file_handlers ?? [])
  for (const icon of handler.icons ?? [])
    if (!exists(icon.src)) fail(`manifest: file handler icon ${icon.src} is missing`);
for (const ext of ['.docx', '.xlsx', '.pptx', '.kwrite', '.kgrid', '.kdeck', '.csv'])
  if (!(webManifest.file_handlers ?? []).some((h) => Object.values(h.accept).flat().includes(ext)))
    fail(`manifest: no file handler for ${ext}`);
if (webManifest.share_target?.method !== 'POST') fail('manifest: no share target');

// ── The service worker and the page.
const sw = existsSync(join(app, 'sw.js')) ? readFileSync(join(app, 'sw.js'), 'utf8') : '';
if (!sw) fail('sw.js is missing');
else if (sw.includes('self.__WB_MANIFEST') || !sw.includes('kiln_bridge_wasm_bg'))
  fail('sw.js has no precache list');
const html = readFileSync(join(app, 'index.html'), 'utf8');
if (!html.includes(`href="${BASE}manifest.webmanifest"`))
  fail(`index.html does not link ${BASE}manifest.webmanifest`);
if (!html.includes(`src="${BASE}assets/`)) fail(`index.html does not load its scripts from ${BASE}assets/`);
if (!existsSync(join(site, 'index.html')) || !existsSync(join(site, 'manual', 'index.html')))
  fail('the landing page or the manual is missing');

// ── The landing page and the manual.
// The manifest sits in app/, so "./" is /kiln/app/; `pwa-site.spec.ts` checks the worker's own scope.
if (new URL(webManifest.scope, `https://x${BASE}`).pathname !== BASE)
  fail(`manifest: the scope ${webManifest.scope} is not ${BASE}`);
const landing = readFileSync(join(site, 'index.html'), 'utf8');
for (const name of ['Kiln Write', 'Kiln Grid', 'Kiln Deck'])
  if (!landing.includes(name)) fail(`the landing page does not name ${name}`);
const pictures = new Set(
  [...landing.matchAll(/(?:src|srcset)="([^"]+)"/g)].flatMap((m) =>
    m[1].split(',').map((part) => part.trim().split(' ')[0]),
  ),
);
const PICTURE_KB = 260;
let pictureCount = 0;
for (const file of pictures) {
  if (/^https?:/.test(file) || file === 'media/demo.gif') continue;
  pictureCount += 1;
  if (!existsSync(join(site, file))) fail(`the landing page loads ${file}, which is missing`);
  else if (file.endsWith('.webp') && statSync(join(site, file)).size > PICTURE_KB * 1000)
    fail(`${file} is over ${PICTURE_KB} KB`);
}
if (!pictures.has('media/banner-1400.webp')) fail('the landing page has no brand banner');
const gif = join(site, 'media', 'demo.gif');
if (!existsSync(gif) || statSync(gif).size >= 8 * 1024 * 1024) fail('demo.gif is missing or not under 8 MB');
const manualPages = readdirSync(join(site, 'manual')).filter((f) => f.endsWith('.md'));
if (manualPages.length < 14) fail(`the manual has ${manualPages.length} pages, at least 14 are required`);

// Engineering notes and brand mockups stay out of the published site.
for (const banned of ['adr', 'design', 'qa', 'RELEASING.md'])
  if (existsSync(join(site, banned))) fail(`the artefact carries ${banned}`);
if (readdirSync(site).some((f) => f.endsWith('.md'))) fail('the artefact carries engineering notes (.md)');
if (readdirSync(join(site, 'media')).some((f) => /mock/i.test(f))) fail('the artefact carries a mockup');

const report = {
  chrome,
  wasm,
  swBytes: sw.length,
  site: { pictures: pictureCount, manualPages: manualPages.length },
};
writeFileSync(resolve(site, '..', 'report.json'), `${JSON.stringify(report, null, 2)}\n`);
console.log(`chrome JS gzip: ${chrome.initialKb} KB (first scripts, budget ${CHROME_BUDGET_KB} KB)`);
console.log(`chrome JS gzip: ${chrome.appWindowKb} KB (an app window, with the editors)`);
for (const w of wasm) console.log(`wasm gzip: ${w.gzipKb} KB (${w.file})`);
if (failures.length > 0) {
  for (const f of failures) console.error(`pages-check: ${f}`);
  process.exit(1);
}
console.log(`site: ${pictureCount} pictures, ${manualPages.length} manual pages`);
console.log('pages-check: the artefact is complete and within its budgets');
