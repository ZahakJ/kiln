/**
 * Bessel functions of integer order to near double precision.
 *
 * Small arguments use the power series (no cancellation there); otherwise the integral
 * representations are evaluated with composite Gauss–Legendre quadrature:
 *   Jn(x) = 1/π ∫₀^π cos(nτ − x sin τ) dτ
 *   In(x) = 1/π ∫₀^π e^{x cos τ} cos(nτ) dτ
 *   Kn(x) = ∫₀^∞ e^{−x cosh t} cosh(nt) dt
 *   Yn(x) = 1/π ∫₀^π sin(x sin τ − nτ) dτ − 1/π ∫₀^∞ (e^{nt} + (−1)ⁿ e^{−nt}) e^{−x sinh t} dt
 */

const GL_N = 20;
const { nodes: GL_X, weights: GL_W } = gaussLegendre(GL_N);

/** Nodes and weights of the n-point Gauss–Legendre rule on [-1, 1] (Newton on Pₙ). */
function gaussLegendre(n: number): { nodes: number[]; weights: number[] } {
  const nodes: number[] = [];
  const weights: number[] = [];
  for (let i = 1; i <= n; i++) {
    let x = Math.cos((Math.PI * (i - 0.25)) / (n + 0.5));
    let dp = 0;
    for (let it = 0; it < 100; it++) {
      let p0 = 1;
      let p1 = x;
      for (let k = 2; k <= n; k++) {
        const p2 = ((2 * k - 1) * x * p1 - (k - 1) * p0) / k;
        p0 = p1;
        p1 = p2;
      }
      dp = (n * (x * p1 - p0)) / (x * x - 1);
      const dx = p1 / dp;
      x -= dx;
      if (Math.abs(dx) < 1e-16) break;
    }
    nodes.push(x);
    weights.push(2 / ((1 - x * x) * dp * dp));
  }
  return { nodes, weights };
}

function integrate(f: (t: number) => number, a: number, b: number, panels: number): number {
  const h = (b - a) / panels;
  let total = 0;
  for (let p = 0; p < panels; p++) {
    const mid = a + (p + 0.5) * h;
    let s = 0;
    for (let i = 0; i < GL_N; i++) s += (GL_W[i] as number) * f(mid + (GL_X[i] as number) * (h / 2));
    total += (s * h) / 2;
  }
  return total;
}

function factorial(n: number): number {
  let r = 1;
  for (let i = 2; i <= n; i++) r *= i;
  return r;
}

/** Σ_k (±1)^k (x/2)^{2k+n} / (k! (n+k)!). */
function series(x: number, n: number, alternating: boolean): number {
  const h = x / 2;
  let term = h ** n / factorial(n);
  let sum = term;
  const h2 = h * h;
  for (let k = 1; k < 500; k++) {
    term *= (alternating ? -h2 : h2) / (k * (n + k));
    sum += term;
    if (Math.abs(term) < 1e-17 * Math.abs(sum)) break;
  }
  return sum;
}

function panelsFor(x: number, n: number): number {
  return Math.max(8, Math.ceil((Math.abs(x) + n) / 2) + 6);
}

export function besselJ(x: number, n: number): number {
  if (x < 0) return (n % 2 === 0 ? 1 : -1) * besselJ(-x, n);
  if (x === 0) return n === 0 ? 1 : 0;
  if ((x * x) / 4 < n + 1) return series(x, n, true);
  return integrate((t) => Math.cos(n * t - x * Math.sin(t)), 0, Math.PI, panelsFor(x, n)) / Math.PI;
}

export function besselI(x: number, n: number): number {
  if (x < 0) return (n % 2 === 0 ? 1 : -1) * besselI(-x, n);
  if (x === 0) return n === 0 ? 1 : 0;
  if (x <= 30 + n) return series(x, n, false);
  // Scaled integrand to avoid overflow: e^{x(cos τ − 1)} · e^x.
  const s = integrate((t) => Math.exp(x * (Math.cos(t) - 1)) * Math.cos(n * t), 0, Math.PI, panelsFor(x, n));
  return (s / Math.PI) * Math.exp(x);
}

export function besselK(x: number, n: number): number {
  // Integrand e^{-x(cosh t − 1)} cosh(nt) · e^{-x}; cut off where it is below 1e-18 of its peak.
  let T = 1;
  while (x * (Math.cosh(T) - 1) - n * T < 45 && T < 50) T += 0.5;
  const f = (t: number) => Math.exp(-x * (Math.cosh(t) - 1)) * Math.cosh(n * t);
  return integrate(f, 0, T, Math.max(16, Math.ceil(T * 8))) * Math.exp(-x);
}

export function besselY(x: number, n: number): number {
  const first = integrate((t) => Math.sin(x * Math.sin(t) - n * t), 0, Math.PI, panelsFor(x, n)) / Math.PI;
  let T = 1;
  while (x * Math.sinh(T) - n * T < 45 && T < 50) T += 0.5;
  const sign = n % 2 === 0 ? 1 : -1;
  const g = (t: number) => (Math.exp(n * t) + sign * Math.exp(-n * t)) * Math.exp(-x * Math.sinh(t));
  // The integrand changes fastest near 0 for small x: use more panels there.
  const second =
    integrate(g, 0, Math.min(T, 4), 64) + (T > 4 ? integrate(g, 4, T, Math.ceil((T - 4) * 8)) : 0);
  return first - second / Math.PI;
}
