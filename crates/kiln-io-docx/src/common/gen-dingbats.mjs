#!/usr/bin/env node
// SPDX-License-Identifier: AGPL-3.0-or-later
// Regenerates dingbats.rs from the dingbat-to-unicode npm package (a dependency of the mammoth
// oracle in packages/testing):
//
//   node crates/kiln-io-docx/src/common/gen-dingbats.mjs > crates/kiln-io-docx/src/common/dingbats.rs
//   cargo fmt -p kiln-io-docx
import { readFileSync } from 'node:fs';
import { createRequire } from 'node:module';
import { dirname, join } from 'node:path';
import { fileURLToPath } from 'node:url';

const repo = join(dirname(fileURLToPath(import.meta.url)), '../../../..');
const fromTesting = createRequire(join(repo, 'packages/testing/package.json'));
const mammothRequire = createRequire(fromTesting.resolve('mammoth'));
const dingbatsPath = mammothRequire.resolve('dingbat-to-unicode/dist/dingbats.js');
const pkgDir = join(dirname(dingbatsPath), '..');
const { version } = JSON.parse(readFileSync(join(pkgDir, 'package.json'), 'utf8'));
const licence = readFileSync(join(pkgDir, 'LICENSE'), 'utf8').trim();
const d = mammothRequire(dingbatsPath).default;

const out = [];
out.push('// SPDX-License-Identifier: AGPL-3.0-or-later');
out.push(
  `// GENERATED from dingbat-to-unicode ${version} (https://github.com/mwilliamson/dingbat-to-unicode)`,
);
out.push('// by gen-dingbats.mjs next to this file; do not edit by hand.');
out.push('// The table data is used under the following licence:');
out.push('//');
for (const l of licence.split('\n')) out.push(`// ${l}`.trimEnd());
out.push('');
out.push('//! Symbol-font code → Unicode tables (the data mammoth and Word converters use).');
out.push('');
const fonts = ['Symbol', 'Webdings', 'Wingdings', 'Wingdings 2', 'Wingdings 3'];
out.push(
  '/// `(lower-case font name, table)`; a table maps the 8-bit code `c` (32..=255) at index `c - 32` to a',
);
out.push('/// Unicode scalar value, 0 meaning "no equivalent".');
out.push(`pub(crate) static FONTS: [(&str, &[u32; 224]); ${fonts.length}] = [`);
for (const name of fonts) {
  const arr = new Array(224).fill(0);
  for (const [code, cp] of Object.entries(d[name.toUpperCase()])) {
    const c = Number(code);
    if (c >= 32 && c <= 255) arr[c - 32] = cp;
  }
  out.push(`    ("${name.toLowerCase()}", &[`);
  for (let i = 0; i < 224; i += 8) {
    out.push(
      `        ${arr
        .slice(i, i + 8)
        .map((v) => `0x${v.toString(16).toUpperCase().padStart(4, '0')}`)
        .join(', ')},`,
    );
  }
  out.push('    ]),');
}
out.push('];');
console.log(out.join('\n'));
