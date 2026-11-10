/**
 * Reports the translated messages of a locale whose ICU syntax, placeholders or plural forms do
 * not match English (`tsx scripts/check-locale.ts de`). The unit test enforces the same rules on
 * every key; this lists the problems while a translation is being written.
 */
import { readFileSync } from 'node:fs';
import { join } from 'node:path';
import IntlMessageFormat from 'intl-messageformat';

type El = { type: number; value?: string; options?: Record<string, { value: El[] }>; children?: El[] };
const dir = join(import.meta.dirname, '..', 'locales');
const load = (l: string) =>
  JSON.parse(readFileSync(join(dir, `${l}.json`), 'utf8')) as Record<string, string>;
const args = (els: El[], out = new Set<string>()): Set<string> => {
  for (const e of els) {
    if (e.type === 0 || e.type === 7) continue;
    if (e.value !== undefined) out.add(`${e.value}:${e.type === 6 ? 'p' : e.type === 5 ? 's' : 'a'}`);
    for (const o of Object.values(e.options ?? {})) args(o.value, out);
    if (e.children) args(e.children, out);
  }
  return out;
};
const plurals = (els: El[], out: Set<string>[] = []): Set<string>[] => {
  for (const e of els) {
    if (e.type === 6) out.push(new Set(Object.keys(e.options ?? {})));
    for (const o of Object.values(e.options ?? {})) plurals(o.value, out);
  }
  return out;
};
const locale = process.argv[2] ?? 'de';
const en = load('en');
const tr = load(locale);
const cats = new Intl.PluralRules(locale).resolvedOptions().pluralCategories;
let bad = 0;
for (const [k, v] of Object.entries(tr)) {
  if (k === '//' || !(k in en)) continue;
  try {
    const a = new IntlMessageFormat(v, locale, undefined, { ignoreTag: true }).getAst() as unknown as El[];
    const b = new IntlMessageFormat(en[k] as string, 'en', undefined, {
      ignoreTag: true,
    }).getAst() as unknown as El[];
    const x = [...args(a)].sort().join();
    const y = [...args(b)].sort().join();
    if (x !== y) {
      bad++;
      console.log(`placeholders ${k}: ${x} ≠ ${y}`);
    }
    for (const p of plurals(a))
      if (!cats.every((c) => p.has(c))) {
        bad++;
        console.log(`plural ${k}: has ${[...p].join(' ')}`);
      }
  } catch (e) {
    bad++;
    console.log(`invalid ${k}: ${(e as Error).message}`);
  }
}
console.log(`${locale}: ${Object.keys(tr).length - 1} messages, ${bad} problems`);
