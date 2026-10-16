// SPDX-License-Identifier: AGPL-3.0-or-later
//! Logical functions: `IF`, `IFS`, `IFERROR`, `IFNA`, `SWITCH`, `AND`, `OR`, `XOR`, `NOT`,
//! `TRUE`, `FALSE`, and the lambda family `LET`, `LAMBDA`, `ISOMITTED`, `MAP`, `REDUCE`, `SCAN`,
//! `BYROW`, `BYCOL`, `MAKEARRAY`.

use std::sync::Arc;

use super::math::core_spec;
use super::support::*;
use crate::engine::{Arg, CallContext, EvalValue, Lambda, Scope, broadcast_dims, broadcast_pick};
use crate::registry::{ArgMask, FunctionSpec};
use crate::values::{Array2D, ErrorValue, Value, lookup_equals, to_bool};

/// Value of an `IF`-style branch: an empty argument or a blank result is 0.
fn branch(args: &mut [Arg<'_>], i: usize, ctx: &mut dyn CallContext) -> EvalValue {
    let Some(a) = args.get_mut(i) else {
        return 0.0.into();
    };
    if a.is_omitted() {
        return 0.0.into();
    }
    match ctx.force(a) {
        EvalValue::Missing | EvalValue::Value(Value::Blank) => 0.0.into(),
        v => v,
    }
}

fn ifs(args: &mut [Arg<'_>], ctx: &mut dyn CallContext) -> EvalValue {
    if !args.len().is_multiple_of(2) {
        return ErrorValue::Value.into();
    }
    let mut i = 0;
    while i < args.len() {
        let v = ctx.force(&mut args[i]);
        let c = ctx.to_value(v);
        let s = match c {
            Value::Array(a) if a.len() > 1 => return ifs_lifted(args, ctx),
            Value::Array(a) => a.get(0, 0).clone(),
            other => other,
        };
        match to_bool(&s) {
            Err(e) => return e.into(),
            Ok(true) => return branch(args, i + 1, ctx),
            Ok(false) => {}
        }
        i += 2;
    }
    ErrorValue::NA.into()
}

fn ifs_lifted(args: &mut [Arg<'_>], ctx: &mut dyn CallContext) -> EvalValue {
    let mut parts = Vec::with_capacity(args.len());
    for a in args.iter_mut() {
        let v = if a.is_omitted() {
            Value::Number(0.0)
        } else {
            let e = ctx.force(a);
            ctx.to_value(e)
        };
        parts.push(v);
    }
    let (rows, cols) = broadcast_dims(parts.iter());
    let out = Array2D::build(rows, cols, |r, c| {
        let mut i = 0;
        while i + 1 < parts.len() {
            match to_bool(&broadcast_pick(&parts[i], r, c)) {
                Err(e) => return Value::Error(e),
                Ok(true) => return broadcast_pick(&parts[i + 1], r, c).blank_to_zero(),
                Ok(false) => {}
            }
            i += 2;
        }
        Value::Error(ErrorValue::NA)
    });
    Value::array(out).into_eval()
}

fn iferror(args: &mut [Arg<'_>], ctx: &mut dyn CallContext, only_na: bool) -> EvalValue {
    let v = scalar(args, 0, ctx);
    match v {
        Value::Error(e) if !only_na || e == ErrorValue::NA => branch(args, 1, ctx),
        other => other.blank_to_zero().into_eval(),
    }
}

fn switch(args: &mut [Arg<'_>], ctx: &mut dyn CallContext) -> EvalValue {
    let x = scalar(args, 0, ctx);
    if let Value::Error(e) = x {
        return e.into();
    }
    let key = x.blank_to_zero();
    let rest = args.len() - 1;
    for i in 0..rest / 2 {
        let v = ctx.force(&mut args[1 + 2 * i]);
        let v = ctx.to_scalar(v);
        if let Value::Error(e) = v {
            return e.into();
        }
        if lookup_equals(&key, &v.blank_to_zero()) {
            return branch(args, 2 + 2 * i, ctx);
        }
    }
    if rest % 2 == 1 {
        return branch(args, args.len() - 1, ctx);
    }
    ErrorValue::NA.into()
}

fn xor(args: &mut [Arg<'_>], ctx: &mut dyn CallContext) -> R<bool> {
    let (mut seen, mut trues) = (false, 0usize);
    for a in args.iter_mut() {
        let v = ctx.force(a);
        let mut err = None;
        ctx.for_each_scalar(&v, &mut |x, from_ref| {
            let b = match x {
                Value::Bool(b) => Some(*b),
                Value::Number(n) => Some(*n != 0.0),
                Value::Error(e) => {
                    err = Some(*e);
                    return false;
                }
                Value::Str(_) if !from_ref => match to_bool(x) {
                    Ok(b) => Some(b),
                    Err(e) => {
                        err = Some(e);
                        return false;
                    }
                },
                _ => None,
            };
            if let Some(b) = b {
                seen = true;
                trues += usize::from(b);
            }
            true
        });
        if let Some(e) = err {
            return Err(e);
        }
    }
    if seen {
        Ok(trues % 2 == 1)
    } else {
        Err(ErrorValue::Value)
    }
}

// ── LET and LAMBDA ─────────────────────────────────────────────────────────────────────────

/// The name of a lazy argument that is a plain name (`x`, not `Sheet1!x`).
fn name_of(a: &Arg<'_>) -> Option<String> {
    match a {
        Arg::Lazy(l) => l.expr().as_local_name().map(str::to_owned),
        Arg::Value(_) => None,
    }
}

fn let_(args: &mut [Arg<'_>], ctx: &mut dyn CallContext) -> EvalValue {
    let n = args.len();
    if n < 3 || n.is_multiple_of(2) {
        return ErrorValue::Value.into();
    }
    let mut scope: Option<Arc<Scope>> = match &args[0] {
        Arg::Lazy(l) => l.scope().cloned(),
        Arg::Value(_) => ctx.scope(),
    };
    for i in (0..n - 1).step_by(2) {
        let Some(name) = name_of(&args[i]) else {
            return ErrorValue::Name.into();
        };
        let Arg::Lazy(value) = &args[i + 1] else {
            return ErrorValue::Value.into();
        };
        let expr = value.expr().clone();
        let v = ctx.evaluate(&expr, scope.clone());
        scope = Some(Scope::bind(scope, &name, v));
    }
    let Arg::Lazy(calc) = &args[n - 1] else {
        return ErrorValue::Value.into();
    };
    let expr = calc.expr().clone();
    ctx.evaluate(&expr, scope)
}

fn lambda(args: &mut [Arg<'_>], ctx: &mut dyn CallContext) -> EvalValue {
    let n = args.len();
    let mut params: Vec<String> = Vec::with_capacity(n.saturating_sub(1));
    for a in &args[..n - 1] {
        let Some(name) = name_of(a) else {
            return ErrorValue::Value.into();
        };
        if params
            .iter()
            .any(|p| p.to_uppercase() == name.to_uppercase())
        {
            return ErrorValue::Value.into();
        }
        params.push(name);
    }
    let Arg::Lazy(body) = &args[n - 1] else {
        return ErrorValue::Value.into();
    };
    let (expr, closure) = (body.expr().clone(), body.scope().cloned());
    EvalValue::Value(ctx.make_lambda(params, expr, closure))
}

/// A lambda result reduced to one array element (nested arrays are `#CALC!`).
fn element(v: EvalValue, ctx: &mut dyn CallContext) -> Value {
    match v {
        EvalValue::Value(Value::Array(a)) => {
            if a.len() == 1 {
                a.get(0, 0).clone()
            } else {
                Value::Error(ErrorValue::Calc)
            }
        }
        EvalValue::Ref(r) if !r.is_single_cell() => match ctx.to_value(EvalValue::Ref(r)) {
            Value::Array(_) => Value::Error(ErrorValue::Calc),
            other => other,
        },
        EvalValue::Value(Value::Lambda(_)) => Value::Error(ErrorValue::Calc),
        other => ctx.to_scalar(other),
    }
}

/// The lambda argument of a helper function, with the parameter count it must have.
fn lambda_arg(
    args: &mut [Arg<'_>],
    i: usize,
    params: usize,
    ctx: &mut dyn CallContext,
) -> R<Arc<Lambda>> {
    match value_at(args, i, ctx) {
        EvalValue::Value(Value::Error(e)) => Err(e),
        EvalValue::Value(Value::Lambda(l)) if l.arity() == params => Ok(l),
        _ => Err(ErrorValue::Value),
    }
}

fn as_array(args: &mut [Arg<'_>], i: usize, ctx: &mut dyn CallContext) -> R<Arc<Array2D>> {
    let v = value_at(args, i, ctx);
    ctx.to_array(v)
}

fn map(args: &mut [Arg<'_>], ctx: &mut dyn CallContext) -> R<Value> {
    let n = args.len();
    let f = lambda_arg(args, n - 1, n - 1, ctx)?;
    let mut arrays = Vec::with_capacity(n - 1);
    for i in 0..n - 1 {
        arrays.push(Value::Array(as_array(args, i, ctx)?));
    }
    let (rows, cols) = broadcast_dims(arrays.iter());
    if rows.saturating_mul(cols) > MAX_CELLS {
        return Err(ErrorValue::Num);
    }
    let mut data = Vec::with_capacity(rows * cols);
    for r in 0..rows {
        for c in 0..cols {
            let call: Vec<EvalValue> = arrays
                .iter()
                .map(|a| EvalValue::Value(broadcast_pick(a, r, c)))
                .collect();
            let out = ctx.invoke(&f, call);
            data.push(element(out, ctx));
        }
    }
    Array2D::new(rows, cols, data)
        .map(Value::array)
        .ok_or(ErrorValue::Calc)
}

fn reduce_or_scan(args: &mut [Arg<'_>], ctx: &mut dyn CallContext, scan: bool) -> R<EvalValue> {
    let has_init = args.len() == 3;
    let n = args.len();
    let f = lambda_arg(args, n - 1, 2, ctx)?;
    let arr = as_array(args, usize::from(has_init), ctx)?;
    let deref = |v: EvalValue, ctx: &mut dyn CallContext| match v {
        r @ EvalValue::Ref(_) => EvalValue::Value(ctx.to_value(r)),
        other => other,
    };
    let mut acc = if has_init {
        let v = value_at(args, 0, ctx);
        deref(v, ctx)
    } else {
        EvalValue::Missing
    };
    let mut out = Vec::with_capacity(if scan { arr.len() } else { 0 });
    for x in arr.values() {
        acc = ctx.invoke(&f, vec![acc, EvalValue::Value(x.clone())]);
        acc = deref(acc, ctx);
        if scan {
            out.push(element(acc.clone(), ctx));
        }
    }
    if scan {
        return Ok(EvalValue::Value(Value::array(
            Array2D::new(arr.rows(), arr.cols(), out).ok_or(ErrorValue::Calc)?,
        )));
    }
    Ok(acc)
}

fn by_row_col(args: &mut [Arg<'_>], ctx: &mut dyn CallContext, by_row: bool) -> R<Value> {
    let arr = as_array(args, 0, ctx)?;
    let f = lambda_arg(args, 1, 1, ctx)?;
    let (lines, len) = if by_row {
        (arr.rows(), arr.cols())
    } else {
        (arr.cols(), arr.rows())
    };
    let mut out = Vec::with_capacity(lines);
    for i in 0..lines {
        let items: Vec<Value> = (0..len)
            .map(|j| {
                if by_row {
                    arr.get(i, j).clone()
                } else {
                    arr.get(j, i).clone()
                }
            })
            .collect();
        let line = if by_row {
            Array2D::row(items)
        } else {
            Array2D::column(items)
        }
        .ok_or(ErrorValue::Calc)?;
        let v = ctx.invoke(&f, vec![EvalValue::Value(Value::array(line))]);
        out.push(element(v, ctx));
    }
    if by_row { column(out) } else { row(out) }
}

fn makearray(args: &mut [Arg<'_>], ctx: &mut dyn CallContext) -> R<Value> {
    let rows = int_of(args, 0, ctx, 0.0)?;
    let cols = int_of(args, 1, ctx, 0.0)?;
    if rows < 1.0 || cols < 1.0 {
        return Err(ErrorValue::Value);
    }
    if rows * cols > MAX_CELLS as f64 {
        return Err(ErrorValue::Num);
    }
    let f = lambda_arg(args, 2, 2, ctx)?;
    let (rows, cols) = (rows as usize, cols as usize);
    let mut data = Vec::with_capacity(rows * cols);
    for r in 0..rows {
        for c in 0..cols {
            let v = ctx.invoke(&f, vec![(r as f64 + 1.0).into(), (c as f64 + 1.0).into()]);
            data.push(element(v, ctx));
        }
    }
    Array2D::new(rows, cols, data)
        .map(Value::array)
        .ok_or(ErrorValue::Calc)
}

/// The logical functions.
pub fn specs() -> Vec<FunctionSpec> {
    let mut v: Vec<FunctionSpec> = ["IF", "AND", "OR", "NOT", "TRUE", "FALSE"]
        .into_iter()
        .filter_map(core_spec)
        .collect();
    v.extend([
        spec("IFS", 2, 254, ifs).lazy(ArgMask::ALL),
        spec("IFERROR", 2, 2, imp!(|a, c| iferror(a, c, false)))
            .lazy(ArgMask::of(&[1]))
            .lift(ArgMask::of(&[0, 1])),
        spec("IFNA", 2, 2, imp!(|a, c| iferror(a, c, true)))
            .lazy(ArgMask::of(&[1]))
            .lift(ArgMask::of(&[0, 1])),
        spec("SWITCH", 3, 254, switch)
            .lazy(AFTER_FIRST)
            .lift(ArgMask::of(&[0])),
        spec("XOR", 1, VARIADIC, imp!(xor)),
        spec("LET", 3, 253, let_).lazy(ArgMask::ALL),
        spec("LAMBDA", 1, 254, lambda).lazy(ArgMask::ALL),
        spec(
            "ISOMITTED",
            1,
            1,
            imp!(|a, c| value_at(a, 0, c) == EvalValue::Missing),
        ),
        spec("MAP", 2, 254, imp!(map)),
        spec("REDUCE", 2, 3, imp!(|a, c| reduce_or_scan(a, c, false))),
        spec("SCAN", 2, 3, imp!(|a, c| reduce_or_scan(a, c, true))),
        spec("BYROW", 2, 2, imp!(|a, c| by_row_col(a, c, true))),
        spec("BYCOL", 2, 2, imp!(|a, c| by_row_col(a, c, false))),
        spec("MAKEARRAY", 3, 3, imp!(makearray)),
    ]);
    v
}
