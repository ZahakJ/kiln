// SPDX-License-Identifier: AGPL-3.0-or-later
//! Conditional aggregates: `COUNTIF(S)`, `SUMIF(S)`, `AVERAGEIF(S)`, `MAXIFS`, `MINIFS`,
//! `COUNTBLANK`.
//!
//! Criteria arguments are lifted, so `COUNTIF(A:A,{"a","b"})` returns an array of counts as in
//! Excel. Ranges are read lazily and only within the used extent of the sheet, so whole-column
//! ranges cost what the data costs.

use std::sync::Arc;

use super::criteria::{Criterion, parse_criterion};
use super::grid::{Grid, grid_of, resized_grid};
use super::support::*;
use crate::engine::{Arg, CallContext, EvalValue, broadcast_dims, broadcast_pick};
use crate::registry::{ArgMask, FunctionImpl, FunctionSpec};
use crate::values::{Array2D, ErrorValue, Value};

fn criterion_of(args: &mut [Arg<'_>], i: usize, ctx: &mut dyn CallContext) -> Criterion {
    let c = scalar(args, i, ctx);
    parse_criterion(&c, &ctx.coercion())
}

/// Positions `(r, c)` where every (grid, criterion) pair matches, plus how many positions beyond
/// every grid's used area (blank everywhere) match too.
fn matches(
    pairs: &[(Grid, Criterion)],
    target: Option<&Grid>,
    ctx: &mut dyn CallContext,
) -> R<(Vec<(usize, usize)>, usize)> {
    let Some((first, _)) = pairs.first() else {
        return Err(ErrorValue::Value);
    };
    let (rows0, cols0) = (first.rows(), first.cols());
    if pairs
        .iter()
        .any(|(g, _)| g.rows() != rows0 || g.cols() != cols0)
    {
        return Err(ErrorValue::Value);
    }
    let mut rows = target.map_or(0, Grid::used_rows);
    let mut cols = target.map_or(0, Grid::used_cols);
    for (g, _) in pairs {
        rows = rows.max(g.used_rows());
        cols = cols.max(g.used_cols());
    }
    let mut hits = Vec::new();
    for r in 0..rows {
        for c in 0..cols {
            let mut ok = true;
            for (g, crit) in pairs {
                if !crit.matches(&g.get(r, c, ctx)) {
                    ok = false;
                    break;
                }
            }
            if ok {
                hits.push((r, c));
            }
        }
    }
    let outside = (rows0 * cols0).saturating_sub(rows * cols);
    let blank_matches = pairs.iter().all(|(_, crit)| crit.matches(&Value::Blank));
    Ok((
        hits,
        if outside > 0 && blank_matches {
            outside
        } else {
            0
        },
    ))
}

fn count_ifs(args: &mut [Arg<'_>], ctx: &mut dyn CallContext) -> R<f64> {
    let mut pairs = Vec::new();
    let mut i = 0;
    while i < args.len() {
        let g = grid_of(args, i, ctx)?;
        let crit = criterion_of(args, i + 1, ctx);
        pairs.push((g, crit));
        i += 2;
    }
    let (hits, outside) = matches(&pairs, None, ctx)?;
    Ok((hits.len() + outside) as f64)
}

type Fold = fn(&[f64]) -> R<f64>;

/// Aggregates the numbers of `target` at positions matching every criterion.
fn fold_ifs(
    target: &Grid,
    pairs: &[(Grid, Criterion)],
    fold: Fold,
    ctx: &mut dyn CallContext,
) -> R<f64> {
    if pairs
        .iter()
        .any(|(g, _)| g.rows() != target.rows() || g.cols() != target.cols())
    {
        return Err(ErrorValue::Value);
    }
    let (hits, _) = matches(pairs, Some(target), ctx)?;
    let mut values = Vec::with_capacity(hits.len());
    let mut err = None;
    for (r, c) in hits {
        match target.get(r, c, ctx) {
            Value::Number(n) => values.push(n),
            Value::Error(e) if err.is_none() => err = Some(e),
            _ => {}
        }
    }
    if let Some(e) = err {
        return Err(e);
    }
    fold(&values)
}

fn sum_fold(xs: &[f64]) -> R<f64> {
    Ok(sum(xs))
}

fn avg_fold(xs: &[f64]) -> R<f64> {
    if xs.is_empty() {
        Err(ErrorValue::Div0)
    } else {
        Ok(sum(xs) / xs.len() as f64)
    }
}

fn max_fold(xs: &[f64]) -> R<f64> {
    Ok(if xs.is_empty() {
        0.0
    } else {
        xs.iter().copied().fold(f64::NEG_INFINITY, f64::max)
    })
}

fn min_fold(xs: &[f64]) -> R<f64> {
    Ok(if xs.is_empty() {
        0.0
    } else {
        xs.iter().copied().fold(f64::INFINITY, f64::min)
    })
}

fn if_single(args: &mut [Arg<'_>], ctx: &mut dyn CallContext, fold: Fold) -> R<f64> {
    let range = grid_of(args, 0, ctx)?;
    let crit = criterion_of(args, 1, ctx);
    let target = if args.len() > 2 && !matches!(&args[2], Arg::Value(EvalValue::Missing)) {
        resized_grid(args, 2, &range, ctx)?
    } else {
        range.clone()
    };
    fold_ifs(&target, &[(range, crit)], fold, ctx)
}

fn if_multi(args: &mut [Arg<'_>], ctx: &mut dyn CallContext, fold: Fold) -> R<f64> {
    if !(args.len() - 1).is_multiple_of(2) {
        return Err(ErrorValue::Value);
    }
    let target = grid_of(args, 0, ctx)?;
    let mut pairs = Vec::new();
    let mut i = 1;
    while i < args.len() {
        let g = grid_of(args, i, ctx)?;
        let crit = criterion_of(args, i + 1, ctx);
        pairs.push((g, crit));
        i += 2;
    }
    fold_ifs(&target, &pairs, fold, ctx)
}

fn countblank(args: &mut [Arg<'_>], ctx: &mut dyn CallContext) -> R<f64> {
    let g = grid_of(args, 0, ctx)?;
    let mut n = (g.rows() * g.cols()).saturating_sub(g.used_rows() * g.used_cols());
    for r in 0..g.used_rows() {
        for c in 0..g.used_cols() {
            let v = g.get(r, c, ctx);
            if v.is_blank() || v.as_str() == Some("") {
                n += 1;
            }
        }
    }
    Ok(n as f64)
}

/// Lifts criteria at positions 64 and beyond (the registry's lift mask covers positions below
/// 64): arrays there make the call element-wise, as for the lower positions.
fn lift_tail(
    args: &mut [Arg<'_>],
    ctx: &mut dyn CallContext,
    first: usize,
    f: FunctionImpl,
) -> EvalValue {
    let mut lifted = false;
    let mut p = first;
    while p < args.len() {
        if p >= 64 {
            let v = ctx.force(&mut args[p]);
            let v = match v {
                EvalValue::Ref(r) if ctx.legacy() => ctx.implicit_intersection(&r),
                EvalValue::Missing => Value::Blank,
                other => ctx.to_value(other),
            };
            lifted |= matches!(&v, Value::Array(a) if a.len() > 1);
            args[p] = Arg::Value(EvalValue::Value(v));
        }
        p += 2;
    }
    if !lifted {
        return f(args, ctx);
    }
    let positions: Vec<usize> = (first..args.len())
        .step_by(2)
        .filter(|&p| p >= 64)
        .collect();
    let operands: Vec<Value> = positions
        .iter()
        .filter_map(|&p| match &args[p] {
            Arg::Value(EvalValue::Value(v @ Value::Array(_))) => Some(v.clone()),
            _ => None,
        })
        .collect();
    let (rows, cols) = broadcast_dims(operands.iter());
    let mut data = Vec::with_capacity(rows * cols);
    for r in 0..rows {
        for c in 0..cols {
            let mut call: Vec<Arg<'_>> = args.to_vec();
            for &p in &positions {
                if let Arg::Value(EvalValue::Value(v @ Value::Array(_))) = &args[p] {
                    call[p] = Arg::Value(EvalValue::Value(broadcast_pick(v, r, c)));
                }
            }
            let out = f(&mut call, ctx);
            data.push(ctx.to_scalar(out));
        }
    }
    Array2D::new(rows, cols, data).map_or(EvalValue::Value(Value::Error(ErrorValue::Calc)), |a| {
        EvalValue::Value(Value::Array(Arc::new(a)))
    })
}

/// Criteria positions `start, start + 2, …` below 64 (the lift mask).
const fn criteria_mask(start: u8) -> ArgMask {
    let mut bits = 0u64;
    let mut i = start;
    while i < 64 {
        bits |= 1u64 << i;
        i += 2;
    }
    ArgMask::bits(bits)
}

fn countifs(args: &mut [Arg<'_>], ctx: &mut dyn CallContext) -> EvalValue {
    if !args.len().is_multiple_of(2) {
        return ErrorValue::Value.into();
    }
    lift_tail(args, ctx, 1, imp!(count_ifs))
}

/// The conditional aggregates.
pub fn specs() -> Vec<FunctionSpec> {
    let one = ArgMask::of(&[1]);
    vec![
        spec("COUNTIF", 2, 2, imp!(count_ifs)).lift(one),
        spec("COUNTIFS", 2, 254, countifs).lift(criteria_mask(1)),
        spec("SUMIF", 2, 3, imp!(|a, c| if_single(a, c, sum_fold))).lift(one),
        spec(
            "SUMIFS",
            3,
            255,
            imp!(|a, c| lift_tail(a, c, 2, imp!(|a, c| if_multi(a, c, sum_fold)))),
        )
        .lift(criteria_mask(2)),
        spec("AVERAGEIF", 2, 3, imp!(|a, c| if_single(a, c, avg_fold))).lift(one),
        spec(
            "AVERAGEIFS",
            3,
            255,
            imp!(|a, c| lift_tail(a, c, 2, imp!(|a, c| if_multi(a, c, avg_fold)))),
        )
        .lift(criteria_mask(2)),
        spec(
            "MAXIFS",
            3,
            255,
            imp!(|a, c| lift_tail(a, c, 2, imp!(|a, c| if_multi(a, c, max_fold)))),
        )
        .lift(criteria_mask(2)),
        spec(
            "MINIFS",
            3,
            255,
            imp!(|a, c| lift_tail(a, c, 2, imp!(|a, c| if_multi(a, c, min_fold)))),
        )
        .lift(criteria_mask(2)),
        spec("COUNTBLANK", 1, 1, imp!(countblank)),
    ]
}
