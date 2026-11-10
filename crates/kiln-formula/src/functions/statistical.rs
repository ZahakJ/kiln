// SPDX-License-Identifier: AGPL-3.0-or-later
//! Statistical functions: descriptive statistics, ranks and percentiles, regression (`LINEST`,
//! `TREND` …) and hypothesis tests. Probability distributions are in
//! [`distributions`](super::distributions).

use std::sync::Arc;

use super::agg;
use super::special::{
    chisq_cdf, f_cdf, gamma, log_gamma, norm_cdf, norm_inv, norm_pdf, t_cdf, t_quantile_upper,
};
use super::support::*;
use crate::engine::{Arg, CallContext, EvalValue, collect_numbers};
use crate::registry::{ArgMask, FunctionSpec};
use crate::values::{Array2D, ErrorValue, Value, parse_number_text, round15};

// ── core aggregates (SUM's siblings) ───────────────────────────────────────────────────────

fn average(args: &mut [Arg<'_>], ctx: &mut dyn CallContext) -> R<f64> {
    let xs = collect_numbers(args, ctx)?;
    if xs.is_empty() {
        return Err(ErrorValue::Div0);
    }
    Ok(xs.iter().sum::<f64>() / xs.len() as f64)
}

fn min(args: &mut [Arg<'_>], ctx: &mut dyn CallContext) -> R<f64> {
    let xs = collect_numbers(args, ctx)?;
    Ok(if xs.is_empty() {
        0.0
    } else {
        xs.iter().copied().fold(f64::INFINITY, f64::min)
    })
}

fn max(args: &mut [Arg<'_>], ctx: &mut dyn CallContext) -> R<f64> {
    let xs = collect_numbers(args, ctx)?;
    Ok(if xs.is_empty() {
        0.0
    } else {
        xs.iter().copied().fold(f64::NEG_INFINITY, f64::max)
    })
}

fn count(args: &mut [Arg<'_>], ctx: &mut dyn CallContext) -> f64 {
    let mut n = 0.0;
    let opts = ctx.coercion();
    for a in args.iter_mut() {
        let v = ctx.force(a);
        ctx.for_each_scalar(&v, &mut |x, from_ref| {
            let counts = match x {
                Value::Number(_) => true,
                Value::Bool(_) => !from_ref,
                Value::Str(s) => !from_ref && parse_number_text(s, &opts).is_some(),
                _ => false,
            };
            if counts {
                n += 1.0;
            }
            true
        });
    }
    n
}

fn counta(args: &mut [Arg<'_>], ctx: &mut dyn CallContext) -> f64 {
    let mut n = 0.0;
    for a in args.iter_mut() {
        let v = ctx.force(a);
        if v == EvalValue::Missing {
            n += 1.0;
            continue;
        }
        ctx.for_each_scalar(&v, &mut |x, from_ref| {
            if !from_ref || !x.is_blank() {
                n += 1.0;
            }
            true
        });
    }
    n
}

// ── helpers ────────────────────────────────────────────────────────────────────────────────

fn covariance(xs: &[f64], ys: &[f64], sample: bool) -> R<f64> {
    let n = xs.len();
    if n == 0 || (sample && n < 2) {
        return Err(ErrorValue::Div0);
    }
    let (mx, my) = (mean(xs), mean(ys));
    let s: f64 = xs.iter().zip(ys).map(|(x, y)| (x - mx) * (y - my)).sum();
    Ok(s / if sample { n - 1 } else { n } as f64)
}

fn correl(xs: &[f64], ys: &[f64]) -> R<f64> {
    if xs.len() < 2 {
        return Err(ErrorValue::Div0);
    }
    let (mx, my) = (mean(xs), mean(ys));
    let (mut sxy, mut sxx, mut syy) = (0.0, 0.0, 0.0);
    for (x, y) in xs.iter().zip(ys) {
        let (dx, dy) = (x - mx, y - my);
        sxy += dx * dy;
        sxx += dx * dx;
        syy += dy * dy;
    }
    if sxx == 0.0 || syy == 0.0 {
        return Err(ErrorValue::Div0);
    }
    Ok(sxy / (sxx * syy).sqrt())
}

/// Simple regression of `ys` on `xs`.
struct Regression {
    sxx: f64,
    sxy: f64,
    syy: f64,
    slope: f64,
    intercept: f64,
}

fn regression(ys: &[f64], xs: &[f64]) -> Regression {
    let (mx, my) = (mean(xs), mean(ys));
    let (mut sxy, mut sxx, mut syy) = (0.0, 0.0, 0.0);
    for (x, y) in xs.iter().zip(ys) {
        let (dx, dy) = (x - mx, y - my);
        sxy += dx * dy;
        sxx += dx * dx;
        syy += dy * dy;
    }
    let slope = sxy / sxx;
    Regression {
        sxx,
        sxy,
        syy,
        slope,
        intercept: my - slope * mx,
    }
}

fn skew(xs: &[f64], population: bool) -> R<f64> {
    let n = xs.len() as f64;
    if n < if population { 1.0 } else { 3.0 } {
        return Err(ErrorValue::Div0);
    }
    let m = mean(xs);
    let sd = (devsq(xs) / if population { n } else { n - 1.0 }).sqrt();
    if sd == 0.0 {
        return Err(ErrorValue::Div0);
    }
    let s3: f64 = xs.iter().map(|x| ((x - m) / sd).powi(3)).sum();
    Ok(if population {
        s3 / n
    } else {
        n / ((n - 1.0) * (n - 2.0)) * s3
    })
}

fn kurt(xs: &[f64]) -> R<f64> {
    let n = xs.len() as f64;
    if n < 4.0 {
        return Err(ErrorValue::Div0);
    }
    let m = mean(xs);
    let sd = (devsq(xs) / (n - 1.0)).sqrt();
    if sd == 0.0 {
        return Err(ErrorValue::Div0);
    }
    let s4: f64 = xs.iter().map(|x| ((x - m) / sd).powi(4)).sum();
    Ok(n * (n + 1.0) / ((n - 1.0) * (n - 2.0) * (n - 3.0)) * s4
        - 3.0 * (n - 1.0).powi(2) / ((n - 2.0) * (n - 3.0)))
}

/// Excel's `PERCENTRANK`: interpolated rank, truncated (not rounded) to `sig` digits.
fn percent_rank(xs: &[f64], x: f64, sig: f64, exclusive: bool) -> R<f64> {
    let n = xs.len();
    if n == 0 || sig < 1.0 {
        return Err(if sig < 1.0 {
            ErrorValue::Num
        } else {
            ErrorValue::NA
        });
    }
    let s = sorted_asc(xs);
    if x < s[0] || x > s[n - 1] {
        return Err(ErrorValue::NA);
    }
    let nf = n as f64;
    let rank_at = |i: usize| {
        let i = i as f64;
        if exclusive {
            (i + 1.0) / (nf + 1.0)
        } else if n == 1 {
            1.0
        } else {
            i / (nf - 1.0)
        }
    };
    let mut lo = 0;
    while lo < n && s[lo] < x {
        lo += 1;
    }
    let r = if lo < n && s[lo] == x {
        rank_at(lo)
    } else {
        let (a, b) = (s[lo - 1], s[lo]);
        let frac = (x - a) / (b - a);
        rank_at(lo - 1) + frac * (rank_at(lo) - rank_at(lo - 1))
    };
    let f = 10f64.powf(sig.trunc());
    Ok((round15(r * f)).floor() / f)
}

fn rank(args: &mut [Arg<'_>], ctx: &mut dyn CallContext, avg: bool) -> R<f64> {
    let x = num(args, 0, ctx, 0.0)?;
    let xs = numbers_of(args, 1, ctx)?;
    let order = if args.len() > 2 {
        num(args, 2, ctx, 0.0)?
    } else {
        0.0
    };
    let (mut better, mut equal) = (0.0, 0.0);
    for v in xs {
        if round15(v) == round15(x) {
            equal += 1.0;
        } else if if order == 0.0 { v > x } else { v < x } {
            better += 1.0;
        }
    }
    if equal == 0.0 {
        return Err(ErrorValue::NA);
    }
    Ok(if avg {
        better + (equal + 1.0) / 2.0
    } else {
        better + 1.0
    })
}

fn frequency(args: &mut [Arg<'_>], ctx: &mut dyn CallContext) -> R<Value> {
    let data = numbers_of(args, 0, ctx)?;
    let bins = numbers_of(args, 1, ctx)?;
    let mut order: Vec<(f64, usize)> = bins.iter().copied().zip(0..).collect();
    order.sort_by(|p, q| p.0.total_cmp(&q.0));
    let mut counts = vec![0.0; bins.len() + 1];
    for x in data {
        let k = order.iter().find(|o| x <= o.0).map_or(bins.len(), |o| o.1);
        counts[k] += 1.0;
    }
    column(nums(counts))
}

fn trimmean(args: &mut [Arg<'_>], ctx: &mut dyn CallContext) -> R<f64> {
    let xs = numbers_of(args, 0, ctx)?;
    let p = number_of(args, 1, ctx, 0.0)?;
    if !(0.0..1.0).contains(&p) || xs.is_empty() {
        return Err(ErrorValue::Num);
    }
    let k = (xs.len() as f64 * p / 2.0).floor() as usize;
    let s = sorted_asc(&xs);
    Ok(mean(&s[k..xs.len() - k]))
}

fn prob(args: &mut [Arg<'_>], ctx: &mut dyn CallContext) -> R<f64> {
    let xa = array_of(args, 0, ctx)?;
    let pa = array_of(args, 1, ctx)?;
    if xa.len() != pa.len() {
        return Err(ErrorValue::NA);
    }
    let lo = number_of(args, 2, ctx, 0.0)?;
    let hi = match slot(args, 3, ctx) {
        Slot::Absent | Slot::Empty => lo,
        Slot::Val(v) => number(&v, ctx)?,
    };
    let (mut total, mut acc) = (0.0, 0.0);
    for (x, p) in xa.values().iter().zip(pa.values()) {
        if let Value::Error(e) = p {
            return Err(*e);
        }
        if let Value::Error(e) = x {
            return Err(*e);
        }
        let Value::Number(p) = p else { continue };
        if !(0.0..=1.0).contains(p) {
            return Err(ErrorValue::Num);
        }
        total += p;
        if let Value::Number(x) = x
            && *x >= lo
            && *x <= hi
        {
            acc += p;
        }
    }
    if (total - 1.0).abs() > 1e-7 {
        return Err(ErrorValue::Num);
    }
    Ok(acc)
}

// ── LINEST family ──────────────────────────────────────────────────────────────────────────

struct Fit {
    /// `[m_k … m_1, b]` in Excel's output order.
    coef: Vec<f64>,
    se: Vec<f64>,
    r2: f64,
    sey: f64,
    f: f64,
    df: f64,
    ssreg: f64,
    ssresid: f64,
}

fn invert(m: &[Vec<f64>]) -> Option<Vec<Vec<f64>>> {
    let n = m.len();
    let mut a = m.to_vec();
    let mut inv: Vec<Vec<f64>> = (0..n)
        .map(|i| (0..n).map(|j| if i == j { 1.0 } else { 0.0 }).collect())
        .collect();
    for c in 0..n {
        let mut piv = c;
        for r in c + 1..n {
            if a[r][c].abs() > a[piv][c].abs() {
                piv = r;
            }
        }
        let pv = a[piv][c];
        if pv.abs() < 1e-300 {
            return None;
        }
        a.swap(piv, c);
        inv.swap(piv, c);
        for j in 0..n {
            a[c][j] /= pv;
            inv[c][j] /= pv;
        }
        for r in 0..n {
            if r == c {
                continue;
            }
            let f = a[r][c];
            if f == 0.0 {
                continue;
            }
            for j in 0..n {
                a[r][j] -= f * a[c][j];
                inv[r][j] -= f * inv[c][j];
            }
        }
    }
    Some(inv)
}

/// Ordinary least squares of `y` on the columns of `x` (with an intercept when `with_const`),
/// regressing on centred data for numerical stability.
fn least_squares(y: &[f64], x: &[Vec<f64>], with_const: bool, want_stats: bool) -> R<Fit> {
    let n = y.len();
    let k = x.first().map_or(0, Vec::len);
    let p = k + usize::from(with_const);
    if n < 1 || k == 0 {
        return Err(ErrorValue::Value);
    }
    let mut mx = vec![0.0; k];
    let mut my = 0.0;
    if with_const {
        for i in 0..n {
            my += y[i];
            for j in 0..k {
                mx[j] += x[i][j];
            }
        }
        my /= n as f64;
        for m in &mut mx {
            *m /= n as f64;
        }
    }
    let xc: Vec<Vec<f64>> = x
        .iter()
        .map(|row| row.iter().zip(&mx).map(|(v, m)| v - m).collect())
        .collect();
    let yc: Vec<f64> = y.iter().map(|v| v - my).collect();
    let mut xtx = vec![vec![0.0; k]; k];
    let mut xty = vec![0.0; k];
    for i in 0..n {
        let r = &xc[i];
        for a in 0..k {
            xty[a] += r[a] * yc[i];
            for b in 0..k {
                xtx[a][b] += r[a] * r[b];
            }
        }
    }
    let inv = invert(&xtx).ok_or(ErrorValue::Num)?;
    let slopes: Vec<f64> = inv
        .iter()
        .map(|row| row.iter().zip(&xty).map(|(v, t)| v * t).sum())
        .collect();
    let intercept = if with_const {
        my - slopes.iter().zip(&mx).map(|(b, m)| b * m).sum::<f64>()
    } else {
        0.0
    };
    let (mut ssresid, mut sstot) = (0.0, 0.0);
    for i in 0..n {
        let fit: f64 = xc[i].iter().zip(&slopes).map(|(a, b)| a * b).sum();
        ssresid += (yc[i] - fit).powi(2);
        sstot += yc[i].powi(2);
    }
    if ssresid <= sstot * 1e-20 {
        ssresid = 0.0;
    }
    let df = n as f64 - p as f64;
    let ssreg = sstot - ssresid;
    let r2 = if sstot == 0.0 { 1.0 } else { ssreg / sstot };
    let sey = if df > 0.0 { (ssresid / df).sqrt() } else { 0.0 };
    let f = if df > 0.0 && ssresid > 0.0 {
        ssreg / k as f64 / (ssresid / df)
    } else {
        f64::NAN
    };
    let mut coef: Vec<f64> = slopes.iter().rev().copied().collect();
    coef.push(intercept);
    let mut se = Vec::new();
    if want_stats {
        let s2 = if df > 0.0 { ssresid / df } else { 0.0 };
        se = (0..k)
            .rev()
            .map(|i| (inv[i][i] * s2).max(0.0).sqrt())
            .collect();
        let mut se_b = f64::NAN;
        if with_const {
            let mut q = 0.0;
            for a in 0..k {
                for b in 0..k {
                    q += mx[a] * inv[a][b] * mx[b];
                }
            }
            se_b = (s2 * (1.0 / n as f64 + q)).max(0.0).sqrt();
        }
        se.push(se_b);
    }
    Ok(Fit {
        coef,
        se,
        r2,
        sey,
        f,
        df,
        ssreg,
        ssresid,
    })
}

#[derive(Clone, Copy, PartialEq)]
enum Shape {
    Col,
    Row,
}

struct Inputs {
    y: Vec<f64>,
    x: Vec<Vec<f64>>,
    y_shape: Shape,
    k: usize,
}

fn all_numbers(a: &Array2D) -> R<Vec<f64>> {
    a.values()
        .iter()
        .map(|v| match v {
            Value::Error(e) => Err(*e),
            Value::Number(n) => Ok(*n),
            _ => Err(ErrorValue::Value),
        })
        .collect()
}

/// Reads `known_y's` and `known_x's` into a vector and a design matrix (columns = predictors).
fn regression_inputs(args: &mut [Arg<'_>], ctx: &mut dyn CallContext) -> R<Inputs> {
    let ya = array_of(args, 0, ctx)?;
    let y = all_numbers(&ya)?;
    let n = y.len();
    let y_shape = if ya.cols() == 1 {
        Shape::Col
    } else {
        Shape::Row
    };
    let x: Vec<Vec<f64>> = if omitted(args, 1) {
        (1..=n).map(|i| vec![i as f64]).collect()
    } else {
        let xa = array_of(args, 1, ctx)?;
        let flat = all_numbers(&xa)?;
        if ya.rows() == 1 || ya.cols() == 1 {
            if xa.len() == n {
                flat.into_iter().map(|v| vec![v]).collect()
            } else if y_shape == Shape::Col && xa.rows() == n {
                flat.chunks(xa.cols()).map(<[f64]>::to_vec).collect()
            } else if y_shape == Shape::Row && xa.cols() == n {
                (0..n)
                    .map(|c| (0..xa.rows()).map(|r| flat[r * xa.cols() + c]).collect())
                    .collect()
            } else {
                return Err(ErrorValue::Ref);
            }
        } else {
            if xa.rows() != ya.rows() || xa.cols() != ya.cols() {
                return Err(ErrorValue::Ref);
            }
            flat.into_iter().map(|v| vec![v]).collect()
        }
    };
    let k = x.first().map_or(0, Vec::len);
    Ok(Inputs { y, x, y_shape, k })
}

fn truthy_or(args: &mut [Arg<'_>], i: usize, ctx: &mut dyn CallContext, dflt: bool) -> R<bool> {
    match slot(args, i, ctx) {
        Slot::Absent | Slot::Empty => Ok(dflt),
        Slot::Val(v) => Ok(number(&v, ctx)? != 0.0),
    }
}

fn linest(args: &mut [Arg<'_>], ctx: &mut dyn CallContext, log: bool) -> R<Value> {
    let inp = regression_inputs(args, ctx)?;
    let with_const = truthy_or(args, 2, ctx, true)?;
    let stats = truthy_or(args, 3, ctx, false)?;
    let mut y = inp.y;
    if log {
        if y.iter().any(|v| *v <= 0.0) {
            return Err(ErrorValue::Num);
        }
        y = y.iter().map(|v| v.ln()).collect();
    }
    let f = least_squares(&y, &inp.x, with_const, stats)?;
    let row1: Vec<Value> = f
        .coef
        .iter()
        .map(|c| Value::number(if log { c.exp() } else { *c }))
        .collect();
    if !stats {
        return row(row1);
    }
    let width = row1.len();
    let pad = |vals: Vec<Value>| -> Vec<Value> {
        let mut v = vals;
        v.resize(width, Value::Error(ErrorValue::NA));
        v
    };
    let se: Vec<Value> =
        f.se.iter()
            .enumerate()
            .map(|(i, v)| {
                if i == width - 1 && !with_const {
                    Value::Error(ErrorValue::NA)
                } else {
                    Value::number(*v)
                }
            })
            .collect();
    let f_value = if f.f.is_finite() {
        Value::number(f.f)
    } else {
        Value::Error(ErrorValue::Num)
    };
    from_rows(vec![
        row1,
        se,
        pad(vec![Value::number(f.r2), Value::number(f.sey)]),
        pad(vec![f_value, Value::number(f.df)]),
        pad(vec![Value::number(f.ssreg), Value::number(f.ssresid)]),
    ])
}

fn trend(args: &mut [Arg<'_>], ctx: &mut dyn CallContext, log: bool) -> R<Value> {
    let inp = regression_inputs(args, ctx)?;
    let with_const = truthy_or(args, 3, ctx, true)?;
    let mut y = inp.y;
    if log {
        if y.iter().any(|v| *v <= 0.0) {
            return Err(ErrorValue::Num);
        }
        y = y.iter().map(|v| v.ln()).collect();
    }
    let f = least_squares(&y, &inp.x, with_const, false)?;
    let k = inp.k;
    let slopes: Vec<f64> = f.coef[..k].iter().rev().copied().collect();
    let b = f.coef[k];
    let new_x: Arc<Array2D> = if omitted(args, 2) {
        let a = if k == 1 {
            let col: Vec<Value> = inp.x.iter().map(|r| Value::Number(r[0])).collect();
            if inp.y_shape == Shape::Col {
                Array2D::column(col)
            } else {
                Array2D::row(col)
            }
        } else {
            Array2D::from_rows(
                inp.x
                    .iter()
                    .map(|r| r.iter().map(|v| Value::Number(*v)).collect())
                    .collect(),
            )
        };
        Arc::new(a.ok_or(ErrorValue::Value)?)
    } else {
        array_of(args, 2, ctx)?
    };
    let pred = |xs: &[f64]| {
        let v = b + slopes.iter().zip(xs).map(|(s, x)| s * x).sum::<f64>();
        if log { v.exp() } else { v }
    };
    if k == 1 {
        return Ok(Value::array(new_x.map(|v| match v {
            Value::Number(x) => Value::number(pred(&[*x])),
            Value::Error(e) => Value::Error(*e),
            _ => Value::Error(ErrorValue::Value),
        })));
    }
    // Several predictors: one prediction per row (or column) of new_x.
    let by_row = new_x.cols() == k;
    let m = if by_row { new_x.rows() } else { new_x.cols() };
    let mut out = Vec::with_capacity(m);
    for i in 0..m {
        let mut xs = Vec::with_capacity(k);
        for j in 0..k {
            let v = if by_row {
                new_x.get(i, j)
            } else {
                new_x.get(j, i)
            };
            match v {
                Value::Number(x) => xs.push(*x),
                _ => return Err(ErrorValue::Value),
            }
        }
        out.push(Value::number(pred(&xs)));
    }
    if by_row { column(out) } else { row(out) }
}

// ── tests ──────────────────────────────────────────────────────────────────────────────────

fn ttest(args: &mut [Arg<'_>], ctx: &mut dyn CallContext) -> R<f64> {
    let tails = int_of(args, 2, ctx, 0.0)?;
    let kind = int_of(args, 3, ctx, 0.0)?;
    if (tails != 1.0 && tails != 2.0) || !(1.0..=3.0).contains(&kind) {
        return Err(ErrorValue::Num);
    }
    let (t, df);
    if kind == 1.0 {
        let (xs, ys) = numeric_pairs(args, 0, 1, ctx)?;
        let d: Vec<f64> = xs.iter().zip(&ys).map(|(x, y)| x - y).collect();
        let n = d.len() as f64;
        if n < 2.0 {
            return Err(ErrorValue::Div0);
        }
        let sd = (devsq(&d) / (n - 1.0)).sqrt();
        if sd == 0.0 {
            return Err(ErrorValue::Div0);
        }
        t = mean(&d) / (sd / n.sqrt());
        df = n - 1.0;
    } else {
        let a = numbers_of(args, 0, ctx)?;
        let b = numbers_of(args, 1, ctx)?;
        let (n1, n2) = (a.len() as f64, b.len() as f64);
        if n1 < 2.0 || n2 < 2.0 {
            return Err(ErrorValue::Div0);
        }
        let v1 = devsq(&a) / (n1 - 1.0);
        let v2 = devsq(&b) / (n2 - 1.0);
        if kind == 2.0 {
            let sp = ((n1 - 1.0) * v1 + (n2 - 1.0) * v2) / (n1 + n2 - 2.0);
            if sp == 0.0 {
                return Err(ErrorValue::Div0);
            }
            t = (mean(&a) - mean(&b)) / (sp * (1.0 / n1 + 1.0 / n2)).sqrt();
            df = n1 + n2 - 2.0;
        } else {
            let s = v1 / n1 + v2 / n2;
            if s == 0.0 {
                return Err(ErrorValue::Div0);
            }
            t = (mean(&a) - mean(&b)) / s.sqrt();
            df = s * s / ((v1 / n1).powi(2) / (n1 - 1.0) + (v2 / n2).powi(2) / (n2 - 1.0));
        }
    }
    let one = 1.0 - t_cdf(t.abs(), df);
    Ok(if tails == 1.0 { one } else { 2.0 * one })
}

fn ftest(args: &mut [Arg<'_>], ctx: &mut dyn CallContext) -> R<f64> {
    let a = numbers_of(args, 0, ctx)?;
    let b = numbers_of(args, 1, ctx)?;
    if a.len() < 2 || b.len() < 2 {
        return Err(ErrorValue::Div0);
    }
    let v1 = devsq(&a) / (a.len() - 1) as f64;
    let v2 = devsq(&b) / (b.len() - 1) as f64;
    if v1 == 0.0 || v2 == 0.0 {
        return Err(ErrorValue::Div0);
    }
    let c = f_cdf(v1 / v2, (a.len() - 1) as f64, (b.len() - 1) as f64);
    Ok(2.0 * c.min(1.0 - c))
}

fn chitest(args: &mut [Arg<'_>], ctx: &mut dyn CallContext) -> R<f64> {
    let act = array_of(args, 0, ctx)?;
    let exp = array_of(args, 1, ctx)?;
    if act.rows() != exp.rows() || act.cols() != exp.cols() {
        return Err(ErrorValue::NA);
    }
    let mut chi = 0.0;
    for (a, e) in act.values().iter().zip(exp.values()) {
        if let Value::Error(x) = a {
            return Err(*x);
        }
        if let Value::Error(x) = e {
            return Err(*x);
        }
        let (Value::Number(a), Value::Number(e)) = (a, e) else {
            continue;
        };
        if *e == 0.0 {
            return Err(ErrorValue::Div0);
        }
        chi += (a - e).powi(2) / e;
    }
    let (r, c) = (act.rows() as f64, act.cols() as f64);
    let df = if r > 1.0 && c > 1.0 {
        (r - 1.0) * (c - 1.0)
    } else {
        r * c - 1.0
    };
    if df < 1.0 {
        return Err(ErrorValue::NA);
    }
    Ok(1.0 - chisq_cdf(chi, df))
}

fn ztest(args: &mut [Arg<'_>], ctx: &mut dyn CallContext) -> R<f64> {
    let xs = numbers_of(args, 0, ctx)?;
    let x = number_of(args, 1, ctx, 0.0)?;
    let n = xs.len() as f64;
    if n == 0.0 {
        return Err(ErrorValue::NA);
    }
    let sigma = match slot(args, 2, ctx) {
        Slot::Absent | Slot::Empty => {
            if n < 2.0 {
                return Err(ErrorValue::Div0);
            }
            (devsq(&xs) / (n - 1.0)).sqrt()
        }
        Slot::Val(v) => number(&v, ctx)?,
    };
    if sigma == 0.0 {
        return Err(ErrorValue::Div0);
    }
    Ok(1.0 - norm_cdf((mean(&xs) - x) / (sigma / n.sqrt())))
}

fn percent_rank_fn(args: &mut [Arg<'_>], ctx: &mut dyn CallContext, exclusive: bool) -> R<f64> {
    let xs = numbers_of(args, 0, ctx)?;
    let x = num(args, 1, ctx, 0.0)?;
    let sig = num(args, 2, ctx, 3.0)?;
    percent_rank(&xs, x, sig, exclusive)
}

fn array_k(
    args: &mut [Arg<'_>],
    ctx: &mut dyn CallContext,
    f: fn(&[f64], f64) -> R<f64>,
) -> R<f64> {
    let xs = numbers_of(args, 0, ctx)?;
    let k = num(args, 1, ctx, 0.0)?;
    f(&xs, k)
}

fn paired(
    args: &mut [Arg<'_>],
    ctx: &mut dyn CallContext,
    f: fn(&[f64], &[f64]) -> R<f64>,
) -> R<f64> {
    let (xs, ys) = numeric_pairs(args, 0, 1, ctx)?;
    f(&xs, &ys)
}

fn slope(ys: &[f64], xs: &[f64]) -> R<f64> {
    if xs.is_empty() {
        return Err(ErrorValue::Div0);
    }
    let r = regression(ys, xs);
    if r.sxx == 0.0 {
        Err(ErrorValue::Div0)
    } else {
        Ok(r.slope)
    }
}

fn intercept(ys: &[f64], xs: &[f64]) -> R<f64> {
    if xs.is_empty() {
        return Err(ErrorValue::Div0);
    }
    let r = regression(ys, xs);
    if r.sxx == 0.0 {
        Err(ErrorValue::Div0)
    } else {
        Ok(r.intercept)
    }
}

fn steyx(ys: &[f64], xs: &[f64]) -> R<f64> {
    let n = xs.len() as f64;
    if n < 3.0 {
        return Err(ErrorValue::Div0);
    }
    let r = regression(ys, xs);
    if r.sxx == 0.0 {
        return Err(ErrorValue::Div0);
    }
    Ok(((r.syy - r.sxy * r.sxy / r.sxx) / (n - 2.0)).sqrt())
}

fn forecast(args: &mut [Arg<'_>], ctx: &mut dyn CallContext) -> R<f64> {
    let x = num(args, 0, ctx, 0.0)?;
    let (ys, xs) = numeric_pairs(args, 1, 2, ctx)?;
    if ys.is_empty() {
        return Err(ErrorValue::Div0);
    }
    let r = regression(&ys, &xs);
    if r.sxx == 0.0 {
        return Err(ErrorValue::Div0);
    }
    Ok(r.intercept + r.slope * x)
}

fn avedev(xs: &[f64]) -> R<f64> {
    if xs.is_empty() {
        return Err(ErrorValue::Num);
    }
    let m = mean(xs);
    let d: Vec<f64> = xs.iter().map(|x| (x - m).abs()).collect();
    Ok(sum(&d) / xs.len() as f64)
}

fn geomean(xs: &[f64]) -> R<f64> {
    if xs.is_empty() || xs.iter().any(|x| *x <= 0.0) {
        return Err(ErrorValue::Num);
    }
    let logs: Vec<f64> = xs.iter().map(|x| x.ln()).collect();
    Ok((sum(&logs) / xs.len() as f64).exp())
}

fn harmean(xs: &[f64]) -> R<f64> {
    if xs.is_empty() || xs.iter().any(|x| *x <= 0.0) {
        return Err(ErrorValue::Num);
    }
    let inv: Vec<f64> = xs.iter().map(|x| 1.0 / x).collect();
    Ok(xs.len() as f64 / sum(&inv))
}

fn confidence_t(alpha: f64, sd: f64, n0: f64) -> R<f64> {
    let n = n0.trunc();
    if alpha <= 0.0 || alpha >= 1.0 || sd <= 0.0 || n < 1.0 {
        return Err(ErrorValue::Num);
    }
    if n == 1.0 {
        return Err(ErrorValue::Div0);
    }
    let t = t_quantile_upper(alpha / 2.0, n - 1.0);
    Ok(t * sd / n.sqrt())
}

/// The statistical functions.
pub fn specs() -> Vec<FunctionSpec> {
    let lift1 = ArgMask::of(&[1]);
    let lift02 = ArgMask::of(&[0, 2]);
    let lift12 = ArgMask::of(&[1, 2]);
    let stdev_s = spec(
        "STDEV.S",
        1,
        VARIADIC,
        imp!(|a, c| numbers(a, c).and_then(|xs| agg::stdev_s(&xs))),
    );
    let stdev_p = spec(
        "STDEV.P",
        1,
        VARIADIC,
        imp!(|a, c| numbers(a, c).and_then(|xs| agg::stdev_p(&xs))),
    );
    let var_s = spec(
        "VAR.S",
        1,
        VARIADIC,
        imp!(|a, c| numbers(a, c).and_then(|xs| agg::var_s(&xs))),
    );
    let var_p = spec(
        "VAR.P",
        1,
        VARIADIC,
        imp!(|a, c| numbers(a, c).and_then(|xs| agg::var_p(&xs))),
    );
    let mode_sngl = spec(
        "MODE.SNGL",
        1,
        VARIADIC,
        imp!(|a, c| numbers(a, c).and_then(|xs| agg::mode_sngl(&xs))),
    );
    let percentile_inc = spec(
        "PERCENTILE.INC",
        2,
        2,
        imp!(|a, c| array_k(a, c, agg::percentile_inc)),
    )
    .lift(lift1);
    let quartile_inc = spec(
        "QUARTILE.INC",
        2,
        2,
        imp!(|a, c| array_k(a, c, agg::quartile_inc)),
    )
    .lift(lift1);
    let covariance_p = spec(
        "COVARIANCE.P",
        2,
        2,
        imp!(|a, c| paired(a, c, |x, y| covariance(x, y, false))),
    );
    let percentrank_inc = spec(
        "PERCENTRANK.INC",
        2,
        3,
        imp!(|a, c| percent_rank_fn(a, c, false)),
    )
    .lift(lift12);
    let rank_eq = spec("RANK.EQ", 2, 3, imp!(|a, c| rank(a, c, false))).lift(lift02);
    let mut v = vec![
        spec("AVERAGE", 1, VARIADIC, imp!(average)),
        spec("MIN", 1, VARIADIC, imp!(min)),
        spec("MAX", 1, VARIADIC, imp!(max)),
        spec("COUNT", 1, VARIADIC, imp!(count)),
        spec("COUNTA", 1, VARIADIC, imp!(counta)),
        spec(
            "AVEDEV",
            1,
            VARIADIC,
            imp!(|a, c| numbers(a, c).and_then(|xs| avedev(&xs))),
        ),
        spec(
            "AVERAGEA",
            1,
            VARIADIC,
            imp!(|a, c| numbers_a(a, c).and_then(|xs| agg::average(&xs))),
        ),
        spec(
            "MAXA",
            1,
            VARIADIC,
            imp!(|a, c| numbers_a(a, c).and_then(|xs| agg::max_of(&xs))),
        ),
        spec(
            "MINA",
            1,
            VARIADIC,
            imp!(|a, c| numbers_a(a, c).and_then(|xs| agg::min_of(&xs))),
        ),
        spec(
            "MEDIAN",
            1,
            VARIADIC,
            imp!(|a, c| numbers(a, c).and_then(|xs| agg::median(&xs))),
        ),
        alias("MODE", &mode_sngl),
        mode_sngl,
        spec(
            "MODE.MULT",
            1,
            VARIADIC,
            imp!(|a, c| {
                let xs = numbers(a, c)?;
                let m = agg::modes(&xs);
                if m.is_empty() {
                    Err(ErrorValue::NA)
                } else {
                    column(nums(m))
                }
            }),
        ),
        spec(
            "DEVSQ",
            1,
            VARIADIC,
            imp!(|a, c| numbers(a, c).and_then(|xs| if xs.is_empty() {
                Err(ErrorValue::Num)
            } else {
                Ok(devsq(&xs))
            })),
        ),
        spec(
            "GEOMEAN",
            1,
            VARIADIC,
            imp!(|a, c| numbers(a, c).and_then(|xs| geomean(&xs))),
        ),
        spec(
            "HARMEAN",
            1,
            VARIADIC,
            imp!(|a, c| numbers(a, c).and_then(|xs| harmean(&xs))),
        ),
        spec(
            "KURT",
            1,
            VARIADIC,
            imp!(|a, c| numbers(a, c).and_then(|xs| kurt(&xs))),
        ),
        spec(
            "SKEW",
            1,
            VARIADIC,
            imp!(|a, c| numbers(a, c).and_then(|xs| skew(&xs, false))),
        ),
        spec(
            "SKEW.P",
            1,
            VARIADIC,
            imp!(|a, c| numbers(a, c).and_then(|xs| skew(&xs, true))),
        ),
        alias("STDEV", &stdev_s),
        stdev_s,
        alias("STDEVP", &stdev_p),
        stdev_p,
        spec(
            "STDEVA",
            1,
            VARIADIC,
            imp!(|a, c| numbers_a(a, c).and_then(|xs| agg::stdev_s(&xs))),
        ),
        spec(
            "STDEVPA",
            1,
            VARIADIC,
            imp!(|a, c| numbers_a(a, c).and_then(|xs| agg::stdev_p(&xs))),
        ),
        alias("VAR", &var_s),
        var_s,
        alias("VARP", &var_p),
        var_p,
        spec(
            "VARA",
            1,
            VARIADIC,
            imp!(|a, c| numbers_a(a, c).and_then(|xs| agg::var_s(&xs))),
        ),
        spec(
            "VARPA",
            1,
            VARIADIC,
            imp!(|a, c| numbers_a(a, c).and_then(|xs| agg::var_p(&xs))),
        ),
        spec("LARGE", 2, 2, imp!(|a, c| array_k(a, c, agg::large))).lift(lift1),
        spec("SMALL", 2, 2, imp!(|a, c| array_k(a, c, agg::small))).lift(lift1),
        alias("PERCENTILE", &percentile_inc),
        percentile_inc,
        spec(
            "PERCENTILE.EXC",
            2,
            2,
            imp!(|a, c| array_k(a, c, agg::percentile_exc)),
        )
        .lift(lift1),
        alias("QUARTILE", &quartile_inc),
        quartile_inc,
        spec(
            "QUARTILE.EXC",
            2,
            2,
            imp!(|a, c| array_k(a, c, agg::quartile_exc)),
        )
        .lift(lift1),
        alias("PERCENTRANK", &percentrank_inc),
        percentrank_inc,
        spec(
            "PERCENTRANK.EXC",
            2,
            3,
            imp!(|a, c| percent_rank_fn(a, c, true)),
        )
        .lift(lift12),
        alias("RANK", &rank_eq),
        rank_eq,
        spec("RANK.AVG", 2, 3, imp!(|a, c| rank(a, c, true))).lift(lift02),
        spec("TRIMMEAN", 2, 2, imp!(trimmean)),
        spec("FREQUENCY", 2, 2, imp!(frequency)),
        spec("PROB", 3, 4, imp!(prob)),
        spec("CORREL", 2, 2, imp!(|a, c| paired(a, c, correl))),
        spec("PEARSON", 2, 2, imp!(|a, c| paired(a, c, correl))),
        spec(
            "RSQ",
            2,
            2,
            imp!(|a, c| paired(a, c, |x, y| correl(x, y).map(|r| r * r))),
        ),
        alias("COVAR", &covariance_p),
        covariance_p,
        spec(
            "COVARIANCE.S",
            2,
            2,
            imp!(|a, c| paired(a, c, |x, y| covariance(x, y, true))),
        ),
        spec("SLOPE", 2, 2, imp!(|a, c| paired(a, c, slope))),
        spec("INTERCEPT", 2, 2, imp!(|a, c| paired(a, c, intercept))),
        spec("STEYX", 2, 2, imp!(|a, c| paired(a, c, steyx))),
        spec("FORECAST", 3, 3, imp!(forecast)).lift(ArgMask::of(&[0])),
        spec("FORECAST.LINEAR", 3, 3, imp!(forecast)).lift(ArgMask::of(&[0])),
        spec("LINEST", 1, 4, imp!(|a, c| linest(a, c, false))),
        spec("LOGEST", 1, 4, imp!(|a, c| linest(a, c, true))),
        spec("TREND", 1, 4, imp!(|a, c| trend(a, c, false))),
        spec("GROWTH", 1, 4, imp!(|a, c| trend(a, c, true))),
        numeric!("STANDARDIZE", 3, 3, [], |x, m, sd| if sd <= 0.0 {
            Err(ErrorValue::Num)
        } else {
            Ok((x - m) / sd)
        }),
        numeric!("FISHER", 1, 1, [], |x| if x <= -1.0 || x >= 1.0 {
            Err(ErrorValue::Num)
        } else {
            Ok(0.5 * ((1.0 + x) / (1.0 - x)).ln())
        }),
        numeric!("FISHERINV", 1, 1, [], |y| y.tanh()),
        numeric!("PERMUT", 2, 2, [], |n0, k0| {
            let (n, k) = (n0.trunc(), k0.trunc());
            if n < 0.0 || k < 0.0 || n < k {
                Err(ErrorValue::Num)
            } else {
                let mut r = 1.0f64;
                let mut i = 0.0;
                while i < k && r.is_finite() {
                    r *= n - i;
                    i += 1.0;
                }
                Ok(r)
            }
        }),
        numeric!("PERMUTATIONA", 2, 2, [], |n0, k0| {
            let (n, k) = (n0.trunc(), k0.trunc());
            if n < 0.0 || k < 0.0 {
                Err(ErrorValue::Num)
            } else {
                Ok(n.powf(k))
            }
        }),
        numeric!("GAMMA", 1, 1, [], |x| if x <= 0.0 && x.fract() == 0.0 {
            Err(ErrorValue::Num)
        } else {
            Ok(gamma(x))
        }),
        numeric!("GAMMALN", 1, 1, [], |x| if x <= 0.0 {
            Err(ErrorValue::Num)
        } else {
            Ok(log_gamma(x))
        }),
        numeric!("GAMMALN.PRECISE", 1, 1, [], |x| if x <= 0.0 {
            Err(ErrorValue::Num)
        } else {
            Ok(log_gamma(x))
        }),
        numeric!("GAUSS", 1, 1, [], |z| norm_cdf(z) - 0.5),
        numeric!("PHI", 1, 1, [], |x| norm_pdf(x)),
        numeric!(
            "CONFIDENCE.NORM",
            3,
            3,
            [],
            |alpha, sd, n0| confidence_norm(alpha, sd, n0)
        ),
        numeric!("CONFIDENCE", 3, 3, [], |alpha, sd, n0| confidence_norm(
            alpha, sd, n0
        )),
        numeric!("CONFIDENCE.T", 3, 3, [], |alpha, sd, n0| confidence_t(
            alpha, sd, n0
        )),
        spec("T.TEST", 4, 4, imp!(ttest)),
        spec("TTEST", 4, 4, imp!(ttest)),
        spec("F.TEST", 2, 2, imp!(ftest)),
        spec("FTEST", 2, 2, imp!(ftest)),
        spec("CHISQ.TEST", 2, 2, imp!(chitest)),
        spec("CHITEST", 2, 2, imp!(chitest)),
        spec("Z.TEST", 2, 3, imp!(ztest)),
        spec("ZTEST", 2, 3, imp!(ztest)),
    ];
    v.shrink_to_fit();
    v
}

fn confidence_norm(alpha: f64, sd: f64, n0: f64) -> R<f64> {
    let n = n0.trunc();
    if alpha <= 0.0 || alpha >= 1.0 || sd <= 0.0 || n < 1.0 {
        return Err(ErrorValue::Num);
    }
    Ok(norm_inv(1.0 - alpha / 2.0) * sd / n.sqrt())
}
