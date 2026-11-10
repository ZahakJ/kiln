#!/usr/bin/env node
// Serves the GitHub Pages artefact (tools/pages-build.sh) as Pages serves it: plain static
// files under /kiln/, no COOP or COEP headers (Pages cannot send them; the service worker adds
// them), `index.html` for a directory, 404 for anything else, and gzip for text and WebAssembly
// when the browser accepts it, as Pages does. For the pwa e2e and Lighthouse.
//
//   node tools/pages-serve.mjs [--port 4180] [--root target/pages/site] [--test]
//
// With --test (the e2e suite), `POST /__kiln/next-version` makes it serve a changed sw.js from
// then on, as a new deployment would, so the update notice can be tested without a second build;
// `POST /__kiln/deploy?root=<site>` serves another build from then on, as a real deployment does;
// and `POST /__kiln/block?files=<regular expression>` answers 404 for matching paths (none: stops).
import { createReadStream, existsSync, readFileSync, statSync } from 'node:fs';
import { createServer } from 'node:http';
import { extname, join, normalize, resolve } from 'node:path';
import { fileURLToPath } from 'node:url';
import { gzipSync } from 'node:zlib';

const args = process.argv.slice(2);
const opt = (name, fallback) => {
  const i = args.indexOf(name);
  return i >= 0 && args[i + 1] ? args[i + 1] : fallback;
};
const repo = fileURLToPath(new URL('..', import.meta.url));
let root = resolve(repo, opt('--root', 'target/pages/site'));
const port = Number(opt('--port', '4180'));
/** The repository's Pages URL is https://zahakj.github.io/kiln/: the site sits under /kiln/. */
const MOUNT = '/kiln/';

const TYPES = {
  '.html': 'text/html; charset=utf-8',
  '.js': 'text/javascript; charset=utf-8',
  '.mjs': 'text/javascript; charset=utf-8',
  '.css': 'text/css; charset=utf-8',
  '.json': 'application/json; charset=utf-8',
  '.webmanifest': 'application/manifest+json; charset=utf-8',
  '.wasm': 'application/wasm',
  '.svg': 'image/svg+xml',
  '.png': 'image/png',
  '.jpg': 'image/jpeg',
  '.webp': 'image/webp',
  '.avif': 'image/avif',
  '.gif': 'image/gif',
  '.ico': 'image/x-icon',
  '.ttf': 'font/ttf',
  '.woff2': 'font/woff2',
  '.md': 'text/markdown; charset=utf-8',
  '.txt': 'text/plain; charset=utf-8',
  '.map': 'application/json; charset=utf-8',
};

/** What Pages compresses: text, JSON, SVG and WebAssembly (fonts and images are left as they are). */
const COMPRESSED = /^(text\/|application\/(javascript|json|manifest\+json|wasm)|image\/svg)/;
const gzipCache = new Map();
function gzipped(file) {
  const stamp = statSync(file).mtimeMs;
  const hit = gzipCache.get(file);
  if (hit && hit.stamp === stamp) return hit.body;
  const body = gzipSync(readFileSync(file), { level: 6 });
  gzipCache.set(file, { stamp, body });
  return body;
}

function fileOf(pathname) {
  if (!pathname.startsWith(MOUNT)) return null;
  let rel = decodeURIComponent(pathname.slice(MOUNT.length));
  rel = normalize(rel).replace(/^(\.\.(\/|\\|$))+/, '');
  let file = join(root, rel);
  try {
    if (statSync(file).isDirectory()) file = join(file, 'index.html');
    return statSync(file).isFile() ? file : null;
  } catch {
    return null;
  }
}

const testing = args.includes('--test');
/** Deployments since start (`--test`): each one changes sw.js by a comment. */
let deployments = 0;

/** What `--test` answers 404 for, as a failed or partial deployment would. */
let blocked = null;

const server = createServer((req, res) => {
  const url = new URL(req.url ?? '/', 'http://localhost');
  // Ready as soon as it listens, built or not: the pwa spec says when the artefact is missing.
  if (url.pathname === '/__kiln/health') {
    res.writeHead(200, { 'Content-Type': 'text/plain; charset=utf-8' });
    res.end(existsSync(join(root, 'app', 'index.html')) ? 'built\n' : 'missing\n');
    return;
  }
  if (testing && req.method === 'POST' && url.pathname === '/__kiln/next-version') {
    deployments += 1;
    res.writeHead(204);
    res.end();
    return;
  }
  if (testing && req.method === 'POST' && url.pathname === '/__kiln/deploy') {
    root = resolve(repo, url.searchParams.get('root') ?? root);
    res.writeHead(204);
    res.end();
    return;
  }
  if (testing && req.method === 'POST' && url.pathname === '/__kiln/block') {
    const files = url.searchParams.get('files');
    blocked = files ? new RegExp(files) : null;
    res.writeHead(204);
    res.end();
    return;
  }
  if (blocked?.test(url.pathname)) {
    res.writeHead(404, { 'Content-Type': 'text/plain; charset=utf-8' });
    res.end('404 Not Found\n');
    return;
  }
  if (url.pathname === '/kiln') {
    res.writeHead(301, { Location: MOUNT });
    res.end();
    return;
  }
  // Pages redirects a directory without its slash.
  const file = fileOf(url.pathname);
  if (!file && fileOf(`${url.pathname}/`)) {
    res.writeHead(301, { Location: `${url.pathname}/` });
    res.end();
    return;
  }
  if (!file || (req.method !== 'GET' && req.method !== 'HEAD')) {
    res.writeHead(404, { 'Content-Type': 'text/plain; charset=utf-8' });
    res.end('404 Not Found\n');
    return;
  }
  if (deployments > 0 && file.endsWith(`${'/app/'}sw.js`)) {
    const body = `${readFileSync(file, 'utf8')}\n// deployment ${deployments}\n`;
    res.writeHead(200, { 'Content-Type': TYPES['.js'], 'Cache-Control': 'max-age=600' });
    res.end(req.method === 'HEAD' ? undefined : body);
    return;
  }
  const type = TYPES[extname(file).toLowerCase()] ?? 'application/octet-stream';
  if (COMPRESSED.test(type) && /\bgzip\b/.test(String(req.headers['accept-encoding'] ?? ''))) {
    const body = gzipped(file);
    res.writeHead(200, {
      'Content-Type': type,
      'Content-Encoding': 'gzip',
      'Cache-Control': 'max-age=600',
      'Content-Length': body.length,
      Vary: 'Accept-Encoding',
    });
    res.end(req.method === 'HEAD' ? undefined : body);
    return;
  }
  // Pages lets browsers keep a file for ten minutes.
  res.writeHead(200, {
    'Content-Type': TYPES[extname(file).toLowerCase()] ?? 'application/octet-stream',
    'Cache-Control': 'max-age=600',
    'Content-Length': statSync(file).size,
  });
  if (req.method === 'HEAD') res.end();
  else createReadStream(file).pipe(res);
});
server.listen(port, '127.0.0.1', () => {
  console.log(`pages-serve: http://127.0.0.1:${port}${MOUNT} -> ${root}`);
});
for (const signal of ['SIGINT', 'SIGTERM']) process.on(signal, () => server.close(() => process.exit(0)));
