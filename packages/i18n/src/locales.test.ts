/**
 * The locale files: German and Arabic carry every English message, each one a valid ICU
 * message with the English placeholders and the plural categories of its language, and the English
 * locale is exactly what the sources use.
 */
import { readFileSync } from 'node:fs';
import { join } from 'node:path';
import IntlMessageFormat from 'intl-messageformat';
import { describe, expect, it } from 'vitest';
import { EN_PATH, extract, readJson } from '../scripts/extract.ts';
import { HEADER_KEY, LOCALES, type Locale } from './index.ts';

const DIR = join(import.meta.dirname, '..', 'locales');

function load(locale: Locale): Record<string, string> {
  return JSON.parse(readFileSync(join(DIR, `${locale}.json`), 'utf8')) as Record<string, string>;
}

const en = load('en');
const keys = Object.keys(en).filter((k) => k !== HEADER_KEY);

/** ICU AST element kinds (intl-messageformat's `TYPE`). */
enum Kind {
  Literal = 0,
  Argument = 1,
  Number = 2,
  Date = 3,
  Time = 4,
  Select = 5,
  Plural = 6,
  Pound = 7,
  Tag = 8,
}

interface Element {
  type: Kind;
  value?: string;
  options?: Record<string, { value: Element[] }>;
  children?: Element[];
}

function ast(message: string, locale: string): Element[] {
  return new IntlMessageFormat(message, locale, undefined, {
    ignoreTag: true,
  }).getAst() as unknown as Element[];
}

/** Every argument of a message with its kind (`count:plural`, `name:argument`). */
function argumentsOf(elements: readonly Element[], out = new Set<string>()): Set<string> {
  for (const e of elements) {
    if (e.type === Kind.Literal || e.type === Kind.Pound) continue;
    if (e.value !== undefined) {
      const kind =
        e.type === Kind.Plural
          ? 'plural'
          : e.type === Kind.Select
            ? 'select'
            : e.type === Kind.Number
              ? 'number'
              : 'arg';
      // A plain argument and a formatted number are interchangeable in a translation.
      out.add(`${e.value}:${kind === 'number' ? 'arg' : kind}`);
    }
    for (const o of Object.values(e.options ?? {})) argumentsOf(o.value, out);
    if (e.children) argumentsOf(e.children, out);
  }
  return out;
}

/** The plural arguments of a message and the categories each one lists. */
function plurals(
  elements: readonly Element[],
  out = new Map<string, Set<string>>(),
): Map<string, Set<string>> {
  for (const e of elements) {
    if (e.type === Kind.Plural && e.value !== undefined) {
      const set = out.get(e.value) ?? new Set<string>();
      for (const k of Object.keys(e.options ?? {})) set.add(k);
      out.set(e.value, set);
    }
    for (const o of Object.values(e.options ?? {})) plurals(o.value, out);
  }
  return out;
}

describe('the English locale', () => {
  it('is what the sources use: every t()/msg() key and command message, nothing stale', () => {
    const expected = extract(readJson(EN_PATH));
    expect(expected.undefinedKeys).toEqual([]);
    const missing = Object.keys(expected.messages).filter((k) => !(k in en));
    const stale = keys.filter((k) => !(k in expected.messages));
    const changed = Object.keys(expected.messages).filter((k) => k in en && en[k] !== expected.messages[k]);
    expect({ missing, stale, changed }).toEqual({ missing: [], stale: [], changed: [] });
  });

  it('holds valid ICU messages', () => {
    const bad = keys.filter((k) => {
      try {
        ast(en[k] as string, 'en');
        return false;
      } catch {
        return true;
      }
    });
    expect(bad).toEqual([]);
  });
});

for (const locale of LOCALES.filter((l) => l !== 'en')) {
  describe(`the ${locale} locale`, () => {
    const messages = load(locale);
    const categories = new Intl.PluralRules(locale).resolvedOptions().pluralCategories;

    it("is marked as machine-drafted and needing review in the file's header", () => {
      expect(Object.keys(messages)[0]).toBe(HEADER_KEY);
      expect(messages[HEADER_KEY]).toMatch(/needs review/i);
    });

    it('has 100 % of the English keys and nothing else', () => {
      const missing = keys.filter((k) => !(k in messages));
      const extra = Object.keys(messages).filter((k) => k !== HEADER_KEY && !(k in en));
      expect({ missing: missing.length, extra }).toEqual({ missing: 0, extra: [] });
      expect(missing).toEqual([]);
    });

    it('translates every message (nothing empty, no placeholder text)', () => {
      const empty = keys.filter((k) => !messages[k]?.trim());
      const placeholder = keys.filter((k) => /\b(TODO|FIXME|TBD|XXX)\b|^\?+$/.test(messages[k] ?? ''));
      expect({ empty, placeholder }).toEqual({ empty: [], placeholder: [] });
    });

    it('uses the English placeholders in every message', () => {
      const mismatched: string[] = [];
      const invalid: string[] = [];
      for (const k of keys) {
        let theirs: Set<string>;
        try {
          theirs = argumentsOf(ast(messages[k] as string, locale));
        } catch {
          invalid.push(k);
          continue;
        }
        const ours = argumentsOf(ast(en[k] as string, 'en'));
        if ([...ours].sort().join() !== [...theirs].sort().join()) mismatched.push(k);
      }
      expect({ invalid, mismatched }).toEqual({ invalid: [], mismatched: [] });
    });

    it(`gives every plural the ${categories.length} categories of the language`, () => {
      const incomplete: string[] = [];
      for (const k of keys) {
        for (const [, listed] of plurals(ast(messages[k] as string, locale))) {
          if (!categories.every((c) => listed.has(c))) incomplete.push(k);
        }
      }
      expect(incomplete).toEqual([]);
      if (locale === 'ar') expect(categories).toHaveLength(6);
    });
  });
}
