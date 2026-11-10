// SPDX-License-Identifier: AGPL-3.0-or-later
/**
 * B4 and B6 for Kiln Write (docs/qa/perf.md): opens fixtures/large/doc-300p.docx in the production
 * web build, reads the memory of the browser's processes (PSS from /proc, Linux), scrolls at
 * 2,000 px/s for 10 s (frame interval and the surface's `kiln:frame` work), then visits every
 * page and reads the memory again after a garbage collection.
 *
 *   pnpm --filter @kiln/web build && pnpm --filter @kiln/web preview &
 *   node e2e/perf/write-300p.mjs   # KILN_URL=http://localhost:4173
 */
import { readdirSync, readFileSync } from 'node:fs';
import { fileURLToPath } from 'node:url';
import { chromium } from '@playwright/test';

const base = process.env.KILN_URL ?? 'http://localhost:4173';
const fixture = fileURLToPath(new URL('../../fixtures/large/doc-300p.docx', import.meta.url));

/** This process's descendants (the browser and its helpers). */
function descendants(root) {
  const kids = new Map();
  for (const d of readdirSync('/proc')) {
    if (!/^\d+$/.test(d)) continue;
    try {
      const st = readFileSync(`/proc/${d}/stat`, 'utf8');
      const ppid = Number(st.slice(st.lastIndexOf(')') + 2).split(' ')[1]);
      if (!kids.has(ppid)) kids.set(ppid, []);
      kids.get(ppid).push(Number(d));
    } catch {}
  }
  const out = [];
  const walk = (p) => {
    for (const k of kids.get(p) ?? []) {
      out.push(k);
      walk(k);
    }
  };
  walk(root);
  return out;
}

function memory() {
  const procs = descendants(process.pid)
    .map((pid) => {
      try {
        const r = readFileSync(`/proc/${pid}/smaps_rollup`, 'utf8');
        const cmd = readFileSync(`/proc/${pid}/cmdline`, 'utf8');
        return {
          pss: Number(/^Pss:\s+(\d+)/m.exec(r)?.[1] ?? 0) / 1024,
          type: /--type=([a-z-]+)/.exec(cmd)?.[1] ?? 'browser',
        };
      } catch {
        return null;
      }
    })
    .filter((p) => p !== null);
  const renderer = Math.max(0, ...procs.filter((p) => p.type === 'renderer').map((p) => p.pss));
  const total = procs.reduce((s, p) => s + p.pss, 0);
  return { renderer_mb: Math.round(renderer), all_processes_mb: Math.round(total) };
}

const browser = await chromium.launch({ args: ['--js-flags=--expose-gc'] });
const context = await browser.newContext({ viewport: { width: 1440, height: 900 } });
const page = await context.newPage();
const cdp = await context.newCDPSession(page);
const settle = async () => {
  await cdp.send('HeapProfiler.collectGarbage');
  for (const w of page.workers()) await w.evaluate(() => globalThis.gc?.()).catch(() => undefined);
  await page.waitForTimeout(1500);
};
const core = async () => {
  for (const w of page.workers()) {
    const b = await w.evaluate(() => globalThis.kilnCoreBytes?.() ?? 0).catch(() => 0);
    if (b) return Math.round(b / 1048576);
  }
  return 0;
};

await page.goto(`${base}/#/home`);
await page
  .getByText(/Your recent files will gather here|Recent/)
  .first()
  .waitFor({ timeout: 60_000 });
await page.evaluate(() => {
  delete window.showOpenFilePicker;
  delete window.showSaveFilePicker;
});
await settle();
const home = memory();
const [chooser] = await Promise.all([page.waitForEvent('filechooser'), page.keyboard.press('Control+o')]);
const t0 = Date.now();
await chooser.setFiles({
  name: 'doc-300p.docx',
  mimeType: 'application/octet-stream',
  buffer: readFileSync(fixture),
});
await page.waitForFunction(() => [...document.querySelectorAll('canvas')].some((c) => c.width > 300), null, {
  timeout: 60_000,
});
const firstPaint = Date.now() - t0;
await page.waitForTimeout(3000);
await settle();
const open = { ...memory(), core_wasm_mb: await core() };

const scroll = await page.evaluate(async () => {
  const scroller = document.querySelector('[data-testid="kiln-surface-scroller"]');
  const work = [];
  const obs = new PerformanceObserver((l) => {
    for (const e of l.getEntries()) if (e.name === 'kiln:frame') work.push(e.duration);
  });
  obs.observe({ type: 'measure', buffered: false });
  const intervals = [];
  await new Promise((done) => {
    let last = performance.now();
    const start = last;
    const step = (t) => {
      intervals.push(t - last);
      scroller.scrollTop += (2000 * (t - last)) / 1000;
      last = t;
      if (t - start < 10_000) requestAnimationFrame(step);
      else done();
    };
    requestAnimationFrame(step);
  });
  await new Promise((r) => setTimeout(r, 200));
  obs.disconnect();
  const q = (a, p) => {
    const s = [...a].sort((x, y) => x - y);
    return Math.round((s[Math.min(s.length - 1, Math.floor(s.length * p))] ?? 0) * 100) / 100;
  };
  const iv = intervals.slice(5);
  return {
    frames: iv.length,
    interval_p50_ms: q(iv, 0.5),
    interval_p95_ms: q(iv, 0.95),
    over_25ms: iv.filter((i) => i > 25).length,
    work_p50_ms: q(work, 0.5),
    work_p95_ms: q(work, 0.95),
    scrolled_px: Math.round(scroller.scrollTop),
  };
});
const scrolled = memory();
await page.evaluate(async () => {
  const s = document.querySelector('[data-testid="kiln-surface-scroller"]');
  for (let y = 0; y < s.scrollHeight; y += s.clientHeight) {
    s.scrollTop = y;
    await new Promise((r) => requestAnimationFrame(() => requestAnimationFrame(r)));
  }
});
await settle();
const everyPage = { ...memory(), core_wasm_mb: await core() };
console.log(
  JSON.stringify(
    {
      first_paint_ms: firstPaint,
      home,
      open,
      scroll,
      after_scrolling: scrolled,
      after_every_page: everyPage,
    },
    null,
    1,
  ),
);
await browser.close();
