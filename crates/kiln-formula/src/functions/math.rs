// SPDX-License-Identifier: AGPL-3.0-or-later
//! Math & Trig: elementary functions, rounding, combinatorics, `SUMPRODUCT` and the pair sums,
//! matrices, `ROMAN`/`ARABIC`/`BASE`/`DECIMAL`, random numbers, `SUBTOTAL` and `AGGREGATE`.

use std::cell::Cell;
use std::f64::consts::PI;

use super::agg::{self, Agg};
use super::grid::ref_grid;
use super::special::{choose, factorial, js_round};
use super::support::*;
use crate::engine::{Arg, CallContext, EvalValue, RowVisibility};
use crate::registry::{ArgMask, FunctionSpec, core_functions};
use crate::values::{Array2D, ErrorValue, Value, excel_round, excel_trunc, round15};

/// Excel's `SIN`/`COS`/`TAN` give `#NUM!` from 2^27 on.
const TRIG_LIMIT: f64 = 134_217_728.0;
const TWO_53: f64 = 9_007_199_254_740_992.0;

/// x / step rounded to 15 digits, so 1.1/0.1 is 11 rather than 11.000000000000002.
fn ratio(x: f64, step: f64) -> f64 {
    round15(x / step)
}

fn ceiling_classic(x: f64, sig: f64) -> R<f64> {
    if sig == 0.0 || x == 0.0 {
        return Ok(0.0);
    }
    if x > 0.0 && sig < 0.0 {
        return Err(ErrorValue::Num);
    }
    Ok(ratio(x, sig).ceil() * sig)
}

fn floor_classic(x: f64, sig: f64) -> R<f64> {
    if x == 0.0 {
        return Ok(0.0);
    }
    if sig == 0.0 {
        return Err(ErrorValue::Div0);
    }
    if x > 0.0 && sig < 0.0 {
        return Err(ErrorValue::Num);
    }
    Ok(ratio(x, sig).floor() * sig)
}

fn ceiling_math(x: f64, sig: f64, mode: f64) -> f64 {
    let s = sig.abs();
    if s == 0.0 || x == 0.0 {
        return 0.0;
    }
    let q = ratio(x, s);
    (if x < 0.0 && mode != 0.0 {
        q.floor()
    } else {
        q.ceil()
    }) * s
}

fn floor_math(x: f64, sig: f64, mode: f64) -> f64 {
    let s = sig.abs();
    if s == 0.0 || x == 0.0 {
        return 0.0;
    }
    let q = ratio(x, s);
    (if x < 0.0 && mode != 0.0 {
        q.ceil()
    } else {
        q.floor()
    }) * s
}

fn round_up(x: f64, digits: f64) -> f64 {
    let d = digits.trunc();
    let sign = if x < 0.0 { -1.0 } else { 1.0 };
    let a = x.abs();
    if d >= 0.0 {
        let f = 10f64.powf(d);
        return sign * round15(a * f).ceil() / f;
    }
    let f = 10f64.powf(-d);
    sign * round15(a / f).ceil() * f
}

fn even_odd(x: f64, odd: bool) -> f64 {
    let sign = if x < 0.0 { -1.0 } else { 1.0 };
    let mut n = round15(x.abs()).ceil();
    let is_odd = n % 2.0 == 1.0;
    if odd != is_odd {
        n += 1.0;
    }
    sign * n
}

fn gcd2(a: f64, b: f64) -> f64 {
    let (mut x, mut y) = (a, b);
    while y != 0.0 {
        let r = x % y;
        x = y;
        y = r;
    }
    x
}

fn integer_args(args: &mut [Arg<'_>], ctx: &mut dyn CallContext) -> R<Vec<f64>> {
    numbers(args, ctx)?
        .into_iter()
        .map(|x| {
            if !(0.0..TWO_53).contains(&x) {
                Err(ErrorValue::Num)
            } else {
                Ok(x.trunc())
            }
        })
        .collect()
}

// ── ROMAN / ARABIC / BASE / DECIMAL ────────────────────────────────────────────────────────

const ROMAN_CHARS: [char; 7] = ['M', 'D', 'C', 'L', 'X', 'V', 'I'];
const ROMAN_VALUES: [i64; 7] = [1000, 500, 100, 50, 10, 5, 1];

/// Excel's `ROMAN` with forms 0 (classic) to 4 (most concise).
pub(crate) fn to_roman(value: i64, mode: i64) -> String {
    let mut val = value;
    let mut out = String::new();
    let max_index = ROMAN_VALUES.len() - 1;
    for i in 0..=max_index / 2 {
        let mut index = 2 * i;
        let digit = val / ROMAN_VALUES[index];
        if digit % 5 == 4 {
            let index2 = if digit == 4 { index - 1 } else { index - 2 };
            let mut steps = 0;
            while steps < mode && index < max_index {
                steps += 1;
                if ROMAN_VALUES[index2] - ROMAN_VALUES[index + 1] <= val {
                    index += 1;
                } else {
                    steps = mode;
                }
            }
            out.push(ROMAN_CHARS[index]);
            out.push(ROMAN_CHARS[index2]);
            val += ROMAN_VALUES[index] - ROMAN_VALUES[index2];
        } else {
            if digit > 4 {
                out.push(ROMAN_CHARS[index - 1]);
            }
            for _ in 0..digit % 5 {
                out.push(ROMAN_CHARS[index]);
            }
            val %= ROMAN_VALUES[index];
        }
    }
    out
}

fn from_roman(text: &str) -> R<f64> {
    let upper = text.trim().to_uppercase();
    if upper.chars().count() > 255 {
        return Err(ErrorValue::Value);
    }
    let (sign, s) = match upper.strip_prefix('-') {
        Some(rest) => (-1.0, rest),
        None => (1.0, upper.as_str()),
    };
    let value = |c: char| match c {
        'I' => Some(1.0),
        'V' => Some(5.0),
        'X' => Some(10.0),
        'L' => Some(50.0),
        'C' => Some(100.0),
        'D' => Some(500.0),
        'M' => Some(1000.0),
        _ => None,
    };
    let chars: Vec<char> = s.chars().collect();
    let mut total = 0.0;
    for (i, &c) in chars.iter().enumerate() {
        let v = value(c).ok_or(ErrorValue::Value)?;
        let next = chars.get(i + 1).and_then(|&n| value(n));
        if next.is_some_and(|n| n > v) {
            total -= v;
        } else {
            total += v;
        }
    }
    Ok(sign * total)
}

fn roman(args: &mut [Arg<'_>], ctx: &mut dyn CallContext) -> R<String> {
    let n = int(args, 0, ctx, 0.0)?;
    let mode = match slot(args, 1, ctx) {
        Slot::Absent | Slot::Empty => 0.0,
        Slot::Val(Value::Bool(b)) => {
            if b {
                0.0
            } else {
                4.0
            }
        }
        Slot::Val(v) => number(&v, ctx)?.trunc(),
    };
    if !(0.0..=3999.0).contains(&n) || !(0.0..=4.0).contains(&mode) {
        return Err(ErrorValue::Value);
    }
    Ok(to_roman(n as i64, mode as i64))
}

fn to_radix(mut n: u64, radix: u64) -> String {
    const DIGITS: &[u8; 36] = b"0123456789ABCDEFGHIJKLMNOPQRSTUVWXYZ";
    if n == 0 {
        return "0".to_owned();
    }
    let mut out = Vec::new();
    while n > 0 {
        out.push(DIGITS[(n % radix) as usize]);
        n /= radix;
    }
    out.reverse();
    String::from_utf8(out).unwrap_or_default()
}

fn base(args: &mut [Arg<'_>], ctx: &mut dyn CallContext) -> R<String> {
    let n = num(args, 0, ctx, 0.0)?;
    let radix = int(args, 1, ctx, 0.0)?;
    let len = int(args, 2, ctx, 0.0)?;
    if !(0.0..TWO_53).contains(&n)
        || !(2.0..=36.0).contains(&radix)
        || !(0.0..=255.0).contains(&len)
    {
        return Err(ErrorValue::Num);
    }
    let s = to_radix(n.trunc() as u64, radix as u64);
    let pad = (len as usize).saturating_sub(s.len());
    Ok(format!("{}{s}", "0".repeat(pad)))
}

fn decimal(args: &mut [Arg<'_>], ctx: &mut dyn CallContext) -> R<f64> {
    let s = text(args, 0, ctx)?;
    let radix = int(args, 1, ctx, 0.0)?;
    if !(2.0..=36.0).contains(&radix) || s.encode_utf16().count() > 255 {
        return Err(ErrorValue::Num);
    }
    let lower = s.trim().to_lowercase();
    let mut t = lower.as_str();
    if radix == 16.0
        && let Some(rest) = t.strip_prefix("0x")
    {
        t = rest;
    }
    if radix == 2.0
        && let Some(rest) = t.strip_suffix('b')
    {
        t = rest;
    }
    let mut v = 0.0f64;
    for ch in t.chars() {
        let d = ch.to_digit(36).ok_or(ErrorValue::Num)?;
        if f64::from(d) >= radix {
            return Err(ErrorValue::Num);
        }
        v = v * radix + f64::from(d);
    }
    if v < TWO_53 {
        Ok(v)
    } else {
        Err(ErrorValue::Num)
    }
}

// ── matrices ───────────────────────────────────────────────────────────────────────────────

fn numeric_matrix(args: &mut [Arg<'_>], i: usize, ctx: &mut dyn CallContext) -> R<Vec<Vec<f64>>> {
    let arr = array_of(args, i, ctx)?;
    let mut out = Vec::with_capacity(arr.rows());
    for r in 0..arr.rows() {
        let mut row = Vec::with_capacity(arr.cols());
        for c in 0..arr.cols() {
            match arr.get(r, c) {
                Value::Error(e) => return Err(*e),
                Value::Number(n) => row.push(*n),
                _ => return Err(ErrorValue::Value),
            }
        }
        out.push(row);
    }
    Ok(out)
}

/// Gauss–Jordan elimination with partial pivoting: the determinant and, if wanted and the
/// matrix is regular, the inverse.
pub(crate) fn lu_solve(m: &[Vec<f64>], want_inverse: bool) -> (f64, Option<Vec<Vec<f64>>>) {
    let n = m.len();
    let mut a: Vec<Vec<f64>> = m.to_vec();
    let mut inv: Vec<Vec<f64>> = if want_inverse {
        (0..n)
            .map(|i| (0..n).map(|j| if i == j { 1.0 } else { 0.0 }).collect())
            .collect()
    } else {
        Vec::new()
    };
    let mut det = 1.0;
    for col in 0..n {
        let mut piv = col;
        for r in col + 1..n {
            if a[r][col].abs() > a[piv][col].abs() {
                piv = r;
            }
        }
        let pv = a[piv][col];
        if pv == 0.0 {
            return (0.0, None);
        }
        if piv != col {
            a.swap(piv, col);
            if want_inverse {
                inv.swap(piv, col);
            }
            det = -det;
        }
        det *= pv;
        for j in 0..n {
            a[col][j] /= pv;
            if want_inverse {
                inv[col][j] /= pv;
            }
        }
        for r in 0..n {
            if r == col {
                continue;
            }
            let f = a[r][col];
            if f == 0.0 {
                continue;
            }
            for j in 0..n {
                a[r][j] -= f * a[col][j];
                if want_inverse {
                    inv[r][j] -= f * inv[col][j];
                }
            }
        }
    }
    (det, want_inverse.then_some(inv))
}

fn square(m: &[Vec<f64>]) -> R<()> {
    if m.first().is_some_and(|r| r.len() == m.len()) {
        Ok(())
    } else {
        Err(ErrorValue::Value)
    }
}

fn mdeterm(args: &mut [Arg<'_>], ctx: &mut dyn CallContext) -> R<f64> {
    let m = numeric_matrix(args, 0, ctx)?;
    square(&m)?;
    Ok(lu_solve(&m, false).0)
}

fn minverse(args: &mut [Arg<'_>], ctx: &mut dyn CallContext) -> R<Value> {
    let m = numeric_matrix(args, 0, ctx)?;
    square(&m)?;
    let inv = lu_solve(&m, true).1.ok_or(ErrorValue::Num)?;
    from_rows(
        inv.into_iter()
            .map(|r| {
                r.into_iter()
                    .map(|v| Value::number(if v.abs() < 1e-15 { 0.0 } else { v }))
                    .collect()
            })
            .collect(),
    )
}

fn mmult(args: &mut [Arg<'_>], ctx: &mut dyn CallContext) -> R<Value> {
    let a = numeric_matrix(args, 0, ctx)?;
    let b = numeric_matrix(args, 1, ctx)?;
    let n = a.first().map_or(0, Vec::len);
    if n != b.len() {
        return Err(ErrorValue::Value);
    }
    let p = b.first().map_or(0, Vec::len);
    if a.len().saturating_mul(p) > MAX_CELLS {
        return Err(ErrorValue::Num);
    }
    let c = matmul(&a, &b, p);
    Ok(Value::array(Array2D::build(a.len(), p, |i, j| {
        Value::number(c[i * p + j])
    })))
}

/// `A·B` as a row-major `rows(A) × p` vector, cache-blocked: each output row is updated a row of
/// `B` at a time (contiguous, vectorisable), `B` is walked in panels that stay in cache while a
/// band of rows reuses them, and every element still sums its products in the order k = 0, 1, …
/// (so results are bit-identical to the textbook triple loop).
fn matmul(a: &[Vec<f64>], b: &[Vec<f64>], p: usize) -> Vec<f64> {
    const ROWS: usize = 32;
    const DEPTH: usize = 128;
    let (m, n) = (a.len(), b.len());
    let bf: Vec<f64> = b.iter().flatten().copied().collect();
    let mut c = vec![0.0; m * p];
    for i0 in (0..m).step_by(ROWS) {
        let i1 = (i0 + ROWS).min(m);
        for k0 in (0..n).step_by(DEPTH) {
            let k1 = (k0 + DEPTH).min(n);
            for i in i0..i1 {
                let row = &mut c[i * p..(i + 1) * p];
                for k in k0..k1 {
                    let aik = a[i][k];
                    for (cj, bj) in row.iter_mut().zip(&bf[k * p..(k + 1) * p]) {
                        *cj += aik * bj;
                    }
                }
            }
        }
    }
    c
}

fn munit(args: &mut [Arg<'_>], ctx: &mut dyn CallContext) -> R<Value> {
    let n = int_of(args, 0, ctx, 0.0)?;
    if n < 1.0 {
        return Err(ErrorValue::Value);
    }
    if n * n > MAX_CELLS as f64 {
        return Err(ErrorValue::Num);
    }
    let n = n as usize;
    Ok(Value::array(Array2D::build(n, n, |r, c| {
        Value::Number(if r == c { 1.0 } else { 0.0 })
    })))
}

// ── SUMPRODUCT and pair sums ───────────────────────────────────────────────────────────────

fn sumproduct(args: &mut [Arg<'_>], ctx: &mut dyn CallContext) -> R<f64> {
    let mut arrays = Vec::with_capacity(args.len());
    for i in 0..args.len() {
        arrays.push(array_of(args, i, ctx)?);
    }
    let Some(first) = arrays.first() else {
        return Err(ErrorValue::Value);
    };
    let (rows, cols) = (first.rows(), first.cols());
    if arrays.iter().any(|a| a.rows() != rows || a.cols() != cols) {
        return Err(ErrorValue::Value);
    }
    let mut terms = Vec::with_capacity(rows * cols);
    for k in 0..rows * cols {
        let mut p = 1.0;
        for a in &arrays {
            match &a.values()[k] {
                Value::Error(e) => return Err(*e),
                Value::Number(n) => p *= n,
                _ => p = 0.0,
            }
        }
        terms.push(p);
    }
    Ok(sum(&terms))
}

fn pair_sum(args: &mut [Arg<'_>], ctx: &mut dyn CallContext, f: fn(f64, f64) -> f64) -> R<f64> {
    let (xs, ys) = numeric_pairs(args, 0, 1, ctx)?;
    let terms: Vec<f64> = xs.iter().zip(&ys).map(|(x, y)| f(*x, *y)).collect();
    Ok(sum(&terms))
}

fn seriessum(args: &mut [Arg<'_>], ctx: &mut dyn CallContext) -> R<f64> {
    let x = number_of(args, 0, ctx, 0.0)?;
    let n = number_of(args, 1, ctx, 0.0)?;
    let m = number_of(args, 2, ctx, 0.0)?;
    let arr = array_of(args, 3, ctx)?;
    let mut s = 0.0;
    for (i, c) in arr.values().iter().enumerate() {
        match c {
            Value::Error(e) => return Err(*e),
            Value::Number(c) => s += c * x.powf(n + i as f64 * m),
            _ => return Err(ErrorValue::Value),
        }
    }
    finite(s)
}

fn gcd(args: &mut [Arg<'_>], ctx: &mut dyn CallContext) -> R<f64> {
    Ok(integer_args(args, ctx)?.into_iter().fold(0.0, gcd2))
}

fn lcm(args: &mut [Arg<'_>], ctx: &mut dyn CallContext) -> R<f64> {
    let mut l = 1.0;
    for x in integer_args(args, ctx)? {
        if x == 0.0 {
            return Ok(0.0);
        }
        l = l / gcd2(l, x) * x;
    }
    if l.is_finite() && l < TWO_53 {
        Ok(l)
    } else {
        Err(ErrorValue::Num)
    }
}

fn multinomial(args: &mut [Arg<'_>], ctx: &mut dyn CallContext) -> R<f64> {
    // (Σx)! / Πx! as a product of binomials C(total + x, x), each built over min(x, total)
    // factors: a binomial over k ≤ n/2 factors is at least 2^k, so the loop overflows to #NUM!
    // within about a thousand steps instead of running for x steps.
    let mut total = 0.0f64;
    let mut r = 1.0f64;
    for x in numbers(args, ctx)? {
        let x = x.trunc();
        if x < 0.0 {
            return Err(ErrorValue::Num);
        }
        let n = total + x;
        let k = x.min(total);
        let mut c = 1.0f64;
        let mut i = 1.0;
        while i <= k {
            c = c * (n - k + i) / i;
            if !c.is_finite() {
                return Err(ErrorValue::Num);
            }
            i += 1.0;
        }
        r *= c;
        if !r.is_finite() {
            return Err(ErrorValue::Num);
        }
        total = n;
    }
    Ok(js_round(r))
}

// ── random numbers ─────────────────────────────────────────────────────────────────────────

fn seed() -> u64 {
    if let Ok(s) = getrandom::u64() {
        return s;
    }
    // No entropy source: mix the address of a stack value with std's hasher keys.
    use std::hash::{BuildHasher, Hasher};
    let marker = 0u8;
    let mut h = std::collections::hash_map::RandomState::new().build_hasher();
    h.write_usize(std::ptr::from_ref(&marker) as usize);
    h.finish()
}

/// A uniform number in [0, 1) (SplitMix64 seeded once per thread from the system).
pub(crate) fn random() -> f64 {
    thread_local! {
        static STATE: Cell<u64> = Cell::new(seed());
    }
    STATE.with(|s| {
        let mut z = s.get().wrapping_add(0x9E37_79B9_7F4A_7C15);
        s.set(z);
        z = (z ^ (z >> 30)).wrapping_mul(0xBF58_476D_1CE4_E5B9);
        z = (z ^ (z >> 27)).wrapping_mul(0x94D0_49BB_1331_11EB);
        z ^= z >> 31;
        (z >> 11) as f64 / TWO_53
    })
}

fn randbetween(args: &mut [Arg<'_>], ctx: &mut dyn CallContext) -> R<f64> {
    let lo = num(args, 0, ctx, 0.0)?;
    let hi = num(args, 1, ctx, 0.0)?;
    let (a, b) = (lo.ceil(), hi.floor());
    if a > b {
        return Err(ErrorValue::Num);
    }
    Ok(a + (random() * (b - a + 1.0)).floor())
}

fn randarray(args: &mut [Arg<'_>], ctx: &mut dyn CallContext) -> R<Value> {
    let rows = int_of(args, 0, ctx, 1.0)?;
    let cols = int_of(args, 1, ctx, 1.0)?;
    let lo = number_of(args, 2, ctx, 0.0)?;
    let hi = number_of(args, 3, ctx, 1.0)?;
    let whole = match slot(args, 4, ctx) {
        Slot::Absent | Slot::Empty => false,
        Slot::Val(v) => number(&v, ctx)? != 0.0,
    };
    if rows < 1.0 || cols < 1.0 || lo > hi {
        return Err(if rows == 0.0 || cols == 0.0 {
            ErrorValue::Calc
        } else {
            ErrorValue::Value
        });
    }
    if whole && (lo.fract() != 0.0 || hi.fract() != 0.0) {
        return Err(ErrorValue::Value);
    }
    if rows * cols > MAX_CELLS as f64 {
        return Err(ErrorValue::Num);
    }
    Ok(Value::array(Array2D::build(
        rows as usize,
        cols as usize,
        |_, _| {
            Value::number(if whole {
                lo + (random() * (hi - lo + 1.0)).floor()
            } else {
                lo + random() * (hi - lo)
            })
        },
    )))
}

// ── SUBTOTAL / AGGREGATE ───────────────────────────────────────────────────────────────────

/// True when formula text calls SUBTOTAL or AGGREGATE (nested subtotals are skipped).
fn calls_subtotal(formula: &str) -> bool {
    let upper = formula.to_ascii_uppercase();
    let bytes = upper.as_bytes();
    for name in ["SUBTOTAL", "AGGREGATE"] {
        let mut from = 0;
        while let Some(i) = upper[from..].find(name).map(|i| i + from) {
            let boundary =
                i == 0 || !(bytes[i - 1].is_ascii_alphanumeric() || bytes[i - 1] == b'_');
            let after = upper[i + name.len()..].trim_start();
            if boundary && after.starts_with('(') {
                return true;
            }
            from = i + name.len();
        }
    }
    false
}

#[derive(Clone, Copy)]
enum Hidden {
    None,
    Filtered,
    All,
}

#[derive(Clone, Copy)]
struct Collect {
    skip_hidden: Hidden,
    skip_errors: bool,
    skip_nested: bool,
    counta: bool,
}

/// Numbers (or a 1 per non-blank value for `COUNTA`) from the arguments, honouring hidden rows
/// and nested subtotals in references.
fn collect_for_subtotal(
    args: &mut [Arg<'_>],
    ctx: &mut dyn CallContext,
    opt: Collect,
) -> R<Vec<f64>> {
    let mut out = Vec::new();
    let push = |x: &Value, out: &mut Vec<f64>| -> R<()> {
        match x {
            Value::Error(e) if !opt.skip_errors => return Err(*e),
            Value::Error(_) => {}
            Value::Blank => {}
            Value::Number(n) if !opt.counta => out.push(*n),
            _ if opt.counta => out.push(1.0),
            _ => {}
        }
        Ok(())
    };
    for a in args.iter_mut() {
        let v = ctx.force(a);
        match v {
            EvalValue::Ref(r) => {
                for area in r.as_areas() {
                    let g = ref_grid(*area, ctx);
                    let (used_rows, used_cols) = (g.used_rows(), g.used_cols());
                    for dr in 0..used_rows {
                        let row = area.bounds.r0 + dr as u32;
                        let vis = ctx.row_visibility(area.sheet, row);
                        let skip = match opt.skip_hidden {
                            Hidden::None => false,
                            Hidden::Filtered => vis == RowVisibility::Filtered,
                            Hidden::All => vis != RowVisibility::Visible,
                        };
                        if skip {
                            continue;
                        }
                        for dc in 0..used_cols {
                            let col = area.bounds.c0 + dc as u32;
                            if opt.skip_nested
                                && ctx
                                    .formula_text(area.sheet, row, col)
                                    .is_some_and(|f| calls_subtotal(&f))
                            {
                                continue;
                            }
                            let x = ctx.read_cell(area.sheet, row, col);
                            push(&x, &mut out)?;
                        }
                    }
                }
            }
            EvalValue::Value(Value::Error(e)) => {
                if !opt.skip_errors {
                    return Err(e);
                }
            }
            other => {
                let arr = ctx.to_array(other)?;
                for x in arr.values() {
                    push(x, &mut out)?;
                }
            }
        }
    }
    Ok(out)
}

fn subtotal_agg(f: f64) -> Option<Agg> {
    Some(match f as i64 {
        1 => agg::average,
        2 | 3 => agg::count,
        4 => agg::max_of,
        5 => agg::min_of,
        6 => agg::product,
        7 => agg::stdev_s,
        8 => agg::stdev_p,
        9 => agg::sum_of,
        10 => agg::var_s,
        11 => agg::var_p,
        12 => agg::median,
        13 => agg::mode_sngl,
        _ => return None,
    })
}

fn subtotal(args: &mut [Arg<'_>], ctx: &mut dyn CallContext) -> R<f64> {
    let f = int_of(args, 0, ctx, 0.0)?;
    let base = if f > 100.0 { f - 100.0 } else { f };
    if !(1.0..=11.0).contains(&base) || (f > 11.0 && f < 101.0) || f > 111.0 {
        return Err(ErrorValue::Value);
    }
    let agg = subtotal_agg(base).ok_or(ErrorValue::Value)?;
    let xs = collect_for_subtotal(
        &mut args[1..],
        ctx,
        Collect {
            skip_hidden: if f > 100.0 {
                Hidden::All
            } else {
                Hidden::Filtered
            },
            skip_errors: false,
            skip_nested: true,
            counta: base == 3.0,
        },
    )?;
    agg(&xs)
}

fn aggregate(args: &mut [Arg<'_>], ctx: &mut dyn CallContext) -> R<f64> {
    let f = int_of(args, 0, ctx, 0.0)?;
    let o = int_of(args, 1, ctx, 0.0)?;
    if !(0.0..=7.0).contains(&o) || !(1.0..=19.0).contains(&f) {
        return Err(ErrorValue::Value);
    }
    let o = o as i64;
    let opt = Collect {
        skip_hidden: if matches!(o, 1 | 3 | 5 | 7) {
            Hidden::All
        } else {
            Hidden::None
        },
        skip_errors: matches!(o, 2 | 3 | 6 | 7),
        skip_nested: o <= 3,
        counta: f == 3.0,
    };
    if f <= 13.0 {
        let xs = collect_for_subtotal(&mut args[2..], ctx, opt)?;
        let agg = subtotal_agg(f).ok_or(ErrorValue::Value)?;
        return agg(&xs);
    }
    if args.len() < 4 {
        return Err(ErrorValue::Value);
    }
    let k = number_of(args, 3, ctx, 0.0)?;
    let xs = collect_for_subtotal(&mut args[2..3], ctx, opt)?;
    match f as i64 {
        14 => agg::large(&xs, k),
        15 => agg::small(&xs, k),
        16 => agg::percentile_inc(&xs, k),
        17 => agg::quartile_inc(&xs, k),
        18 => agg::percentile_exc(&xs, k),
        _ => agg::quartile_exc(&xs, k),
    }
}

// ── core-set functions with the library's documentation ────────────────────────────────────

fn round(args: &mut [Arg<'_>], ctx: &mut dyn CallContext) -> R<f64> {
    let x = num(args, 0, ctx, 0.0)?;
    let d = num(args, 1, ctx, 0.0)?;
    Ok(excel_round(x, d.trunc()))
}

fn modulo(args: &mut [Arg<'_>], ctx: &mut dyn CallContext) -> R<f64> {
    let x = num(args, 0, ctx, 0.0)?;
    let d = num(args, 1, ctx, 0.0)?;
    if d == 0.0 {
        return Err(ErrorValue::Div0);
    }
    let r = x - d * (x / d).floor();
    // Snap results that are within rounding noise of the divisor or zero.
    if (r - d).abs() < d.abs() * 1e-15 {
        return Ok(0.0);
    }
    Ok(r)
}

/// The engine's `SUM` (the core set) documented by the library.
pub(crate) fn core_spec(name: &str) -> Option<FunctionSpec> {
    core_functions()
        .into_iter()
        .find(|s| s.name == name)
        .map(|s| FunctionSpec {
            docs: crate::registry::DocsHandle::Key(s.name),
            ..s
        })
}

/// The Math & Trig functions.
pub fn specs() -> Vec<FunctionSpec> {
    let all = ArgMask::ALL;
    let mut v = vec![
        numeric!("ACOS", 1, 1, [], |x| if !(-1.0..=1.0).contains(&x) {
            Err(ErrorValue::Num)
        } else {
            Ok(x.acos())
        }),
        numeric!("ACOSH", 1, 1, [], |x| if x < 1.0 {
            Err(ErrorValue::Num)
        } else {
            Ok(x.acosh())
        }),
        numeric!("ACOT", 1, 1, [], |x| PI / 2.0 - x.atan()),
        numeric!("ACOTH", 1, 1, [], |x| if x.abs() <= 1.0 {
            Err(ErrorValue::Num)
        } else {
            Ok(0.5 * ((x + 1.0) / (x - 1.0)).ln())
        }),
        numeric!("ASIN", 1, 1, [], |x| if !(-1.0..=1.0).contains(&x) {
            Err(ErrorValue::Num)
        } else {
            Ok(x.asin())
        }),
        numeric!("ASINH", 1, 1, [], |x| x.asinh()),
        numeric!("ATAN", 1, 1, [], |x| x.atan()),
        numeric!("ATAN2", 2, 2, [], |x, y| if x == 0.0 && y == 0.0 {
            Err(ErrorValue::Div0)
        } else {
            Ok(y.atan2(x))
        }),
        numeric!("ATANH", 1, 1, [], |x| if x.abs() >= 1.0 {
            Err(ErrorValue::Num)
        } else {
            Ok(x.atanh())
        }),
        numeric!("COS", 1, 1, [], |x| if x.abs() >= TRIG_LIMIT {
            Err(ErrorValue::Num)
        } else {
            Ok(x.cos())
        }),
        numeric!("COSH", 1, 1, [], |x| x.cosh()),
        numeric!("COT", 1, 1, [], |x| if x == 0.0 {
            Err(ErrorValue::Div0)
        } else if x.abs() >= TRIG_LIMIT {
            Err(ErrorValue::Num)
        } else {
            Ok(1.0 / x.tan())
        }),
        numeric!("COTH", 1, 1, [], |x| if x == 0.0 {
            Err(ErrorValue::Div0)
        } else {
            Ok(1.0 / x.tanh())
        }),
        numeric!("CSC", 1, 1, [], |x| if x == 0.0 {
            Err(ErrorValue::Div0)
        } else if x.abs() >= TRIG_LIMIT {
            Err(ErrorValue::Num)
        } else {
            Ok(1.0 / x.sin())
        }),
        numeric!("CSCH", 1, 1, [], |x| if x == 0.0 {
            Err(ErrorValue::Div0)
        } else {
            Ok(1.0 / x.sinh())
        }),
        numeric!("SEC", 1, 1, [], |x| if x.abs() >= TRIG_LIMIT {
            Err(ErrorValue::Num)
        } else {
            Ok(1.0 / x.cos())
        }),
        numeric!("SECH", 1, 1, [], |x| 1.0 / x.cosh()),
        numeric!("SIN", 1, 1, [], |x| if x.abs() >= TRIG_LIMIT {
            Err(ErrorValue::Num)
        } else {
            Ok(x.sin())
        }),
        numeric!("SINH", 1, 1, [], |x| x.sinh()),
        numeric!("TAN", 1, 1, [], |x| if x.abs() >= TRIG_LIMIT {
            Err(ErrorValue::Num)
        } else {
            Ok(x.tan())
        }),
        numeric!("TANH", 1, 1, [], |x| x.tanh()),
        numeric!("DEGREES", 1, 1, [], |x| x * 180.0 / PI),
        numeric!("RADIANS", 1, 1, [], |x| x * PI / 180.0),
        spec("PI", 0, 0, imp!(|_a, _c| PI)),
        numeric!("EXP", 1, 1, [], |x| x.exp()),
        numeric!("LN", 1, 1, [], |x| if x <= 0.0 {
            Err(ErrorValue::Num)
        } else {
            Ok(x.ln())
        }),
        numeric!("LOG10", 1, 1, [], |x| if x <= 0.0 {
            Err(ErrorValue::Num)
        } else {
            Ok(x.log10())
        }),
        numeric!("LOG", 1, 2, [10.0], |x, b| {
            if x <= 0.0 || b <= 0.0 {
                Err(ErrorValue::Num)
            } else if b == 1.0 {
                Err(ErrorValue::Div0)
            } else if b == 10.0 {
                Ok(x.log10())
            } else if b == 2.0 {
                Ok(x.log2())
            } else {
                Ok(round15(x.ln() / b.ln()))
            }
        }),
        numeric!("SQRT", 1, 1, [], |x| if x < 0.0 {
            Err(ErrorValue::Num)
        } else {
            Ok(x.sqrt())
        }),
        numeric!("SQRTPI", 1, 1, [], |x| if x < 0.0 {
            Err(ErrorValue::Num)
        } else {
            Ok((x * PI).sqrt())
        }),
        numeric!("POWER", 2, 2, [], |x, y| {
            if x == 0.0 && y == 0.0 {
                Err(ErrorValue::Num)
            } else if x == 0.0 && y < 0.0 {
                Err(ErrorValue::Div0)
            } else {
                let r = x.powf(y);
                if r.is_nan() {
                    Err(ErrorValue::Num)
                } else {
                    Ok(r)
                }
            }
        }),
        numeric!("SIGN", 1, 1, [], |x| if x > 0.0 {
            1.0
        } else if x < 0.0 {
            -1.0
        } else {
            0.0
        }),
        numeric!("CEILING", 2, 2, [], |x, s| ceiling_classic(x, s)),
        numeric!("CEILING.MATH", 1, 3, [1.0, 0.0], |x, s, m| ceiling_math(
            x, s, m
        )),
        numeric!("CEILING.PRECISE", 1, 2, [1.0], |x, s| ceiling_math(
            x, s, 0.0
        )),
        numeric!("ISO.CEILING", 1, 2, [1.0], |x, s| ceiling_math(x, s, 0.0)),
        numeric!("FLOOR", 2, 2, [], |x, s| floor_classic(x, s)),
        numeric!("FLOOR.MATH", 1, 3, [1.0, 0.0], |x, s, m| floor_math(
            x, s, m
        )),
        numeric!("FLOOR.PRECISE", 1, 2, [1.0], |x, s| floor_math(x, s, 0.0)),
        numeric!("MROUND", 2, 2, [], |x, m| {
            if m == 0.0 || x == 0.0 {
                Ok(0.0)
            } else if x.signum() != m.signum() {
                Err(ErrorValue::Num)
            } else {
                Ok(round15(excel_round(x / m, 0.0) * m))
            }
        }),
        numeric!("EVEN", 1, 1, [], |x| even_odd(x, false)),
        numeric!("ODD", 1, 1, [], |x| even_odd(x, true)),
        numeric!("ROUNDDOWN", 2, 2, [], |x, d| excel_trunc(x, d)),
        numeric!("ROUNDUP", 2, 2, [], |x, d| round_up(x, d)),
        numeric!("TRUNC", 1, 2, [0.0], |x, d| excel_trunc(x, d)),
        numeric!("FACT", 1, 1, [], |x| if !(0.0..171.0).contains(&x) {
            Err(ErrorValue::Num)
        } else {
            Ok(factorial(x.trunc()))
        }),
        numeric!("FACTDOUBLE", 1, 1, [], |x| {
            let n = x.trunc();
            if n < 0.0 {
                Err(ErrorValue::Num)
            } else {
                let mut r = 1.0f64;
                let mut k = n;
                while k > 1.0 && r.is_finite() {
                    r *= k;
                    k -= 2.0;
                }
                Ok(r)
            }
        }),
        numeric!("COMBIN", 2, 2, [], |n0, k0| {
            let (n, k) = (n0.trunc(), k0.trunc());
            if n < 0.0 || k < 0.0 || n < k {
                Err(ErrorValue::Num)
            } else {
                Ok(choose(n, k))
            }
        }),
        numeric!("COMBINA", 2, 2, [], |n0, k0| {
            let (n, k) = (n0.trunc(), k0.trunc());
            if n < 0.0 || k < 0.0 || n < k {
                Err(ErrorValue::Num)
            } else if k == 0.0 {
                Ok(1.0)
            } else {
                Ok(choose(n + k - 1.0, k))
            }
        }),
        spec("GCD", 1, VARIADIC, imp!(gcd)),
        spec("LCM", 1, VARIADIC, imp!(lcm)),
        spec("MULTINOMIAL", 1, VARIADIC, imp!(multinomial)),
        spec(
            "PRODUCT",
            1,
            VARIADIC,
            imp!(|a, c| numbers(a, c).and_then(|xs| agg::product(&xs))),
        ),
        numeric!("QUOTIENT", 2, 2, [], |x, y| if y == 0.0 {
            Err(ErrorValue::Div0)
        } else {
            Ok(round15(x / y).trunc())
        }),
        spec("RAND", 0, 0, imp!(|_a, _c| random())).volatile(),
        spec("RANDBETWEEN", 2, 2, imp!(randbetween))
            .lift(all)
            .volatile(),
        spec("RANDARRAY", 0, 5, imp!(randarray)).volatile(),
        spec("ROMAN", 1, 2, imp!(roman)).lift(all),
        spec(
            "ARABIC",
            1,
            1,
            imp!(|a, c| text(a, 0, c).and_then(|s| from_roman(&s))),
        )
        .lift(all),
        spec("BASE", 2, 3, imp!(base)).lift(all),
        spec("DECIMAL", 2, 2, imp!(decimal)).lift(all),
        spec(
            "SUMSQ",
            1,
            VARIADIC,
            imp!(|a, c| numbers(a, c).map(|xs| sum(&xs.iter().map(|x| x * x).collect::<Vec<_>>()))),
        ),
        spec(
            "SUMX2MY2",
            2,
            2,
            imp!(|a, c| pair_sum(a, c, |x, y| x * x - y * y)),
        ),
        spec(
            "SUMX2PY2",
            2,
            2,
            imp!(|a, c| pair_sum(a, c, |x, y| x * x + y * y)),
        ),
        spec(
            "SUMXMY2",
            2,
            2,
            imp!(|a, c| pair_sum(a, c, |x, y| (x - y) * (x - y))),
        ),
        spec("SUMPRODUCT", 1, VARIADIC, imp!(sumproduct)),
        spec(
            "PERCENTOF",
            2,
            2,
            imp!(|a, c| {
                let part = sum(&numbers_of(a, 0, c)?);
                let all = sum(&numbers_of(a, 1, c)?);
                if all == 0.0 {
                    Err(ErrorValue::Div0)
                } else {
                    finite(part / all)
                }
            }),
        ),
        spec("SERIESSUM", 4, 4, imp!(seriessum)),
        spec("MDETERM", 1, 1, imp!(mdeterm)),
        spec("MINVERSE", 1, 1, imp!(minverse)),
        spec("MMULT", 2, 2, imp!(mmult)),
        spec("MUNIT", 1, 1, imp!(munit)),
        spec("SUBTOTAL", 2, VARIADIC, imp!(subtotal)),
        spec("AGGREGATE", 3, VARIADIC, imp!(aggregate)),
        spec("ROUND", 2, 2, imp!(round)).lift(all),
        spec("ABS", 1, 1, imp!(|a, c| num(a, 0, c, 0.0).map(f64::abs))).lift(all),
        spec("INT", 1, 1, imp!(|a, c| num(a, 0, c, 0.0).map(f64::floor))).lift(all),
        spec("MOD", 2, 2, imp!(modulo)).lift(all),
    ];
    v.extend(core_spec("SUM"));
    v
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn roman_forms() {
        assert_eq!(to_roman(499, 0), "CDXCIX");
        assert_eq!(to_roman(499, 1), "LDVLIV");
        assert_eq!(to_roman(499, 2), "XDIX");
        assert_eq!(to_roman(499, 3), "VDIV");
        assert_eq!(to_roman(499, 4), "ID");
        assert_eq!(from_roman("mcmxii"), Ok(1912.0));
    }

    #[test]
    fn nested_subtotals_are_detected() {
        assert!(calls_subtotal("=SUBTOTAL(9,A1:A6)"));
        assert!(calls_subtotal("=1+aggregate (9,0,A:A)"));
        assert!(!calls_subtotal("=MYSUBTOTAL(1)"));
        assert!(!calls_subtotal("=\"SUBTOTAL\""));
    }

    #[test]
    fn random_numbers_are_uniform_in_range() {
        let xs: Vec<f64> = (0..10_000).map(|_| random()).collect();
        assert!(xs.iter().all(|x| (0.0..1.0).contains(x)));
        let m = xs.iter().sum::<f64>() / xs.len() as f64;
        assert!((m - 0.5).abs() < 0.02, "{m}");
    }
}
