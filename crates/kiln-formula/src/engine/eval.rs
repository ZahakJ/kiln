// SPDX-License-Identifier: AGPL-3.0-or-later
//! The expression interpreter.
//!
//! References evaluate to [`Reference`]s and are dereferenced only where a value is needed, so
//! functions such as `ROW`, `OFFSET` and `INDEX` and the reference operators (`:`, space, `,`) see
//! real references. Arguments in a function's lazy mask are passed unevaluated; arguments in its
//! lift mask are lifted over arrays. Dynamic-array semantics apply unless the formula is legacy,
//! in which case ranges in single-value positions use implicit intersection.

use std::sync::Arc;

use rustc_hash::FxHashMap;
use smallvec::SmallVec;

use super::expr::{Expr, FuncRef, NameScope, RelAddress, SheetSel};
use super::host::TableDef;
use super::structured::resolve_structured;
use super::types::{
    Area, Arg, EvalValue, Lambda, LazyArg, Reference, RowVisibility, Scope, SheetId,
};
use crate::parser::{BinaryOp, PostfixOp, RefStyle, StructuredRef, UnaryOp};
use crate::refs::{Bounds, MAX_COLS, MAX_ROWS};
use crate::registry::{ArgMask, FunctionImpl, FunctionRegistry};
use crate::values::{
    Array2D, CoercionOptions, ErrorValue, Value, compare_values, to_number, to_text,
};

/// Deepest evaluation recursion (sub-expressions, names and lambda calls together); deeper
/// formulas give `#NUM!` instead of exhausting the stack.
pub const MAX_EVAL_DEPTH: u32 = 2048;
/// Deepest chain of lambda calls (recursive named lambdas).
pub const MAX_LAMBDA_DEPTH: u32 = 400;
/// Deepest chain of defined names referring to names.
pub const MAX_NAME_DEPTH: u32 = 32;

/// Workbook facts the evaluator needs (implemented by the engine).
pub(crate) trait Workbook {
    fn sheet_id(&self, name: &str) -> Option<SheetId>;
    fn sheet_name(&self, id: SheetId) -> Option<String>;
    fn sheets_between(&self, a: SheetId, b: SheetId) -> Option<Vec<SheetId>>;
    fn sheet_order(&self) -> Vec<SheetId>;
    fn lookup_name(&self, name: &str, sheet: SheetId) -> Option<Arc<Expr>>;
    fn table(&self, name: &str) -> Option<TableDef>;
    fn table_at(&self, sheet: SheetId, row: u32, col: u32) -> Option<TableDef>;
    fn used_bounds(&self, sheet: SheetId) -> (u32, u32);
    fn registry(&self) -> &dyn FunctionRegistry;
    fn coercion(&self) -> CoercionOptions;
    fn now(&self) -> f64;
    fn is_formula_cell(&self, sheet: SheetId, row: u32, col: u32) -> bool;
    fn formula_text(&self, sheet: SheetId, row: u32, col: u32) -> Option<String>;
    fn row_visibility(&self, sheet: SheetId, row: u32) -> RowVisibility;
    /// Parses and compiles reference text for `INDIRECT`.
    fn compile_text(&self, text: &str, style: RefStyle, anchor: (u32, u32)) -> Option<Expr>;
}

/// Where cell values come from: the engine pulling pending formulas (serial) or a read-only
/// snapshot (parallel evaluation of a topological level).
pub(crate) trait CellSource {
    fn workbook(&self) -> &dyn Workbook;
    fn read_cell(&mut self, sheet: SheetId, row: u32, col: u32) -> Value;
    fn spill_area(&mut self, sheet: SheetId, row: u32, col: u32) -> Result<Bounds, ErrorValue>;
}

/// What a function implementation can do with its evaluation context.
pub trait CallContext {
    /// The sheet of the cell being calculated.
    fn sheet(&self) -> SheetId;
    /// Row of the cell being calculated (0-based).
    fn row(&self) -> u32;
    /// Column of the cell being calculated (0-based).
    fn col(&self) -> u32;
    /// Legacy (pre-dynamic-array) semantics: implicit intersection instead of lifting.
    fn legacy(&self) -> bool;
    /// The 1904 date system is in use.
    fn date1904(&self) -> bool;
    /// Text-to-number options (date system, current year).
    fn coercion(&self) -> CoercionOptions;
    /// The current time as a serial (fixed for the duration of one recalculation).
    fn now(&self) -> f64;
    /// The lexical scope at this point (LET/LAMBDA variables).
    fn scope(&self) -> Option<Arc<Scope>>;
    /// Evaluates a (lazy) argument once; later calls return the cached value.
    fn force(&mut self, arg: &mut Arg<'_>) -> EvalValue;
    /// Evaluates an expression in a lexical scope.
    fn evaluate(&mut self, expr: &Expr, scope: Option<Arc<Scope>>) -> EvalValue;
    /// Dereferences: a single cell gives its value, an area an array; multi-area references are
    /// `#VALUE!`; an omitted argument is a blank.
    fn to_value(&mut self, v: EvalValue) -> Value;
    /// Reduces to one scalar (implicit intersection for ranges, top-left for arrays).
    fn to_scalar(&mut self, v: EvalValue) -> Value;
    /// Converts to an array (scalars become 1×1).
    fn to_array(&mut self, v: EvalValue) -> Result<Arc<Array2D>, ErrorValue>;
    /// Visits every scalar of a value; the flag is true for values from a range or array (where
    /// Excel's aggregates ignore text and logicals). Return `false` to stop.
    fn for_each_scalar(&mut self, v: &EvalValue, cb: &mut dyn FnMut(&Value, bool) -> bool);
    /// Value of one cell (evaluating a pending formula first).
    fn read_cell(&mut self, sheet: SheetId, row: u32, col: u32) -> Value;
    /// Calls a lambda with evaluated arguments.
    fn invoke(&mut self, lambda: &Lambda, args: Vec<EvalValue>) -> EvalValue;
    /// A lambda value closing over `closure`, with relative references anchored here.
    fn make_lambda(
        &mut self,
        params: Vec<String>,
        body: Arc<Expr>,
        closure: Option<Arc<Scope>>,
    ) -> Value;
    /// Resolves reference text (`INDIRECT`), A1 or R1C1 relative to the current cell.
    fn resolve_reference_text(&mut self, text: &str, a1: bool) -> Result<Reference, ErrorValue>;
    /// Excel's implicit intersection of a reference with the current row or column.
    fn implicit_intersection(&mut self, r: &Reference) -> Value;
    /// Used extent `(rows, cols)` of a sheet (exclusive).
    fn used_bounds(&self, sheet: SheetId) -> (u32, u32);
    /// An area with whole-row/column parts clamped to the used extent (at least one cell).
    fn clamp_area(&self, area: Area) -> Area;
    /// A sheet's name.
    fn sheet_name(&self, sheet: SheetId) -> Option<String>;
    /// A sheet's id by name (case-insensitive).
    fn sheet_id(&self, name: &str) -> Option<SheetId>;
    /// Sheet ids in workbook order.
    fn sheet_ids(&self) -> Vec<SheetId>;
    /// A table by name.
    fn table(&self, name: &str) -> Option<TableDef>;
    /// True when the cell holds a formula.
    fn is_formula_cell(&self, sheet: SheetId, row: u32, col: u32) -> bool;
    /// Formula text of a cell (with `=`).
    fn formula_text(&self, sheet: SheetId, row: u32, col: u32) -> Option<String>;
    /// Visibility of a row for `SUBTOTAL`/`AGGREGATE`.
    fn row_visibility(&self, sheet: SheetId, row: u32) -> RowVisibility;
    /// Records a hyperlink target for the cell being calculated (`HYPERLINK`).
    fn set_hyperlink(&mut self, target: String);
}

/// The interpreter state for one formula evaluation.
pub(crate) struct Evaluator<'s> {
    src: &'s mut dyn CellSource,
    sheet: SheetId,
    row: u32,
    col: u32,
    anchor: (u32, u32),
    legacy: bool,
    scope: Option<Arc<Scope>>,
    depth: u32,
    lambda_depth: u32,
    name_depth: u32,
    coercion: CoercionOptions,
    pub(crate) hyperlink: Option<String>,
    /// Per-expression evaluation record for the Formula Lab (`None` outside `Engine::explain`).
    trace: Option<FxHashMap<usize, TraceEntry>>,
}

/// What one sub-expression evaluated to during a traced evaluation.
#[derive(Debug, Clone)]
pub(crate) struct TraceEntry {
    /// The last value it produced (references are not dereferenced).
    pub(crate) value: EvalValue,
    /// How many times it was evaluated (more than once inside lambdas).
    pub(crate) count: u32,
}

/// The trace key of an expression: its address. Every traced expression of a formula lives in
/// the compiled tree, which is borrowed for the whole evaluation, so addresses are stable and
/// distinct; temporaries (INDIRECT targets, defined-name bodies) never alias a live tree node.
pub(crate) fn trace_key(e: &Expr) -> usize {
    std::ptr::from_ref(e) as usize
}

fn err(e: ErrorValue) -> EvalValue {
    EvalValue::Value(Value::Error(e))
}

/// Element (r, c) of a broadcast operand: a single row or column repeats; positions outside a
/// smaller operand are `#N/A`, as in Excel.
pub fn broadcast_pick(v: &Value, r: usize, c: usize) -> Value {
    match v {
        Value::Array(a) => {
            let rr = if a.rows() == 1 { 0 } else { r };
            let cc = if a.cols() == 1 { 0 } else { c };
            a.get(rr, cc).clone()
        }
        other => other.clone(),
    }
}

/// The broadcast shape of several operands.
pub fn broadcast_dims<'a>(values: impl IntoIterator<Item = &'a Value>) -> (usize, usize) {
    let (mut rows, mut cols) = (1, 1);
    for v in values {
        if let Value::Array(a) = v {
            rows = rows.max(a.rows());
            cols = cols.max(a.cols());
        }
    }
    (rows, cols)
}

/// Applies a scalar operation element-wise when either operand is an array.
pub fn lift_binary(a: &Value, b: &Value, mut f: impl FnMut(&Value, &Value) -> Value) -> Value {
    if !matches!(a, Value::Array(_)) && !matches!(b, Value::Array(_)) {
        return f(a, b);
    }
    let (rows, cols) = broadcast_dims([a, b]);
    Value::array(Array2D::build(rows, cols, |r, c| {
        f(&broadcast_pick(a, r, c), &broadcast_pick(b, r, c))
    }))
}

/// Excel's arithmetic operators on two scalars (`+ - * / ^`).
pub fn arithmetic(op: BinaryOp, a: &Value, b: &Value, opts: &CoercionOptions) -> Value {
    let x = match to_number(a, opts) {
        Ok(x) => x,
        Err(e) => return Value::Error(e),
    };
    let y = match to_number(b, opts) {
        Ok(y) => y,
        Err(e) => return Value::Error(e),
    };
    let r = match op {
        BinaryOp::Add => x + y,
        BinaryOp::Sub => x - y,
        BinaryOp::Mul => x * y,
        BinaryOp::Div => {
            if y == 0.0 {
                return Value::Error(ErrorValue::Div0);
            }
            x / y
        }
        BinaryOp::Pow => {
            if x == 0.0 && y == 0.0 {
                return Value::Error(ErrorValue::Num);
            }
            if x == 0.0 && y < 0.0 {
                return Value::Error(ErrorValue::Div0);
            }
            x.powf(y)
        }
        _ => return Value::Error(ErrorValue::Value),
    };
    Value::number(r)
}

fn compare_op(op: BinaryOp, a: &Value, b: &Value) -> Value {
    use std::cmp::Ordering::{Equal, Greater, Less};
    match compare_values(a, b) {
        Err(e) => Value::Error(e),
        Ok(c) => Value::Bool(match op {
            BinaryOp::Eq => c == Equal,
            BinaryOp::Ne => c != Equal,
            BinaryOp::Lt => c == Less,
            BinaryOp::Gt => c == Greater,
            BinaryOp::Le => c != Greater,
            _ => c != Less,
        }),
    }
}

fn concat(a: &Value, b: &Value) -> Value {
    let x = match to_text(a) {
        Ok(x) => x,
        Err(e) => return Value::Error(e),
    };
    let y = match to_text(b) {
        Ok(y) => y,
        Err(e) => return Value::Error(e),
    };
    let mut s = String::with_capacity(x.len() + y.len());
    s.push_str(&x);
    s.push_str(&y);
    Value::from(s)
}

fn range_union(l: &Reference, r: &Reference) -> EvalValue {
    let all: SmallVec<[&Area; 4]> = l.as_areas().iter().chain(r.as_areas()).collect();
    let sheet = all[0].sheet;
    if all.iter().any(|a| a.sheet != sheet) {
        return err(ErrorValue::Value);
    }
    let b = Bounds {
        r0: all.iter().map(|a| a.bounds.r0).min().unwrap_or(0),
        c0: all.iter().map(|a| a.bounds.c0).min().unwrap_or(0),
        r1: all.iter().map(|a| a.bounds.r1).max().unwrap_or(0),
        c1: all.iter().map(|a| a.bounds.c1).max().unwrap_or(0),
    };
    EvalValue::Ref(Reference::area(Area { sheet, bounds: b }))
}

fn intersect(l: &Reference, r: &Reference) -> EvalValue {
    let mut out = Vec::new();
    for a in l.as_areas() {
        for b in r.as_areas() {
            if a.sheet != b.sheet {
                continue;
            }
            let bounds = Bounds {
                r0: a.bounds.r0.max(b.bounds.r0),
                r1: a.bounds.r1.min(b.bounds.r1),
                c0: a.bounds.c0.max(b.bounds.c0),
                c1: a.bounds.c1.min(b.bounds.c1),
            };
            if bounds.r0 <= bounds.r1 && bounds.c0 <= bounds.c1 {
                out.push(Area {
                    sheet: a.sheet,
                    bounds,
                });
            }
        }
    }
    match Reference::areas(out) {
        Some(r) => EvalValue::Ref(r),
        None => err(ErrorValue::Null),
    }
}

impl<'s> Evaluator<'s> {
    pub(crate) fn new(
        src: &'s mut dyn CellSource,
        sheet: SheetId,
        row: u32,
        col: u32,
        legacy: bool,
    ) -> Evaluator<'s> {
        let coercion = src.workbook().coercion();
        Evaluator {
            src,
            sheet,
            row,
            col,
            anchor: (row, col),
            legacy,
            scope: None,
            depth: 0,
            lambda_depth: 0,
            name_depth: 0,
            coercion,
            hyperlink: None,
            trace: None,
        }
    }

    /// Records the value and evaluation count of every sub-expression from now on.
    pub(crate) fn enable_trace(&mut self) {
        self.trace = Some(FxHashMap::default());
    }

    /// The trace entry of an expression, if tracing is on and it was evaluated.
    pub(crate) fn traced(&self, e: &Expr) -> Option<&TraceEntry> {
        self.trace.as_ref()?.get(&trace_key(e))
    }

    /// The function registry of the workbook.
    pub(crate) fn registry(&self) -> &dyn FunctionRegistry {
        self.src.workbook().registry()
    }

    /// The formula's sheet.
    pub(crate) fn formula_sheet(&self) -> SheetId {
        self.sheet
    }

    /// Evaluates a whole formula and reduces the result to what a cell holds.
    pub(crate) fn evaluate_formula(&mut self, e: &Expr) -> Value {
        let v = self.eval(e);
        self.finalize(v)
    }

    /// Converts an evaluation result to a cell value (dynamic arrays, or implicit intersection).
    fn finalize(&mut self, v: EvalValue) -> Value {
        match v {
            EvalValue::Value(Value::Lambda(_)) => Value::Error(ErrorValue::Calc),
            EvalValue::Missing | EvalValue::Value(Value::Blank) => Value::Number(0.0),
            EvalValue::Ref(r) => {
                if self.legacy {
                    return self.implicit_intersection(&r).blank_to_zero();
                }
                match self.to_value(EvalValue::Ref(r)) {
                    Value::Array(a) => Value::array(a.map(|x| x.clone().blank_to_zero())),
                    other => other.blank_to_zero(),
                }
            }
            EvalValue::Value(Value::Array(a)) => {
                if self.legacy || a.len() == 1 {
                    a.get(0, 0).clone().blank_to_zero()
                } else {
                    Value::array(a.map(|x| x.clone().blank_to_zero()))
                }
            }
            EvalValue::Value(v) => v,
        }
    }

    fn eval(&mut self, e: &Expr) -> EvalValue {
        if self.depth >= MAX_EVAL_DEPTH {
            return err(ErrorValue::Num);
        }
        self.depth += 1;
        let v = self.eval_inner(e);
        self.depth -= 1;
        if let Some(trace) = &mut self.trace {
            trace
                .entry(trace_key(e))
                .and_modify(|t| {
                    t.value = v.clone();
                    t.count = t.count.saturating_add(1);
                })
                .or_insert_with(|| TraceEntry {
                    value: v.clone(),
                    count: 1,
                });
        }
        v
    }

    fn eval_inner(&mut self, e: &Expr) -> EvalValue {
        match e {
            Expr::Number(n) => EvalValue::Value(Value::Number(*n)),
            Expr::Str(s) => EvalValue::Value(Value::Str(s.clone())),
            Expr::Bool(b) => EvalValue::Value(Value::Bool(*b)),
            Expr::Error(x) => err(*x),
            Expr::Empty => EvalValue::Missing,
            Expr::Ref(sel, addr) => self.eval_ref(sel, addr),
            Expr::Name { name, scope } => self.eval_name(name, scope),
            Expr::Structured(s) => self.eval_structured(s),
            Expr::Array(a) => EvalValue::Value(Value::Array(a.clone())),
            Expr::Unary(op, x) => match op {
                UnaryOp::Plus => self.eval(x),
                UnaryOp::ImplicitIntersection => match self.eval(x) {
                    EvalValue::Ref(r) => EvalValue::Value(self.implicit_intersection(&r)),
                    EvalValue::Value(Value::Array(a)) => EvalValue::Value(a.get(0, 0).clone()),
                    other => other,
                },
                UnaryOp::Neg => {
                    let v = self.eval(x);
                    self.lift_numeric(v, |n| -n)
                }
            },
            Expr::Postfix(PostfixOp::Percent, x) => {
                let v = self.eval(x);
                self.lift_numeric(v, |n| n / 100.0)
            }
            Expr::Postfix(PostfixOp::Spill, x) => match self.eval(x) {
                EvalValue::Ref(r) if r.is_single_cell() => {
                    let a = r.as_areas()[0];
                    match self.src.spill_area(a.sheet, a.bounds.r0, a.bounds.c0) {
                        Ok(bounds) => EvalValue::Ref(Reference::area(Area {
                            sheet: a.sheet,
                            bounds,
                        })),
                        Err(e) => err(e),
                    }
                }
                EvalValue::Value(Value::Error(e)) => err(e),
                _ => err(ErrorValue::Ref),
            },
            Expr::Binary(op, l, r) => self.eval_binary(*op, l, r),
            Expr::Call(f, args) => self.eval_call(f, args),
            Expr::Invoke(callee, args) => match self.eval(callee) {
                EvalValue::Value(Value::Error(e)) => err(e),
                EvalValue::Value(Value::Lambda(l)) => {
                    let args: Vec<EvalValue> = args.iter().map(|a| self.eval(a)).collect();
                    self.invoke_lambda(&l, args)
                }
                _ => err(ErrorValue::Value),
            },
        }
    }

    // ── references ──────────────────────────────────────────────────────────────────────────

    fn resolve_sheets(&self, sel: &SheetSel) -> Result<SmallVec<[SheetId; 1]>, ErrorValue> {
        let mut out = SmallVec::new();
        match sel {
            SheetSel::Current => out.push(self.sheet),
            SheetSel::Sheet(id) => out.push(*id),
            SheetSel::Span(a, b) => {
                let list = self
                    .src
                    .workbook()
                    .sheets_between(*a, *b)
                    .ok_or(ErrorValue::Ref)?;
                out.extend(list);
            }
            SheetSel::Unresolved | SheetSel::External => return Err(ErrorValue::Ref),
        }
        Ok(out)
    }

    fn eval_ref(&mut self, sel: &SheetSel, addr: &RelAddress) -> EvalValue {
        let sheets = match self.resolve_sheets(sel) {
            Ok(s) => s,
            Err(e) => return err(e),
        };
        let Some(bounds) = addr.resolve(self.anchor) else {
            return err(ErrorValue::Ref);
        };
        match Reference::areas(sheets.into_iter().map(|sheet| Area { sheet, bounds })) {
            Some(r) => EvalValue::Ref(r),
            None => err(ErrorValue::Ref),
        }
    }

    fn eval_name(&mut self, name: &str, scope: &NameScope) -> EvalValue {
        if *scope == NameScope::Default
            && let Some(s) = &self.scope
            && let Some(v) = s.lookup(name)
        {
            return v.clone();
        }
        let sheet = match scope {
            NameScope::Default => self.sheet,
            NameScope::Sheet(id) => *id,
            NameScope::Unresolved => return err(ErrorValue::Ref),
        };
        if let Some(body) = self.src.workbook().lookup_name(name, sheet) {
            if self.name_depth >= MAX_NAME_DEPTH {
                return err(ErrorValue::Name);
            }
            self.name_depth += 1;
            let saved_scope = self.scope.take();
            let saved_anchor = std::mem::take(&mut self.anchor);
            let v = self.eval(&body);
            self.anchor = saved_anchor;
            self.scope = saved_scope;
            self.name_depth -= 1;
            return v;
        }
        if *scope == NameScope::Default
            && let Some(t) = self.src.workbook().table(name)
        {
            return self.table_ref(&t, &[], false, None);
        }
        err(ErrorValue::Name)
    }

    fn eval_structured(&mut self, s: &StructuredRef) -> EvalValue {
        let wb = self.src.workbook();
        let table = match &s.table {
            Some(name) => wb.table(name),
            None => wb.table_at(self.sheet, self.row, self.col),
        };
        let Some(t) = table else {
            return err(ErrorValue::Ref);
        };
        self.table_ref(&t, &s.specials, s.this_row, s.columns.as_ref())
    }

    fn table_ref(
        &self,
        t: &TableDef,
        specials: &[crate::parser::StructuredSpecial],
        this_row: bool,
        columns: Option<&(String, Option<String>)>,
    ) -> EvalValue {
        match resolve_structured(t, specials, this_row, columns, (self.sheet, self.row)) {
            Ok(a) => EvalValue::Ref(Reference::area(a)),
            Err(e) => err(e),
        }
    }

    // ── operators ───────────────────────────────────────────────────────────────────────────

    /// Evaluates and dereferences (fast path for plain cell references).
    fn eval_value(&mut self, e: &Expr) -> Value {
        if let Expr::Ref(SheetSel::Current, RelAddress::Cell { row, col }) = e
            && self.trace.is_none()
        {
            return match (
                row.resolve(self.anchor.0, MAX_ROWS),
                col.resolve(self.anchor.1, MAX_COLS),
            ) {
                (Some(r), Some(c)) => self.src.read_cell(self.sheet, r, c),
                _ => Value::Error(ErrorValue::Ref),
            };
        }
        let v = self.eval(e);
        self.operand_value(v)
    }

    /// Dereferences an operator operand: legacy formulas intersect ranges with the formula's row
    /// or column (Excel's implicit intersection); dynamic-array formulas lift over them.
    fn operand_value(&mut self, v: EvalValue) -> Value {
        match v {
            EvalValue::Ref(r) if self.legacy => self.implicit_intersection(&r),
            other => self.to_value(other),
        }
    }

    fn eval_binary(&mut self, op: BinaryOp, l: &Expr, r: &Expr) -> EvalValue {
        if matches!(op, BinaryOp::Range | BinaryOp::Intersect | BinaryOp::Union) {
            let lv = self.eval(l);
            if lv.as_error().is_some() {
                return lv;
            }
            let rv = self.eval(r);
            if rv.as_error().is_some() {
                return rv;
            }
            let (EvalValue::Ref(a), EvalValue::Ref(b)) = (&lv, &rv) else {
                return err(ErrorValue::Value);
            };
            return match op {
                BinaryOp::Union => {
                    let areas = a.as_areas().iter().chain(b.as_areas()).copied();
                    Reference::areas(areas).map_or(err(ErrorValue::Value), EvalValue::Ref)
                }
                BinaryOp::Range => range_union(a, b),
                _ => intersect(a, b),
            };
        }
        let lv = self.eval_value(l);
        let rv = self.eval_value(r);
        if matches!(lv, Value::Lambda(_)) || matches!(rv, Value::Lambda(_)) {
            return err(ErrorValue::Value);
        }
        let out = match op {
            BinaryOp::Concat => lift_binary(&lv, &rv, concat),
            BinaryOp::Eq
            | BinaryOp::Ne
            | BinaryOp::Lt
            | BinaryOp::Gt
            | BinaryOp::Le
            | BinaryOp::Ge => lift_binary(&lv, &rv, |x, y| compare_op(op, x, y)),
            _ => {
                let opts = self.coercion;
                lift_binary(&lv, &rv, |x, y| arithmetic(op, x, y, &opts))
            }
        };
        EvalValue::Value(out)
    }

    fn lift_numeric(&mut self, v: EvalValue, f: impl Fn(f64) -> f64) -> EvalValue {
        let value = self.operand_value(v);
        let opts = self.coercion;
        let one = |x: &Value| match to_number(x, &opts) {
            Ok(n) => Value::number(f(n)),
            Err(e) => Value::Error(e),
        };
        EvalValue::Value(match &value {
            Value::Lambda(_) => Value::Error(ErrorValue::Value),
            Value::Array(a) => Value::array(a.map(one)),
            other => one(other),
        })
    }

    // ── functions and lambdas ───────────────────────────────────────────────────────────────

    fn eval_call(&mut self, f: &FuncRef, arg_exprs: &[Arc<Expr>]) -> EvalValue {
        let id = match f {
            FuncRef::Unknown(name) => {
                let target = match self.scope.as_ref().and_then(|s| s.lookup(name)) {
                    Some(v) => v.clone(),
                    None => self.eval_name(name, &NameScope::Default),
                };
                return match target {
                    EvalValue::Value(Value::Lambda(l)) => {
                        let args: Vec<EvalValue> = arg_exprs.iter().map(|a| self.eval(a)).collect();
                        self.invoke_lambda(&l, args)
                    }
                    EvalValue::Value(Value::Error(e)) if e != ErrorValue::Name => err(e),
                    _ => err(ErrorValue::Name),
                };
            }
            FuncRef::Known(id) => *id,
        };
        let Some(spec) = self.src.workbook().registry().spec(id) else {
            return err(ErrorValue::Name);
        };
        let (min, max, lazy, lift, implementation) = (
            spec.min_args,
            spec.max_args,
            spec.lazy,
            spec.lift,
            spec.implementation,
        );
        let argc = arg_exprs.len();
        if argc < usize::from(min) || argc > usize::from(max) {
            return err(ErrorValue::Value);
        }
        let mut args: Vec<Arg<'_>> = Vec::with_capacity(argc);
        for (i, a) in arg_exprs.iter().enumerate() {
            if lazy.contains(i) {
                args.push(Arg::Lazy(LazyArg {
                    expr: a,
                    scope: self.scope.clone(),
                    cached: None,
                }));
            } else {
                let v = self.eval(a);
                args.push(Arg::Value(v));
            }
        }
        if !lift.is_empty() {
            return self.call_lifted(lift, implementation, args);
        }
        implementation(&mut args, self)
    }

    fn call_lifted(
        &mut self,
        lift: ArgMask,
        implementation: FunctionImpl,
        mut args: Vec<Arg<'_>>,
    ) -> EvalValue {
        let mut any_array = false;
        for (i, a) in args.iter_mut().enumerate() {
            if !lift.contains(i) {
                continue;
            }
            if let Arg::Value(v) = a {
                if *v == EvalValue::Missing {
                    continue;
                }
                let lv = self.liftable(std::mem::replace(v, EvalValue::Missing));
                any_array |= matches!(lv, Value::Array(_));
                *v = EvalValue::Value(lv);
            }
        }
        if !any_array {
            return implementation(&mut args, self);
        }
        // Force lifted lazy arguments once; they take part in the broadcast.
        for (i, slot) in args.iter_mut().enumerate() {
            if lift.contains(i) && matches!(slot, Arg::Lazy(_)) {
                let mut a = std::mem::replace(slot, Arg::Value(EvalValue::Missing));
                let v = self.force(&mut a);
                *slot = Arg::Value(EvalValue::Value(self.liftable(v)));
            }
        }
        let operands: Vec<Value> = args
            .iter()
            .enumerate()
            .filter_map(|(i, a)| match a {
                Arg::Value(EvalValue::Value(v @ Value::Array(_))) if lift.contains(i) => {
                    Some(v.clone())
                }
                _ => None,
            })
            .collect();
        let (rows, cols) = broadcast_dims(operands.iter());
        let mut data = Vec::with_capacity(rows * cols);
        for r in 0..rows {
            for c in 0..cols {
                let mut call: Vec<Arg<'_>> = args
                    .iter()
                    .enumerate()
                    .map(|(i, a)| match a {
                        Arg::Value(EvalValue::Value(v @ Value::Array(_))) if lift.contains(i) => {
                            Arg::Value(EvalValue::Value(broadcast_pick(v, r, c)))
                        }
                        other => other.clone(),
                    })
                    .collect();
                let out = implementation(&mut call, self);
                data.push(self.to_scalar(out));
            }
        }
        match Array2D::new(rows, cols, data) {
            Some(a) => EvalValue::Value(Value::array(a)),
            None => err(ErrorValue::Calc),
        }
    }

    /// Dereference for a lifted position: a scalar or an array (implicit intersection if legacy).
    fn liftable(&mut self, v: EvalValue) -> Value {
        match v {
            EvalValue::Missing => Value::Blank,
            EvalValue::Ref(r) => {
                if self.legacy {
                    return self.implicit_intersection(&r);
                }
                match self.to_value(EvalValue::Ref(r)) {
                    Value::Lambda(_) => Value::Error(ErrorValue::Value),
                    v => v,
                }
            }
            EvalValue::Value(Value::Lambda(_)) => Value::Error(ErrorValue::Value),
            EvalValue::Value(Value::Array(a)) if a.len() == 1 => a.get(0, 0).clone(),
            EvalValue::Value(v) => v,
        }
    }

    fn invoke_lambda(&mut self, l: &Lambda, args: Vec<EvalValue>) -> EvalValue {
        if args.len() > l.arity() {
            return err(ErrorValue::Value);
        }
        if self.lambda_depth >= MAX_LAMBDA_DEPTH {
            return err(ErrorValue::Num);
        }
        let mut scope = l.closure().cloned();
        let mut args = args.into_iter();
        for p in l.param_names() {
            let v = args.next().unwrap_or(EvalValue::Missing);
            scope = Some(Scope::bind(scope, p, v));
        }
        self.lambda_depth += 1;
        let saved_scope = std::mem::replace(&mut self.scope, scope);
        let saved_anchor = std::mem::replace(&mut self.anchor, l.anchor());
        let body = l.body().clone();
        let v = self.eval(&body);
        self.anchor = saved_anchor;
        self.scope = saved_scope;
        self.lambda_depth -= 1;
        v
    }

    fn clamp(&self, a: Area) -> Area {
        let whole_col = a.bounds.r0 == 0 && a.bounds.r1 == MAX_ROWS - 1;
        let whole_row = a.bounds.c0 == 0 && a.bounds.c1 == MAX_COLS - 1;
        if !whole_col && !whole_row {
            return a;
        }
        let (rows, cols) = self.src.workbook().used_bounds(a.sheet);
        let mut b = a.bounds;
        if whole_col {
            b.r1 = b.r0.max(b.r1.min(rows.saturating_sub(1)));
        }
        if whole_row {
            b.c1 = b.c0.max(b.c1.min(cols.saturating_sub(1)));
        }
        Area {
            sheet: a.sheet,
            bounds: b,
        }
    }
}

impl CallContext for Evaluator<'_> {
    fn sheet(&self) -> SheetId {
        self.sheet
    }

    fn row(&self) -> u32 {
        self.row
    }

    fn col(&self) -> u32 {
        self.col
    }

    fn legacy(&self) -> bool {
        self.legacy
    }

    fn date1904(&self) -> bool {
        self.coercion.date1904
    }

    fn coercion(&self) -> CoercionOptions {
        self.coercion
    }

    fn now(&self) -> f64 {
        self.src.workbook().now()
    }

    fn scope(&self) -> Option<Arc<Scope>> {
        self.scope.clone()
    }

    fn force(&mut self, arg: &mut Arg<'_>) -> EvalValue {
        match arg {
            Arg::Value(v) => v.clone(),
            Arg::Lazy(l) => {
                if let Some(c) = &l.cached {
                    return c.clone();
                }
                let v = self.evaluate(l.expr, l.scope.clone());
                l.cached = Some(v.clone());
                v
            }
        }
    }

    fn evaluate(&mut self, expr: &Expr, scope: Option<Arc<Scope>>) -> EvalValue {
        let saved = std::mem::replace(&mut self.scope, scope);
        let v = self.eval(expr);
        self.scope = saved;
        v
    }

    fn to_value(&mut self, v: EvalValue) -> Value {
        match v {
            EvalValue::Ref(r) => {
                let Some(&a) = r.single() else {
                    return Value::Error(ErrorValue::Value);
                };
                if a.is_cell() {
                    return self.src.read_cell(a.sheet, a.bounds.r0, a.bounds.c0);
                }
                let c = self.clamp(a);
                let (rows, cols) = c.dims();
                let mut data = Vec::with_capacity(rows * cols);
                for r in c.bounds.r0..=c.bounds.r1 {
                    for col in c.bounds.c0..=c.bounds.c1 {
                        data.push(self.src.read_cell(c.sheet, r, col));
                    }
                }
                Array2D::new(rows, cols, data).map_or(Value::Error(ErrorValue::Calc), Value::array)
            }
            EvalValue::Missing => Value::Blank,
            EvalValue::Value(v) => v,
        }
    }

    fn to_scalar(&mut self, v: EvalValue) -> Value {
        match v {
            EvalValue::Ref(r) => self.implicit_intersection(&r),
            EvalValue::Value(Value::Array(a)) => a.get(0, 0).clone(),
            EvalValue::Value(Value::Lambda(_)) => Value::Error(ErrorValue::Calc),
            EvalValue::Missing => Value::Blank,
            EvalValue::Value(v) => v,
        }
    }

    fn to_array(&mut self, v: EvalValue) -> Result<Arc<Array2D>, ErrorValue> {
        let multi = matches!(&v, EvalValue::Ref(r) if r.as_areas().len() != 1);
        match self.to_value(v) {
            Value::Array(a) => Ok(a),
            Value::Lambda(_) => Err(ErrorValue::Value),
            Value::Error(e) if multi => Err(e),
            other => Ok(Arc::new(Array2D::filled(1, 1, other))),
        }
    }

    fn for_each_scalar(&mut self, v: &EvalValue, cb: &mut dyn FnMut(&Value, bool) -> bool) {
        match v {
            EvalValue::Ref(r) => {
                for area in r.as_areas() {
                    let a = self.clamp(*area);
                    for row in a.bounds.r0..=a.bounds.r1 {
                        for col in a.bounds.c0..=a.bounds.c1 {
                            let x = self.src.read_cell(a.sheet, row, col);
                            if !cb(&x, true) {
                                return;
                            }
                        }
                    }
                }
            }
            EvalValue::Value(Value::Array(a)) => {
                for x in a.values() {
                    if !cb(x, true) {
                        return;
                    }
                }
            }
            EvalValue::Missing => {}
            EvalValue::Value(Value::Lambda(_)) => {
                cb(&Value::Error(ErrorValue::Value), false);
            }
            EvalValue::Value(x) => {
                cb(x, false);
            }
        }
    }

    fn read_cell(&mut self, sheet: SheetId, row: u32, col: u32) -> Value {
        self.src.read_cell(sheet, row, col)
    }

    fn invoke(&mut self, lambda: &Lambda, args: Vec<EvalValue>) -> EvalValue {
        self.invoke_lambda(lambda, args)
    }

    fn make_lambda(
        &mut self,
        params: Vec<String>,
        body: Arc<Expr>,
        closure: Option<Arc<Scope>>,
    ) -> Value {
        let params = params.into_iter().map(Arc::from).collect();
        Value::Lambda(Arc::new(Lambda::new(params, body, closure, self.anchor)))
    }

    fn resolve_reference_text(&mut self, text: &str, a1: bool) -> Result<Reference, ErrorValue> {
        let t = text.trim();
        let t = t.strip_prefix('=').unwrap_or(t);
        let style = if a1 { RefStyle::A1 } else { RefStyle::R1C1 };
        let here = (self.row, self.col);
        let expr = self
            .src
            .workbook()
            .compile_text(t, style, here)
            .ok_or(ErrorValue::Ref)?;
        if !expr.is_reference_expression() {
            return Err(ErrorValue::Ref);
        }
        let saved_scope = self.scope.take();
        let saved_anchor = std::mem::replace(&mut self.anchor, here);
        let v = self.eval(&expr);
        self.anchor = saved_anchor;
        self.scope = saved_scope;
        match v {
            EvalValue::Ref(r) => Ok(r),
            _ => Err(ErrorValue::Ref),
        }
    }

    fn implicit_intersection(&mut self, r: &Reference) -> Value {
        let Some(&a) = r.single() else {
            return Value::Error(ErrorValue::Value);
        };
        let b = a.bounds;
        if a.is_cell() {
            return self.src.read_cell(a.sheet, b.r0, b.c0);
        }
        // Like Excel, the intersection uses the formula's row or column on whatever sheet the
        // range is.
        if b.c0 == b.c1 && self.row >= b.r0 && self.row <= b.r1 {
            return self.src.read_cell(a.sheet, self.row, b.c0);
        }
        if b.r0 == b.r1 && self.col >= b.c0 && self.col <= b.c1 {
            return self.src.read_cell(a.sheet, b.r0, self.col);
        }
        Value::Error(ErrorValue::Value)
    }

    fn used_bounds(&self, sheet: SheetId) -> (u32, u32) {
        self.src.workbook().used_bounds(sheet)
    }

    fn clamp_area(&self, area: Area) -> Area {
        self.clamp(area)
    }

    fn sheet_name(&self, sheet: SheetId) -> Option<String> {
        self.src.workbook().sheet_name(sheet)
    }

    fn sheet_id(&self, name: &str) -> Option<SheetId> {
        self.src.workbook().sheet_id(name)
    }

    fn sheet_ids(&self) -> Vec<SheetId> {
        self.src.workbook().sheet_order()
    }

    fn table(&self, name: &str) -> Option<TableDef> {
        self.src.workbook().table(name)
    }

    fn is_formula_cell(&self, sheet: SheetId, row: u32, col: u32) -> bool {
        self.src.workbook().is_formula_cell(sheet, row, col)
    }

    fn formula_text(&self, sheet: SheetId, row: u32, col: u32) -> Option<String> {
        self.src.workbook().formula_text(sheet, row, col)
    }

    fn row_visibility(&self, sheet: SheetId, row: u32) -> RowVisibility {
        self.src.workbook().row_visibility(sheet, row)
    }

    fn set_hyperlink(&mut self, target: String) {
        self.hyperlink = Some(target);
    }
}
