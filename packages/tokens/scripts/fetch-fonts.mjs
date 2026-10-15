#!/usr/bin/env node
// Downloads the bundled document fonts (PLAN.md §1.2) from their upstream repositories into
// packages/tokens/fonts/. Plain Node (fetch + zlib + a tiny tar reader): no external tools.
//
//   node packages/tokens/scripts/fetch-fonts.mjs           download, verify and (re)write every font
//   node packages/tokens/scripts/fetch-fonts.mjs --check   verify the committed files against the pins
//
// Every file is pinned by SHA-256; a mismatch aborts without writing anything, so the committed fonts
// can only change when someone deliberately updates a pin here. All fonts are SIL OFL 1.1.
import { createHash } from 'node:crypto';
import { existsSync, mkdirSync, readFileSync, writeFileSync } from 'node:fs';
import { dirname, join } from 'node:path';
import { fileURLToPath } from 'node:url';
import { gunzipSync } from 'node:zlib';

const FONTS_DIR = join(dirname(fileURLToPath(import.meta.url)), '..', 'fonts');

const CARLITO =
  'https://raw.githubusercontent.com/googlefonts/carlito/3a810cab78ebd6e2e4eed42af9e8453c4f9b850a';
const CALADEA =
  'https://raw.githubusercontent.com/huertatipografica/Caladea/336a529cfad3d103d6527752686f8331d13e820a';
const LIBERATION =
  'https://github.com/liberationfonts/liberation-fonts/files/7261482/liberation-fonts-ttf-2.1.5.tar.gz';

const STYLES = ['Regular', 'Bold', 'Italic', 'BoldItalic'];

/**
 * Sources. `archive` entries are extracted from a .tar.gz; `url` entries are fetched directly.
 * Output checksums live in fonts/pins.json (recorded from the upstream files on 2026-10-15).
 */
export const SOURCES = [
  {
    archive: LIBERATION,
    archiveSha256: '7191c669bf38899f73a2094ed00f7b800553364f90e2637010a69c0e268f25d0',
    files: [
      ...['Sans', 'Serif', 'Mono'].flatMap((family) =>
        STYLES.map((style) => ({
          entry: `liberation-fonts-ttf-2.1.5/Liberation${family}-${style}.ttf`,
          out: `Liberation${family}-${style}.ttf`,
        })),
      ),
      { entry: 'liberation-fonts-ttf-2.1.5/LICENSE', out: 'LICENSE-Liberation.txt' },
    ],
  },
  ...STYLES.map((style) => ({
    url: `${CARLITO}/fonts/ttf/Carlito-${style}.ttf`,
    out: `Carlito-${style}.ttf`,
  })),
  { url: `${CARLITO}/OFL.txt`, out: 'LICENSE-Carlito.txt' },
  ...STYLES.map((style) => ({
    url: `${CALADEA}/fonts/ttf/Caladea-${style}.ttf`,
    out: `Caladea-${style}.ttf`,
  })),
  { url: `${CALADEA}/OFL.txt`, out: 'LICENSE-Caladea.txt' },
];

/** SHA-256 of every output file (verified on download and by `--check`). */
export const PINS = JSON.parse(readFileSync(join(FONTS_DIR, 'pins.json'), 'utf8'));

const sha256 = (buf) => createHash('sha256').update(buf).digest('hex');

async function download(url) {
  const res = await fetch(url, { redirect: 'follow' });
  if (!res.ok) throw new Error(`GET ${url} -> HTTP ${res.status}`);
  return Buffer.from(await res.arrayBuffer());
}

/** Minimal ustar reader: returns Map<path, Buffer> of regular files. */
export function untar(buf) {
  const files = new Map();
  let offset = 0;
  while (offset + 512 <= buf.length) {
    const header = buf.subarray(offset, offset + 512);
    if (header.every((b) => b === 0)) break;
    const field = (start, len) =>
      header
        .subarray(start, start + len)
        .toString('utf8')
        .replace(/\0.*$/s, '');
    const name = field(0, 100);
    const prefix = field(345, 155);
    const size = Number.parseInt(field(124, 12).trim() || '0', 8);
    const type = field(156, 1) || '0';
    const path = prefix ? `${prefix}/${name}` : name;
    const start = offset + 512;
    if (type === '0') files.set(path, buf.subarray(start, start + size));
    offset = start + Math.ceil(size / 512) * 512;
  }
  return files;
}

async function fetchAll() {
  /** @type {Map<string, Buffer>} */
  const out = new Map();
  for (const source of SOURCES) {
    if (source.archive) {
      const tgz = await download(source.archive);
      if (sha256(tgz) !== source.archiveSha256) {
        throw new Error(`archive checksum mismatch for ${source.archive}: got ${sha256(tgz)}`);
      }
      const entries = untar(gunzipSync(tgz));
      for (const file of source.files) {
        const data = entries.get(file.entry);
        if (!data) throw new Error(`${file.entry} missing from ${source.archive}`);
        out.set(file.out, Buffer.from(data));
      }
    } else {
      out.set(source.out, await download(source.url));
    }
  }
  return out;
}

function verify(name, data) {
  const pin = PINS[name];
  if (!pin) throw new Error(`no SHA-256 pin for ${name} in fonts/pins.json`);
  if (sha256(data) !== pin)
    throw new Error(`checksum mismatch for ${name}: got ${sha256(data)}, pinned ${pin}`);
}

async function main() {
  if (process.argv.includes('--check')) {
    for (const name of Object.keys(PINS)) {
      const file = join(FONTS_DIR, name);
      if (!existsSync(file)) throw new Error(`${name} is missing; run fetch-fonts.mjs`);
      verify(name, readFileSync(file));
    }
    console.log(`fetch-fonts: ${Object.keys(PINS).length} files match their pins`);
    return;
  }
  const files = await fetchAll();
  for (const [name, data] of files) verify(name, data);
  mkdirSync(FONTS_DIR, { recursive: true });
  let total = 0;
  for (const [name, data] of files) {
    writeFileSync(join(FONTS_DIR, name), data);
    total += data.length;
  }
  console.log(
    `fetch-fonts: wrote ${files.size} files (${(total / 1024 / 1024).toFixed(2)} MB) to ${FONTS_DIR}`,
  );
}

const isMain = import.meta.url === `file://${process.argv[1]}`;
if (isMain) {
  main().catch((error) => {
    console.error(`fetch-fonts: ${error.message}`);
    process.exit(1);
  });
}
