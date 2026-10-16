// SPDX-License-Identifier: AGPL-3.0-or-later
//! Shared helpers for the engine tests: an engine with a fixed clock, cell filling by A1 address,
//! a deterministic PRNG, and a handful of test-only functions registered through the public
//! `FunctionRegistry` (LET, LAMBDA, ISOMITTED, SEQUENCE, ROW, COLUMN, NOW, INDIRECT, OFFSET,
//! COUNT, COUNTA, IFERROR, INDEX). The real implementations live in the function library; these
//! exist to exercise the engine's lazy arguments, lambdas, spilling, volatility and dynamic
//! references, and to prove the registry is a sufficient extension point.
#![allow(dead_code)]

use std::sync::Arc;

use kiln_formula::engine::{
    Arg, CallContext, CellInput, Engine, EngineOptions, EvalValue, Reference, Scope, SheetId,
    force, number_arg, scalar_arg,
};
use kiln_formula::refs::{Bounds, MAX_COLS, MAX_ROWS, parse_cell_address};
use kiln_formula::registry::{ArgMask, FunctionRegistry, FunctionSpec, FunctionTable, MAX_ARGS};
use kiln_formula::values::{Array2D, ErrorValue, Value, parse_number_text, to_bool, to_text};

/// 2024-01-15 12:00 (what the v1.0 tests used).
pub const NOW: f64 = 45306.5;

pub fn addr(a1: &str) -> (u32, u32) {
    parse_cell_address(a1).unwrap_or_else(|| panic!("bad address {a1}"))
}

fn err(e: ErrorValue) -> EvalValue {
    EvalValue::Value(Value::Error(e))
}

fn let_(args: &mut [Arg<'_>], ctx: &mut dyn CallContext) -> EvalValue {
    if args.len() < 3 || args.len().is_multiple_of(2) {
        return err(ErrorValue::Value);
    }
    let base = match &args[0] {
        Arg::Lazy(l) => l.scope().cloned(),
        Arg::Value(_) => None,
    };
    let mut scope: Option<Arc<Scope>> = base;
    let n = args.len();
    for i in (0..n - 1).step_by(2) {
        let Arg::Lazy(name_arg) = &args[i] else {
            return err(ErrorValue::Name);
        };
        let Some(name) = name_arg.expr().as_local_name() else {
            return err(ErrorValue::Name);
        };
        let name = name.to_owned();
        let Arg::Lazy(value_arg) = &args[i + 1] else {
            return err(ErrorValue::Value);
        };
        let expr = value_arg.expr().clone();
        let v = ctx.evaluate(&expr, scope.clone());
        scope = Some(Scope::bind(scope, &name, v));
    }
    let Arg::Lazy(calc) = &args[n - 1] else {
        return err(ErrorValue::Value);
    };
    let expr = calc.expr().clone();
    ctx.evaluate(&expr, scope)
}

fn lambda(args: &mut [Arg<'_>], ctx: &mut dyn CallContext) -> EvalValue {
    let n = args.len();
    let mut params: Vec<String> = Vec::new();
    for a in &args[..n - 1] {
        let Arg::Lazy(l) = a else {
            return err(ErrorValue::Value);
        };
        let Some(name) = l.expr().as_local_name() else {
            return err(ErrorValue::Value);
        };
        if params.iter().any(|p| p.eq_ignore_ascii_case(name)) {
            return err(ErrorValue::Value);
        }
        params.push(name.to_owned());
    }
    let Arg::Lazy(body) = &args[n - 1] else {
        return err(ErrorValue::Value);
    };
    let (expr, closure) = (body.expr().clone(), body.scope().cloned());
    EvalValue::Value(ctx.make_lambda(params, expr, closure))
}

fn isomitted(args: &mut [Arg<'_>], ctx: &mut dyn CallContext) -> EvalValue {
    let v = force(args.first_mut(), ctx);
    EvalValue::Value(Value::Bool(v == EvalValue::Missing))
}

fn sequence(args: &mut [Arg<'_>], ctx: &mut dyn CallContext) -> EvalValue {
    let mut it = args.iter_mut();
    let rows = number_arg(it.next(), ctx, None);
    let opt = |a: Option<&mut Arg<'_>>, ctx: &mut dyn CallContext, d: f64| match a {
        Some(a) if !a.is_omitted() => number_arg(Some(a), ctx, None),
        _ => Ok(d),
    };
    let cols = opt(it.next(), ctx, 1.0);
    let start = opt(it.next(), ctx, 1.0);
    let step = opt(it.next(), ctx, 1.0);
    let (Ok(rows), Ok(cols), Ok(start), Ok(step)) = (rows, cols, start, step) else {
        return err(ErrorValue::Value);
    };
    let (rows, cols) = (rows.trunc(), cols.trunc());
    if rows < 0.0 || cols < 0.0 {
        return err(ErrorValue::Value);
    }
    if rows == 0.0 || cols == 0.0 {
        return err(ErrorValue::Calc);
    }
    if rows * cols > 1_048_576.0 * 16.0 {
        return err(ErrorValue::Num);
    }
    let (r, c) = (rows as usize, cols as usize);
    let a = Array2D::build(r, c, |i, j| {
        Value::number(start + step * (i * c + j) as f64)
    });
    if a.len() == 1 {
        return EvalValue::Value(a.get(0, 0).clone());
    }
    EvalValue::Value(Value::array(a))
}

fn row_or_col(args: &mut [Arg<'_>], ctx: &mut dyn CallContext, row: bool) -> EvalValue {
    if args.is_empty() || args[0].is_omitted() {
        return EvalValue::from(f64::from(if row { ctx.row() } else { ctx.col() }) + 1.0);
    }
    match ctx.force(&mut args[0]) {
        EvalValue::Ref(r) => {
            let Some(a) = r.single() else {
                return err(ErrorValue::Value);
            };
            let b = a.bounds;
            let (lo, hi) = if row { (b.r0, b.r1) } else { (b.c0, b.c1) };
            if lo == hi {
                return EvalValue::from(f64::from(lo) + 1.0);
            }
            let vals: Vec<Value> = (lo..=hi)
                .map(|i| Value::number(f64::from(i) + 1.0))
                .collect();
            let arr = if row {
                Array2D::column(vals)
            } else {
                Array2D::row(vals)
            };
            arr.map_or(err(ErrorValue::Calc), |a| EvalValue::Value(Value::array(a)))
        }
        _ => err(ErrorValue::Value),
    }
}

fn row(args: &mut [Arg<'_>], ctx: &mut dyn CallContext) -> EvalValue {
    row_or_col(args, ctx, true)
}

fn column(args: &mut [Arg<'_>], ctx: &mut dyn CallContext) -> EvalValue {
    row_or_col(args, ctx, false)
}

fn now(_: &mut [Arg<'_>], ctx: &mut dyn CallContext) -> EvalValue {
    EvalValue::from(ctx.now())
}

fn indirect(args: &mut [Arg<'_>], ctx: &mut dyn CallContext) -> EvalValue {
    let text = scalar_arg(args.first_mut(), ctx);
    let a1 = match args.get_mut(1) {
        Some(a) if !a.is_omitted() => {
            let v = scalar_arg(Some(a), ctx);
            to_bool(&v).unwrap_or(true)
        }
        _ => true,
    };
    let Ok(t) = to_text(&text) else {
        return err(ErrorValue::Ref);
    };
    match ctx.resolve_reference_text(&t, a1) {
        Ok(r) => EvalValue::Ref(r),
        Err(e) => err(e),
    }
}

fn offset(args: &mut [Arg<'_>], ctx: &mut dyn CallContext) -> EvalValue {
    let base = ctx.force(&mut args[0]);
    let EvalValue::Ref(r) = base else {
        return err(ErrorValue::Value);
    };
    let Some(a) = r.single().copied() else {
        return err(ErrorValue::Value);
    };
    let mut nums = Vec::new();
    for i in 1..5 {
        match args.get_mut(i) {
            Some(x) if !x.is_omitted() => match number_arg(Some(x), ctx, None) {
                Ok(n) => nums.push(Some(n.trunc() as i64)),
                Err(e) => return err(e),
            },
            _ => nums.push(None),
        }
    }
    let (dr, dc) = (nums[0].unwrap_or(0), nums[1].unwrap_or(0));
    let (h0, w0) = a.dims();
    let h = nums[2].unwrap_or(h0 as i64);
    let w = nums[3].unwrap_or(w0 as i64);
    if h == 0 || w == 0 {
        return err(ErrorValue::Ref);
    }
    let r0 = i64::from(a.bounds.r0) + dr;
    let c0 = i64::from(a.bounds.c0) + dc;
    let (ra, rb) = if h > 0 {
        (r0, r0 + h - 1)
    } else {
        (r0 + h + 1, r0)
    };
    let (ca, cb) = if w > 0 {
        (c0, c0 + w - 1)
    } else {
        (c0 + w + 1, c0)
    };
    if ra < 0 || ca < 0 || rb >= i64::from(MAX_ROWS) || cb >= i64::from(MAX_COLS) {
        return err(ErrorValue::Ref);
    }
    let area = kiln_formula::engine::Area {
        sheet: a.sheet,
        bounds: Bounds {
            r0: ra as u32,
            c0: ca as u32,
            r1: rb as u32,
            c1: cb as u32,
        },
    };
    EvalValue::Ref(Reference::area(area))
}

fn count_like(args: &mut [Arg<'_>], ctx: &mut dyn CallContext, all: bool) -> EvalValue {
    let mut n = 0.0;
    let opts = ctx.coercion();
    for a in args.iter_mut() {
        let v = ctx.force(a);
        ctx.for_each_scalar(&v, &mut |x, from_ref| {
            let counts = if all {
                !x.is_blank() || !from_ref
            } else {
                match x {
                    Value::Number(_) => true,
                    Value::Bool(_) => !from_ref,
                    Value::Str(s) => !from_ref && parse_number_text(s, &opts).is_some(),
                    _ => false,
                }
            };
            if counts {
                n += 1.0;
            }
            true
        });
    }
    EvalValue::from(n)
}

fn count(args: &mut [Arg<'_>], ctx: &mut dyn CallContext) -> EvalValue {
    count_like(args, ctx, false)
}

fn counta(args: &mut [Arg<'_>], ctx: &mut dyn CallContext) -> EvalValue {
    count_like(args, ctx, true)
}

fn iferror(args: &mut [Arg<'_>], ctx: &mut dyn CallContext) -> EvalValue {
    let v = scalar_arg(args.first_mut(), ctx);
    if v.as_error().is_some() {
        let b = force(args.get_mut(1), ctx);
        return match b {
            EvalValue::Missing | EvalValue::Value(Value::Blank) => EvalValue::from(0.0),
            other => other,
        };
    }
    EvalValue::Value(v.blank_to_zero())
}

fn index(args: &mut [Arg<'_>], ctx: &mut dyn CallContext) -> EvalValue {
    let base = ctx.force(&mut args[0]);
    let r = match number_arg(args.get_mut(1), ctx, Some(1.0)) {
        Ok(n) => n.trunc() as i64,
        Err(e) => return err(e),
    };
    let c = match number_arg(args.get_mut(2), ctx, Some(1.0)) {
        Ok(n) => n.trunc() as i64,
        Err(e) => return err(e),
    };
    if r < 1 || c < 1 {
        return err(ErrorValue::Value);
    }
    match base {
        EvalValue::Ref(re) => {
            let Some(a) = re.single().copied() else {
                return err(ErrorValue::Ref);
            };
            let (h, w) = a.dims();
            if r as usize > h || c as usize > w {
                return err(ErrorValue::Ref);
            }
            let row = a.bounds.r0 + (r - 1) as u32;
            let col = a.bounds.c0 + (c - 1) as u32;
            EvalValue::Ref(Reference::area(kiln_formula::engine::Area::cell(
                a.sheet, row, col,
            )))
        }
        other => {
            let v = ctx.to_value(other);
            match v {
                Value::Array(arr) => {
                    if r as usize > arr.rows() || c as usize > arr.cols() {
                        return err(ErrorValue::Ref);
                    }
                    EvalValue::Value(arr.get(r as usize - 1, c as usize - 1).clone())
                }
                s if r == 1 && c == 1 => EvalValue::Value(s),
                _ => err(ErrorValue::Ref),
            }
        }
    }
}

/// The core functions plus the test-only functions above.
pub fn test_registry() -> FunctionTable {
    let mut t = FunctionTable::with_core();
    t.register_all([
        FunctionSpec {
            lazy: ArgMask::ALL,
            ..FunctionSpec::new("LET", 3, MAX_ARGS, let_)
        },
        FunctionSpec {
            lazy: ArgMask::ALL,
            ..FunctionSpec::new("LAMBDA", 1, MAX_ARGS, lambda)
        },
        FunctionSpec {
            lazy: ArgMask::ALL,
            ..FunctionSpec::new("ISOMITTED", 1, 1, isomitted)
        },
        FunctionSpec::new("SEQUENCE", 1, 4, sequence),
        FunctionSpec::new("ROW", 0, 1, row),
        FunctionSpec::new("COLUMN", 0, 1, column),
        FunctionSpec {
            volatile: true,
            ..FunctionSpec::new("NOW", 0, 0, now)
        },
        FunctionSpec {
            volatile: true,
            ..FunctionSpec::new("INDIRECT", 1, 2, indirect)
        },
        FunctionSpec {
            volatile: true,
            ..FunctionSpec::new("OFFSET", 3, 5, offset)
        },
        FunctionSpec::new("COUNT", 1, MAX_ARGS, count),
        FunctionSpec::new("COUNTA", 1, MAX_ARGS, counta),
        FunctionSpec {
            lazy: ArgMask::of(&[1]),
            lift: ArgMask::of(&[0, 1]),
            ..FunctionSpec::new("IFERROR", 2, 2, iferror)
        },
        FunctionSpec::new("INDEX", 2, 3, index),
    ]);
    t
}

pub fn options() -> EngineOptions {
    let registry: Arc<dyn FunctionRegistry> = Arc::new(test_registry());
    EngineOptions {
        clock: Some(Arc::new(|| NOW)),
        registry: Some(registry),
        ..EngineOptions::default()
    }
}

/// An engine with the test registry, a fixed clock and an empty `Sheet1`.
pub fn engine() -> Engine {
    engine_with(options())
}

pub fn engine_with(options: EngineOptions) -> Engine {
    let mut e = Engine::with_options(options);
    e.add_sheet("Sheet1").expect("add Sheet1");
    e
}

/// A cell's content: text starting with `=` is a formula.
#[derive(Debug, Clone)]
pub enum Lit {
    N(f64),
    S(&'static str),
    B(bool),
}

impl From<f64> for Lit {
    fn from(n: f64) -> Lit {
        Lit::N(n)
    }
}
impl From<i32> for Lit {
    fn from(n: i32) -> Lit {
        Lit::N(f64::from(n))
    }
}
impl From<&'static str> for Lit {
    fn from(s: &'static str) -> Lit {
        Lit::S(s)
    }
}
impl From<bool> for Lit {
    fn from(b: bool) -> Lit {
        Lit::B(b)
    }
}

pub fn fill_on(e: &mut Engine, sheet: &str, cells: &[(&str, Lit)]) {
    for (a, v) in cells {
        let (row, col) = addr(a);
        let input = match v {
            Lit::S(s) if s.starts_with('=') => CellInput::Formula {
                text: (*s).to_owned(),
                legacy: false,
            },
            Lit::S(s) => CellInput::Value(Value::from(*s)),
            Lit::N(n) => CellInput::Value(Value::Number(*n)),
            Lit::B(b) => CellInput::Value(Value::Bool(*b)),
        };
        e.set_cell(sheet, row, col, input)
            .unwrap_or_else(|err| panic!("{a}: {err}"));
    }
}

pub fn fill(e: &mut Engine, cells: &[(&str, Lit)]) {
    fill_on(e, "Sheet1", cells);
}

/// An engine with `Sheet1` filled with `cells`.
pub fn sheet(cells: &[(&str, Lit)]) -> Engine {
    let mut e = engine();
    fill(&mut e, cells);
    e
}

pub fn get(e: &Engine, a1: &str) -> Value {
    get_on(e, "Sheet1", a1)
}

pub fn get_on(e: &Engine, sheet: &str, a1: &str) -> Value {
    let (row, col) = addr(a1);
    e.value(sheet, row, col).expect("value")
}

/// Evaluates a formula in a fresh engine filled with `cells` (after a recalculation).
pub fn calc(formula: &str, cells: &[(&str, Lit)]) -> Value {
    let mut e = sheet(cells);
    e.recalc();
    e.evaluate(formula)
        .unwrap_or_else(|err| panic!("{formula}: {err}"))
}

pub fn num(n: f64) -> Value {
    Value::Number(n)
}

pub fn text(s: &str) -> Value {
    Value::from(s)
}

pub fn error(e: ErrorValue) -> Value {
    Value::Error(e)
}

/// An array value from rows.
pub fn arr(rows: Vec<Vec<Value>>) -> Value {
    Value::array(Array2D::from_rows(rows).expect("rectangular"))
}

pub fn sheet1(e: &Engine) -> SheetId {
    e.sheet_id("Sheet1").expect("Sheet1")
}

/// mulberry32, as in the TypeScript tests.
pub struct Rng(pub u32);

impl Rng {
    pub fn next(&mut self) -> f64 {
        self.0 = self.0.wrapping_add(0x6d2b_79f5);
        let mut t = self.0;
        t = (t ^ (t >> 15)).wrapping_mul(t | 1);
        t ^= t.wrapping_add((t ^ (t >> 7)).wrapping_mul(t | 61));
        f64::from(t ^ (t >> 14)) / 4_294_967_296.0
    }

    pub fn int(&mut self, lo: i64, hi: i64) -> i64 {
        lo + (self.next() * (hi - lo + 1) as f64).floor() as i64
    }
}
