// SPDX-License-Identifier: AGPL-3.0-or-later
//! Information functions: the `IS*` tests, `N`, `NA`, `ERROR.TYPE`, `TYPE`, `CELL`, `INFO`,
//! `SHEET`, `SHEETS`, `ISFORMULA` and `FORMULATEXT`.

use super::support::*;
use crate::engine::{Area, Arg, CallContext, EvalValue, Reference};
use crate::refs::{format_cell, quote_sheet, sheet_needs_quotes};
use crate::registry::{ArgMask, FunctionSpec};
use crate::values::{Array2D, ErrorValue, Value, to_number, to_text};

/// The scalar of an `IS*` function (lifted; an empty argument is blank).
fn is_arg(args: &mut [Arg<'_>], ctx: &mut dyn CallContext) -> Value {
    scalar(args, 0, ctx)
}

fn parity(v: &Value, odd: bool, ctx: &dyn CallContext) -> R<bool> {
    if matches!(v, Value::Bool(_)) {
        return Err(ErrorValue::Value);
    }
    let n = to_number(v, &ctx.coercion())?;
    let k = n.trunc().abs() % 2.0;
    Ok(if odd { k == 1.0 } else { k == 0.0 })
}

/// A reference argument: `None` when omitted; values that are not references are `#VALUE!`.
fn ref_arg(args: &mut [Arg<'_>], i: usize, ctx: &mut dyn CallContext) -> R<Option<Reference>> {
    if omitted(args, i) {
        return Ok(None);
    }
    match value_at(args, i, ctx) {
        EvalValue::Value(Value::Error(e)) => Err(e),
        EvalValue::Ref(r) => Ok(Some(r)),
        _ => Err(ErrorValue::Value),
    }
}

fn cell(args: &mut [Arg<'_>], ctx: &mut dyn CallContext) -> R<Value> {
    let info_v = value_at(args, 0, ctx);
    let info_v = ctx.to_scalar(info_v);
    let info = to_text(&info_v)?;
    let r = ref_arg(args, 1, ctx)?;
    let area = match &r {
        Some(r) => *r.as_areas().first().ok_or(ErrorValue::Value)?,
        None => Area::cell(ctx.sheet(), ctx.row(), ctx.col()),
    };
    let (sheet, row, col) = (area.sheet, area.bounds.r0, area.bounds.c0);
    match info.trim().to_lowercase().as_str() {
        "address" => {
            let a = format_cell(row, col, true, true);
            if sheet == ctx.sheet() {
                return Ok(Value::from(a));
            }
            Ok(Value::from(match ctx.sheet_name(sheet) {
                Some(name) if sheet_needs_quotes(&name) => format!("{}!{a}", quote_sheet(&name)),
                Some(name) => format!("{name}!{a}"),
                None => a,
            }))
        }
        "col" => Ok(Value::number(f64::from(col) + 1.0)),
        "row" => Ok(Value::number(f64::from(row) + 1.0)),
        "contents" => Ok(ctx.read_cell(sheet, row, col).blank_to_zero()),
        "type" => Ok(Value::from(match ctx.read_cell(sheet, row, col) {
            Value::Blank => "b",
            Value::Str(_) => "l",
            _ => "v",
        })),
        // A workbook formula never sees a file path (Kiln does not expose one).
        "filename" => Ok(Value::from("")),
        _ => Err(ErrorValue::Value),
    }
}

fn info(args: &mut [Arg<'_>], ctx: &mut dyn CallContext) -> R<Value> {
    let v = value_at(args, 0, ctx);
    let v = ctx.to_scalar(v);
    let t = to_text(&v)?;
    Ok(match t.trim().to_lowercase().as_str() {
        "numfile" => Value::number(ctx.sheet_ids().len() as f64),
        "recalc" => Value::from("Automatic"),
        "release" => Value::from("16.0"),
        "system" => Value::from("pcdos"),
        "osversion" => Value::from("Kiln"),
        "origin" => Value::from("$A:$A$1"),
        "directory" => return Err(ErrorValue::NA),
        _ => return Err(ErrorValue::Value),
    })
}

fn sheet(args: &mut [Arg<'_>], ctx: &mut dyn CallContext) -> R<f64> {
    let ids = ctx.sheet_ids();
    let index_of = |id| {
        ids.iter()
            .position(|x| *x == id)
            .map(|i| i as f64 + 1.0)
            .ok_or(ErrorValue::NA)
    };
    if args.is_empty() || omitted(args, 0) {
        return index_of(ctx.sheet());
    }
    match value_at(args, 0, ctx) {
        EvalValue::Ref(r) => index_of(r.as_areas()[0].sheet),
        EvalValue::Value(Value::Error(e)) => Err(e),
        other => match ctx.to_scalar(other) {
            Value::Str(s) => index_of(ctx.sheet_id(&s).ok_or(ErrorValue::NA)?),
            _ => Err(ErrorValue::NA),
        },
    }
}

fn sheets(args: &mut [Arg<'_>], ctx: &mut dyn CallContext) -> R<f64> {
    if args.is_empty() || omitted(args, 0) {
        return Ok(ctx.sheet_ids().len() as f64);
    }
    match value_at(args, 0, ctx) {
        EvalValue::Value(Value::Error(e)) => Err(e),
        EvalValue::Ref(r) => {
            let mut ids: Vec<_> = r.as_areas().iter().map(|a| a.sheet).collect();
            ids.sort_unstable();
            ids.dedup();
            Ok(ids.len() as f64)
        }
        _ => Err(ErrorValue::Value),
    }
}

fn type_fn(args: &mut [Arg<'_>], ctx: &mut dyn CallContext) -> f64 {
    let n = match value_at(args, 0, ctx) {
        EvalValue::Ref(r) => {
            if !r.is_single_cell() {
                64
            } else {
                let a = r.as_areas()[0];
                ctx.read_cell(a.sheet, a.bounds.r0, a.bounds.c0)
                    .type_number()
            }
        }
        EvalValue::Missing => 1,
        EvalValue::Value(v) => v.type_number(),
    };
    f64::from(n)
}

fn isformula(args: &mut [Arg<'_>], ctx: &mut dyn CallContext) -> R<Value> {
    let r = match value_at(args, 0, ctx) {
        EvalValue::Value(Value::Error(e)) => return Err(e),
        EvalValue::Ref(r) => r,
        _ => return Err(ErrorValue::Value),
    };
    let a = *r.as_areas().first().ok_or(ErrorValue::Value)?;
    if r.is_single_cell() {
        return Ok(Value::Bool(ctx.is_formula_cell(
            a.sheet,
            a.bounds.r0,
            a.bounds.c0,
        )));
    }
    let c = ctx.clamp_area(a);
    let (rows, cols) = c.dims();
    if rows.saturating_mul(cols) > MAX_CELLS {
        return Err(ErrorValue::Num);
    }
    Ok(Value::array(Array2D::build(rows, cols, |r, col| {
        Value::Bool(ctx.is_formula_cell(a.sheet, c.bounds.r0 + r as u32, c.bounds.c0 + col as u32))
    })))
}

fn formulatext(args: &mut [Arg<'_>], ctx: &mut dyn CallContext) -> R<String> {
    match value_at(args, 0, ctx) {
        EvalValue::Value(Value::Error(e)) => Err(e),
        EvalValue::Ref(r) => {
            let a = *r.as_areas().first().ok_or(ErrorValue::NA)?;
            ctx.formula_text(a.sheet, a.bounds.r0, a.bounds.c0)
                .ok_or(ErrorValue::NA)
        }
        _ => Err(ErrorValue::NA),
    }
}

/// The information functions.
pub fn specs() -> Vec<FunctionSpec> {
    let l0 = ArgMask::of(&[0]);
    vec![
        spec("ISBLANK", 1, 1, imp!(|a, c| is_arg(a, c).is_blank())).lift(l0),
        spec(
            "ISERR",
            1,
            1,
            imp!(|a, c| matches!(is_arg(a, c), Value::Error(e) if e != ErrorValue::NA)),
        )
        .lift(l0),
        spec(
            "ISERROR",
            1,
            1,
            imp!(|a, c| matches!(is_arg(a, c), Value::Error(_))),
        )
        .lift(l0),
        spec(
            "ISNA",
            1,
            1,
            imp!(|a, c| matches!(is_arg(a, c), Value::Error(ErrorValue::NA))),
        )
        .lift(l0),
        spec(
            "ISLOGICAL",
            1,
            1,
            imp!(|a, c| matches!(is_arg(a, c), Value::Bool(_))),
        )
        .lift(l0),
        spec(
            "ISNUMBER",
            1,
            1,
            imp!(|a, c| matches!(is_arg(a, c), Value::Number(_))),
        )
        .lift(l0),
        spec(
            "ISTEXT",
            1,
            1,
            imp!(|a, c| matches!(is_arg(a, c), Value::Str(_))),
        )
        .lift(l0),
        spec(
            "ISNONTEXT",
            1,
            1,
            imp!(|a, c| !matches!(is_arg(a, c), Value::Str(_))),
        )
        .lift(l0),
        spec(
            "ISEVEN",
            1,
            1,
            imp!(|a, c| {
                let v = is_arg(a, c);
                parity(&v, false, c)
            }),
        )
        .lift(l0),
        spec(
            "ISODD",
            1,
            1,
            imp!(|a, c| {
                let v = is_arg(a, c);
                parity(&v, true, c)
            }),
        )
        .lift(l0),
        spec(
            "ISREF",
            1,
            1,
            imp!(|a, c| matches!(value_at(a, 0, c), EvalValue::Ref(_))),
        ),
        spec("ISFORMULA", 1, 1, imp!(isformula)),
        spec("FORMULATEXT", 1, 1, imp!(formulatext)),
        spec(
            "N",
            1,
            1,
            imp!(|a, c| match is_arg(a, c) {
                v @ (Value::Number(_) | Value::Error(_)) => v,
                Value::Bool(b) => Value::Number(if b { 1.0 } else { 0.0 }),
                _ => Value::Number(0.0),
            }),
        )
        .lift(l0),
        spec("NA", 0, 0, imp!(|_a, _c| ErrorValue::NA)),
        spec(
            "ERROR.TYPE",
            1,
            1,
            imp!(|a, c| match is_arg(a, c) {
                Value::Error(e) => Ok(f64::from(e.type_number())),
                _ => Err(ErrorValue::NA),
            }),
        )
        .lift(l0),
        spec("TYPE", 1, 1, imp!(type_fn)),
        spec("CELL", 1, 2, imp!(cell)),
        spec("INFO", 1, 1, imp!(info)),
        spec("SHEET", 0, 1, imp!(sheet)),
        spec("SHEETS", 0, 1, imp!(sheets)),
    ]
}
