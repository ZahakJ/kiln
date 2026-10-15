/**
 * Probability distributions (Statistical) and their pre-2010 compatibility names.
 */
import type { FunctionDescriptor } from '../engine/functions/registry.ts';
import { Errors, type ErrorValue } from '../values/errors.ts';
import { numeric } from './lib.ts';
import {
  betaCdf,
  betaI,
  betaPdf,
  binomCdf,
  binomPmf,
  chisqCdf,
  chisqPdf,
  fCdf,
  fPdf,
  gammaCdf,
  gammaPdf,
  gammaQ,
  hypgeomPmf,
  invertCdf,
  negbinomPmf,
  normCdf,
  normInv,
  normPdf,
  poissonCdf,
  poissonPmf,
  tCdf,
  tPdf,
} from './special.ts';

type R = number | ErrorValue;
const t = Math.trunc;

// ── normal family ────────────────────────────────────────────────────────

function normDist(x: number, m: number, sd: number, cum: number): R {
  if (sd <= 0) return Errors.NUM;
  const z = (x - m) / sd;
  return cum ? normCdf(z) : normPdf(z) / sd;
}

function normInvFn(p: number, m: number, sd: number): R {
  if (p <= 0 || p >= 1 || sd <= 0) return Errors.NUM;
  return m + sd * normInv(p);
}

function lognormDist(x: number, m: number, sd: number, cum: number): R {
  if (x <= 0 || sd <= 0) return Errors.NUM;
  const z = (Math.log(x) - m) / sd;
  return cum ? normCdf(z) : normPdf(z) / (sd * x);
}

function lognormInv(p: number, m: number, sd: number): R {
  if (p <= 0 || p >= 1 || sd <= 0) return Errors.NUM;
  return Math.exp(m + sd * normInv(p));
}

// ── t ────────────────────────────────────────────────────────────────────

function tInvLeft(p: number, df: number): number {
  if (p === 0.5) return 0;
  if (p < 0.5) return -tInvLeft(1 - p, df);
  // Solve on the upper tail for precision.
  const q = 1 - p;
  return invertCdf((x) => -(1 - tCdf(x, df)), -q, 0, 1);
}

function tDist(x: number, df0: number, cum: number): R {
  const df = t(df0);
  if (df < 1) return Errors.NUM;
  return cum ? tCdf(x, df) : tPdf(x, df);
}

function tDistRT(x: number, df0: number): R {
  const df = t(df0);
  if (df < 1) return Errors.NUM;
  return 1 - tCdf(x, df);
}

function tDist2T(x: number, df0: number): R {
  const df = t(df0);
  if (df < 1 || x < 0) return Errors.NUM;
  return 2 * (1 - tCdf(x, df));
}

function tdistLegacy(x: number, df0: number, tails0: number): R {
  const df = t(df0);
  const tails = t(tails0);
  if (df < 1 || x < 0 || (tails !== 1 && tails !== 2)) return Errors.NUM;
  return tails * (1 - tCdf(x, df));
}

function tInv(p: number, df0: number): R {
  const df = t(df0);
  if (p <= 0 || p >= 1 || df < 1) return Errors.NUM;
  return tInvLeft(p, df);
}

function tInv2T(p: number, df0: number): R {
  const df = t(df0);
  if (p <= 0 || p > 1 || df < 1) return Errors.NUM;
  return tInvLeft(1 - p / 2, df);
}

// ── chi-squared ──────────────────────────────────────────────────────────

function chisqDist(x: number, df0: number, cum: number): R {
  const df = t(df0);
  if (x < 0 || df < 1 || df > 1e10) return Errors.NUM;
  return cum ? chisqCdf(x, df) : chisqPdf(x, df);
}

function chisqDistRT(x: number, df0: number): R {
  const df = t(df0);
  if (x < 0 || df < 1 || df > 1e10) return Errors.NUM;
  return gammaQ(df / 2, x / 2);
}

function chisqInv(p: number, df0: number): R {
  const df = t(df0);
  if (p < 0 || p >= 1 || df < 1 || df > 1e10) return Errors.NUM;
  if (p === 0) return 0;
  return invertCdf((x) => chisqCdf(x, df), p, 0, Math.max(1, df));
}

function chisqInvRT(p: number, df0: number): R {
  const df = t(df0);
  if (p <= 0 || p > 1 || df < 1 || df > 1e10) return Errors.NUM;
  if (p === 1) return 0;
  // Solve 1 - cdf = p on the upper tail.
  return invertCdf((x) => -gammaQ(df / 2, x / 2), -p, 0, Math.max(1, df));
}

// ── F ────────────────────────────────────────────────────────────────────

function fDist(x: number, a: number, b: number, cum: number): R {
  const d1 = t(a);
  const d2 = t(b);
  if (x < 0 || d1 < 1 || d2 < 1) return Errors.NUM;
  return cum ? fCdf(x, d1, d2) : fPdf(x, d1, d2);
}

function fDistRT(x: number, a: number, b: number): R {
  const d1 = t(a);
  const d2 = t(b);
  if (x < 0 || d1 < 1 || d2 < 1) return Errors.NUM;
  if (x === 0) return 1;
  return betaI(d2 / (d2 + d1 * x), d2 / 2, d1 / 2);
}

function fInv(p: number, a: number, b: number): R {
  const d1 = t(a);
  const d2 = t(b);
  if (p < 0 || p > 1 || d1 < 1 || d2 < 1) return Errors.NUM;
  if (p === 0) return 0;
  if (p === 1) return Errors.NUM;
  return invertCdf((x) => fCdf(x, d1, d2), p, 0, 1);
}

function fInvRT(p: number, a: number, b: number): R {
  const d1 = t(a);
  const d2 = t(b);
  if (p <= 0 || p > 1 || d1 < 1 || d2 < 1) return Errors.NUM;
  if (p === 1) return 0;
  return invertCdf((x) => -betaI(d2 / (d2 + d1 * x), d2 / 2, d1 / 2), -p, 0, 1);
}

// ── beta, gamma, exponential, Weibull ────────────────────────────────────

function betaDist(x: number, a: number, b: number, cum: number, lo: number, hi: number): R {
  if (a <= 0 || b <= 0 || x < lo || x > hi || lo === hi) return Errors.NUM;
  const z = (x - lo) / (hi - lo);
  return cum ? betaCdf(z, a, b) : betaPdf(z, a, b) / (hi - lo);
}

function betaInv(p: number, a: number, b: number, lo: number, hi: number): R {
  if (p <= 0 || p > 1 || a <= 0 || b <= 0 || lo >= hi) return Errors.NUM;
  if (p === 1) return hi;
  const z = invertCdf((x) => betaCdf(x, a, b), p, 0, 1, false);
  return lo + z * (hi - lo);
}

function gammaDist(x: number, a: number, b: number, cum: number): R {
  if (x < 0 || a <= 0 || b <= 0) return Errors.NUM;
  return cum ? gammaCdf(x, a, b) : gammaPdf(x, a, b);
}

function gammaInv(p: number, a: number, b: number): R {
  if (p < 0 || p >= 1 || a <= 0 || b <= 0) return Errors.NUM;
  if (p === 0) return 0;
  return invertCdf((x) => gammaCdf(x, a, b), p, 0, Math.max(1, a * b));
}

function exponDist(x: number, lambda: number, cum: number): R {
  if (x < 0 || lambda <= 0) return Errors.NUM;
  return cum ? -Math.expm1(-lambda * x) : lambda * Math.exp(-lambda * x);
}

function weibullDist(x: number, a: number, b: number, cum: number): R {
  if (x < 0 || a <= 0 || b <= 0) return Errors.NUM;
  const r = (x / b) ** a;
  return cum ? -Math.expm1(-r) : (a / b ** a) * x ** (a - 1) * Math.exp(-r);
}

// ── discrete ─────────────────────────────────────────────────────────────

function binomDist(s0: number, n0: number, p: number, cum: number): R {
  const s = t(s0);
  const n = t(n0);
  if (s < 0 || s > n || p < 0 || p > 1) return Errors.NUM;
  return cum ? binomCdf(s, n, p) : binomPmf(s, n, p);
}

function binomDistRange(n0: number, p: number, s0: number, s20: number): R {
  const n = t(n0);
  const s = t(s0);
  const s2 = t(s20);
  if (n < 0 || p < 0 || p > 1 || s < 0 || s > n || s2 < s || s2 > n) return Errors.NUM;
  let acc = 0;
  for (let k = s; k <= s2; k++) acc += binomPmf(k, n, p);
  return acc;
}

function binomInv(n0: number, p: number, alpha: number): R {
  const n = t(n0);
  if (n < 0 || p < 0 || p > 1 || alpha < 0 || alpha > 1) return Errors.NUM;
  let acc = 0;
  for (let k = 0; k <= n; k++) {
    acc += binomPmf(k, n, p);
    if (acc >= alpha * (1 - 1e-15)) return k;
  }
  return n;
}

function poissonDist(x0: number, mean: number, cum: number): R {
  const x = t(x0);
  if (x < 0 || mean < 0) return Errors.NUM;
  return cum ? poissonCdf(x, mean) : poissonPmf(x, mean);
}

function hypgeomDist(s0: number, n0: number, K0: number, N0: number, cum: number): R {
  const s = t(s0);
  const n = t(n0);
  const K = t(K0);
  const N = t(N0);
  if (s < 0 || n <= 0 || K <= 0 || N <= 0 || n > N || K > N || s > n || s > K || n - s > N - K)
    return Errors.NUM;
  if (!cum) return hypgeomPmf(s, n, K, N);
  let acc = 0;
  for (let k = Math.max(0, n + K - N); k <= s; k++) acc += hypgeomPmf(k, n, K, N);
  return Math.min(1, acc);
}

function negbinomDist(f0: number, s0: number, p: number, cum: number): R {
  const f = t(f0);
  const s = t(s0);
  if (f < 0 || s < 1 || p < 0 || p > 1) return Errors.NUM;
  if (!cum) return negbinomPmf(f, s, p);
  return betaI(p, s, f + 1);
}

export const distributionFunctions: FunctionDescriptor[] = [
  numeric('NORM.DIST', 4, 4, normDist),
  numeric('NORMDIST', 4, 4, normDist),
  numeric('NORM.INV', 3, 3, normInvFn),
  numeric('NORMINV', 3, 3, normInvFn),
  numeric('NORM.S.DIST', 2, 2, (z, c) => normDist(z, 0, 1, c)),
  numeric('NORMSDIST', 1, 1, (z) => normCdf(z)),
  numeric('NORM.S.INV', 1, 1, (p) => normInvFn(p, 0, 1)),
  numeric('NORMSINV', 1, 1, (p) => normInvFn(p, 0, 1)),
  numeric('LOGNORM.DIST', 4, 4, lognormDist),
  numeric('LOGNORMDIST', 3, 3, (x, m, sd) => lognormDist(x, m, sd, 1)),
  numeric('LOGNORM.INV', 3, 3, lognormInv),
  numeric('LOGINV', 3, 3, lognormInv),
  numeric('T.DIST', 3, 3, tDist),
  numeric('T.DIST.RT', 2, 2, tDistRT),
  numeric('T.DIST.2T', 2, 2, tDist2T),
  numeric('TDIST', 3, 3, tdistLegacy),
  numeric('T.INV', 2, 2, tInv),
  numeric('T.INV.2T', 2, 2, tInv2T),
  numeric('TINV', 2, 2, tInv2T),
  numeric('CHISQ.DIST', 3, 3, chisqDist),
  numeric('CHISQ.DIST.RT', 2, 2, chisqDistRT),
  numeric('CHIDIST', 2, 2, chisqDistRT),
  numeric('CHISQ.INV', 2, 2, chisqInv),
  numeric('CHISQ.INV.RT', 2, 2, chisqInvRT),
  numeric('CHIINV', 2, 2, chisqInvRT),
  numeric('F.DIST', 4, 4, fDist),
  numeric('F.DIST.RT', 3, 3, fDistRT),
  numeric('FDIST', 3, 3, fDistRT),
  numeric('F.INV', 3, 3, fInv),
  numeric('F.INV.RT', 3, 3, fInvRT),
  numeric('FINV', 3, 3, fInvRT),
  numeric('BETA.DIST', 4, 6, betaDist, [0, 1]),
  numeric('BETADIST', 3, 5, (x, a, b, lo, hi) => betaDist(x, a, b, 1, lo, hi), [0, 1]),
  numeric('BETA.INV', 3, 5, betaInv, [0, 1]),
  numeric('BETAINV', 3, 5, betaInv, [0, 1]),
  numeric('GAMMA.DIST', 4, 4, gammaDist),
  numeric('GAMMADIST', 4, 4, gammaDist),
  numeric('GAMMA.INV', 3, 3, gammaInv),
  numeric('GAMMAINV', 3, 3, gammaInv),
  numeric('EXPON.DIST', 3, 3, exponDist),
  numeric('EXPONDIST', 3, 3, exponDist),
  numeric('WEIBULL.DIST', 4, 4, weibullDist),
  numeric('WEIBULL', 4, 4, weibullDist),
  numeric('BINOM.DIST', 4, 4, binomDist),
  numeric('BINOMDIST', 4, 4, binomDist),
  numeric('BINOM.DIST.RANGE', 3, 4, (n, p, s, s2) => binomDistRange(n, p, s, Number.isNaN(s2) ? s : s2), [
    Number.NaN,
  ]),
  numeric('BINOM.INV', 3, 3, binomInv),
  numeric('CRITBINOM', 3, 3, binomInv),
  numeric('POISSON.DIST', 3, 3, poissonDist),
  numeric('POISSON', 3, 3, poissonDist),
  numeric('HYPGEOM.DIST', 5, 5, hypgeomDist),
  numeric('HYPGEOMDIST', 4, 4, (s, n, K, N) => hypgeomDist(s, n, K, N, 0)),
  numeric('NEGBINOM.DIST', 4, 4, negbinomDist),
  numeric('NEGBINOMDIST', 3, 3, (f, s, p) => negbinomDist(f, s, p, 0)),
];
