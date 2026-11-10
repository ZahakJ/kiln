#!/usr/bin/env node
// Lighthouse on the landing page and a manual page: all four categories, the mobile and the
// desktop presets, three runs each, medians reported, against the local Pages emulation
// (tools/pages-serve.mjs, started here) or a deployed site. Fails under 95.
//
//   node e2e/perf/lighthouse-site.mjs [--site target/pages/site] [--url <site URL>] [--runs 3]
//
// Writes target/pages/lighthouse-site.json.
import { spawn, spawnSync } from 'node:child_process';
import { mkdirSync, readFileSync, rmSync, writeFileSync } from 'node:fs';
import { createRequire } from 'node:module';
import { dirname, join, resolve } from 'node:path';
import { fileURLToPath } from 'node:url';
import { chromium } from '@playwright/test';

const repo = fileURLToPath(new URL('../..', import.meta.url));
const args = process.argv.slice(2);
const opt = (name, fallback) => {
  const i = args.indexOf(name);
  return i >= 0 && args[i + 1] ? args[i + 1] : fallback;
};
const site = resolve(repo, opt('--site', 'target/pages/site'));
const runs = Number(opt('--runs', '3'));
const PORT = 4182;
const MINIMUM = 95;
const CATEGORIES = ['performance', 'accessibility', 'best-practices', 'seo'];

const require = createRequire(import.meta.url);
const cli = join(dirname(require.resolve('lighthouse/package.json')), 'cli', 'index.js');

let server = null;
let base = opt('--url', '');
if (!base) {
  server = spawn(
    process.execPath,
    [join(repo, 'tools/pages-serve.mjs'), '--port', String(PORT), '--root', site],
    { stdio: 'ignore' },
  );
  base = `http://127.0.0.1:${PORT}/kiln/`;
  for (let i = 0; i < 50; i++) {
    if (
      await fetch(base).then(
        (r) => r.ok,
        () => false,
      )
    )
      break;
    await new Promise((r) => setTimeout(r, 100));
  }
}

const out = join(repo, 'target/pages/lighthouse-site');
rmSync(out, { recursive: true, force: true });
mkdirSync(out, { recursive: true });
const PAGES = { landing: '', manual: 'manual/?p=getting-started' };
const PRESETS = { mobile: [], desktop: ['--preset=desktop'] };

function once(url, preset, file) {
  const r = spawnSync(
    process.execPath,
    [
      cli,
      url,
      ...PRESETS[preset],
      `--only-categories=${CATEGORIES.join(',')}`,
      '--throttling-method=simulate',
      '--output=json',
      `--output-path=${file}`,
      '--quiet',
      '--chrome-flags=--headless=new --no-sandbox --disable-gpu',
    ],
    { env: { ...process.env, CHROME_PATH: chromium.executablePath() }, stdio: 'inherit' },
  );
  if (r.status !== 0) throw new Error(`lighthouse exited with ${r.status} on ${url}`);
  return JSON.parse(readFileSync(file, 'utf8'));
}

const median = (list) => [...list].sort((a, b) => a - b)[Math.floor(list.length / 2)];
const results = {};
let failed = false;
try {
  for (const [page, path] of Object.entries(PAGES))
    for (const preset of Object.keys(PRESETS)) {
      const reports = [];
      for (let n = 0; n < runs; n++)
        reports.push(once(new URL(path, base).href, preset, join(out, `${page}-${preset}-${n}.json`)));
      const row = {};
      for (const c of CATEGORIES)
        row[c] = median(reports.map((r) => Math.round(r.categories[c].score * 100)));
      row.lcpMs = Math.round(median(reports.map((r) => r.audits['largest-contentful-paint'].numericValue)));
      row.cls = median(reports.map((r) => r.audits['cumulative-layout-shift'].numericValue));
      results[`${page} (${preset})`] = row;
      console.log(`${page} (${preset}): ${JSON.stringify(row)}`);
      for (const c of CATEGORIES) {
        if (row[c] >= MINIMUM) continue;
        failed = true;
        const audits = Object.values(reports[0].audits)
          .filter((a) => a.score !== null && a.score < 0.9 && a.scoreDisplayMode !== 'informative')
          .map((a) => a.id);
        console.error(`${page} (${preset}): ${c} ${row[c]} < ${MINIMUM}: ${audits.slice(0, 12).join(', ')}`);
      }
    }
} finally {
  server?.kill();
}
writeFileSync(join(repo, 'target/pages/lighthouse-site.json'), `${JSON.stringify(results, null, 2)}\n`);
process.exit(failed ? 1 : 0);
