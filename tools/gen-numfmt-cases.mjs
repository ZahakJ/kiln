#!/usr/bin/env node
// SPDX-License-Identifier: AGPL-3.0-or-later
//
// Generates conformance/numfmt/*.csv: (format, value, kind, expected) rows for Excel number
// format codes, with the expected text produced by the `numfmt` package (MIT, by Borgar
// Þorsteinsson; a dev-only oracle that is never shipped). The Rust formatter in
// crates/kiln-formula/src/numfmt is tested against every row by tests/numfmt_conformance.rs.
//
// numfmt rounds with binary floating point, while Excel (and Kiln) round the decimal value, so
// `0.00` shows 1.005 as "1.01" in Excel but "1.00" in numfmt. Cases where the two can disagree are
// left out (see `binarySafe`), as are a few numfmt quirks listed in `QUIRKS` with the reason.
//
//   node tools/gen-numfmt-cases.mjs          # rewrite the CSVs
//   node tools/gen-numfmt-cases.mjs --check  # fail if they are stale

import { mkdirSync, readFileSync, writeFileSync } from 'node:fs';
import { dirname, join } from 'node:path';
import { fileURLToPath } from 'node:url';
import { format } from 'numfmt';

const ROOT = join(dirname(fileURLToPath(import.meta.url)), '..');
const OUT = join(ROOT, 'conformance', 'numfmt');

/** Decimal places a numeric format rounds to (the most in any section). */
function decimalsOf(code) {
  let best = 0;
  for (const section of code.replace(/"[^"]*"|\\./g, '').split(';')) {
    const m = /\.([0#?]+)/.exec(section);
    if (m && m[1].length > best) best = m[1].length;
  }
  return best;
}

/** True when rounding `v` to `d` decimals gives the same digits in binary and in decimal. */
function binarySafe(v, d, scale = 1) {
  const x = Math.abs(v * scale) * 10 ** d;
  return Math.round(x) === Math.round(Number(x.toPrecision(15)));
}

/** numfmt behaviours that differ from Excel; rows hitting them are not generated. */
const QUIRKS = [
  // JavaScript prints very large and very small numbers with an exponent ("1e+21").
  (_code, _v, out) => /\de[+-]\d/.test(out),
  // General rounds 9.999995E-05 to "10E-05" instead of 1E-04.
  (_code, _v, out) => /(^|-)10E/.test(out),
  // General truncates (rather than rounds) 10- and 11-digit non-integers.
  (code, v) => code === 'General' && Math.abs(v) >= 1e9 && !Number.isInteger(v),
];

function csvField(s) {
  const t = String(s);
  return /[",\n\r]/.test(t) || /^\s|\s$/.test(t) ? `"${t.replace(/"/g, '""')}"` : t;
}

function emit(rows) {
  return `${['format,value,kind,expected', ...rows.map((r) => r.map(csvField).join(','))].join('\n')}\n`;
}

function build(cases, kind = 'number') {
  const rows = [];
  for (const [code, values] of cases) {
    const d = decimalsOf(code);
    const scale = code.replace(/"[^"]*"/g, '').includes('%') ? 100 : 1;
    for (const v of values) {
      let out;
      try {
        out = format(code, v);
      } catch {
        continue;
      }
      if (kind === 'number') {
        if (typeof v === 'number' && !binarySafe(v, d, scale)) continue;
        if (QUIRKS.some((q) => q(code, v, out))) continue;
      }
      rows.push([
        code,
        typeof v === 'number' ? String(v) : v,
        typeof v === 'boolean' ? 'bool' : typeof v === 'number' ? 'number' : 'text',
        out,
      ]);
    }
  }
  return rows;
}

const NUMS = [
  0,
  1,
  -1,
  0.5,
  -0.5,
  7,
  -42,
  1500,
  1234.5678,
  -1234.5678,
  0.001234,
  12345678,
  6.02214,
  1e6,
  -0.25,
  0.1 + 0.2,
  99.5,
  0.075,
];

const numberCases = [
  'General',
  '0',
  '0.0',
  '0.00',
  '0.000',
  '#',
  '#.##',
  '#.#',
  '0.0#',
  '#,##0',
  '#,##0.00',
  '#,###',
  '#,##0.0',
  '00000',
  '000.00',
  '???.??',
  '#,##0,',
  '#,##0,,',
  '0.0,,"M"',
  '#,##0,"K"',
  '0%',
  '0.0%',
  '0.00%',
  '#%',
  '0.00E+00',
  '0.0E+0',
  '##0.0E+0',
  '0E+00',
  '0.###E-0',
  '$#,##0',
  '$#,##0.00',
  '$#,##0.00_);($#,##0.00)',
  '$#,##0.00_);[Red]($#,##0.00)',
  '#,##0.00;[Red]-#,##0.00',
  '0.00;(0.00)',
  '0;-0;"zero"',
  '0;-0;-;@',
  '[Red]0.00;[Blue]-0.00',
  '[Green]#,##0;[Red]-#,##0;[Blue]0',
  '[Color10]0.0',
  '+0;-0;0',
  '"Total: "0.00',
  '0.00" kg"',
  '\\$0.00',
  '0 "items"',
  '#,##0.00 €',
  '[$€-409]#,##0.00',
  '[$£-809]#,##0.00',
  '_(0.00_)',
  '* #,##0',
  '#,##0.00 *.',
  '0.00_-',
  '(0)',
  '0-0',
  '000-00-0000',
  '00000-0000',
  '[<0]"neg";[>0]"pos";"zero"',
  '[>=1000]#,##0,"K";0',
  '[<=-1]"low";[>=1]"high";0.0',
  '[=1]"one";[=2]"two";0',
  '[>100][Red]0;[Blue]0',
  '# ?/?',
  '# ??/??',
  '# ???/???',
  '?/?',
  '??/??',
  '# ?/2',
  '# ?/4',
  '# ?/8',
  '# ??/16',
  '# ?/10',
  '# ??/100',
  '0 ?/?',
  '#" "?/?',
  '# ?/? "in"',
  '0.0 "x"',
  '#,##0;(#,##0)',
];

const dateNums = [
  1, 59, 60, 61, 366, 32874.25, 36526.5, 43831.0208333333, 45306, 45306.75, 45306.123456, 45658.999988,
  2958465,
];
const dateCases = [
  'd',
  'dd',
  'ddd',
  'dddd',
  'm',
  'mm',
  'mmm',
  'mmmm',
  'mmmmm',
  'yy',
  'yyyy',
  'm/d/yyyy',
  'mm/dd/yyyy',
  'd-mmm-yy',
  'dd-mmm-yyyy',
  'mmm d, yyyy',
  'dddd, mmmm d, yyyy',
  'yyyy-mm-dd',
  'yyyy-mm-dd hh:mm:ss',
  'm/d/yy h:mm',
  'h:mm',
  'hh:mm',
  'h:mm:ss',
  'h:mm AM/PM',
  'h:mm:ss AM/PM',
  'hh:mm A/P',
  'h AM/PM',
  '[h]:mm:ss',
  '[h]:mm',
  '[mm]:ss',
  '[ss]',
  'mm:ss',
  'mm:ss.0',
  'hh:mm:ss.00',
  'h:mm:ss.000',
  'yyyy-mm-dd"T"hh:mm:ss',
  'd/m/yyyy',
  'mmmm yyyy',
  '[$-409]m/d/yy h:mm AM/PM',
  '[$-409]dddd, mmmm d, yyyy',
  'bbbb',
  'dd/mm/bbbb',
  'ddd d mmm',
  'yyyy" year"',
  '[$-1809]d mmm yyyy',
];

const generalNums = [
  0,
  1,
  -1,
  0.1,
  0.5,
  1 / 3,
  2 / 3,
  -1 / 3,
  123456789,
  1234567890,
  12345678901,
  123456789012,
  1e15,
  1e21,
  -1e21,
  0.0001,
  0.00001,
  0.000123456,
  0.000000123,
  0.0000001234,
  1.23e-10,
  1.5e-20,
  123.456,
  -123.456,
  6.02214076543219,
  1234567.891,
  99999.99999,
  0.1 + 0.2,
  1e-5,
  12.5,
  100,
  1e10,
  9.87654321e12,
  -0.000012345,
  4.5e100,
  7.2e-50,
  65536,
  2 ** 31,
  2 ** 53,
  0.999999999999,
  1.000000001,
  50000.5,
  0.05,
  1e9,
  -1e9,
];

const textCases = [
  ['@', ['abc', '', 'Kiln Grid', '  padded  ', 'TRUE']],
  ['"Name: "@', ['Ada', 'x']],
  ['@" units"', ['5', 'many']],
  ['0;-0;0;"text: "@', ['hello']],
  ['0.00;-0.00;0;[Red]@', ['warn']],
  ['General', ['plain', 'a"b']],
  ['@@', ['ab']],
  ['_(@_)', ['pad']],
];

const sets = {
  'numbers.csv': build(numberCases.map((c) => [c, NUMS])),
  'dates.csv': build(dateCases.map((c) => [c, dateNums])),
  'general.csv': build([
    ['General', generalNums],
    ['0.00;General', [-5.5, -0.001]],
  ]),
  'text.csv': build(textCases, 'text').concat(
    build(
      [
        ['General', [true, false]],
        ['0;-0;0;@', [true]],
      ],
      'text',
    ),
  ),
};

const check = process.argv.includes('--check');
mkdirSync(OUT, { recursive: true });
let total = 0;
let stale = false;
for (const [file, rows] of Object.entries(sets)) {
  total += rows.length;
  const text = emit(rows);
  const path = join(OUT, file);
  if (check) {
    let old = '';
    try {
      old = readFileSync(path, 'utf8');
    } catch {}
    if (old !== text) {
      console.error(`${file} is stale`);
      stale = true;
    }
  } else writeFileSync(path, text);
}
if (total < 600) {
  console.error(`only ${total} rows`);
  process.exit(1);
}
console.log(`${check ? 'checked' : 'wrote'} ${total} rows in ${Object.keys(sets).length} files`);
if (stale) process.exit(1);
