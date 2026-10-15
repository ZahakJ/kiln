/** Shared helpers: units, symbol mapping, XML fragments, and the worker-compatibility guarantee. */
import { readdirSync, readFileSync, statSync } from 'node:fs';
import { join } from 'node:path';
import { describe, expect, it } from 'vitest';
import {
  fragmentXml,
  parseFragment,
  parseHalfPoints,
  parseTableWidth,
  parseTwips,
  symbolText,
  symbolToUnicode,
} from '../../src/index.ts';

describe('units', () => {
  it('parses twips from integers and universal measures', () => {
    expect(parseTwips('1440')).toBe(1440);
    expect(parseTwips('1in')).toBe(1440);
    expect(parseTwips('2.54cm')).toBe(1440);
    expect(parseTwips('12pt')).toBe(240);
    expect(parseTwips('-0.5in')).toBe(-720);
    expect(parseTwips('')).toBeUndefined();
    expect(parseTwips('abc')).toBeUndefined();
  });

  it('parses half-points', () => {
    expect(parseHalfPoints('24')).toBe(24);
    expect(parseHalfPoints('10.5pt')).toBe(21);
  });

  it('parses table widths in every type and notation', () => {
    expect(parseTableWidth('5000', 'pct')).toEqual({ type: 'pct', value: 5000 });
    expect(parseTableWidth('100%', 'pct')).toEqual({ type: 'pct', value: 5000 });
    expect(parseTableWidth('33.3%', undefined)).toEqual({ type: 'pct', value: 1665 });
    expect(parseTableWidth('2in', 'dxa')).toEqual({ type: 'dxa', value: 2880 });
    expect(parseTableWidth('1000', undefined)).toEqual({ type: 'dxa', value: 1000 });
    expect(parseTableWidth('0', 'auto')).toEqual({ type: 'auto', value: 0 });
    expect(parseTableWidth(undefined, 'nil')).toEqual({ type: 'nil', value: 0 });
    expect(parseTableWidth(undefined, 'dxa')).toBeUndefined();
  });
});

describe('symbols', () => {
  it('maps Symbol and Wingdings private-use codes to Unicode', () => {
    expect(symbolToUnicode('Symbol', 'F061')).toBe('α');
    expect(symbolToUnicode('Symbol', '61')).toBe('α');
    expect(symbolToUnicode('Wingdings', 'F04A')).toBe('☺');
    expect(symbolToUnicode('wingdings', 'f0fc')).toBe('✓');
    expect(symbolToUnicode('Unknown Font', 'F041')).toBeUndefined();
    expect(symbolToUnicode('Symbol', 'zz')).toBeUndefined();
  });

  it('falls back to the raw code point', () => {
    expect(symbolText('Marlett', 'F031')).toBe('');
    expect(symbolText('Marlett', 'nope')).toBe('');
  });
});

describe('XML fragments', () => {
  it('omits declarations of stable Office prefixes and round-trips', () => {
    const el = parseFragment('<w:rPr><w:b/><w14:ligatures w14:val="standard"/></w:rPr>');
    expect(fragmentXml(el)).toBe('<w:rPr><w:b/><w14:ligatures w14:val="standard"/></w:rPr>');
  });

  it('declares foreign prefixes from the part scope', () => {
    const el = {
      type: 'element' as const,
      name: 'foo:bar',
      attrs: Object.assign(Object.create(null), { 'foo:x': '1' }),
      children: [],
    };
    expect(fragmentXml(el, { 'xmlns:foo': 'urn:foo' })).toBe('<foo:bar xmlns:foo="urn:foo" foo:x="1"/>');
  });
});

describe('worker compatibility', () => {
  const srcDir = join(import.meta.dirname, '../../src');
  const files: string[] = [];
  const walk = (d: string) => {
    for (const f of readdirSync(d)) {
      const p = join(d, f);
      if (statSync(p).isDirectory()) walk(p);
      else if (p.endsWith('.ts')) files.push(p);
    }
  };
  walk(srcDir);

  it('uses no DOM or Node-only APIs in the reader', () => {
    expect(files.length).toBeGreaterThan(10);
    const forbidden =
      /(?<![\w:])(document\.|window\.|DOMParser|XMLSerializer|Buffer\b|require\(|process\.)|from 'node:/;
    for (const f of files) {
      const code = readFileSync(f, 'utf8').replace(/\/\*[\s\S]*?\*\/|\/\/.*$/gm, '');
      expect(forbidden.test(code), f).toBe(false);
    }
  });
});
