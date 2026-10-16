#!/usr/bin/env node
// SPDX-License-Identifier: AGPL-3.0-or-later
/**
 * mammoth (an independent .docx reader) as an oracle for the writer's round-trip tests: prints
 * mammoth's raw text of every file given on the command line, each followed by a NUL byte.
 * Simple fields are unwrapped into their result runs first (mammoth ignores `w:fldSimple`),
 * exactly as `packages/testing/scripts/mammoth-docx-text.mjs` does for the reader's oracle.
 *
 *   node mammoth.mjs a.docx b.docx   # → text of a \0 text of b \0
 *
 * mammoth is resolved from `packages/testing` (its dev dependency); exit status 3 when it is not
 * installed, so the Rust test can skip.
 */
import { readFileSync } from 'node:fs';
import { createRequire } from 'node:module';
import { dirname, join, resolve } from 'node:path';
import { fileURLToPath } from 'node:url';

const repo = resolve(dirname(fileURLToPath(import.meta.url)), '../../../..');
let mammoth;
let JSZip;
try {
  const require = createRequire(join(repo, 'packages/testing/package.json'));
  mammoth = require('mammoth');
  JSZip = createRequire(require.resolve('mammoth'))('jszip');
} catch {
  process.exit(3);
}

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

for (const path of process.argv.slice(2)) {
  const bytes = await unwrapSimpleFields(readFileSync(path));
  const { value } = await mammoth.extractRawText({ buffer: bytes });
  process.stdout.write(`${value}\0`);
}
