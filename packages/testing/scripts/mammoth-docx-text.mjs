#!/usr/bin/env node
// SPDX-License-Identifier: AGPL-3.0-or-later
/**
 * The mammoth text oracle for the DOCX reader (kiln-io-docx, WP-62): writes
 * `fixtures/docx/read/<name>.mammoth.txt` with mammoth's raw text of every
 * `fixtures/docx/features/*.docx`. The Rust test `tests/read_mammoth.rs` compares the reader's body
 * text with these files paragraph by paragraph.
 *
 * mammoth ignores `w:fldSimple` (it warns "unrecognised element"), so it is given the file with
 * simple fields unwrapped into their result runs — the content Word displays.
 *
 *   node packages/testing/scripts/mammoth-docx-text.mjs           # regenerate
 *   node packages/testing/scripts/mammoth-docx-text.mjs --check   # fail when a file is stale
 */
import { existsSync, mkdirSync, readdirSync, readFileSync, writeFileSync } from 'node:fs';
import { createRequire } from 'node:module';
import { dirname, join, resolve } from 'node:path';
import { fileURLToPath } from 'node:url';

const require = createRequire(import.meta.url);
const mammoth = require('mammoth');
// jszip is mammoth's own zip library; resolve it through mammoth so no extra dependency is needed.
const JSZip = createRequire(require.resolve('mammoth'))('jszip');

const repo = resolve(dirname(fileURLToPath(import.meta.url)), '../../..');
const features = join(repo, 'fixtures/docx/features');
const outDir = join(repo, 'fixtures/docx/read');
const check = process.argv.includes('--check');

async function unwrapSimpleFields(bytes) {
  const zip = await JSZip.loadAsync(bytes);
  const file = zip.file('word/document.xml');
  if (!file) return bytes;
  const xml = await file.async('string');
  if (!xml.includes('fldSimple')) return bytes;
  const unwrapped = xml
    .replace(/<w:fldSimple\b[^>]*\/>/g, '')
    .replace(/<w:fldSimple\b[^>]*>|<\/w:fldSimple>/g, '');
  zip.file('word/document.xml', unwrapped);
  return zip.generateAsync({ type: 'nodebuffer' });
}

mkdirSync(outDir, { recursive: true });
let stale = 0;
for (const name of readdirSync(features)
  .filter((f) => f.endsWith('.docx'))
  .sort()) {
  const bytes = await unwrapSimpleFields(readFileSync(join(features, name)));
  const { value } = await mammoth.extractRawText({ buffer: bytes });
  const target = join(outDir, name.replace(/\.docx$/, '.mammoth.txt'));
  if (check) {
    if (!existsSync(target) || readFileSync(target, 'utf8') !== value) {
      console.error(`stale: ${target}`);
      stale += 1;
    }
  } else {
    writeFileSync(target, value);
    console.log(`wrote ${target}`);
  }
}
if (stale > 0) process.exit(1);
