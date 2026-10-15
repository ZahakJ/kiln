#!/usr/bin/env node
// Officina licence gate (PLAN.md §1.1a, §4.2).
//
//   node tools/check-licenses.mjs            check every production dependency against the allow-list
//   node tools/check-licenses.mjs --notices  write THIRD-PARTY-NOTICES.md content to stdout
//
// Input: `pnpm licenses list --json --prod --recursive` (all workspace projects). Licence fields are
// SPDX expressions; an expression passes when it evaluates to true with every allowed licence id as
// true (`OR` needs one allowed side, `AND` needs both). Unknown/missing licences always fail.
import { execFileSync } from 'node:child_process';
import { existsSync, readdirSync, readFileSync } from 'node:fs';
import { join } from 'node:path';

/** Licence ids allowed in shipped code (PLAN.md §4.2). */
export const ALLOWED_IDS = new Set([
  'MIT',
  'Apache-2.0',
  'BSD-2-Clause',
  'BSD-3-Clause',
  'ISC',
  '0BSD',
  'MPL-2.0',
  'OFL-1.1',
  'CC0-1.0',
  'Unlicense',
  'Python-2.0',
  'BlueOak-1.0.0',
  // zlib licence: permissive, FSF-approved and GPL/AGPL-compatible. Needed for pako (pdf-lib), whose
  // licence is "(MIT AND Zlib)". Not in the PLAN.md §4.2 list verbatim; added in WP-01 (see ADR 0001).
  'Zlib',
]);

/** Whole expressions allowed verbatim even if they contain non-SPDX ids (PLAN.md §4.2). */
export const ALLOWED_EXPRESSIONS = new Set([
  '(MIT AND BSD)',
  '(MIT OR Apache-2.0)',
  '(MPL-2.0 OR Apache-2.0)',
  '(MIT OR GPL-3.0-or-later)',
]);

/** Licences that must never appear, reported with a specific message. */
const DENIED_PATTERNS = [/^GPL-2\.0(-only)?$/, /^SSPL/, /^BUSL/, /^CC-BY-NC/, /^Unknown$/i, /^UNLICENSED$/];

/** Tokenise and evaluate an SPDX licence expression. Returns true when it is satisfiable. */
export function isAllowed(expression) {
  const expr = String(expression ?? '').trim();
  if (!expr) return false;
  if (ALLOWED_EXPRESSIONS.has(expr) || ALLOWED_EXPRESSIONS.has(`(${expr})`)) return true;
  const tokens = expr.match(/\(|\)|[^\s()]+/g) ?? [];
  let i = 0;
  const peek = () => tokens[i];
  const next = () => tokens[i++];
  const primary = () => {
    const t = next();
    if (t === undefined) throw new Error(`unexpected end of licence expression "${expr}"`);
    if (t === '(') {
      const v = orExpr();
      if (next() !== ')') throw new Error(`unbalanced parentheses in "${expr}"`);
      return v;
    }
    // `WITH <exception>` keeps the base licence's verdict (exceptions only grant permissions).
    if (peek() === 'WITH') {
      next();
      next();
    }
    return ALLOWED_IDS.has(t.replace(/\+$/, ''));
  };
  const andExpr = () => {
    let v = primary();
    while (peek() === 'AND') {
      next();
      v = primary() && v;
    }
    return v;
  };
  const orExpr = () => {
    let v = andExpr();
    while (peek() === 'OR') {
      next();
      v = andExpr() || v;
    }
    return v;
  };
  try {
    const v = orExpr();
    return i === tokens.length && v;
  } catch {
    return false;
  }
}

function loadLicences() {
  let out;
  try {
    out = execFileSync('pnpm', ['licenses', 'list', '--json', '--prod', '--recursive'], {
      encoding: 'utf8',
      maxBuffer: 256 * 1024 * 1024,
      stdio: ['ignore', 'pipe', 'pipe'],
    });
  } catch (error) {
    console.error('check-licenses: `pnpm licenses list` failed. Run `pnpm install` first.');
    console.error(error.stderr?.toString() ?? error.message);
    process.exit(2);
  }
  /** @type {Record<string, Array<{name:string, versions:string[], paths:string[], license:string, author?:string, homepage?:string}>>} */
  const byLicence = JSON.parse(out || '{}');
  const packages = [];
  for (const [licence, list] of Object.entries(byLicence)) {
    for (const pkg of list) packages.push({ ...pkg, license: pkg.license ?? licence });
  }
  packages.push(...loadExtraAssets());
  packages.sort((a, b) => a.name.localeCompare(b.name));
  return packages;
}

/**
 * Bundled non-npm assets (fonts, dictionaries, ...) declared in `packages/<pkg>/<dir>/NOTICES.json`
 * (`{ "assets": [{ name, version, license, author?, homepage?, licenseFile }] }`, licenseFile relative to
 * the NOTICES.json). Added in WP-04 for the document fonts in packages/tokens/fonts.
 */
function loadExtraAssets() {
  const root = join(import.meta.dirname, '..', 'packages');
  const assets = [];
  for (const pkg of readdirSync(root)) {
    for (const dir of ['fonts', 'assets']) {
      const file = join(root, pkg, dir, 'NOTICES.json');
      if (!existsSync(file)) continue;
      for (const a of JSON.parse(readFileSync(file, 'utf8')).assets ?? []) {
        assets.push({
          name: a.name,
          versions: [a.version],
          license: a.license,
          author: a.author,
          homepage: a.homepage,
          paths: [join(root, pkg, dir)],
          licenseFile: a.licenseFile ? join(root, pkg, dir, a.licenseFile) : undefined,
        });
      }
    }
  }
  return assets;
}

function check(packages) {
  const violations = packages.filter((p) => !isAllowed(p.license));
  const counts = new Map();
  for (const p of packages) counts.set(p.license, (counts.get(p.license) ?? 0) + 1);
  console.log(`check-licenses: ${packages.length} production packages`);
  for (const [licence, n] of [...counts].sort((a, b) => b[1] - a[1])) {
    console.log(`  ${String(n).padStart(4)}  ${licence}`);
  }
  if (packages.length === 0) {
    console.error('check-licenses: no packages found; is the workspace installed?');
    process.exit(1);
  }
  if (violations.length) {
    console.error(`\ncheck-licenses: ${violations.length} package(s) outside the allow-list:`);
    for (const p of violations) {
      const denied = DENIED_PATTERNS.some((re) => re.test(p.license)) ? ' (explicitly denied)' : '';
      console.error(`  ${p.name}@${p.versions.join(', ')}: ${p.license || 'no licence'}${denied}`);
    }
    process.exit(1);
  }
  console.log('check-licenses: all licences allowed');
}

function licenceText(pkgPath) {
  if (!pkgPath || !existsSync(pkgPath)) return null;
  const file = readdirSync(pkgPath).find((f) => /^(licen[cs]e|copying)(\.(md|txt))?$/i.test(f));
  return file ? readFileSync(join(pkgPath, file), 'utf8').trim() : null;
}

function notices(packages) {
  const lines = [
    '# Third-party notices',
    '',
    'Officina is licensed under the GNU Affero General Public License v3.0 or later. It includes the',
    'third-party software listed below, each under its own licence. This file is generated by',
    '`pnpm licenses:notices` (`node tools/check-licenses.mjs --notices`); do not edit it by hand.',
    '',
    '| Package | Version | Licence |',
    '|---|---|---|',
    ...packages.map((p) => `| ${p.name} | ${p.versions.join(', ')} | ${p.license} |`),
    '',
  ];
  for (const p of packages) {
    lines.push(`## ${p.name} ${p.versions.join(', ')}`, '');
    lines.push(`- Licence: ${p.license}`);
    if (p.author) lines.push(`- Author: ${p.author}`);
    if (p.homepage) lines.push(`- Homepage: ${p.homepage}`);
    const text = p.licenseFile ? readFileSync(p.licenseFile, 'utf8').trim() : licenceText(p.paths?.[0]);
    lines.push('');
    if (text) lines.push('```text', text.replace(/```/g, "'''"), '```', '');
  }
  process.stdout.write(`${lines.join('\n').trimEnd()}\n`);
}

const isMain = import.meta.url === `file://${process.argv[1]}`;
if (isMain) {
  process.stdout.on('error', (error) => {
    if (error.code === 'EPIPE') process.exit(0);
    throw error;
  });
  const packages = loadLicences();
  if (process.argv.includes('--notices')) notices(packages);
  else check(packages);
}
