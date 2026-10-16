/**
 * Engineering functions: base conversions, bitwise operations, complex numbers, CONVERT, the error
 * function and Bessel functions. Text results match Excel exactly (upper-case hex digits,
 * 15-significant-digit complex numbers).
 */
import type { CallContext } from '../engine/context.ts';
import type { Arg, FunctionDescriptor } from '../engine/functions/registry.ts';
import { toNumber, toText } from '../values/coerce.ts';
import { Errors, ErrorValue } from '../values/errors.ts';
import { numberToText } from '../values/number.ts';
import { BLANK, MISSING, type Scalar } from '../values/types.ts';
import { besselI, besselJ, besselK, besselY } from './bessel.ts';
import { fn, isErr, num, numeric, scalarsOf, str, VARIADIC } from './lib.ts';
import { erf, erfc } from './special.ts';
import { CONVERT_UNITS } from './units.ts';

type R = number | string | ErrorValue;

// ── base conversions ─────────────────────────────────────────────────────

interface Base {
  radix: number;
  /** Two's-complement width in bits (10 digits). */
  bits: number;
  re: RegExp;
}

const BIN: Base = { radix: 2, bits: 10, re: /^[01]{1,10}$/ };
const OCT: Base = { radix: 8, bits: 30, re: /^[0-7]{1,10}$/ };
const HEX: Base = { radix: 16, bits: 40, re: /^[0-9A-Fa-f]{1,10}$/ };

/** The input text of a base conversion (numbers are read as their digits). */
function digitsArg(a: Arg | undefined): string | ErrorValue {
  if (a === undefined || a === MISSING) return '';
  const v = a as Scalar;
  if (typeof v === 'boolean') return Errors.VALUE;
  if (typeof v === 'number') {
    if (v < 0 || !Number.isInteger(v)) return Errors.NUM;
    return numberToText(v);
  }
  const t = toText(v);
  return t;
}

function parseBase(text: string, base: Base): number | ErrorValue {
  const s = text.trim();
  if (s === '') return 0;
  if (!base.re.test(s)) return Errors.NUM;
  const v = Number.parseInt(s, base.radix);
  const limit = 2 ** base.bits;
  // Ten digits with the top bit set are negative (two's complement).
  return s.length === 10 && v >= limit / 2 ? v - limit : v;
}

function formatBase(value: number, base: Base, places: Arg | undefined, min: number, max: number): R {
  if (!Number.isFinite(value) || value < min || value > max) return Errors.NUM;
  const v = Math.trunc(value);
  if (v < 0) return (2 ** base.bits + v).toString(base.radix).toUpperCase();
  const s = v.toString(base.radix).toUpperCase();
  if (places === undefined || places === MISSING) return s;
  const p = toNumber(places as Scalar);
  if (isErr(p)) return p;
  const n = Math.trunc(p);
  if (n < 0 || n > 10 || n < s.length) return Errors.NUM;
  return s.padStart(n, '0');
}

const RANGES: Record<number, [number, number]> = {
  2: [-512, 511],
  8: [-(2 ** 29), 2 ** 29 - 1],
  16: [-(2 ** 39), 2 ** 39 - 1],
};

function convertBase(from: Base, to: Base | 'dec'): FunctionDescriptor['impl'] {
  return (args) => {
    const t = digitsArg(args[0]);
    if (isErr(t)) return t;
    const v = parseBase(t, from);
    if (isErr(v)) return v;
    if (to === 'dec') return v;
    const [min, max] = RANGES[to.radix] as [number, number];
    return formatBase(v, to, args[1], min, max);
  };
}

function fromDecimal(to: Base): FunctionDescriptor['impl'] {
  return (args) => {
    const v = args[0];
    if (typeof v === 'boolean') return Errors.VALUE;
    const n = num(v);
    if (isErr(n)) return n;
    const [min, max] = RANGES[to.radix] as [number, number];
    return formatBase(Math.trunc(n), to, args[1], min, max);
  };
}

// ── bitwise ──────────────────────────────────────────────────────────────

const BIT_LIMIT = 2 ** 48;

function bitArg(x: number): number | ErrorValue {
  if (x < 0 || x >= BIT_LIMIT || !Number.isInteger(x)) return Errors.NUM;
  return x;
}

function bitwise(a: number, b: number, op: 'and' | 'or' | 'xor'): number {
  const hiA = Math.floor(a / 2 ** 24);
  const loA = a % 2 ** 24;
  const hiB = Math.floor(b / 2 ** 24);
  const loB = b % 2 ** 24;
  const f = (x: number, y: number) => (op === 'and' ? x & y : op === 'or' ? x | y : x ^ y);
  return f(hiA, hiB) * 2 ** 24 + f(loA, loB);
}

// ── complex numbers ──────────────────────────────────────────────────────

interface Complex {
  re: number;
  im: number;
  unit: 'i' | 'j' | '';
}

const NUM = '(?:\\d+\\.?\\d*|\\.\\d+)(?:[eE][+-]?\\d+)?';
const COMPLEX_RE = new RegExp(`^([+-]?${NUM})?(?:([+-])(${NUM})?([ij]))?$`);
const IMAG_ONLY_RE = new RegExp(`^([+-]?)(${NUM})?([ij])$`);

/** Parses Excel complex text ("3+4i", "-2j", "i", "5"); numbers are real values. */
export function parseComplex(v: Scalar): Complex | ErrorValue {
  if (v instanceof ErrorValue) return v;
  if (typeof v === 'number') return { re: v, im: 0, unit: '' };
  if (typeof v === 'boolean') return Errors.VALUE;
  if (v === BLANK) return { re: 0, im: 0, unit: '' };
  const s = (v as string).trim();
  if (s === '') return { re: 0, im: 0, unit: '' };
  let m = IMAG_ONLY_RE.exec(s);
  if (m) {
    const mag = m[2] === undefined ? 1 : Number(m[2]);
    return { re: 0, im: m[1] === '-' ? -mag : mag, unit: m[3] as 'i' | 'j' };
  }
  m = COMPLEX_RE.exec(s);
  if (!m || m[1] === undefined) return Errors.NUM;
  const re = Number(m[1]);
  if (m[4] === undefined) return { re, im: 0, unit: '' };
  const mag = m[3] === undefined ? 1 : Number(m[3]);
  return { re, im: m[2] === '-' ? -mag : mag, unit: m[4] as 'i' | 'j' };
}

/** Formats like Excel: 15 significant digits, unit coefficient 1 omitted, zero parts dropped. */
export function formatComplex(re: number, im: number, unit: 'i' | 'j' | ''): string | ErrorValue {
  if (!Number.isFinite(re) || !Number.isFinite(im)) return Errors.NUM;
  const u = unit || 'i';
  const r = Number(re.toPrecision(15));
  const i = Number(im.toPrecision(15));
  if (i === 0) return numberToText(r);
  const imText = i === 1 ? '' : i === -1 ? '-' : numberToText(i);
  if (r === 0) return `${imText}${u}`;
  return `${numberToText(r)}${i > 0 ? '+' : ''}${imText}${u}`;
}

function cArg(a: Arg | undefined): Complex | ErrorValue {
  return parseComplex(a === undefined || a === MISSING ? BLANK : (a as Scalar));
}

function unitOf(...zs: Complex[]): 'i' | 'j' | '' | ErrorValue {
  let u: 'i' | 'j' | '' = '';
  for (const z of zs) {
    if (z.unit === '') continue;
    if (u !== '' && u !== z.unit) return Errors.VALUE;
    u = z.unit;
  }
  return u;
}

type C = [number, number];
const cmul = ([a, b]: C, [c, d]: C): C => [a * c - b * d, a * d + b * c];
const cdiv = ([a, b]: C, [c, d]: C): C => {
  const den = c * c + d * d;
  return [(a * c + b * d) / den, (b * c - a * d) / den];
};
const cexp = ([a, b]: C): C => [Math.exp(a) * Math.cos(b), Math.exp(a) * Math.sin(b)];
const cln = ([a, b]: C): C => [Math.log(Math.hypot(a, b)), Math.atan2(b, a)];
const csin = ([a, b]: C): C => [Math.sin(a) * Math.cosh(b), Math.cos(a) * Math.sinh(b)];
const ccos = ([a, b]: C): C => [Math.cos(a) * Math.cosh(b), -Math.sin(a) * Math.sinh(b)];
const csinh = ([a, b]: C): C => [Math.sinh(a) * Math.cos(b), Math.cosh(a) * Math.sin(b)];
const ccosh = ([a, b]: C): C => [Math.cosh(a) * Math.cos(b), Math.sinh(a) * Math.sin(b)];
const ONE: C = [1, 0];

/** A complex function of one argument returning a complex number. */
function c1(name: string, f: (z: C) => C | ErrorValue): FunctionDescriptor {
  return fn(
    name,
    1,
    1,
    (args) => {
      const z = cArg(args[0]);
      if (isErr(z)) return z;
      const r = f([z.re, z.im]);
      return isErr(r) ? r : formatComplex(r[0], r[1], z.unit);
    },
    { lift: 'all' },
  );
}

function complexFold(args: Arg[], ctx: CallContext, product: boolean): R {
  let acc: C = product ? [1, 0] : [0, 0];
  let unit: 'i' | 'j' | '' = '';
  for (const a of args) {
    for (const v of scalarsOf(a, ctx)) {
      if (v === BLANK) continue;
      const z = parseComplex(v);
      if (isErr(z)) return z;
      const u = unitOf({ re: 0, im: 0, unit }, z);
      if (isErr(u)) return u;
      unit = u;
      acc = product ? cmul(acc, [z.re, z.im]) : [acc[0] + z.re, acc[1] + z.im];
    }
  }
  return formatComplex(acc[0], acc[1], unit);
}

// ── Bessel ───────────────────────────────────────────────────────────────

const BESSEL = { BESSELI: besselI, BESSELJ: besselJ, BESSELK: besselK, BESSELY: besselY };

function bessel(name: keyof typeof BESSEL): FunctionDescriptor {
  const impl = BESSEL[name];
  return numeric(name, 2, 2, (x, n0) => {
    const n = Math.trunc(n0);
    if (n < 0) return Errors.NUM;
    if ((name === 'BESSELK' || name === 'BESSELY') && x <= 0) return Errors.NUM;
    const r = impl(x, n);
    return Number.isFinite(r) ? r : Errors.NUM;
  });
}

// ── CONVERT ──────────────────────────────────────────────────────────────

const PREFIXES: Record<string, number> = {
  Y: 1e24,
  Z: 1e21,
  E: 1e18,
  P: 1e15,
  T: 1e12,
  G: 1e9,
  M: 1e6,
  k: 1e3,
  h: 1e2,
  da: 1e1,
  e: 1e1,
  d: 1e-1,
  c: 1e-2,
  m: 1e-3,
  u: 1e-6,
  n: 1e-9,
  p: 1e-12,
  f: 1e-15,
  a: 1e-18,
  z: 1e-21,
  y: 1e-24,
};
const BINARY_PREFIXES: Record<string, number> = {
  ki: 2 ** 10,
  Mi: 2 ** 20,
  Gi: 2 ** 30,
  Ti: 2 ** 40,
  Pi: 2 ** 50,
  Ei: 2 ** 60,
  Zi: 2 ** 70,
  Yi: 2 ** 80,
};

interface UnitHit {
  cat: string;
  factor: number;
  offset?: number;
  temp?: string;
}

function lookupUnit(u: string): UnitHit | undefined {
  const direct = CONVERT_UNITS[u];
  if (direct) return { cat: direct.cat, factor: direct.factor, temp: direct.temp };
  for (const [p, f] of Object.entries(BINARY_PREFIXES)) {
    if (u.startsWith(p)) {
      const base = CONVERT_UNITS[u.slice(p.length)];
      if (base && base.cat === 'info') return { cat: base.cat, factor: base.factor * f };
    }
  }
  for (const [p, f] of Object.entries(PREFIXES)) {
    if (u.startsWith(p) && u.length > p.length) {
      const base = CONVERT_UNITS[u.slice(p.length)];
      if (base?.prefix)
        return { cat: base.cat, factor: base.factor * f ** (base.power ?? 1), temp: base.temp };
    }
  }
  return undefined;
}

const TO_KELVIN: Record<string, (x: number) => number> = {
  C: (x) => x + 273.15,
  F: (x) => ((x - 32) * 5) / 9 + 273.15,
  K: (x) => x,
  Rank: (x) => (x * 5) / 9,
  Reau: (x) => x * 1.25 + 273.15,
};
const FROM_KELVIN: Record<string, (x: number) => number> = {
  C: (x) => x - 273.15,
  F: (x) => ((x - 273.15) * 9) / 5 + 32,
  K: (x) => x,
  Rank: (x) => (x * 9) / 5,
  Reau: (x) => (x - 273.15) * 0.8,
};

function convert(args: Arg[]): R {
  const x = num(args[0]);
  if (isErr(x)) return x;
  const from = str(args[1]);
  if (isErr(from)) return from;
  const to = str(args[2]);
  if (isErr(to)) return to;
  const a = lookupUnit(from);
  const b = lookupUnit(to);
  if (!a || !b || a.cat !== b.cat) return Errors.NA;
  if (a.cat === 'temperature') {
    const k = (TO_KELVIN[a.temp as string] as (x: number) => number)(x * a.factor);
    return (FROM_KELVIN[b.temp as string] as (x: number) => number)(k) / b.factor;
  }
  return Number(((x * a.factor) / b.factor).toPrecision(15));
}

export const engineeringFunctions: FunctionDescriptor[] = [
  fn('BIN2DEC', 1, 1, convertBase(BIN, 'dec'), { lift: 'all' }),
  fn('BIN2HEX', 1, 2, convertBase(BIN, HEX), { lift: 'all' }),
  fn('BIN2OCT', 1, 2, convertBase(BIN, OCT), { lift: 'all' }),
  fn('OCT2DEC', 1, 1, convertBase(OCT, 'dec'), { lift: 'all' }),
  fn('OCT2BIN', 1, 2, convertBase(OCT, BIN), { lift: 'all' }),
  fn('OCT2HEX', 1, 2, convertBase(OCT, HEX), { lift: 'all' }),
  fn('HEX2DEC', 1, 1, convertBase(HEX, 'dec'), { lift: 'all' }),
  fn('HEX2BIN', 1, 2, convertBase(HEX, BIN), { lift: 'all' }),
  fn('HEX2OCT', 1, 2, convertBase(HEX, OCT), { lift: 'all' }),
  fn('DEC2BIN', 1, 2, fromDecimal(BIN), { lift: 'all' }),
  fn('DEC2OCT', 1, 2, fromDecimal(OCT), { lift: 'all' }),
  fn('DEC2HEX', 1, 2, fromDecimal(HEX), { lift: 'all' }),
  numeric('BITAND', 2, 2, (a, b) => {
    const x = bitArg(a);
    if (isErr(x)) return x;
    const y = bitArg(b);
    return isErr(y) ? y : bitwise(x, y, 'and');
  }),
  numeric('BITOR', 2, 2, (a, b) => {
    const x = bitArg(a);
    if (isErr(x)) return x;
    const y = bitArg(b);
    return isErr(y) ? y : bitwise(x, y, 'or');
  }),
  numeric('BITXOR', 2, 2, (a, b) => {
    const x = bitArg(a);
    if (isErr(x)) return x;
    const y = bitArg(b);
    return isErr(y) ? y : bitwise(x, y, 'xor');
  }),
  ...(['BITLSHIFT', 'BITRSHIFT'] as const).map((name) =>
    numeric(name, 2, 2, (a, s0) => {
      const x = bitArg(a);
      if (isErr(x)) return x;
      const s = Math.trunc(s0);
      if (Math.abs(s) > 53) return Errors.NUM;
      const shift = name === 'BITLSHIFT' ? s : -s;
      const r = shift >= 0 ? x * 2 ** shift : Math.floor(x / 2 ** -shift);
      return r >= BIT_LIMIT ? Errors.NUM : r;
    }),
  ),
  numeric('DELTA', 1, 2, (a, b) => (a === b ? 1 : 0), [0]),
  numeric('GESTEP', 1, 2, (a, b) => (a >= b ? 1 : 0), [0]),
  fn(
    'ERF',
    1,
    2,
    (args) => {
      const lo = num(args[0]);
      if (isErr(lo)) return lo;
      if (args[1] === undefined) return erf(lo);
      const hi = num(args[1]);
      return isErr(hi) ? hi : erf(hi) - erf(lo);
    },
    { lift: 'all' },
  ),
  numeric('ERF.PRECISE', 1, 1, (x) => erf(x)),
  numeric('ERFC', 1, 1, (x) => erfc(x)),
  numeric('ERFC.PRECISE', 1, 1, (x) => erfc(x)),
  bessel('BESSELI'),
  bessel('BESSELJ'),
  bessel('BESSELK'),
  bessel('BESSELY'),
  fn('CONVERT', 3, 3, convert, { lift: 'all' }),
  fn(
    'COMPLEX',
    2,
    3,
    (args) => {
      const re = num(args[0]);
      if (isErr(re)) return re;
      const im = num(args[1]);
      if (isErr(im)) return im;
      const u = args[2] === undefined || args[2] === MISSING ? 'i' : str(args[2]);
      if (isErr(u)) return u;
      if (u !== 'i' && u !== 'j') return Errors.VALUE;
      return formatComplex(re, im, u);
    },
    { lift: 'all' },
  ),
  fn(
    'IMREAL',
    1,
    1,
    (args) => {
      const z = cArg(args[0]);
      return isErr(z) ? z : z.re;
    },
    { lift: 'all' },
  ),
  fn(
    'IMAGINARY',
    1,
    1,
    (args) => {
      const z = cArg(args[0]);
      return isErr(z) ? z : z.im;
    },
    { lift: 'all' },
  ),
  fn(
    'IMABS',
    1,
    1,
    (args) => {
      const z = cArg(args[0]);
      return isErr(z) ? z : Math.hypot(z.re, z.im);
    },
    { lift: 'all' },
  ),
  fn(
    'IMARGUMENT',
    1,
    1,
    (args) => {
      const z = cArg(args[0]);
      if (isErr(z)) return z;
      return z.re === 0 && z.im === 0 ? Errors.DIV0 : Math.atan2(z.im, z.re);
    },
    { lift: 'all' },
  ),
  c1('IMCONJUGATE', ([a, b]) => [a, -b]),
  c1('IMEXP', cexp),
  c1('IMLN', (z) => (z[0] === 0 && z[1] === 0 ? Errors.NUM : cln(z))),
  c1('IMLOG10', (z) => {
    if (z[0] === 0 && z[1] === 0) return Errors.NUM;
    const [a, b] = cln(z);
    return [a / Math.LN10, b / Math.LN10];
  }),
  c1('IMLOG2', (z) => {
    if (z[0] === 0 && z[1] === 0) return Errors.NUM;
    const [a, b] = cln(z);
    return [a / Math.LN2, b / Math.LN2];
  }),
  c1('IMSQRT', ([a, b]) => {
    const r = Math.sqrt(Math.hypot(a, b));
    const t = Math.atan2(b, a) / 2;
    return [r * Math.cos(t), r * Math.sin(t)];
  }),
  c1('IMSIN', csin),
  c1('IMCOS', ccos),
  c1('IMTAN', (z) => cdiv(csin(z), ccos(z))),
  c1('IMCOT', (z) => (z[0] === 0 && z[1] === 0 ? Errors.NUM : cdiv(ccos(z), csin(z)))),
  c1('IMSEC', (z) => cdiv(ONE, ccos(z))),
  c1('IMCSC', (z) => (z[0] === 0 && z[1] === 0 ? Errors.NUM : cdiv(ONE, csin(z)))),
  c1('IMSINH', csinh),
  c1('IMCOSH', ccosh),
  c1('IMSECH', (z) => cdiv(ONE, ccosh(z))),
  c1('IMCSCH', (z) => (z[0] === 0 && z[1] === 0 ? Errors.NUM : cdiv(ONE, csinh(z)))),
  fn(
    'IMPOWER',
    2,
    2,
    (args) => {
      const z = cArg(args[0]);
      if (isErr(z)) return z;
      const n = num(args[1]);
      if (isErr(n)) return n;
      if (z.re === 0 && z.im === 0) return n > 0 ? '0' : Errors.NUM;
      const r = Math.hypot(z.re, z.im) ** n;
      const t = Math.atan2(z.im, z.re) * n;
      return formatComplex(r * Math.cos(t), r * Math.sin(t), z.unit);
    },
    { lift: 'all' },
  ),
  fn(
    'IMDIV',
    2,
    2,
    (args) => {
      const a = cArg(args[0]);
      if (isErr(a)) return a;
      const b = cArg(args[1]);
      if (isErr(b)) return b;
      const u = unitOf(a, b);
      if (isErr(u)) return u;
      if (b.re === 0 && b.im === 0) return Errors.NUM;
      const [re, im] = cdiv([a.re, a.im], [b.re, b.im]);
      return formatComplex(re, im, u);
    },
    { lift: 'all' },
  ),
  fn(
    'IMSUB',
    2,
    2,
    (args) => {
      const a = cArg(args[0]);
      if (isErr(a)) return a;
      const b = cArg(args[1]);
      if (isErr(b)) return b;
      const u = unitOf(a, b);
      if (isErr(u)) return u;
      return formatComplex(a.re - b.re, a.im - b.im, u);
    },
    { lift: 'all' },
  ),
  fn('IMSUM', 1, VARIADIC, (args, ctx) => complexFold(args, ctx, false)),
  fn('IMPRODUCT', 1, VARIADIC, (args, ctx) => complexFold(args, ctx, true)),
];
