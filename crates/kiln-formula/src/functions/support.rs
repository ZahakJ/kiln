// SPDX-License-Identifier: AGPL-3.0-or-later
//! Building blocks shared by the families: spec builders, argument access for lifted (scalar) and
//! ordinary arguments, and collectors that reproduce how Excel's aggregate functions treat text,
//! logicals and blanks in ranges versus typed-in arguments.

use std::sync::Arc;

use crate::engine::{Arg, CallContext, EvalValue};
use crate::registry::{ArgMask, DocsHandle, FunctionImpl, FunctionSpec};
use crate::values::{Array2D, ErrorValue, Value, parse_number_text, to_bool, to_number, to_text};

/// Result of most helpers.
pub(crate) type R<T> = Result<T, ErrorValue>;

/// Excel's maximum argument count (variadic functions).
pub(crate) const VARIADIC: u8 = crate::registry::MAX_ARGS;

/// Every position after the first (the lazy branches of `CHOOSE` and `SWITCH`).
pub(crate) const AFTER_FIRST: ArgMask = ArgMask::NONE.and_from(1);

/// Largest array a function builds (rows × columns), as in v1.0 (`#NUM!` beyond).
pub(crate) const MAX_CELLS: usize = 1_048_576 * 16;

/// Excel's text length limit.
pub(crate) const MAX_TEXT: usize = 32_767;

/// Conversion of an implementation's result into what the engine stores.
pub(crate) trait IntoEval {
    fn into_eval(self) -> EvalValue;
}

impl IntoEval for EvalValue {
    fn into_eval(self) -> EvalValue {
        self
    }
}

impl IntoEval for Value {
    fn into_eval(self) -> EvalValue {
        EvalValue::Value(self)
    }
}

impl IntoEval for f64 {
    fn into_eval(self) -> EvalValue {
        EvalValue::Value(Value::number(self))
    }
}

impl IntoEval for bool {
    fn into_eval(self) -> EvalValue {
        EvalValue::Value(Value::Bool(self))
    }
}

impl IntoEval for String {
    fn into_eval(self) -> EvalValue {
        EvalValue::Value(Value::from(self))
    }
}

impl IntoEval for Arc<str> {
    fn into_eval(self) -> EvalValue {
        EvalValue::Value(Value::Str(self))
    }
}

impl IntoEval for &'static str {
    fn into_eval(self) -> EvalValue {
        EvalValue::Value(Value::from(self))
    }
}

impl IntoEval for ErrorValue {
    fn into_eval(self) -> EvalValue {
        EvalValue::Value(Value::Error(self))
    }
}

impl IntoEval for Array2D {
    fn into_eval(self) -> EvalValue {
        EvalValue::Value(Value::array(self))
    }
}

impl<T: IntoEval> IntoEval for R<T> {
    fn into_eval(self) -> EvalValue {
        match self {
            Ok(v) => v.into_eval(),
            Err(e) => EvalValue::Value(Value::Error(e)),
        }
    }
}

/// Wraps an implementation body into a [`FunctionImpl`]:
/// `imp!(path::to_fn)` or `imp!(|args, ctx| expression)`; the result may be anything that
/// implements `IntoEval` (numbers, text, values, arrays, `Result`s of those).
macro_rules! imp {
    (|$a:ident, $c:ident| $body:expr) => {{
        // The body runs in a closure so that `?` works inside it.
        #[allow(clippy::redundant_closure_call, unused_mut)]
        fn imp(
            $a: &mut [$crate::engine::Arg<'_>],
            $c: &mut dyn $crate::engine::CallContext,
        ) -> $crate::engine::EvalValue {
            $crate::functions::support::IntoEval::into_eval((|| $body)())
        }
        imp as $crate::registry::FunctionImpl
    }};
    ($f:path) => {
        imp!(|a, c| $f(a, c))
    };
}

/// A numeric function of fixed arity: every argument lifted over arrays and read as a number
/// (omitted trailing arguments take `defaults`, empty ones are 0); non-finite results are `#NUM!`.
///
/// `numeric!("NAME", min, max, [defaults…], |x, y| body)` where `body` is an `f64` or `R<f64>`.
macro_rules! numeric {
    ($name:literal, $min:literal, $max:literal, [$($d:expr),*], |$($x:ident),+| $body:expr) => {{
        $crate::functions::support::spec(
            $name,
            $min,
            $max,
            imp!(|args, ctx| {
                const DEFAULTS: &[f64] = &[$($d),*];
                match $crate::functions::support::numeric_args(args, ctx, $min, $max, DEFAULTS) {
                    Err(e) => Err(e),
                    Ok(xs) => {
                        let mut it = xs.into_iter();
                        $(let $x: f64 = it.next().unwrap_or(0.0);)+
                        let r: $crate::functions::support::R<f64> =
                            (|| $crate::functions::support::finite($body))();
                        r
                    }
                }
            }),
        )
        .lift($crate::registry::ArgMask::ALL)
    }};
}

/// Like [`numeric!`], with the call context available to the body:
/// `numeric_ctx!("NAME", min, max, [defaults…], |ctx, x, y| body)`.
macro_rules! numeric_ctx {
    ($name:literal, $min:literal, $max:literal, [$($d:expr),*], |$ctx:ident, $($x:ident),+| $body:expr) => {{
        $crate::functions::support::spec(
            $name,
            $min,
            $max,
            imp!(|args, $ctx| {
                const DEFAULTS: &[f64] = &[$($d),*];
                match $crate::functions::support::numeric_args(args, $ctx, $min, $max, DEFAULTS) {
                    Err(e) => Err(e),
                    Ok(xs) => {
                        let mut it = xs.into_iter();
                        $(let $x: f64 = it.next().unwrap_or(0.0);)+
                        let $ctx: &dyn $crate::engine::CallContext = $ctx;
                        let r: $crate::functions::support::R<f64> =
                            (|| $crate::functions::support::finite($body))();
                        r
                    }
                }
            }),
        )
        .lift($crate::registry::ArgMask::ALL)
    }};
}

/// A spec documented by the library's documentation JSON.
pub(crate) fn spec(name: &'static str, min: u8, max: u8, f: FunctionImpl) -> FunctionSpec {
    FunctionSpec {
        docs: DocsHandle::Key(name),
        ..FunctionSpec::new(name, min, max, f)
    }
}

/// The same implementation under another name (`STDEV` for `STDEV.S`).
pub(crate) fn alias(name: &'static str, of: &FunctionSpec) -> FunctionSpec {
    FunctionSpec {
        name,
        docs: DocsHandle::Key(name),
        ..of.clone()
    }
}

/// Chained setters for the less common spec fields.
pub(crate) trait SpecExt {
    fn lift(self, mask: ArgMask) -> Self;
    fn lazy(self, mask: ArgMask) -> Self;
    fn volatile(self) -> Self;
}

impl SpecExt for FunctionSpec {
    fn lift(mut self, mask: ArgMask) -> FunctionSpec {
        self.lift = mask;
        self
    }

    fn lazy(mut self, mask: ArgMask) -> FunctionSpec {
        self.lazy = mask;
        self
    }

    fn volatile(mut self) -> FunctionSpec {
        self.volatile = true;
        self
    }
}

/// A body result as a finite number (`#NUM!` otherwise).
pub(crate) trait IntoNumber {
    fn into_number(self) -> R<f64>;
}

impl IntoNumber for f64 {
    fn into_number(self) -> R<f64> {
        Ok(self)
    }
}

impl IntoNumber for R<f64> {
    fn into_number(self) -> R<f64> {
        self
    }
}

pub(crate) fn finite(x: impl IntoNumber) -> R<f64> {
    let x = x.into_number()?;
    if x.is_finite() {
        Ok(x)
    } else {
        Err(ErrorValue::Num)
    }
}

// ── argument access ────────────────────────────────────────────────────────────────────────

/// What an argument position holds.
#[derive(Debug, Clone)]
pub(crate) enum Slot {
    /// Not passed at all.
    Absent,
    /// Passed but left empty (`ROUND(2.5,)`).
    Empty,
    /// A scalar (references reduced by implicit intersection, arrays to their top-left value).
    Val(Value),
}

/// The scalar at a position (forcing lazy arguments).
pub(crate) fn slot(args: &mut [Arg<'_>], i: usize, ctx: &mut dyn CallContext) -> Slot {
    let Some(a) = args.get_mut(i) else {
        return Slot::Absent;
    };
    match ctx.force(a) {
        EvalValue::Missing => Slot::Empty,
        EvalValue::Value(Value::Array(arr)) => Slot::Val(arr.get(0, 0).clone()),
        EvalValue::Value(Value::Lambda(_)) => Slot::Val(Value::Error(ErrorValue::Value)),
        EvalValue::Value(v) => Slot::Val(v),
        r @ EvalValue::Ref(_) => Slot::Val(ctx.to_scalar(r)),
    }
}

/// The scalar at a position; absent and empty arguments are blank.
pub(crate) fn scalar(args: &mut [Arg<'_>], i: usize, ctx: &mut dyn CallContext) -> Value {
    match slot(args, i, ctx) {
        Slot::Val(v) => v,
        Slot::Absent | Slot::Empty => Value::Blank,
    }
}

/// True when the position was not passed or left empty.
pub(crate) fn omitted(args: &[Arg<'_>], i: usize) -> bool {
    args.get(i).is_none_or(Arg::is_omitted)
}

/// A number from a value (Excel's arithmetic coercion).
pub(crate) fn number(v: &Value, ctx: &dyn CallContext) -> R<f64> {
    to_number(v, &ctx.coercion())
}

/// A lifted number: absent → `dflt`, empty → 0 (as `ROUND(2.5,)`).
pub(crate) fn num(args: &mut [Arg<'_>], i: usize, ctx: &mut dyn CallContext, dflt: f64) -> R<f64> {
    num_or(args, i, ctx, dflt, 0.0)
}

/// A lifted number with an explicit value for an empty argument.
pub(crate) fn num_or(
    args: &mut [Arg<'_>],
    i: usize,
    ctx: &mut dyn CallContext,
    dflt: f64,
    empty: f64,
) -> R<f64> {
    match slot(args, i, ctx) {
        Slot::Absent => Ok(dflt),
        Slot::Empty => Ok(empty),
        Slot::Val(v) => number(&v, ctx),
    }
}

/// A lifted integer (truncated toward zero).
pub(crate) fn int(args: &mut [Arg<'_>], i: usize, ctx: &mut dyn CallContext, dflt: f64) -> R<f64> {
    num(args, i, ctx, dflt).map(f64::trunc)
}

/// A number from an ordinary argument: absent or empty → `dflt`.
pub(crate) fn number_of(
    args: &mut [Arg<'_>],
    i: usize,
    ctx: &mut dyn CallContext,
    dflt: f64,
) -> R<f64> {
    match slot(args, i, ctx) {
        Slot::Absent | Slot::Empty => Ok(dflt),
        Slot::Val(v) => number(&v, ctx),
    }
}

/// An integer from an ordinary argument: absent or empty → `dflt`.
pub(crate) fn int_of(
    args: &mut [Arg<'_>],
    i: usize,
    ctx: &mut dyn CallContext,
    dflt: f64,
) -> R<f64> {
    number_of(args, i, ctx, dflt).map(f64::trunc)
}

/// Text: absent and empty arguments are `""`.
pub(crate) fn text(args: &mut [Arg<'_>], i: usize, ctx: &mut dyn CallContext) -> R<Arc<str>> {
    match slot(args, i, ctx) {
        Slot::Absent | Slot::Empty => Ok(Arc::from("")),
        Slot::Val(v) => to_text(&v),
    }
}

/// A lifted logical: absent → `dflt`, empty → FALSE.
pub(crate) fn flag(
    args: &mut [Arg<'_>],
    i: usize,
    ctx: &mut dyn CallContext,
    dflt: bool,
) -> R<bool> {
    match slot(args, i, ctx) {
        Slot::Absent => Ok(dflt),
        Slot::Empty => Ok(false),
        Slot::Val(v) => to_bool(&v),
    }
}

/// A logical from an ordinary argument: absent or empty → `dflt`.
pub(crate) fn flag_of(
    args: &mut [Arg<'_>],
    i: usize,
    ctx: &mut dyn CallContext,
    dflt: bool,
) -> R<bool> {
    match slot(args, i, ctx) {
        Slot::Absent | Slot::Empty => Ok(dflt),
        Slot::Val(v) => to_bool(&v),
    }
}

/// Reads the arguments of a [`numeric!`] function.
pub(crate) fn numeric_args(
    args: &mut [Arg<'_>],
    ctx: &mut dyn CallContext,
    min: usize,
    max: usize,
    defaults: &[f64],
) -> R<Vec<f64>> {
    let mut xs = Vec::with_capacity(max);
    for i in 0..max {
        let d = i
            .checked_sub(min)
            .and_then(|k| defaults.get(k))
            .copied()
            .unwrap_or(0.0);
        xs.push(num(args, i, ctx, d)?);
    }
    Ok(xs)
}

/// The evaluated argument at a position (`Missing` when absent).
pub(crate) fn value_at(args: &mut [Arg<'_>], i: usize, ctx: &mut dyn CallContext) -> EvalValue {
    match args.get_mut(i) {
        Some(a) => ctx.force(a),
        None => EvalValue::Missing,
    }
}

// ── collectors ─────────────────────────────────────────────────────────────────────────────

/// Collects numbers like `SUM`: in ranges and arrays only numbers count; typed-in logicals and
/// numeric text count; other typed-in text is `#VALUE!`; an empty argument counts as 0. Errors
/// propagate.
pub(crate) fn numbers(args: &mut [Arg<'_>], ctx: &mut dyn CallContext) -> R<Vec<f64>> {
    collect(args, ctx, false)
}

/// Collects values like the "A" functions (`AVERAGEA`, `MAXA` …): in ranges text counts as 0 and
/// logicals as 1/0, blanks are skipped; typed-in values behave as in [`numbers`].
pub(crate) fn numbers_a(args: &mut [Arg<'_>], ctx: &mut dyn CallContext) -> R<Vec<f64>> {
    collect(args, ctx, true)
}

fn collect(args: &mut [Arg<'_>], ctx: &mut dyn CallContext, a_mode: bool) -> R<Vec<f64>> {
    let mut out = Vec::new();
    let opts = ctx.coercion();
    for a in args.iter_mut() {
        let v = ctx.force(a);
        if v == EvalValue::Missing {
            out.push(0.0);
            continue;
        }
        let mut err: Option<ErrorValue> = None;
        ctx.for_each_scalar(&v, &mut |x, from_ref| {
            match x {
                Value::Number(n) => out.push(*n),
                Value::Error(e) => {
                    err = Some(*e);
                    return false;
                }
                Value::Bool(b) if a_mode || !from_ref => out.push(if *b { 1.0 } else { 0.0 }),
                Value::Str(_) if a_mode && from_ref => out.push(0.0),
                Value::Str(s) if !from_ref => match parse_number_text(s, &opts) {
                    Some(n) => out.push(n),
                    None => {
                        err = Some(ErrorValue::Value);
                        return false;
                    }
                },
                Value::Blank if !from_ref => out.push(0.0),
                _ => {}
            }
            true
        });
        if let Some(e) = err {
            return Err(e);
        }
    }
    Ok(out)
}

/// Numbers of one argument (text, logicals and blanks in ranges skipped; errors propagate).
pub(crate) fn numbers_of(args: &mut [Arg<'_>], i: usize, ctx: &mut dyn CallContext) -> R<Vec<f64>> {
    match args.get_mut(i) {
        Some(a) => numbers(std::slice::from_mut(a), ctx),
        None => Ok(Vec::new()),
    }
}

/// Every scalar of an argument in row-major order.
pub(crate) fn scalars_of(args: &mut [Arg<'_>], i: usize, ctx: &mut dyn CallContext) -> Vec<Value> {
    let mut out = Vec::new();
    let v = value_at(args, i, ctx);
    if v == EvalValue::Missing {
        return out;
    }
    ctx.for_each_scalar(&v, &mut |x, _| {
        out.push(x.clone());
        true
    });
    out
}

/// An argument as a 2-D array (references dereferenced, scalars as 1×1). Absent arguments,
/// multi-area references and lambdas are `#VALUE!`.
pub(crate) fn array_of(
    args: &mut [Arg<'_>],
    i: usize,
    ctx: &mut dyn CallContext,
) -> R<Arc<Array2D>> {
    let Some(a) = args.get_mut(i) else {
        return Err(ErrorValue::Value);
    };
    let v = ctx.force(a);
    match &v {
        EvalValue::Ref(r) if r.as_areas().len() > 1 => return Err(ErrorValue::Value),
        EvalValue::Value(Value::Lambda(_)) => return Err(ErrorValue::Value),
        _ => {}
    }
    ctx.to_array(v)
}

/// Two equally sized arrays as paired numbers, keeping only positions where both are numbers
/// (`CORREL`, `SLOPE`, `COVARIANCE` …). Different sizes are `#N/A`; errors propagate.
pub(crate) fn numeric_pairs(
    args: &mut [Arg<'_>],
    i: usize,
    j: usize,
    ctx: &mut dyn CallContext,
) -> R<(Vec<f64>, Vec<f64>)> {
    let x = array_of(args, i, ctx)?;
    let y = array_of(args, j, ctx)?;
    if x.len() != y.len() {
        return Err(ErrorValue::NA);
    }
    let (mut xs, mut ys) = (Vec::new(), Vec::new());
    for (p, q) in x.values().iter().zip(y.values()) {
        if let Value::Error(e) = p {
            return Err(*e);
        }
        if let Value::Error(e) = q {
            return Err(*e);
        }
        if let (Value::Number(a), Value::Number(b)) = (p, q) {
            xs.push(*a);
            ys.push(*b);
        }
    }
    Ok((xs, ys))
}

// ── numeric helpers ────────────────────────────────────────────────────────────────────────

/// Neumaier summation: long sums of decimals stay as close to Excel's as possible.
pub(crate) fn sum(xs: &[f64]) -> f64 {
    let (mut s, mut c) = (0.0f64, 0.0f64);
    for &x in xs {
        let t = s + x;
        if s.abs() >= x.abs() {
            c += s - t + x;
        } else {
            c += x - t + s;
        }
        s = t;
    }
    s + c
}

pub(crate) fn mean(xs: &[f64]) -> f64 {
    sum(xs) / xs.len() as f64
}

/// Sum of squared deviations from the mean.
pub(crate) fn devsq(xs: &[f64]) -> f64 {
    let m = mean(xs);
    xs.iter().map(|x| (x - m) * (x - m)).sum()
}

pub(crate) fn sorted_asc(xs: &[f64]) -> Vec<f64> {
    let mut v = xs.to_vec();
    v.sort_by(f64::total_cmp);
    v
}

// ── results ────────────────────────────────────────────────────────────────────────────────

/// A column array (`#CALC!` when empty).
pub(crate) fn column(values: Vec<Value>) -> R<Value> {
    Array2D::column(values)
        .map(Value::array)
        .ok_or(ErrorValue::Calc)
}

/// A row array (`#CALC!` when empty).
pub(crate) fn row(values: Vec<Value>) -> R<Value> {
    Array2D::row(values)
        .map(Value::array)
        .ok_or(ErrorValue::Calc)
}

/// An array from rows (`#CALC!` when empty or ragged).
pub(crate) fn from_rows(rows: Vec<Vec<Value>>) -> R<Value> {
    Array2D::from_rows(rows)
        .map(Value::array)
        .ok_or(ErrorValue::Calc)
}

/// Numbers as values.
pub(crate) fn nums(xs: impl IntoIterator<Item = f64>) -> Vec<Value> {
    xs.into_iter().map(Value::number).collect()
}

/// The rows of an array, cloned.
pub(crate) fn rows_of(a: &Array2D) -> Vec<Vec<Value>> {
    (0..a.rows())
        .map(|r| (0..a.cols()).map(|c| a.get(r, c).clone()).collect())
        .collect()
}

/// `#VALUE!` for text longer than Excel's limit (32,767 UTF-16 units).
pub(crate) fn capped(s: String) -> R<String> {
    if s.len() > MAX_TEXT && s.encode_utf16().count() > MAX_TEXT {
        Err(ErrorValue::Value)
    } else {
        Ok(s)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn neumaier_sum_keeps_decimals() {
        assert_eq!(sum(&[0.1; 10]), 1.0);
        assert_eq!(sum(&[1e100, 1.0, -1e100]), 1.0);
        assert_eq!(sum(&[]), 0.0);
    }

    #[test]
    fn finite_maps_overflow_to_num() {
        assert_eq!(finite(f64::INFINITY), Err(ErrorValue::Num));
        assert_eq!(finite(f64::NAN), Err(ErrorValue::Num));
        assert_eq!(finite(Ok(2.0)), Ok(2.0));
    }
}
