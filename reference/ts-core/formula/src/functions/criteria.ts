/**
 * Criteria for COUNTIF(S), SUMIF(S), AVERAGEIF(S), MAXIFS, MINIFS and the database functions.
 *
 * A criterion is a number (`5`), a logical, an error, or text that may start with a comparison
 * operator (`">=10"`, `"<>apple"`, `"=*.xlsx"`). Excel's rules, reproduced here:
 *
 * - The operand after the operator is read as a number when it looks like one, including dates and
 *   times in text (`">=2024-01-01"`), percentages and currency; as a logical for `TRUE`/`FALSE`; as an
 *   error for `#N/A` and friends; otherwise as text.
 * - Numeric operands match numbers (and text that reads as the same number for `=`).
 * - Text operands with `=`/`<>` (or no operator) use wildcards (`*`, `?`, `~` escapes) and ignore case;
 *   `<`, `>`, `<=`, `>=` compare text alphabetically against text cells only.
 * - `""` or `"="` matches empty cells; `"<>"` matches non-empty cells.
 */
import { wildcardRegex } from '../engine/functions/builtins/lookup.ts';
import { parseNumberText } from '../values/coerce.ts';
import { compareText } from '../values/compare.ts';
import { ErrorValue, errorFromCode } from '../values/errors.ts';
import { round15 } from '../values/number.ts';
import { BLANK, type Scalar } from '../values/types.ts';

export type Predicate = (v: Scalar) => boolean;

type Op = '=' | '<>' | '<' | '>' | '<=' | '>=';

function cmpNum(op: Op, a: number, b: number): boolean {
  const x = round15(a);
  const y = round15(b);
  switch (op) {
    case '=':
      return x === y;
    case '<>':
      return x !== y;
    case '<':
      return x < y;
    case '>':
      return x > y;
    case '<=':
      return x <= y;
    default:
      return x >= y;
  }
}

function cmpOrder(op: Op, c: number): boolean {
  switch (op) {
    case '<':
      return c < 0;
    case '>':
      return c > 0;
    case '<=':
      return c <= 0;
    case '>=':
      return c >= 0;
    case '=':
      return c === 0;
    default:
      return c !== 0;
  }
}

const cache = new Map<string, Predicate>();

/** Builds a predicate for a criterion value. */
export function parseCriterion(crit: Scalar, date1904 = false): Predicate {
  if (typeof crit === 'number') return numberPredicate('=', crit);
  if (typeof crit === 'boolean') return (v) => v === crit;
  if (crit instanceof ErrorValue) return (v) => v instanceof ErrorValue && v.code === crit.code;
  if (crit === BLANK) return numberPredicate('=', 0);
  const key = `${date1904 ? 1 : 0}${crit}`;
  const hit = cache.get(key);
  if (hit) return hit;
  const p = textCriterion(crit as string, date1904);
  if (cache.size > 1000) cache.clear();
  cache.set(key, p);
  return p;
}

function numberPredicate(op: Op, n: number): Predicate {
  return (v) => {
    if (typeof v === 'number') return cmpNum(op, v, n);
    if (op === '<>') {
      if (typeof v === 'string') {
        const p = parseNumberText(v);
        return p === undefined || !cmpNum('=', p, n);
      }
      return true;
    }
    if (op === '=' && typeof v === 'string') {
      const p = parseNumberText(v);
      return p !== undefined && cmpNum('=', p, n);
    }
    return false;
  };
}

function textCriterion(raw: string, date1904: boolean): Predicate {
  let op: Op = '=';
  let rest = raw;
  const m = /^(<=|>=|<>|=|<|>)/.exec(raw);
  if (m) {
    op = m[1] as Op;
    rest = raw.slice(op.length);
  }
  if (rest === '') {
    if (op === '=') return (v) => v === BLANK || v === '';
    if (op === '<>') return (v) => v !== BLANK; // a formula returning "" is not empty
    // "<" or ">" with nothing after: compares against empty text; matches nothing useful.
    return () => false;
  }
  const n = parseNumberText(rest, { date1904 });
  if (n !== undefined) return numberPredicate(op, n);
  const upper = rest.toUpperCase();
  if (upper === 'TRUE' || upper === 'FALSE') {
    const b = upper === 'TRUE';
    if (op === '=') return (v) => v === b;
    if (op === '<>') return (v) => v !== b;
    return (v) => typeof v === 'boolean' && cmpOrder(op, (v ? 1 : 0) - (b ? 1 : 0));
  }
  const err = errorFromCode(rest);
  if (err) {
    if (op === '=') return (v) => v instanceof ErrorValue && v.code === err.code;
    if (op === '<>') return (v) => !(v instanceof ErrorValue && v.code === err.code);
    return () => false;
  }
  if (op === '=' || op === '<>') {
    const re = wildcardRegex(rest);
    const eq = (v: Scalar) => typeof v === 'string' && re.test(v);
    return op === '=' ? eq : (v) => !eq(v);
  }
  return (v) => typeof v === 'string' && cmpOrder(op, compareText(v, rest));
}
