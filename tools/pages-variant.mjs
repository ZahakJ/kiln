#!/usr/bin/env node
// Makes a second deployment from a built Pages site, for the e2e that switches between
// two real builds: every content-hashed file of the app changes its bytes and so its name, every
// reference follows, and sw.js lists the new files, as a build with changes everywhere would.
//
//   node tools/pages-variant.mjs <site> <out> <seed>
import { createHash } from 'node:crypto';
import { cpSync, readdirSync, readFileSync, renameSync, rmSync, writeFileSync } from 'node:fs';
import { extname, join } from 'node:path';

const [, , source, out, seed] = process.argv;
if (!source || !out || !seed) {
  console.error('usage: pages-variant.mjs <site> <out> <seed>');
  process.exit(2);
}
rmSync(out, { recursive: true, force: true });
cpSync(source, out, { recursive: true, filter: (path) => !path.endsWith('.map') });

const app = join(out, 'app');
const assets = join(app, 'assets');
const TEXT = new Set(['.js', '.css']);
const HASHED = /-[A-Za-z0-9_-]{8}(\.[a-z0-9]+)$/;
const digest = (text) => createHash('md5').update(text).digest('base64url');

const names = new Map();
for (const name of readdirSync(assets)) {
  if (!HASHED.test(name)) continue;
  names.set(
    name,
    name.replace(HASHED, (_, ext) => `-${digest(seed + name).slice(0, 8)}${ext}`),
  );
}
const pattern = new RegExp(
  [...names.keys()].map((n) => n.replace(/[.*+?^${}()|[\]\\]/g, '\\$&')).join('|'),
  'g',
);
const renamed = (text) => text.replace(pattern, (name) => names.get(name));

/** New bytes that change nothing a reader looks at: a custom section, a comment, a trailer. */
function touched(name, bytes) {
  const mark = Buffer.from(`build ${seed}`);
  if (name.endsWith('.wasm')) return Buffer.concat([bytes, Buffer.from([0, mark.length + 2, 1, 0x62]), mark]);
  if (name.endsWith('.svg') || TEXT.has(extname(name)))
    return Buffer.from(`${renamed(bytes.toString('utf8'))}\n/* ${mark} */\n`);
  return Buffer.concat([bytes, Buffer.from('\n'), mark]);
}

for (const [name, next] of names) {
  const file = join(assets, name);
  const before = readFileSync(file);
  // An SVG takes an XML comment, not a CSS one.
  const after = name.endsWith('.svg')
    ? Buffer.concat([before, Buffer.from(`\n<!-- build ${seed} -->\n`)])
    : touched(name, before);
  writeFileSync(file, after);
  renameSync(file, join(assets, next));
}

const html = renamed(readFileSync(join(app, 'index.html'), 'utf8'));
writeFileSync(join(app, 'index.html'), html);
const worker = renamed(readFileSync(join(app, 'sw.js'), 'utf8')).replace(
  /"revision":"[0-9a-f]+","url":"index\.html"/,
  `"revision":"${createHash('md5').update(html).digest('hex')}","url":"index.html"`,
);
writeFileSync(join(app, 'sw.js'), worker);
console.log(`pages-variant: ${names.size} files renamed, ${out}`);
