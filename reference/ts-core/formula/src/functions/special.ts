/**
 * Special functions and probability distributions, implemented to double precision (≈1e-14
 * relative) so statistical functions agree with Excel and LibreOffice well inside the conformance
 * tolerance. References: Lanczos (g=7) for log-gamma, series / Lentz continued fractions for the
 * regularised incomplete gamma and beta functions, Acklam's rational approximation refined with
 * Halley steps for the normal quantile, and safeguarded bisection/Newton for other quantiles.
 */

const LANCZOS = [
  0.9999999999998099, 676.5203681218851, -1259.1392167224028, 771.3234287776531, -176.61502916214059,
  12.507343278686905, -0.13857109526572012, 9.984369578019572e-6, 1.5056327351493116e-7,
];
const LN_SQRT_2PI = 0.9189385332046728;
const EPS = 1e-16;
const FPMIN = 1e-300;

/** ln|Γ(x)|. */
export function logGamma(x: number): number {
  if (x < 0.5) return Math.log(Math.PI / Math.abs(Math.sin(Math.PI * x))) - logGamma(1 - x);
  if (Number.isInteger(x) && x <= 171) return Math.log(factorial(x - 1));
  const z = x - 1;
  let a = LANCZOS[0] as number;
  const t = z + 7.5;
  for (let i = 1; i < 9; i++) a += (LANCZOS[i] as number) / (z + i);
  return LN_SQRT_2PI + (z + 0.5) * Math.log(t) - t + Math.log(a);
}

const FACT: number[] = [1];
for (let i = 1; i <= 170; i++) FACT[i] = (FACT[i - 1] as number) * i;

/** n! for integers 0…170 (Infinity above). */
export function factorial(n: number): number {
  if (n < 0) return Number.NaN;
  if (n > 170) return Number.POSITIVE_INFINITY;
  return FACT[Math.floor(n)] as number;
}

/** Γ(x); NaN at poles. */
export function gamma(x: number): number {
  if (Number.isInteger(x)) {
    if (x <= 0) return Number.NaN;
    return factorial(x - 1);
  }
  if (x < 0.5) return Math.PI / (Math.sin(Math.PI * x) * gamma(1 - x));
  if (x > 171.7) return Number.POSITIVE_INFINITY;
  return Math.exp(logGamma(x));
}

/** ln B(a, b). */
export function logBeta(a: number, b: number): number {
  return logGamma(a) + logGamma(b) - logGamma(a + b);
}

/** Regularised lower incomplete gamma P(a, x). */
export function gammaP(a: number, x: number): number {
  if (x <= 0) return 0;
  if (x < a + 1) return gammaSeries(a, x);
  return 1 - gammaCF(a, x);
}

/** Regularised upper incomplete gamma Q(a, x) = 1 − P(a, x). */
export function gammaQ(a: number, x: number): number {
  if (x <= 0) return 1;
  if (x < a + 1) return 1 - gammaSeries(a, x);
  return gammaCF(a, x);
}

function gammaSeries(a: number, x: number): number {
  let ap = a;
  let sum = 1 / a;
  let del = sum;
  for (let n = 0; n < 10000; n++) {
    ap += 1;
    del *= x / ap;
    sum += del;
    if (Math.abs(del) < Math.abs(sum) * EPS) break;
  }
  return sum * Math.exp(-x + a * Math.log(x) - logGamma(a));
}

function gammaCF(a: number, x: number): number {
  let b = x + 1 - a;
  let c = 1 / FPMIN;
  let d = 1 / b;
  let h = d;
  for (let i = 1; i < 10000; i++) {
    const an = -i * (i - a);
    b += 2;
    d = an * d + b;
    if (Math.abs(d) < FPMIN) d = FPMIN;
    c = b + an / c;
    if (Math.abs(c) < FPMIN) c = FPMIN;
    d = 1 / d;
    const del = d * c;
    h *= del;
    if (Math.abs(del - 1) < EPS) break;
  }
  return Math.exp(-x + a * Math.log(x) - logGamma(a)) * h;
}

/** Regularised incomplete beta I_x(a, b). */
export function betaI(x: number, a: number, b: number): number {
  if (x <= 0) return 0;
  if (x >= 1) return 1;
  const bt = Math.exp(a * Math.log(x) + b * Math.log1p(-x) - logBeta(a, b));
  if (x < (a + 1) / (a + b + 2)) return (bt * betaCF(x, a, b)) / a;
  return 1 - (bt * betaCF(1 - x, b, a)) / b;
}

function betaCF(x: number, a: number, b: number): number {
  const qab = a + b;
  const qap = a + 1;
  const qam = a - 1;
  let c = 1;
  let d = 1 - (qab * x) / qap;
  if (Math.abs(d) < FPMIN) d = FPMIN;
  d = 1 / d;
  let h = d;
  for (let m = 1; m <= 10000; m++) {
    const m2 = 2 * m;
    let aa = (m * (b - m) * x) / ((qam + m2) * (a + m2));
    d = 1 + aa * d;
    if (Math.abs(d) < FPMIN) d = FPMIN;
    c = 1 + aa / c;
    if (Math.abs(c) < FPMIN) c = FPMIN;
    d = 1 / d;
    h *= d * c;
    aa = (-(a + m) * (qab + m) * x) / ((a + m2) * (qap + m2));
    d = 1 + aa * d;
    if (Math.abs(d) < FPMIN) d = FPMIN;
    c = 1 + aa / c;
    if (Math.abs(c) < FPMIN) c = FPMIN;
    d = 1 / d;
    const del = d * c;
    h *= del;
    if (Math.abs(del - 1) < EPS) break;
  }
  return h;
}

/** Error function. */
export function erf(x: number): number {
  if (x === 0) return 0;
  if (Math.abs(x) > 6) return Math.sign(x);
  // Taylor series is very accurate for small |x|; incomplete gamma otherwise.
  if (Math.abs(x) < 0.5) {
    let term = x;
    let sum = x;
    const x2 = x * x;
    for (let n = 1; n < 60; n++) {
      term *= -x2 / n;
      const add = term / (2 * n + 1);
      sum += add;
      if (Math.abs(add) < 1e-17 * Math.abs(sum)) break;
    }
    return (2 / Math.sqrt(Math.PI)) * sum;
  }
  const p = gammaP(0.5, x * x);
  return x < 0 ? -p : p;
}

/** Complementary error function, accurate in the tails. */
export function erfc(x: number): number {
  if (x < 0.5) return 1 - erf(x);
  return gammaQ(0.5, x * x);
}

// ── normal ───────────────────────────────────────────────────────────────

export function normPdf(z: number): number {
  return Math.exp(-0.5 * z * z - LN_SQRT_2PI);
}

export function normCdf(z: number): number {
  return 0.5 * erfc(-z / Math.SQRT2);
}

/** Evaluates a polynomial with coefficients from the highest power down. */
function horner(coeffs: readonly number[], x: number): number {
  let r = 0;
  for (const k of coeffs) r = r * x + k;
  return r;
}

/** Standard normal quantile. */
export function normInv(p: number): number {
  if (p <= 0 || p >= 1) return Number.NaN;
  const a = [
    -39.69683028665376, 220.9460984245205, -275.9285104469687, 138.357751867269, -30.66479806614716,
    2.506628277459239,
  ];
  const b = [
    -54.47609879822406, 161.5858368580409, -155.6989798598866, 66.80131188771972, -13.28068155288572,
  ];
  const c = [
    -0.007784894002430293, -0.3223964580411365, -2.400758277161838, -2.549732539343734, 4.374664141464968,
    2.938163982698783,
  ];
  const d = [0.007784695709041462, 0.3224671290700398, 2.445134137142996, 3.754408661907416];
  const pl = 0.02425;
  let x: number;
  if (p < pl) {
    const q = Math.sqrt(-2 * Math.log(p));
    x = horner(c, q) / (horner(d, q) * q + 1);
  } else if (p <= 1 - pl) {
    const q = p - 0.5;
    const r = q * q;
    x = (horner(a, r) * q) / (horner(b, r) * r + 1);
  } else {
    const q = Math.sqrt(-2 * Math.log1p(-p));
    x = -horner(c, q) / (horner(d, q) * q + 1);
  }
  // Halley refinement (two steps reach full double precision).
  for (let i = 0; i < 2; i++) {
    const e = p < 0.5 ? normCdf(x) - p : -(normCdfUpper(x) - (1 - p));
    const u = e * Math.sqrt(2 * Math.PI) * Math.exp((x * x) / 2);
    x = x - u / (1 + (x * u) / 2);
  }
  return x;
}

function normCdfUpper(z: number): number {
  return 0.5 * erfc(z / Math.SQRT2);
}

// ── other continuous distributions ───────────────────────────────────────

export function chisqCdf(x: number, df: number): number {
  return x <= 0 ? 0 : gammaP(df / 2, x / 2);
}

export function chisqPdf(x: number, df: number): number {
  if (x < 0) return 0;
  if (x === 0) return df === 2 ? 0.5 : df < 2 ? Number.POSITIVE_INFINITY : 0;
  const k = df / 2;
  return Math.exp((k - 1) * Math.log(x) - x / 2 - k * Math.LN2 - logGamma(k));
}

export function gammaCdf(x: number, alpha: number, beta: number): number {
  return x <= 0 ? 0 : gammaP(alpha, x / beta);
}

export function gammaPdf(x: number, alpha: number, beta: number): number {
  if (x < 0) return 0;
  if (x === 0) return alpha === 1 ? 1 / beta : alpha < 1 ? Number.POSITIVE_INFINITY : 0;
  return Math.exp((alpha - 1) * Math.log(x) - x / beta - logGamma(alpha) - alpha * Math.log(beta));
}

export function betaCdf(x: number, a: number, b: number): number {
  return betaI(x, a, b);
}

export function betaPdf(x: number, a: number, b: number): number {
  if (x < 0 || x > 1) return 0;
  if (x === 0) return a === 1 ? b : a < 1 ? Number.POSITIVE_INFINITY : 0;
  if (x === 1) return b === 1 ? a : b < 1 ? Number.POSITIVE_INFINITY : 0;
  return Math.exp((a - 1) * Math.log(x) + (b - 1) * Math.log1p(-x) - logBeta(a, b));
}

/** Student's t CDF. */
export function tCdf(t: number, df: number): number {
  const x = df / (df + t * t);
  const tail = 0.5 * betaI(x, df / 2, 0.5);
  return t >= 0 ? 1 - tail : tail;
}

export function tPdf(t: number, df: number): number {
  return Math.exp(
    logGamma((df + 1) / 2) -
      logGamma(df / 2) -
      0.5 * Math.log(df * Math.PI) -
      ((df + 1) / 2) * Math.log1p((t * t) / df),
  );
}

export function fCdf(x: number, d1: number, d2: number): number {
  if (x <= 0) return 0;
  return betaI((d1 * x) / (d1 * x + d2), d1 / 2, d2 / 2);
}

export function fPdf(x: number, d1: number, d2: number): number {
  if (x < 0) return 0;
  if (x === 0) return d1 === 2 ? 1 : d1 < 2 ? Number.POSITIVE_INFINITY : 0;
  return Math.exp(
    0.5 * (d1 * Math.log(d1 * x) + d2 * Math.log(d2) - (d1 + d2) * Math.log(d1 * x + d2)) -
      Math.log(x) -
      logBeta(d1 / 2, d2 / 2),
  );
}

/**
 * Inverts a continuous increasing CDF on [lo, hi] (hi may grow) to full precision by bisection.
 * `upper` lets callers solve on the upper tail (1 − cdf) for accuracy when p is close to 1.
 */
export function invertCdf(
  cdf: (x: number) => number,
  p: number,
  lo: number,
  hi: number,
  unbounded = true,
): number {
  let a = lo;
  let b = hi;
  if (unbounded) {
    let guard = 0;
    while (cdf(b) < p && guard++ < 2000) {
      a = b;
      b *= 2;
    }
  }
  for (let i = 0; i < 300; i++) {
    const m = (a + b) / 2;
    if (m === a || m === b) break;
    if (cdf(m) < p) a = m;
    else b = m;
  }
  return (a + b) / 2;
}

// ── discrete distributions ───────────────────────────────────────────────

/** ln C(n, k). */
export function logChoose(n: number, k: number): number {
  return logGamma(n + 1) - logGamma(k + 1) - logGamma(n - k + 1);
}

/** C(n, k) exactly for moderate sizes, rounded to an integer. */
export function choose(n: number, k: number): number {
  if (k < 0 || k > n) return 0;
  const kk = Math.min(k, n - k);
  let r = 1;
  for (let i = 1; i <= kk; i++) {
    r = (r * (n - kk + i)) / i;
    if (!Number.isFinite(r)) return Number.POSITIVE_INFINITY;
  }
  return Math.round(r);
}

export function binomPmf(k: number, n: number, p: number): number {
  if (k < 0 || k > n) return 0;
  if (p === 0) return k === 0 ? 1 : 0;
  if (p === 1) return k === n ? 1 : 0;
  if (n <= 1000) return choose(n, k) * p ** k * (1 - p) ** (n - k);
  return Math.exp(logChoose(n, k) + k * Math.log(p) + (n - k) * Math.log1p(-p));
}

export function binomCdf(k: number, n: number, p: number): number {
  if (k < 0) return 0;
  if (k >= n) return 1;
  // I_{1-p}(n-k, k+1) is exact and fast.
  return betaI(1 - p, n - k, k + 1);
}

export function poissonPmf(k: number, mean: number): number {
  if (k < 0) return 0;
  if (mean === 0) return k === 0 ? 1 : 0;
  return Math.exp(k * Math.log(mean) - mean - logGamma(k + 1));
}

export function poissonCdf(k: number, mean: number): number {
  if (k < 0) return 0;
  return gammaQ(k + 1, mean);
}

export function hypgeomPmf(k: number, n: number, K: number, N: number): number {
  if (k < Math.max(0, n + K - N) || k > Math.min(n, K)) return 0;
  return Math.exp(logChoose(K, k) + logChoose(N - K, n - k) - logChoose(N, n));
}

export function negbinomPmf(f: number, s: number, p: number): number {
  return Math.exp(logChoose(f + s - 1, s - 1) + s * Math.log(p) + f * Math.log1p(-p));
}
