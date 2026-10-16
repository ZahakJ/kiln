// SPDX-License-Identifier: AGPL-3.0-or-later
//! Lookup & Reference and the dynamic-array functions: `VLOOKUP`, `HLOOKUP`, `LOOKUP`, `MATCH`,
//! `XLOOKUP`, `XMATCH`, `INDEX`, `ROW(S)`, `COLUMN(S)`, `OFFSET`, `INDIRECT`, `ADDRESS`, `AREAS`,
//! `CHOOSE*`, `HYPERLINK`, and `SEQUENCE`, `FILTER`, `SORT`, `SORTBY`, `UNIQUE`, `TRANSPOSE`,
//! `TAKE`, `DROP`, `EXPAND`, `HSTACK`, `VSTACK`, `TOCOL`, `TOROW`, `WRAPROWS`, `WRAPCOLS`.

use std::cmp::Ordering;
use std::sync::Arc;

use rustc_hash::FxHashMap;

use super::matching::{approx_ascending, approx_descending, find_exact, xmatch};
use super::support::*;
use crate::engine::{Area, Arg, CallContext, EvalValue, Reference};
use crate::refs::{Bounds, MAX_COLS, MAX_ROWS, column_to_letters, quote_sheet, sheet_needs_quotes};
use crate::registry::{ArgMask, FunctionSpec};
use crate::values::{
    Array2D, ErrorValue, Value, lookup_equals, round15, sort_compare, to_bool, to_number,
};

/// A lookup key: blanks look for 0.
fn lookup_key(v: Value) -> Value {
    v.blank_to_zero()
}

/// A lifted integer where an empty argument takes the default (the lookup functions' rule).
fn int_d(args: &mut [Arg<'_>], i: usize, ctx: &mut dyn CallContext, dflt: f64) -> R<f64> {
    int_of(args, i, ctx, dflt)
}

/// A one-dimensional list of values from a range or array, its orientation and its declared
/// length (whole-column references read only their used part).
struct Vector {
    values: Vec<Value>,
    vertical: bool,
    len: usize,
}

fn vector(v: EvalValue, ctx: &mut dyn CallContext) -> R<Vector> {
    let declared = match &v {
        EvalValue::Ref(r) => r.single().map(Area::dims),
        _ => None,
    };
    let arr = ctx.to_array(v)?;
    let (rows, cols) = declared.unwrap_or((arr.rows(), arr.cols()));
    if cols == 1 {
        Ok(Vector {
            values: (0..arr.rows()).map(|r| arr.get(r, 0).clone()).collect(),
            vertical: true,
            len: rows,
        })
    } else if rows == 1 {
        Ok(Vector {
            values: (0..arr.cols()).map(|c| arr.get(0, c).clone()).collect(),
            vertical: false,
            len: cols,
        })
    } else {
        Err(ErrorValue::NA)
    }
}

fn sub_area(a: &Area, r0: usize, c0: usize, r1: usize, c1: usize) -> EvalValue {
    EvalValue::Ref(Reference::area(Area {
        sheet: a.sheet,
        bounds: Bounds {
            r0: a.bounds.r0 + r0 as u32,
            c0: a.bounds.c0 + c0 as u32,
            r1: a.bounds.r0 + r1 as u32,
            c1: a.bounds.c0 + c1 as u32,
        },
    }))
}

fn slice_array(arr: &Array2D, r0: usize, c0: usize, r1: usize, c1: usize) -> EvalValue {
    if r0 == r1 && c0 == c1 {
        return EvalValue::Value(arr.get(r0, c0).clone());
    }
    EvalValue::Value(Value::array(Array2D::build(
        r1 - r0 + 1,
        c1 - c0 + 1,
        |r, c| arr.get(r0 + r, c0 + c).clone(),
    )))
}

// ── VLOOKUP / HLOOKUP / LOOKUP / MATCH / XLOOKUP / XMATCH ──────────────────────────────────

fn approx_flag(args: &mut [Arg<'_>], i: usize, ctx: &mut dyn CallContext) -> R<bool> {
    if args.len() <= i {
        return Ok(true);
    }
    match slot(args, i, ctx) {
        Slot::Absent | Slot::Empty => Ok(false),
        Slot::Val(v) => to_bool(&v),
    }
}

fn vlookup(args: &mut [Arg<'_>], ctx: &mut dyn CallContext) -> R<EvalValue> {
    let x = scalar(args, 0, ctx);
    if let Value::Error(e) = x {
        return Err(e);
    }
    let col = int_d(args, 2, ctx, 1.0)?;
    let approx = approx_flag(args, 3, ctx)?;
    if col < 1.0 {
        return Err(ErrorValue::Value);
    }
    let key = lookup_key(x);
    match value_at(args, 1, ctx) {
        EvalValue::Ref(r) => {
            let a0 = *r.single().ok_or(ErrorValue::Ref)?;
            let a = ctx.clamp_area(a0);
            let width = a0.dims().1 as f64;
            if col > width {
                return Err(ErrorValue::Ref);
            }
            let first: Vec<Value> = (a.bounds.r0..=a.bounds.r1)
                .map(|row| ctx.read_cell(a.sheet, row, a.bounds.c0))
                .collect();
            let i = if approx {
                approx_ascending(&first, &key)
            } else {
                find_exact(&first, &key, true, false)
            }
            .ok_or(ErrorValue::NA)?;
            Ok(EvalValue::Value(ctx.read_cell(
                a.sheet,
                a.bounds.r0 + i as u32,
                a.bounds.c0 + col as u32 - 1,
            )))
        }
        other => {
            let arr = ctx.to_array(other)?;
            if col > arr.cols() as f64 {
                return Err(ErrorValue::Ref);
            }
            let first: Vec<Value> = (0..arr.rows()).map(|r| arr.get(r, 0).clone()).collect();
            let i = if approx {
                approx_ascending(&first, &key)
            } else {
                find_exact(&first, &key, true, false)
            }
            .ok_or(ErrorValue::NA)?;
            Ok(EvalValue::Value(arr.get(i, col as usize - 1).clone()))
        }
    }
}

fn hlookup(args: &mut [Arg<'_>], ctx: &mut dyn CallContext) -> R<Value> {
    let x = scalar(args, 0, ctx);
    if let Value::Error(e) = x {
        return Err(e);
    }
    let row_n = int(args, 2, ctx, 0.0)?;
    // Unlike VLOOKUP, an empty range_lookup keeps the approximate match.
    let approx = match slot(args, 3, ctx) {
        Slot::Absent | Slot::Empty => true,
        Slot::Val(v) => to_bool(&v)?,
    };
    if row_n < 1.0 {
        return Err(ErrorValue::Value);
    }
    let table = array_of(args, 1, ctx)?;
    if row_n > table.rows() as f64 {
        return Err(ErrorValue::Ref);
    }
    let first: Vec<Value> = (0..table.cols()).map(|c| table.get(0, c).clone()).collect();
    let key = lookup_key(x);
    let i = if approx {
        approx_ascending(&first, &key)
    } else {
        find_exact(&first, &key, true, false)
    }
    .ok_or(ErrorValue::NA)?;
    Ok(table.get(row_n as usize - 1, i).clone())
}

fn first_line(a: &Array2D, by_row: bool, last: bool) -> Vec<Value> {
    if by_row {
        let r = if last { a.rows() - 1 } else { 0 };
        (0..a.cols()).map(|c| a.get(r, c).clone()).collect()
    } else {
        let c = if last { a.cols() - 1 } else { 0 };
        (0..a.rows()).map(|r| a.get(r, c).clone()).collect()
    }
}

fn lookup(args: &mut [Arg<'_>], ctx: &mut dyn CallContext) -> R<Value> {
    let x = scalar(args, 0, ctx);
    if let Value::Error(e) = x {
        return Err(e);
    }
    let vec = array_of(args, 1, ctx)?;
    let key = lookup_key(x);
    let (keys, results) = if args.len() > 2 {
        let res = array_of(args, 2, ctx)?;
        (
            first_line(&vec, vec.rows() == 1, false),
            first_line(&res, res.rows() == 1, false),
        )
    } else if vec.cols() > vec.rows() {
        (first_line(&vec, true, false), first_line(&vec, true, true))
    } else {
        (
            first_line(&vec, false, false),
            first_line(&vec, false, true),
        )
    };
    let i = approx_ascending(&keys, &key).ok_or(ErrorValue::NA)?;
    results.get(i).cloned().ok_or(ErrorValue::NA)
}

fn match_fn(args: &mut [Arg<'_>], ctx: &mut dyn CallContext) -> R<f64> {
    let x = scalar(args, 0, ctx);
    if let Value::Error(e) = x {
        return Err(e);
    }
    let dflt = if args.len() > 2 { 0.0 } else { 1.0 };
    let kind = int_d(args, 2, ctx, dflt)?;
    let v = value_at(args, 1, ctx);
    let vec = vector(v, ctx)?;
    let key = lookup_key(x);
    let i = if kind == 0.0 {
        find_exact(&vec.values, &key, true, false)
    } else if kind > 0.0 {
        approx_ascending(&vec.values, &key)
    } else {
        approx_descending(&vec.values, &key)
    };
    i.map(|i| i as f64 + 1.0).ok_or(ErrorValue::NA)
}

fn modes(args: &mut [Arg<'_>], m: usize, s: usize, ctx: &mut dyn CallContext) -> R<(i32, i32)> {
    let mm = int_d(args, m, ctx, 0.0)?;
    let sm = int_d(args, s, ctx, 1.0)?;
    if ![0.0, -1.0, 1.0, 2.0].contains(&mm) || ![1.0, -1.0, 2.0, -2.0].contains(&sm) {
        return Err(ErrorValue::Value);
    }
    Ok((mm as i32, sm as i32))
}

fn xlookup(args: &mut [Arg<'_>], ctx: &mut dyn CallContext) -> R<EvalValue> {
    let x = scalar(args, 0, ctx);
    if let Value::Error(e) = x {
        return Err(e);
    }
    let (mm, sm) = modes(args, 4, 5, ctx)?;
    let v = value_at(args, 1, ctx);
    let vec = vector(v, ctx).map_err(|_| ErrorValue::Value)?;
    let ret = value_at(args, 2, ctx);
    let (ret_area, ret_arr) = match ret {
        EvalValue::Ref(r) => (Some(*r.single().ok_or(ErrorValue::Value)?), None),
        other => (None, Some(ctx.to_array(other)?)),
    };
    let (ret_rows, ret_cols) = match (&ret_area, &ret_arr) {
        (Some(a), _) => a.dims(),
        (None, Some(a)) => (a.rows(), a.cols()),
        (None, None) => return Err(ErrorValue::Value),
    };
    if (vec.vertical && ret_rows != vec.len) || (!vec.vertical && ret_cols != vec.len) {
        return Err(ErrorValue::Value);
    }
    let Some(i) = xmatch(&vec.values, &lookup_key(x), mm, sm) else {
        if args.len() > 3 && !omitted(args, 3) {
            return Ok(value_at(args, 3, ctx));
        }
        return Err(ErrorValue::NA);
    };
    Ok(match (ret_area, ret_arr) {
        (Some(a), _) if vec.vertical => sub_area(&a, i, 0, i, ret_cols - 1),
        (Some(a), _) => sub_area(&a, 0, i, ret_rows - 1, i),
        (None, Some(arr)) if vec.vertical => slice_array(&arr, i, 0, i, ret_cols - 1),
        (None, Some(arr)) => slice_array(&arr, 0, i, ret_rows - 1, i),
        (None, None) => return Err(ErrorValue::Value),
    })
}

fn xmatch_fn(args: &mut [Arg<'_>], ctx: &mut dyn CallContext) -> R<f64> {
    let x = scalar(args, 0, ctx);
    if let Value::Error(e) = x {
        return Err(e);
    }
    let (mm, sm) = modes(args, 2, 3, ctx)?;
    let arr = array_of(args, 1, ctx)?;
    if arr.rows() > 1 && arr.cols() > 1 {
        return Err(ErrorValue::Value);
    }
    xmatch(arr.values(), &lookup_key(x), mm, sm)
        .map(|i| i as f64 + 1.0)
        .ok_or(ErrorValue::NA)
}

// ── INDEX / ROW / COLUMN / OFFSET / INDIRECT ───────────────────────────────────────────────

fn to_int(v: &Value, ctx: &dyn CallContext) -> R<f64> {
    to_number(v, &ctx.coercion()).map(f64::trunc)
}

fn index_one(
    src: &EvalValue,
    row_v: &Value,
    col_v: Option<&Value>,
    area_n: Option<f64>,
    ctx: &mut dyn CallContext,
) -> R<EvalValue> {
    let mut r = to_int(row_v, ctx)?;
    let mut c = match col_v {
        Some(v) => Some(to_int(v, ctx)?),
        None => None,
    };
    let (area, arr) = match src {
        EvalValue::Ref(reference) => {
            let k = area_n.unwrap_or(1.0);
            let areas = reference.as_areas();
            if k < 1.0 || k > areas.len() as f64 {
                return Err(ErrorValue::Ref);
            }
            (Some(areas[k as usize - 1]), None)
        }
        other => (None, Some(ctx.to_array(other.clone())?)),
    };
    let (rows, cols) = match (&area, &arr) {
        (Some(a), _) => a.dims(),
        (None, Some(a)) => (a.rows(), a.cols()),
        (None, None) => return Err(ErrorValue::Value),
    };
    let c = match c.take() {
        Some(c) => c,
        None => {
            if rows == 1 && cols > 1 {
                let col = r;
                r = 1.0;
                col
            } else if cols == 1 {
                1.0
            } else {
                0.0
            }
        }
    };
    if r < 0.0 || c < 0.0 {
        return Err(ErrorValue::Value);
    }
    if r > rows as f64 || c > cols as f64 {
        return Err(ErrorValue::Ref);
    }
    let (r0, r1) = if r == 0.0 {
        (0, rows - 1)
    } else {
        (r as usize - 1, r as usize - 1)
    };
    let (c0, c1) = if c == 0.0 {
        (0, cols - 1)
    } else {
        (c as usize - 1, c as usize - 1)
    };
    Ok(match (area, arr) {
        (Some(a), _) => sub_area(&a, r0, c0, r1, c1),
        (None, Some(arr)) => slice_array(&arr, r0, c0, r1, c1),
        (None, None) => return Err(ErrorValue::Value),
    })
}

fn index(args: &mut [Arg<'_>], ctx: &mut dyn CallContext) -> R<EvalValue> {
    let src = value_at(args, 0, ctx);
    if let Some(e) = src.as_error() {
        return Err(e);
    }
    let rv = value_at(args, 1, ctx);
    let row_arg = ctx.to_value(rv);
    let col_arg = if args.len() > 2 {
        let cv = value_at(args, 2, ctx);
        Some(ctx.to_value(cv))
    } else {
        None
    };
    let area_n = if args.len() > 3 && !omitted(args, 3) && matches!(src, EvalValue::Ref(_)) {
        Some(int_of(args, 3, ctx, 1.0)?)
    } else {
        None
    };
    let is_arr = |v: &Value| matches!(v, Value::Array(_));
    if is_arr(&row_arg) || col_arg.as_ref().is_some_and(is_arr) {
        let parts = [
            row_arg.clone(),
            col_arg.clone().unwrap_or(Value::Number(0.0)),
        ];
        let (rows, cols) = crate::engine::broadcast_dims(parts.iter());
        let pick = |p: &Value, r: usize, c: usize| match p {
            Value::Array(a) => a.get(r.min(a.rows() - 1), c.min(a.cols() - 1)).clone(),
            other => other.clone(),
        };
        let mut data = Vec::with_capacity(rows * cols);
        for r in 0..rows {
            for c in 0..cols {
                let rv = pick(&row_arg, r, c);
                let cv = col_arg.as_ref().map(|v| pick(v, r, c));
                let one = match index_one(&src, &rv, cv.as_ref(), area_n, ctx) {
                    Ok(v) => ctx.to_scalar(v),
                    Err(e) => Value::Error(e),
                };
                data.push(one);
            }
        }
        return Ok(EvalValue::Value(Value::array(
            Array2D::new(rows, cols, data).ok_or(ErrorValue::Calc)?,
        )));
    }
    index_one(&src, &row_arg, col_arg.as_ref(), area_n, ctx)
}

fn row_or_column(args: &mut [Arg<'_>], ctx: &mut dyn CallContext, is_row: bool) -> R<Value> {
    if args.is_empty() || omitted(args, 0) {
        return Ok(Value::number(
            f64::from(if is_row { ctx.row() } else { ctx.col() }) + 1.0,
        ));
    }
    let r = match value_at(args, 0, ctx) {
        EvalValue::Value(Value::Error(e)) => return Err(e),
        EvalValue::Ref(r) => r,
        _ => return Err(ErrorValue::Value),
    };
    let a = ctx.clamp_area(*r.single().ok_or(ErrorValue::Value)?);
    let (lo, hi) = if is_row {
        (a.bounds.r0, a.bounds.r1)
    } else {
        (a.bounds.c0, a.bounds.c1)
    };
    if lo == hi || ctx.legacy() {
        return Ok(Value::number(f64::from(lo) + 1.0));
    }
    let values = nums((lo..=hi).map(|i| f64::from(i) + 1.0));
    if is_row { column(values) } else { row(values) }
}

fn offset(args: &mut [Arg<'_>], ctx: &mut dyn CallContext) -> R<EvalValue> {
    let r = match value_at(args, 0, ctx) {
        EvalValue::Value(Value::Error(e)) => return Err(e),
        EvalValue::Ref(r) => r,
        _ => return Err(ErrorValue::Value),
    };
    let a = *r.single().ok_or(ErrorValue::Value)?;
    let dr = int_of(args, 1, ctx, 0.0)?;
    let dc = int_of(args, 2, ctx, 0.0)?;
    let (h0, w0) = a.dims();
    let h = if omitted(args, 3) {
        h0 as f64
    } else {
        int_of(args, 3, ctx, 0.0)?
    };
    let w = if omitted(args, 4) {
        w0 as f64
    } else {
        int_of(args, 4, ctx, 0.0)?
    };
    if h == 0.0 || w == 0.0 {
        return Err(ErrorValue::Ref);
    }
    let mut r0 = f64::from(a.bounds.r0) + dr;
    let mut c0 = f64::from(a.bounds.c0) + dc;
    let mut r1 = r0 + h - 1.0;
    let mut c1 = c0 + w - 1.0;
    if h < 0.0 {
        (r0, r1) = (r0 + h + 1.0, r0);
    }
    if w < 0.0 {
        (c0, c1) = (c0 + w + 1.0, c0);
    }
    if r0 < 0.0 || c0 < 0.0 || r1 >= f64::from(MAX_ROWS) || c1 >= f64::from(MAX_COLS) {
        return Err(ErrorValue::Ref);
    }
    Ok(EvalValue::Ref(Reference::area(Area {
        sheet: a.sheet,
        bounds: Bounds {
            r0: r0 as u32,
            c0: c0 as u32,
            r1: r1 as u32,
            c1: c1 as u32,
        },
    })))
}

fn indirect(args: &mut [Arg<'_>], ctx: &mut dyn CallContext) -> R<EvalValue> {
    let tv = value_at(args, 0, ctx);
    let tv = ctx.to_scalar(tv);
    let t = crate::values::to_text(&tv)?;
    let a1 = if args.len() > 1 && !omitted(args, 1) {
        let v = value_at(args, 1, ctx);
        let v = ctx.to_scalar(v);
        to_bool(&v)?
    } else {
        true
    };
    ctx.resolve_reference_text(&t, a1).map(EvalValue::Ref)
}

fn address(args: &mut [Arg<'_>], ctx: &mut dyn CallContext) -> R<String> {
    let row_n = int(args, 0, ctx, 0.0)?;
    let col_n = int(args, 1, ctx, 0.0)?;
    let abs = num_or(args, 2, ctx, 1.0, 1.0)?.trunc();
    let a1 = flag(args, 3, ctx, true)?;
    if row_n < 1.0
        || col_n < 1.0
        || row_n > f64::from(MAX_ROWS)
        || col_n > f64::from(MAX_COLS)
        || !(1.0..=4.0).contains(&abs)
    {
        return Err(ErrorValue::Value);
    }
    let row_abs = abs == 1.0 || abs == 2.0;
    let col_abs = abs == 1.0 || abs == 3.0;
    let (r, c) = (row_n as u32, col_n as u32);
    let mut out = if a1 {
        format!(
            "{}{}{}{r}",
            if col_abs { "$" } else { "" },
            column_to_letters(c - 1),
            if row_abs { "$" } else { "" }
        )
    } else {
        format!(
            "{}{}",
            if row_abs {
                format!("R{r}")
            } else {
                format!("R[{r}]")
            },
            if col_abs {
                format!("C{c}")
            } else {
                format!("C[{c}]")
            }
        )
    };
    if let Slot::Val(_) = slot(args, 4, ctx) {
        let sheet = text(args, 4, ctx)?;
        if !sheet.is_empty() {
            let external = sheet.starts_with('[') && sheet[1..].contains(']');
            let prefix = if external || !sheet_needs_quotes(&sheet) {
                sheet.to_string()
            } else {
                quote_sheet(&sheet)
            };
            out = format!("{prefix}!{out}");
        }
    }
    Ok(out)
}

fn areas(args: &mut [Arg<'_>], ctx: &mut dyn CallContext) -> R<f64> {
    match value_at(args, 0, ctx) {
        EvalValue::Value(Value::Error(e)) => Err(e),
        EvalValue::Ref(r) => Ok(r.as_areas().len() as f64),
        _ => Err(ErrorValue::Value),
    }
}

fn choose(args: &mut [Arg<'_>], ctx: &mut dyn CallContext) -> R<EvalValue> {
    let idx = scalar(args, 0, ctx);
    let i = to_number(&idx, &ctx.coercion())?.trunc();
    if i < 1.0 || i >= args.len() as f64 {
        return Err(ErrorValue::Value);
    }
    let i = i as usize;
    if omitted(args, i) {
        return Ok(0.0.into());
    }
    Ok(value_at(args, i, ctx))
}

fn hyperlink(args: &mut [Arg<'_>], ctx: &mut dyn CallContext) -> R<Value> {
    let link = text(args, 0, ctx)?;
    ctx.set_hyperlink(link.to_string());
    match slot(args, 1, ctx) {
        Slot::Absent | Slot::Empty => Ok(Value::Str(link)),
        Slot::Val(v) => Ok(v.blank_to_zero()),
    }
}

fn dims(args: &mut [Arg<'_>], ctx: &mut dyn CallContext) -> R<(usize, usize)> {
    match value_at(args, 0, ctx) {
        EvalValue::Ref(r) => r.single().map(Area::dims).ok_or(ErrorValue::Ref),
        EvalValue::Value(Value::Error(e)) => Err(e),
        EvalValue::Value(Value::Lambda(_)) => Err(ErrorValue::Value),
        other => {
            let a = ctx.to_array(other)?;
            Ok((a.rows(), a.cols()))
        }
    }
}

// ── dynamic arrays ─────────────────────────────────────────────────────────────────────────

fn sequence(args: &mut [Arg<'_>], ctx: &mut dyn CallContext) -> R<Value> {
    let rows = int_of(args, 0, ctx, 0.0)?;
    let cols = int_of(args, 1, ctx, 1.0)?;
    let start = number_of(args, 2, ctx, 1.0)?;
    let step = number_of(args, 3, ctx, 1.0)?;
    if rows < 0.0 || cols < 0.0 {
        return Err(ErrorValue::Value);
    }
    if rows == 0.0 || cols == 0.0 {
        return Err(ErrorValue::Calc);
    }
    if rows * cols > MAX_CELLS as f64 {
        return Err(ErrorValue::Num);
    }
    let (r, c) = (rows as usize, cols as usize);
    Ok(Value::array(Array2D::build(r, c, |i, j| {
        Value::number(start + (i * c + j) as f64 * step)
    })))
}

fn filter(args: &mut [Arg<'_>], ctx: &mut dyn CallContext) -> R<EvalValue> {
    let v = value_at(args, 0, ctx);
    let arr = ctx.to_array(v)?;
    let iv = value_at(args, 1, ctx);
    let inc = ctx.to_array(iv)?;
    let by_rows = if inc.cols() == 1 && inc.rows() == arr.rows() {
        true
    } else if inc.rows() == 1 && inc.cols() == arr.cols() {
        false
    } else {
        return Err(ErrorValue::Value);
    };
    let mut keep = Vec::with_capacity(inc.len());
    for v in inc.values() {
        keep.push(match v {
            Value::Error(e) => return Err(*e),
            Value::Blank => false,
            other => to_bool(other)?,
        });
    }
    if !keep.iter().any(|k| *k) {
        if args.len() > 2 && !omitted(args, 2) {
            return Ok(value_at(args, 2, ctx));
        }
        return Err(ErrorValue::Calc);
    }
    let rows: Vec<Vec<Value>> = if by_rows {
        rows_of(&arr)
            .into_iter()
            .zip(&keep)
            .filter(|(_, k)| **k)
            .map(|(r, _)| r)
            .collect()
    } else {
        rows_of(&arr)
            .into_iter()
            .map(|r| {
                r.into_iter()
                    .zip(&keep)
                    .filter(|(_, k)| **k)
                    .map(|(v, _)| v)
                    .collect()
            })
            .collect()
    };
    from_rows(rows).map(EvalValue::Value)
}

/// Numbers from a scalar-or-array argument (`SORT`'s sort_index and sort_order).
fn number_list(args: &mut [Arg<'_>], i: usize, ctx: &mut dyn CallContext) -> R<Vec<f64>> {
    if omitted(args, i) {
        return Ok(vec![1.0]);
    }
    let v = value_at(args, i, ctx);
    let items = match ctx.to_value(v) {
        Value::Lambda(_) => return Err(ErrorValue::Value),
        Value::Array(a) => a.values().to_vec(),
        other => vec![other],
    };
    items.iter().map(|x| to_int(x, ctx)).collect()
}

/// `SORT`'s ordering of two keys: blanks last in both directions.
fn sort_keys(x: &Value, y: &Value, dir: f64) -> Ordering {
    match (x.is_blank(), y.is_blank()) {
        (true, true) => Ordering::Equal,
        (true, false) => Ordering::Greater,
        (false, true) => Ordering::Less,
        _ => {
            let c = sort_compare(x, y);
            if dir < 0.0 { c.reverse() } else { c }
        }
    }
}

fn sort(args: &mut [Arg<'_>], ctx: &mut dyn CallContext) -> R<Value> {
    let v = value_at(args, 0, ctx);
    let arr = ctx.to_array(v)?;
    let by_col = flag_of(args, 3, ctx, false)?;
    let keys = number_list(args, 1, ctx)?;
    let orders = number_list(args, 2, ctx)?;
    let src = if by_col {
        Arc::new(arr.transpose())
    } else {
        arr
    };
    if keys.iter().any(|k| *k < 1.0 || *k > src.cols() as f64) {
        return Err(ErrorValue::Value);
    }
    if orders.iter().any(|o| *o != 1.0 && *o != -1.0) {
        return Err(ErrorValue::Value);
    }
    let mut rows = rows_of(&src);
    rows.sort_by(|a, b| {
        for (i, k) in keys.iter().enumerate() {
            let k = *k as usize - 1;
            let dir = orders[i.min(orders.len() - 1)];
            let c = sort_keys(&a[k], &b[k], dir);
            if c != Ordering::Equal {
                return c;
            }
        }
        Ordering::Equal
    });
    let out = Array2D::from_rows(rows).ok_or(ErrorValue::Calc)?;
    Ok(Value::array(if by_col { out.transpose() } else { out }))
}

fn sortby(args: &mut [Arg<'_>], ctx: &mut dyn CallContext) -> R<Value> {
    let arr = array_of(args, 0, ctx)?;
    let mut keys: Vec<(Vec<Value>, f64)> = Vec::new();
    let mut by_col: Option<bool> = None;
    let mut i = 1;
    while i < args.len() {
        let by = array_of(args, i, ctx)?;
        let col = if by.cols() == 1 && by.rows() == arr.rows() {
            false
        } else if by.rows() == 1 && by.cols() == arr.cols() {
            true
        } else {
            return Err(ErrorValue::Value);
        };
        match by_col {
            None => by_col = Some(col),
            Some(b) if b != col => return Err(ErrorValue::Value),
            Some(_) => {}
        }
        let o = if omitted(args, i + 1) {
            1.0
        } else {
            let v = value_at(args, i + 1, ctx);
            let v = ctx.to_scalar(v);
            to_number(&v, &ctx.coercion())?
        };
        if o != 1.0 && o != -1.0 {
            return Err(ErrorValue::Value);
        }
        keys.push((by.values().to_vec(), o));
        i += 2;
    }
    let by_col = by_col.unwrap_or(false);
    let n = if by_col { arr.cols() } else { arr.rows() };
    let mut order: Vec<usize> = (0..n).collect();
    order.sort_by(|&a, &b| {
        for (values, dir) in &keys {
            let c = sort_keys(&values[a], &values[b], *dir);
            if c != Ordering::Equal {
                return c;
            }
        }
        a.cmp(&b)
    });
    Ok(Value::array(if by_col {
        Array2D::build(arr.rows(), arr.cols(), |r, c| arr.get(r, order[c]).clone())
    } else {
        Array2D::build(arr.rows(), arr.cols(), |r, c| arr.get(order[r], c).clone())
    }))
}

/// A hash key consistent with [`lookup_equals`] for grouping rows.
/// [`round15`] at a fraction of its cost: the correctly rounded 17-digit expansion decides the
/// 15-digit rounding unless its last two digits are exactly `50`, the only case in which the
/// true value could lie on either side of the rounding midpoint; that case, integers and
/// non-finite values go to [`round15`] itself, so the result is always identical to it.
pub(crate) fn round15_fast(x: f64) -> f64 {
    if x == 0.0 || !x.is_finite() || (x.fract() == 0.0 && x.abs() < 1e15) {
        return round15(x);
    }
    let s = format!("{:.16e}", x.abs());
    let Some((mant, exp)) = s.split_once('e') else {
        return round15(x);
    };
    let Ok(mut exp) = exp.parse::<i32>() else {
        return round15(x);
    };
    let mut d: Vec<u8> = mant
        .bytes()
        .filter(u8::is_ascii_digit)
        .map(|b| b - b'0')
        .collect();
    if d.len() != 17 || (d[15] == 5 && d[16] == 0) {
        return round15(x);
    }
    let up = d[15] >= 5;
    d.truncate(15);
    if up {
        let mut i = 15;
        loop {
            if i == 0 {
                d.insert(0, 1);
                d.pop();
                exp += 1;
                break;
            }
            i -= 1;
            if d[i] == 9 {
                d[i] = 0;
            } else {
                d[i] += 1;
                break;
            }
        }
    }
    let mut t = String::with_capacity(24);
    if x < 0.0 {
        t.push('-');
    }
    for (i, digit) in d.iter().enumerate() {
        if i == 1 {
            t.push('.');
        }
        t.push(char::from(b'0' + digit));
    }
    t.push('e');
    t.push_str(&exp.to_string());
    t.parse().unwrap_or_else(|_| round15(x))
}

/// A hash of row `r` consistent with [`lookup_equals`]: numbers at 15 significant digits, text
/// ignoring case.
fn unique_hash(a: &Array2D, r: usize) -> u64 {
    use std::hash::{Hash, Hasher};
    let mut h = rustc_hash::FxHasher::default();
    for c in 0..a.cols() {
        match a.get(r, c) {
            Value::Str(s) => {
                1u8.hash(&mut h);
                s.to_lowercase().hash(&mut h);
            }
            Value::Number(n) => {
                2u8.hash(&mut h);
                round15_fast(*n).to_bits().hash(&mut h);
            }
            Value::Bool(b) => {
                3u8.hash(&mut h);
                b.hash(&mut h);
            }
            Value::Error(e) => {
                4u8.hash(&mut h);
                e.code().hash(&mut h);
            }
            _ => 5u8.hash(&mut h),
        }
    }
    h.finish()
}

fn unique(args: &mut [Arg<'_>], ctx: &mut dyn CallContext) -> R<Value> {
    const END: usize = usize::MAX;
    let v = value_at(args, 0, ctx);
    let arr = ctx.to_array(v)?;
    let by_col = flag_of(args, 1, ctx, false)?;
    let once = flag_of(args, 2, ctx, false)?;
    let src = if by_col {
        Arc::new(arr.transpose())
    } else {
        arr
    };
    // Groups of equal rows, each held as its first row's index and a count; groups whose rows
    // share a hash are chained through `next`, so nothing is cloned or allocated per row.
    let (rows, cols) = (src.rows(), src.cols());
    let mut heads: FxHashMap<u64, usize> = FxHashMap::default();
    heads.reserve(rows.min(1 << 20));
    let (mut first, mut count, mut next) = (Vec::new(), Vec::new(), Vec::new());
    for r in 0..rows {
        let h = unique_hash(&src, r);
        let mut g = heads.get(&h).copied().unwrap_or(END);
        while g != END && !(0..cols).all(|c| lookup_equals(src.get(first[g], c), src.get(r, c))) {
            g = next[g];
        }
        if g == END {
            next.push(heads.insert(h, first.len()).unwrap_or(END));
            first.push(r);
            count.push(1usize);
        } else {
            count[g] += 1;
        }
    }
    let out: Vec<Vec<Value>> = first
        .iter()
        .zip(&count)
        .filter(|&(_, &n)| !once || n == 1)
        .map(|(&r, _)| (0..cols).map(|c| src.get(r, c).clone()).collect())
        .collect();
    if out.is_empty() {
        return Err(ErrorValue::Calc);
    }
    let out = Array2D::from_rows(out).ok_or(ErrorValue::Calc)?;
    Ok(Value::array(if by_col { out.transpose() } else { out }))
}

fn transpose(args: &mut [Arg<'_>], ctx: &mut dyn CallContext) -> Value {
    let v = value_at(args, 0, ctx);
    match ctx.to_value(v) {
        Value::Array(a) => Value::array(a.transpose()),
        other => other,
    }
}

fn pick_indices(n: usize, args: &mut [Arg<'_>], ctx: &mut dyn CallContext) -> R<Vec<usize>> {
    let mut out = Vec::new();
    for i in 0..args.len() {
        let arr = array_of(args, i, ctx)?;
        for v in arr.values() {
            let k = to_int(v, ctx)?;
            if k == 0.0 || k.abs() > n as f64 {
                return Err(ErrorValue::Value);
            }
            out.push(if k > 0.0 {
                k as usize - 1
            } else {
                (n as f64 + k) as usize
            });
        }
    }
    Ok(out)
}

fn choose_rows_cols(args: &mut [Arg<'_>], ctx: &mut dyn CallContext, cols: bool) -> R<Value> {
    let arr = array_of(args, 0, ctx)?;
    let src = if cols {
        arr.transpose()
    } else {
        (*arr).clone()
    };
    let idx = pick_indices(src.rows(), &mut args[1..], ctx)?;
    let rows: Vec<Vec<Value>> = idx
        .into_iter()
        .map(|i| (0..src.cols()).map(|c| src.get(i, c).clone()).collect())
        .collect();
    let out = Array2D::from_rows(rows).ok_or(ErrorValue::Calc)?;
    Ok(Value::array(if cols { out.transpose() } else { out }))
}

fn count_arg(args: &mut [Arg<'_>], i: usize, ctx: &mut dyn CallContext) -> R<Option<f64>> {
    if omitted(args, i) {
        return Ok(None);
    }
    let v = value_at(args, i, ctx);
    let v = ctx.to_scalar(v);
    to_int(&v, ctx).map(Some)
}

fn take(args: &mut [Arg<'_>], ctx: &mut dyn CallContext, drop: bool) -> R<Value> {
    let arr = array_of(args, 0, ctx)?;
    let r = count_arg(args, 1, ctx)?;
    let c = count_arg(args, 2, ctx)?;
    let span = |n: Option<f64>, size: usize| -> (usize, usize) {
        let s = size as f64;
        let (a, b) = match n {
            None => (0.0, s),
            Some(n) if drop => {
                if n >= 0.0 {
                    (n.min(s), s)
                } else {
                    (0.0, (s + n).max(0.0))
                }
            }
            Some(n) => {
                if n >= 0.0 {
                    (0.0, n.min(s))
                } else {
                    ((s + n).max(0.0), s)
                }
            }
        };
        (a as usize, b as usize)
    };
    if !drop && (r == Some(0.0) || c == Some(0.0)) {
        return Err(ErrorValue::Calc);
    }
    let (r0, r1) = span(r, arr.rows());
    let (c0, c1) = span(c, arr.cols());
    if r1 <= r0 || c1 <= c0 {
        return Err(ErrorValue::Calc);
    }
    Ok(Value::array(Array2D::build(r1 - r0, c1 - c0, |i, j| {
        arr.get(r0 + i, c0 + j).clone()
    })))
}

fn pad_arg(args: &mut [Arg<'_>], i: usize, ctx: &mut dyn CallContext) -> Value {
    if omitted(args, i) {
        Value::Error(ErrorValue::NA)
    } else {
        let v = value_at(args, i, ctx);
        ctx.to_scalar(v)
    }
}

fn expand(args: &mut [Arg<'_>], ctx: &mut dyn CallContext) -> R<Value> {
    let arr = array_of(args, 0, ctx)?;
    let r = count_arg(args, 1, ctx)?;
    let c = count_arg(args, 2, ctx)?;
    let rows = r.unwrap_or(arr.rows() as f64);
    let cols = c.unwrap_or(arr.cols() as f64);
    if rows < arr.rows() as f64 || cols < arr.cols() as f64 {
        return Err(ErrorValue::Value);
    }
    if rows * cols > MAX_CELLS as f64 {
        return Err(ErrorValue::Num);
    }
    let pad = pad_arg(args, 3, ctx);
    Ok(Value::array(Array2D::build(
        rows as usize,
        cols as usize,
        |i, j| {
            if i < arr.rows() && j < arr.cols() {
                arr.get(i, j).clone()
            } else {
                pad.clone()
            }
        },
    )))
}

fn stack(args: &mut [Arg<'_>], ctx: &mut dyn CallContext, horizontal: bool) -> R<Value> {
    let mut parts = Vec::with_capacity(args.len());
    for i in 0..args.len() {
        parts.push(array_of(args, i, ctx)?);
    }
    let na = || Value::Error(ErrorValue::NA);
    if horizontal {
        let rows = parts.iter().map(|p| p.rows()).max().unwrap_or(0);
        let cols: usize = parts.iter().map(|p| p.cols()).sum();
        if cols == 0 || rows == 0 {
            return Err(ErrorValue::Calc);
        }
        if rows.saturating_mul(cols) > MAX_CELLS {
            return Err(ErrorValue::Num);
        }
        let mut data = vec![Vec::with_capacity(cols); rows];
        for p in &parts {
            for (i, line) in data.iter_mut().enumerate() {
                for j in 0..p.cols() {
                    line.push(if i < p.rows() {
                        p.get(i, j).clone()
                    } else {
                        na()
                    });
                }
            }
        }
        return from_rows(data);
    }
    let cols = parts.iter().map(|p| p.cols()).max().unwrap_or(0);
    let rows: usize = parts.iter().map(|p| p.rows()).sum();
    if rows.saturating_mul(cols) > MAX_CELLS {
        return Err(ErrorValue::Num);
    }
    let mut data = Vec::with_capacity(rows);
    for p in &parts {
        for i in 0..p.rows() {
            data.push(
                (0..cols)
                    .map(|j| {
                        if j < p.cols() {
                            p.get(i, j).clone()
                        } else {
                            na()
                        }
                    })
                    .collect(),
            );
        }
    }
    from_rows(data)
}

fn to_vector(args: &mut [Arg<'_>], ctx: &mut dyn CallContext, as_column: bool) -> R<Value> {
    let arr = array_of(args, 0, ctx)?;
    let ignore = if omitted(args, 1) {
        0.0
    } else {
        let v = value_at(args, 1, ctx);
        let v = ctx.to_scalar(v);
        to_number(&v, &ctx.coercion())?
    };
    let by_col = if omitted(args, 2) {
        false
    } else {
        let v = value_at(args, 2, ctx);
        let v = ctx.to_scalar(v);
        to_bool(&v)?
    };
    if !(0.0..=3.0).contains(&ignore) {
        return Err(ErrorValue::Value);
    }
    let src = if by_col {
        Arc::new(arr.transpose())
    } else {
        arr
    };
    let skip_blank = ignore == 1.0 || ignore == 3.0;
    let skip_err = ignore == 2.0 || ignore == 3.0;
    let vals: Vec<Value> = src
        .values()
        .iter()
        .filter(|v| !(skip_blank && v.is_blank()) && !(skip_err && matches!(v, Value::Error(_))))
        .cloned()
        .collect();
    if vals.is_empty() {
        return Err(ErrorValue::Calc);
    }
    if as_column { column(vals) } else { row(vals) }
}

fn wrap(args: &mut [Arg<'_>], ctx: &mut dyn CallContext, rows_first: bool) -> R<Value> {
    let arr = array_of(args, 0, ctx)?;
    if arr.rows() > 1 && arr.cols() > 1 {
        return Err(ErrorValue::Value);
    }
    let n = int_of(args, 1, ctx, 0.0)?;
    if n < 1.0 {
        return Err(ErrorValue::Num);
    }
    let pad = pad_arg(args, 2, ctx);
    let vals = arr.values();
    let n = (n as usize).min(MAX_CELLS);
    let lines = vals.len().div_ceil(n);
    if lines.saturating_mul(n) > MAX_CELLS {
        return Err(ErrorValue::Num);
    }
    let out = Array2D::build(lines, n, |i, j| {
        vals.get(i * n + j).cloned().unwrap_or_else(|| pad.clone())
    });
    Ok(Value::array(if rows_first { out } else { out.transpose() }))
}

/// The lookup, reference and dynamic-array functions.
pub fn specs() -> Vec<FunctionSpec> {
    let l023 = ArgMask::of(&[0, 2, 3]);
    vec![
        spec("VLOOKUP", 3, 4, imp!(vlookup)).lift(l023),
        spec("XLOOKUP", 3, 6, imp!(xlookup)).lift(ArgMask::of(&[0, 4, 5])),
        spec("MATCH", 2, 3, imp!(match_fn)).lift(ArgMask::of(&[0, 2])),
        spec("INDEX", 2, 4, imp!(index)),
        spec("ROW", 0, 1, imp!(|a, c| row_or_column(a, c, true))),
        spec("COLUMN", 0, 1, imp!(|a, c| row_or_column(a, c, false))),
        spec("OFFSET", 3, 5, imp!(offset)).volatile(),
        spec("INDIRECT", 1, 2, imp!(indirect)).volatile(),
        spec("SEQUENCE", 1, 4, imp!(sequence)),
        spec("FILTER", 2, 3, imp!(filter)),
        spec("SORT", 1, 4, imp!(sort)),
        spec("UNIQUE", 1, 3, imp!(unique)),
        spec("TRANSPOSE", 1, 1, imp!(transpose)),
        spec("ADDRESS", 2, 5, imp!(address)).lift(ArgMask::ALL),
        spec("AREAS", 1, 1, imp!(areas)),
        spec("CHOOSE", 2, 254, imp!(choose))
            .lazy(AFTER_FIRST)
            .lift(ArgMask::of(&[0])),
        spec(
            "CHOOSEROWS",
            2,
            VARIADIC,
            imp!(|a, c| choose_rows_cols(a, c, false)),
        ),
        spec(
            "CHOOSECOLS",
            2,
            VARIADIC,
            imp!(|a, c| choose_rows_cols(a, c, true)),
        ),
        spec("ROWS", 1, 1, imp!(|a, c| dims(a, c).map(|d| d.0 as f64))),
        spec("COLUMNS", 1, 1, imp!(|a, c| dims(a, c).map(|d| d.1 as f64))),
        spec("TAKE", 2, 3, imp!(|a, c| take(a, c, false))),
        spec("DROP", 2, 3, imp!(|a, c| take(a, c, true))),
        spec("EXPAND", 2, 4, imp!(expand)),
        spec("HSTACK", 1, VARIADIC, imp!(|a, c| stack(a, c, true))),
        spec("VSTACK", 1, VARIADIC, imp!(|a, c| stack(a, c, false))),
        spec("TOCOL", 1, 3, imp!(|a, c| to_vector(a, c, true))),
        spec("TOROW", 1, 3, imp!(|a, c| to_vector(a, c, false))),
        spec("WRAPROWS", 2, 3, imp!(|a, c| wrap(a, c, true))),
        spec("WRAPCOLS", 2, 3, imp!(|a, c| wrap(a, c, false))),
        spec("HLOOKUP", 3, 4, imp!(hlookup)).lift(l023),
        spec("LOOKUP", 2, 3, imp!(lookup)).lift(ArgMask::of(&[0])),
        spec("SORTBY", 2, VARIADIC, imp!(sortby)),
        spec("XMATCH", 2, 4, imp!(xmatch_fn)).lift(ArgMask::of(&[0, 2, 3])),
        spec("HYPERLINK", 1, 2, imp!(hyperlink)).lift(ArgMask::ALL),
    ]
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn round15_fast_is_round15() {
        let mut state = 0x5EED_u64;
        let mut next = || {
            state = state.wrapping_add(0x9E37_79B9_7F4A_7C15);
            let mut z = state;
            z = (z ^ (z >> 30)).wrapping_mul(0xBF58_476D_1CE4_E5B9);
            z = (z ^ (z >> 27)).wrapping_mul(0x94D0_49BB_1331_11EB);
            z ^ (z >> 31)
        };
        let mut xs = vec![
            0.1 + 0.2,
            1.000_000_000_000_005,
            1_234_567_890_123_455.0,
            9_999_999_999_999_995.0,
            0.999_999_999_999_999_5,
            -2.675_000_000_000_000_5,
            5e-324,
            f64::MAX,
            1e15 + 0.5,
        ];
        for _ in 0..40_000 {
            let bits = next();
            let x = f64::from_bits(bits);
            if x.is_finite() {
                xs.push(x);
            }
            // Uniform values and values one ulp either side of 16-digit midpoints.
            let u = (next() >> 11) as f64 / (1u64 << 53) as f64;
            xs.push(u);
            let m: f64 = format!("{u:.14e}")
                .replacen('e', "5e", 1)
                .parse()
                .unwrap_or(u);
            xs.push(m);
            xs.push(f64::from_bits(m.to_bits() + 1));
            xs.push(f64::from_bits(m.to_bits().saturating_sub(1)));
        }
        for x in xs {
            let (fast, slow) = (round15_fast(x), round15(x));
            assert!(
                fast.to_bits() == slow.to_bits(),
                "{x:e}: {fast:e} != {slow:e}"
            );
        }
    }
}
