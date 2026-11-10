#!/usr/bin/env node
// Lighthouse on the web app as GitHub Pages serves it: the pinned
// Lighthouse CLI with Playwright's Chromium (CHROME_PATH), against the local Pages emulation
// (tools/pages-serve.mjs, started here) or a deployed site, on a cold first visit to Kiln Home.
//
// Two network profiles, three runs each, medians reported:
//   4g        Lighthouse's desktop preset: its "dense 4G" profile (40 ms round trip, 10 Mbit/s,
//             no CPU slowdown). B12's time to interactive (< 2.5 s) is asserted here.
//   slow-4g   Lighthouse's default mobile profile ("slow 4G": 150 ms, 1.6 Mbit/s, the CPU four
//             times slower), recorded for comparison.
//
// Writes target/pages/lighthouse.json. Usage:
//   node e2e/perf/lighthouse.mjs [--site target/pages/site] [--url <app URL>] [--runs 3]
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
const PORT = 4181;
/** B12: time to interactive on the 4G profile. */
const TTI_BUDGET_MS = 2500;

const require = createRequire(import.meta.url);
const cli = join(dirname(require.resolve('lighthouse/package.json')), 'cli', 'index.js');
const chromePath = chromium.executablePath();

let server = null;
let url = opt('--url', '');
if (!url) {
  server = spawn(
    process.execPath,
    [join(repo, 'tools/pages-serve.mjs'), '--port', String(PORT), '--root', site],
    {
      stdio: 'ignore',
    },
  );
  url = `http://127.0.0.1:${PORT}/kiln/app/`;
  for (let i = 0; i < 50; i++) {
    const ok = await fetch(url).then(
      (r) => r.ok,
      () => false,
    );
    if (ok) break;
    await new Promise((r) => setTimeout(r, 100));
  }
}

const out = join(repo, 'target/pages/lighthouse');
rmSync(out, { recursive: true, force: true });
mkdirSync(out, { recursive: true });

const PROFILES = {
  '4g': ['--preset=desktop'],
  'slow-4g': [],
};

function once(profile, n) {
  const file = join(out, `${profile}-${n}.json`);
  const r = spawnSync(
    process.execPath,
    [
      cli,
      url,
      ...PROFILES[profile],
      '--only-categories=performance',
      '--throttling-method=simulate',
      '--output=json',
      `--output-path=${file}`,
      '--quiet',
      '--chrome-flags=--headless=new --no-sandbox --disable-gpu',
    ],
    { env: { ...process.env, CHROME_PATH: chromePath }, stdio: ['ignore', 'ignore', 'pipe'] },
  );
  if (r.status !== 0) throw new Error(`lighthouse (${profile}) failed: ${r.stderr?.toString().slice(-2000)}`);
  const lhr = JSON.parse(readFileSync(file, 'utf8'));
  const a = lhr.audits;
  const bytes = (type) =>
    a['resource-summary']?.details?.items?.find((i) => i.resourceType === type)?.transferSize ?? 0;
  return {
    interactiveMs: Math.round(a.interactive.numericValue),
    fcpMs: Math.round(a['first-contentful-paint'].numericValue),
    lcpMs: Math.round(a['largest-contentful-paint'].numericValue),
    tbtMs: Math.round(a['total-blocking-time'].numericValue),
    performance: Math.round((lhr.categories.performance.score ?? 0) * 100),
    scriptKb: Math.round(bytes('script') / 1000),
    totalKb: Math.round(bytes('total') / 1000),
  };
}

const median = (values) => {
  const s = [...values].sort((x, y) => x - y);
  return s[Math.floor(s.length / 2)];
};

const results = {};
try {
  for (const profile of Object.keys(PROFILES)) {
    const all = [];
    for (let n = 1; n <= runs; n++) all.push(once(profile, n));
    const pick = (k) => median(all.map((r) => r[k]));
    results[profile] = {
      interactiveMs: pick('interactiveMs'),
      fcpMs: pick('fcpMs'),
      lcpMs: pick('lcpMs'),
      tbtMs: pick('tbtMs'),
      performance: pick('performance'),
      scriptKb: pick('scriptKb'),
      totalKb: pick('totalKb'),
      runs: all,
    };
    const r = results[profile];
    console.log(
      `lighthouse ${profile}: TTI ${r.interactiveMs} ms, FCP ${r.fcpMs} ms, LCP ${r.lcpMs} ms, TBT ${r.tbtMs} ms, ` +
        `performance ${r.performance}, scripts ${r.scriptKb} KB, total ${r.totalKb} KB (median of ${runs})`,
    );
  }
} finally {
  server?.kill();
}
writeFileSync(
  join(repo, 'target/pages/lighthouse.json'),
  `${JSON.stringify({ url, lighthouse: require('lighthouse/package.json').version, results }, null, 2)}\n`,
);
if (results['4g'].interactiveMs >= TTI_BUDGET_MS) {
  console.error(
    `lighthouse: B12 time to interactive on 4G is ${results['4g'].interactiveMs} ms, the limit is ${TTI_BUDGET_MS} ms`,
  );
  process.exit(1);
}
console.log(`lighthouse: B12 time to interactive on 4G under ${TTI_BUDGET_MS} ms`);
