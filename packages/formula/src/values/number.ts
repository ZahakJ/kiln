/**
 * Number helpers implementing Excel's 15-significant-digit behaviour.
 */

/** Rounds to 15 significant digits, as Excel does before displaying and comparing numbers. */
export function round15(x: number): number {
  if (x === 0 || !Number.isFinite(x)) return x === 0 ? 0 : x;
  return Number(x.toPrecision(15));
}

/**
 * Excel's ROUND: half away from zero, applied to the 15-digit value so that `ROUND(1.005, 2)` is 1.01
 * (binary 1.00499999… would otherwise round down).
 */
export function excelRound(x: number, digits: number): number {
  if (!Number.isFinite(x) || x === 0) return x;
  const d = Math.trunc(digits);
  const sign = x < 0 ? -1 : 1;
  const a = Math.abs(x);
  if (d > 300) return x;
  if (d >= 0) {
    const f = 10 ** d;
    const scaled = Number((a * f).toPrecision(15));
    const r = Math.round(scaled) / f;
    return sign * (Number.isFinite(r) ? r : a);
  }
  if (d < -308) return 0;
  const f = 10 ** -d;
  const scaled = Number((a / f).toPrecision(15));
  return sign * Math.round(scaled) * f;
}

/** Truncates toward zero at `digits` decimals (Excel's TRUNC / ROUNDDOWN), 15-digit aware. */
export function excelTrunc(x: number, digits = 0): number {
  if (!Number.isFinite(x) || x === 0) return x;
  const d = Math.trunc(digits);
  const sign = x < 0 ? -1 : 1;
  const a = Math.abs(x);
  if (d >= 0) {
    const f = 10 ** d;
    return (sign * Math.floor(Number((a * f).toPrecision(15)))) / f;
  }
  const f = 10 ** -d;
  return sign * Math.floor(Number((a / f).toPrecision(15))) * f;
}

/**
 * Converts a number to text the way Excel does when a number is used as text (`=A1&""`, `LEN(A1)`,
 * `CONCAT`): at most 15 significant digits, no trailing zeros, scientific notation (`1E+15`,
 * `1.5E-10`) for very large or very small magnitudes.
 */
export function numberToText(x: number): string {
  if (x === 0) return '0';
  if (!Number.isFinite(x)) return x > 0 ? '1E+308' : '-1E+308';
  const p = round15(x);
  const [mant, expStr] = p.toExponential(14).split('e') as [string, string];
  const e = Number(expStr);
  const neg = mant.startsWith('-');
  const digits = mant.replace('-', '').replace('.', '').replace(/0+$/, '') || '0';
  const nSig = digits.length;
  const scientific = e >= 15 || e <= -10 || (e < 0 && -e - 1 + nSig > 17);
  let body: string;
  if (scientific) {
    const m = digits.length > 1 ? `${digits[0]}.${digits.slice(1)}` : digits;
    const absE = Math.abs(e);
    body = `${m}E${e < 0 ? '-' : '+'}${absE < 10 ? `0${absE}` : absE}`;
  } else if (e >= 0) {
    body =
      nSig <= e + 1 ? digits + '0'.repeat(e + 1 - nSig) : `${digits.slice(0, e + 1)}.${digits.slice(e + 1)}`;
  } else {
    body = `0.${'0'.repeat(-e - 1)}${digits}`;
  }
  return neg ? `-${body}` : body;
}

/** Excel's "almost equal" for numbers: equal after rounding both to 15 significant digits. */
export function numbersEqual(a: number, b: number): boolean {
  return a === b || round15(a) === round15(b);
}
