#!/usr/bin/env node
// SPDX-License-Identifier: AGPL-3.0-or-later
//
// Builds the brand glyph set of @kiln/ui-kit from the brand kit in brand/ (PLAN.md §1.2, WP-51):
//
//   brand/icons/mono/<app>-currentColor.svg -> marks.generated.ts (single-colour app glyphs)
//   brand/mockups/src/*.html                -> marks.generated.ts (the spark and the flicked flame)
//   brand/icons/svg/<app>{,-48,-32,-16}.svg -> tiles/ (glazed tiles at each level of detail)
//
// Run by tools/import-brand.sh; `--check` exits 1 when the committed files differ from the kit.
import { execFileSync } from 'node:child_process';
import { mkdirSync, readFileSync, writeFileSync } from 'node:fs';
import { createRequire } from 'node:module';
import { dirname, join } from 'node:path';
import { fileURLToPath } from 'node:url';

const HERE = dirname(fileURLToPath(import.meta.url));
const BRAND = join(HERE, '..', '..', '..', '..', '..', 'brand');
const APPS = ['suite', 'write', 'grid', 'deck', 'notes'];
const LEVELS = ['', '-48', '-32', '-16'];
const check = process.argv.includes('--check');
const stale = [];

const read = (p) => readFileSync(join(BRAND, p), 'utf8');

function emit(path, content) {
  let current = null;
  try {
    current = readFileSync(path, 'utf8');
  } catch {
    current = null;
  }
  if (current === content) return;
  if (check) stale.push(path);
  else {
    mkdirSync(dirname(path), { recursive: true });
    writeFileSync(path, content);
  }
}

/** The root element's viewBox and inner markup of an SVG document. */
function svgParts(svg, what) {
  const m = svg.match(/<svg\b([^>]*)>([\s\S]*)<\/svg>\s*$/);
  if (!m) throw new Error(`build: ${what} is not an SVG document`);
  const viewBox = m[1].match(/viewBox="([^"]+)"/)?.[1];
  if (!viewBox) throw new Error(`build: ${what} has no viewBox`);
  const inner = m[2].trim();
  if (/<script|\son[a-z]+=|javascript:/i.test(inner)) throw new Error(`build: ${what} contains script`);
  return { viewBox, inner };
}

const marks = {};
for (const app of APPS) {
  marks[app] = svgParts(read(`icons/mono/${app}-currentColor.svg`), `icons/mono/${app}-currentColor.svg`);
}

// The spark and the flame are drawn inline in the kit's mockups (BRAND.md §5).
const writeMock = read('mockups/src/write-light.html');
const spark = writeMock.match(/<svg class="sp" viewBox="([^"]+)"[^>]*>([\s\S]*?)<\/svg>/);
const flame = writeMock.match(/<svg class="fl" viewBox="([^"]+)"[^>]*>([\s\S]*?)<\/svg>/);
if (!spark || !flame) throw new Error('build: spark or flame not found in mockups/src/write-light.html');
marks.spark = { viewBox: spark[1], inner: spark[2].trim() };
marks.flame = { viewBox: flame[1], inner: flame[2].trim() };

/**
 * Parses the kit's simple SVG markup (elements and quoted attributes, no text) into a compact element
 * tree `[tag, attributes, children]`, so React can render it without `dangerouslySetInnerHTML`.
 */
function parseMarkup(markup, what) {
  const root = ['root', {}, []];
  const stack = [root];
  const re = /<(\/?)([a-zA-Z][\w:-]*)((?:\s+[\w:-]+="[^"]*")*)\s*(\/?)>|([^<]+)/g;
  for (const m of markup.matchAll(re)) {
    if (m[5] !== undefined) {
      if (m[5].trim()) throw new Error(`build: unexpected text in ${what}`);
      continue;
    }
    const [, closing, tag, attrText, selfClosing] = m;
    if (closing) {
      const open = stack.pop();
      if (!open || open[0] !== tag) throw new Error(`build: mismatched </${tag}> in ${what}`);
      continue;
    }
    const attrs = Object.fromEntries([...attrText.matchAll(/([\w:-]+)="([^"]*)"/g)].map((a) => [a[1], a[2]]));
    const node = [tag, attrs, []];
    stack.at(-1)[2].push(node);
    if (!selfClosing) stack.push(node);
  }
  if (stack.length !== 1) throw new Error(`build: unclosed element in ${what}`);
  if (markup.replace(re, '').trim()) throw new Error(`build: unparsed markup in ${what}`);
  return root[2];
}

const lines = Object.entries(marks).map(
  ([name, { viewBox, inner }]) =>
    `  ${name}: {\n    viewBox: '${viewBox}',\n    nodes: ${JSON.stringify(parseMarkup(inner, name))},\n  },`,
);
/** Formats generated TS with the repository's Biome settings (so `biome check` accepts it as is). */
function formatTs(source, fileName) {
  const biome = createRequire(import.meta.url).resolve('@biomejs/biome/bin/biome');
  return execFileSync(process.execPath, [biome, 'format', `--stdin-file-path=${fileName}`], {
    cwd: HERE,
    input: source,
    encoding: 'utf8',
  });
}

emit(
  join(HERE, 'marks.generated.ts'),
  formatTs(
    `/*
 * The brand kit's single-colour marks (app glyphs, spark, flame) as SVG element trees.
 * GENERATED from brand/ by packages/ui-kit/src/icons/brand/build.mjs (run by tools/import-brand.sh).
 * Do not edit by hand.
 */
import type { SvgNode } from './svg-node.ts';

export const BRAND_MARK_SVG = {
${lines.join('\n')}
} as const satisfies Record<string, { viewBox: string; nodes: readonly SvgNode[] }>;
`,
    'marks.generated.ts',
  ),
);

const TITLES = {
  suite: 'Kiln',
  write: 'Kiln Write',
  grid: 'Kiln Grid',
  deck: 'Kiln Deck',
  notes: 'Kiln Notes',
};
for (const app of APPS) {
  for (const level of LEVELS) {
    // Verbatim, plus an accessible <title> (required by the a11y lint for SVGs in the package).
    const svg = read(`icons/svg/${app}${level}.svg`);
    const titled = svg.includes('<title>')
      ? svg
      : svg.replace(/<svg\b[^>]*>/, (tag) => `${tag}<title>${TITLES[app]}</title>`);
    emit(join(HERE, 'tiles', `${app}${level}.svg`), titled);
  }
}

if (check && stale.length) {
  console.error(`brand icons are stale; run tools/import-brand.sh:\n  ${stale.join('\n  ')}`);
  process.exit(1);
}
if (!check) console.log('ui-kit: brand marks and tiles are up to date with brand/');
