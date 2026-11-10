#!/usr/bin/env node
// Grid's threads module against its single-threaded one: the same workbook opened and
// recalculated in a Worker of headless Chromium, on a cross-origin isolated page, with the
// modules `cargo xtask wasm --release` writes (apps/web/src/wasm/grid and grid-threads).
//
// The workbook is the B8 recalculation workbook of kiln-bench (one input column and ten formula
// columns per row: arithmetic, IF, AND, row sums and a sliding ten-row SUM), with no cached values,
// so opening it calculates every formula. Calculate Now (F9) then runs seven times.
//
// Usage: node e2e/perf/grid-threads.mjs [--rows 50000] [--threads 4]
// Prints open and recalculation times (median) for each module and writes
// target/pages/grid-threads.json.
import { mkdirSync, readFileSync, statSync, writeFileSync } from 'node:fs';
import { createServer } from 'node:http';
import { extname, join, normalize } from 'node:path';
import { fileURLToPath } from 'node:url';
import { crc32 } from 'node:zlib';
import { chromium } from '@playwright/test';

const repo = fileURLToPath(new URL('../..', import.meta.url));
const args = process.argv.slice(2);
const opt = (name, fallback) => {
  const i = args.indexOf(name);
  return i >= 0 && args[i + 1] ? Number(args[i + 1]) : fallback;
};
const ROWS = opt('--rows', 50_000);
const THREADS = opt('--threads', 4);
const wasmDir = join(repo, 'apps/web/src/wasm');

// ── The workbook: a stored (uncompressed) zip of the minimal SpreadsheetML parts.
function zip(files) {
  const locals = [];
  const central = [];
  let offset = 0;
  for (const [name, text] of files) {
    const data = Buffer.from(text, 'utf8');
    const nameBytes = Buffer.from(name, 'utf8');
    const crc = crc32(data);
    const local = Buffer.alloc(30);
    local.writeUInt32LE(0x04034b50, 0);
    local.writeUInt16LE(20, 4);
    local.writeUInt32LE(crc, 14);
    local.writeUInt32LE(data.length, 18);
    local.writeUInt32LE(data.length, 22);
    local.writeUInt16LE(nameBytes.length, 26);
    const entry = Buffer.alloc(46);
    entry.writeUInt32LE(0x02014b50, 0);
    entry.writeUInt16LE(20, 4);
    entry.writeUInt16LE(20, 6);
    entry.writeUInt32LE(crc, 16);
    entry.writeUInt32LE(data.length, 20);
    entry.writeUInt32LE(data.length, 24);
    entry.writeUInt16LE(nameBytes.length, 28);
    entry.writeUInt32LE(offset, 42);
    locals.push(local, nameBytes, data);
    central.push(entry, nameBytes);
    offset += 30 + nameBytes.length + data.length;
  }
  const size = central.reduce((n, b) => n + b.length, 0);
  const end = Buffer.alloc(22);
  end.writeUInt32LE(0x06054b50, 0);
  end.writeUInt16LE(files.length, 8);
  end.writeUInt16LE(files.length, 10);
  end.writeUInt32LE(size, 12);
  end.writeUInt32LE(offset, 16);
  return Buffer.concat([...locals, ...central, end]);
}

function workbook(rows) {
  let seed = 7;
  const next = () => {
    seed = (seed * 1103515245 + 12345) % 2 ** 31;
    return seed % 1000;
  };
  const cols = 'BCDEFGHIJK';
  const parts = [];
  for (let r = 1; r <= rows; r++) {
    const r0 = Math.max(r - 9, 1);
    const f = [
      `A${r}*2+1`,
      `IF(B${r}>100,B${r}-100,B${r}+100)`,
      `SUM(A${r}:C${r})`,
      `AND(B${r}>C${r},D${r}>0)`,
      `IF(E${r},D${r},-D${r})`,
      `SUM(F${r0}:F${r})`,
      `B${r}+C${r}*D${r}-G${r}`,
      `IF(H${r}>0,H${r}/2,0-H${r})`,
      `G${r}+I${r}`,
      `SUM(B${r}:J${r})`,
    ];
    parts.push(
      `<row r="${r}"><c r="A${r}"><v>${next()}</v></c>${f.map((x, i) => `<c r="${cols[i]}${r}"><f>${x}</f></c>`).join('')}</row>`,
    );
  }
  const ns = 'http://schemas.openxmlformats.org';
  return zip([
    [
      '[Content_Types].xml',
      `<?xml version="1.0" encoding="UTF-8"?><Types xmlns="${ns}/package/2006/content-types"><Default Extension="rels" ContentType="application/vnd.openxmlformats-package.relationships+xml"/><Default Extension="xml" ContentType="application/xml"/><Override PartName="/xl/workbook.xml" ContentType="application/vnd.openxmlformats-officedocument.spreadsheetml.sheet.main+xml"/><Override PartName="/xl/worksheets/sheet1.xml" ContentType="application/vnd.openxmlformats-officedocument.spreadsheetml.worksheet+xml"/></Types>`,
    ],
    [
      '_rels/.rels',
      `<?xml version="1.0" encoding="UTF-8"?><Relationships xmlns="${ns}/package/2006/relationships"><Relationship Id="rId1" Type="${ns}/officeDocument/2006/relationships/officeDocument" Target="xl/workbook.xml"/></Relationships>`,
    ],
    [
      'xl/workbook.xml',
      `<?xml version="1.0" encoding="UTF-8"?><workbook xmlns="${ns}/spreadsheetml/2006/main" xmlns:r="${ns}/officeDocument/2006/relationships"><sheets><sheet name="Data" sheetId="1" r:id="rId1"/></sheets></workbook>`,
    ],
    [
      'xl/_rels/workbook.xml.rels',
      `<?xml version="1.0" encoding="UTF-8"?><Relationships xmlns="${ns}/package/2006/relationships"><Relationship Id="rId1" Type="${ns}/officeDocument/2006/relationships/worksheet" Target="worksheets/sheet1.xml"/></Relationships>`,
    ],
    [
      'xl/worksheets/sheet1.xml',
      `<?xml version="1.0" encoding="UTF-8"?><worksheet xmlns="${ns}/spreadsheetml/2006/main"><sheetData>${parts.join('')}</sheetData></worksheet>`,
    ],
  ]);
}

// ── A cross-origin isolated page with a Worker that loads one module and times it.
const PAGE = '<!doctype html><meta charset="utf-8"><title>Grid threads</title>';
const WORKER = `
self.onmessage = async ({ data: { variant, threads } }) => {
  try {
    const m = await import('/wasm/' + variant + '/kiln_bridge_wasm.js');
    await m.default();
    if (threads > 0) await m.initThreadPool(threads);
    const bytes = new Uint8Array(await (await fetch('/workbook.xlsx')).arrayBuffer());
    const s = new m.KilnSession();
    let t = performance.now();
    const summary = JSON.parse(s.open(bytes, JSON.stringify({ name: 'recalc.xlsx', path: null, format: 'Xlsx', read_only: false })));
    const open = performance.now() - t;
    const recalc = [];
    for (let i = 0; i < 7; i++) {
      t = performance.now();
      s.execute(summary.doc, JSON.stringify({ Grid: 'CalculateNow' }));
      recalc.push(performance.now() - t);
    }
    postMessage({ ok: true, open, recalc });
  } catch (e) {
    postMessage({ ok: false, error: String((e && e.stack) || e) });
  }
};`;
const TYPES = { '.js': 'text/javascript', '.wasm': 'application/wasm' };
const book = workbook(ROWS);
const server = createServer((req, res) => {
  const url = new URL(req.url ?? '/', 'http://localhost');
  const headers = {
    'Cross-Origin-Opener-Policy': 'same-origin',
    'Cross-Origin-Embedder-Policy': 'require-corp',
    'Cache-Control': 'no-store',
  };
  let body = null;
  let type = 'text/html';
  if (url.pathname === '/') body = PAGE;
  else if (url.pathname === '/worker.js') [body, type] = [WORKER, TYPES['.js']];
  else if (url.pathname === '/workbook.xlsx') [body, type] = [book, 'application/octet-stream'];
  else if (url.pathname.startsWith('/wasm/')) {
    let file = join(wasmDir, normalize(url.pathname.slice(6)).replace(/^(\.\.(\/|$))+/, ''));
    // wasm-bindgen-rayon's Workers import the module's directory.
    try {
      if (statSync(file).isDirectory()) file = join(file, 'index.js');
      body = readFileSync(file);
      type = TYPES[extname(file)] ?? 'application/octet-stream';
    } catch {
      body = null;
    }
  }
  if (body === null) {
    res.writeHead(404, headers);
    res.end();
    return;
  }
  res.writeHead(200, { ...headers, 'Content-Type': type });
  res.end(body);
});
await new Promise((r) => server.listen(0, '127.0.0.1', r));
const base = `http://127.0.0.1:${server.address().port}/`;

const median = (v) => [...v].sort((a, b) => a - b)[Math.floor(v.length / 2)];
const browser = await chromium.launch();
const results = {};
try {
  const page = await browser.newPage();
  await page.goto(base);
  if (!(await page.evaluate(() => crossOriginIsolated)))
    throw new Error('the page is not cross-origin isolated');
  const run = (variant, threads) =>
    page.evaluate(
      ([v, n]) =>
        new Promise((resolve) => {
          const w = new Worker('/worker.js', { type: 'module' });
          w.onmessage = (e) => {
            resolve(e.data);
            w.terminate();
          };
          w.postMessage({ variant: v, threads: n });
        }),
      [variant, threads],
    );
  for (const [name, variant, threads] of [
    ['single', 'grid', 0],
    ['threads', 'grid-threads', THREADS],
  ]) {
    const opens = [];
    const recalcs = [];
    for (let i = 0; i < 3; i++) {
      const r = await run(variant, threads);
      if (!r.ok) throw new Error(`${name}: ${r.error}`);
      opens.push(r.open);
      recalcs.push(median(r.recalc));
    }
    results[name] = { threads, openMs: Math.round(median(opens)), recalcMs: Math.round(median(recalcs)) };
    console.log(
      `grid ${name} (${threads || 1} thread${threads > 1 ? 's' : ''}): open ${results[name].openMs} ms, recalculation ${results[name].recalcMs} ms`,
    );
  }
} finally {
  await browser.close();
  server.close();
}
const formulas = ROWS * 10;
console.log(
  `grid threads: ${formulas.toLocaleString('en')} formulas, recalculation ${(results.single.recalcMs / results.threads.recalcMs).toFixed(1)}× faster, open ${(results.single.openMs / results.threads.openMs).toFixed(2)}× faster`,
);
mkdirSync(join(repo, 'target/pages'), { recursive: true });
writeFileSync(
  join(repo, 'target/pages/grid-threads.json'),
  `${JSON.stringify({ rows: ROWS, formulas, results }, null, 2)}\n`,
);
