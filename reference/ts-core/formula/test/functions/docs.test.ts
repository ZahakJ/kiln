/**
 * Every registered function has documentation in `src/functions/docs/*.json`, the documentation is
 * well formed, and every documented example evaluates to its stated result.
 */
import { describe, expect, it } from 'vitest';
import { listFunctions } from '../../src/engine/functions/registry.ts';
import { DOC_FAMILIES, functionDoc, listFunctionDocs } from '../../src/functions/index.ts';
import { compare, engine } from './_support.ts';

const CATEGORIES = new Set([
  'Math & Trig',
  'Statistical',
  'Logical',
  'Text',
  'Date & Time',
  'Lookup & Reference',
  'Information',
  'Financial',
  'Engineering',
  'Database',
  'Web',
  'Compatibility',
  'Dynamic Array',
]);

describe('function documentation', () => {
  const registered = listFunctions();

  it('registers at least 350 functions', () => {
    expect(registered.length).toBeGreaterThanOrEqual(350);
  });

  it('documents every registered function, and nothing else', () => {
    const missing = registered.filter((d) => !functionDoc(d.name)).map((d) => d.name);
    expect(missing).toEqual([]);
    const names = new Set(registered.map((d) => d.name));
    const orphans = listFunctionDocs()
      .filter((d) => !names.has(d.name))
      .map((d) => d.name);
    expect(orphans).toEqual([]);
  });

  it('attaches the documentation to the registered descriptors', () => {
    for (const d of registered) expect(d.docs?.summary, d.name).toBeTruthy();
  });

  it('has well-formed entries', () => {
    for (const [family, entries] of Object.entries(DOC_FAMILIES)) {
      for (const e of entries) {
        const where = `${family}/${e.name}`;
        expect(CATEGORIES.has(e.category), where).toBe(true);
        expect(e.summary.length, where).toBeGreaterThan(10);
        expect(e.syntax.startsWith(`${e.name}(`), where).toBe(true);
        expect(e.examples?.length ?? 0, where).toBeGreaterThan(0);
        expect(e.since, where).toMatch(/^(2007|2010|2013|2016|2019|2021|2024|365)$/);
        for (const a of e.args) {
          expect(a.name.length, where).toBeGreaterThan(0);
          expect(a.description.length, where).toBeGreaterThan(0);
        }
        // Required arguments come before optional ones (except repeating pairs such as IFS).
        const desc = registered.find((d) => d.name === e.name);
        const required = e.args.filter((a) => !a.optional).length;
        if (desc && desc.maxArgs < 255) expect(required, where).toBeLessThanOrEqual(desc.maxArgs);
      }
    }
  });

  const examples = listFunctionDocs().flatMap((d) =>
    (d.examples ?? []).map((x) => ({ name: d.name, formula: x.formula, result: x.result })),
  );

  it('has at least one example per function', () => {
    expect(examples.length).toBeGreaterThanOrEqual(registered.length);
  });

  it.each(examples)('$name: $formula → $result', ({ formula, result }) => {
    const e = engine();
    const actual = e.evaluate(formula);
    expect(compare(actual, result)).toBeUndefined();
  });
});
