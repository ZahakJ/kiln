// SPDX-License-Identifier: AGPL-3.0-or-later
//! Evaluation-time types: sheet ids, cell keys, references, evaluation results, lexical scopes,
//! lambdas and function arguments.

use std::sync::Arc;

use smallvec::SmallVec;

use super::expr::Expr;
use crate::refs::Bounds;
use crate::values::{ErrorValue, Value};

/// A sheet's stable id (survives renames and moves).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub struct SheetId(pub u32);

/// `(sheet, row, col)` packed into one integer: sheet in the high bits, then 20 bits of row and
/// 14 bits of column, so keys of one sheet sort row-major.
pub type CellKey = u64;

/// Packs a cell position into a [`CellKey`].
pub const fn cell_key(sheet: SheetId, row: u32, col: u32) -> CellKey {
    ((sheet.0 as u64) << 34) | ((row as u64) << 14) | col as u64
}

/// Unpacks a [`CellKey`].
pub const fn unpack_key(key: CellKey) -> (SheetId, u32, u32) {
    (
        SheetId((key >> 34) as u32),
        ((key >> 14) & 0xF_FFFF) as u32,
        (key & 0x3FFF) as u32,
    )
}

/// A rectangle on one sheet.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct Area {
    /// The sheet.
    pub sheet: SheetId,
    /// The rectangle.
    pub bounds: Bounds,
}

impl Area {
    /// One cell.
    pub const fn cell(sheet: SheetId, row: u32, col: u32) -> Area {
        Area {
            sheet,
            bounds: Bounds {
                r0: row,
                c0: col,
                r1: row,
                c1: col,
            },
        }
    }

    /// True for a single cell.
    pub fn is_cell(&self) -> bool {
        self.bounds.r0 == self.bounds.r1 && self.bounds.c0 == self.bounds.c1
    }

    /// Rows × columns.
    pub fn dims(&self) -> (usize, usize) {
        (
            (self.bounds.r1 - self.bounds.r0) as usize + 1,
            (self.bounds.c1 - self.bounds.c0) as usize + 1,
        )
    }
}

/// A reference: one area, or several for unions and 3-D references.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Reference {
    areas: SmallVec<[Area; 1]>,
}

impl Reference {
    /// A reference to one area.
    pub fn area(a: Area) -> Reference {
        let mut areas = SmallVec::new();
        areas.push(a);
        Reference { areas }
    }

    /// A reference to several areas (at least one).
    pub fn areas(areas: impl IntoIterator<Item = Area>) -> Option<Reference> {
        let areas: SmallVec<[Area; 1]> = areas.into_iter().collect();
        (!areas.is_empty()).then_some(Reference { areas })
    }

    /// The areas.
    pub fn as_areas(&self) -> &[Area] {
        &self.areas
    }

    /// The only area, if there is exactly one.
    pub fn single(&self) -> Option<&Area> {
        match self.areas.as_slice() {
            [a] => Some(a),
            _ => None,
        }
    }

    /// True for a single cell.
    pub fn is_single_cell(&self) -> bool {
        self.single().is_some_and(Area::is_cell)
    }
}

/// What an expression evaluates to: a value, a reference (kept as such so `ROW`, `OFFSET`,
/// `INDEX` and the reference operators see references), or an omitted argument.
#[derive(Debug, Clone, PartialEq)]
pub enum EvalValue {
    /// A value.
    Value(Value),
    /// A reference.
    Ref(Reference),
    /// An omitted argument (`IF(A1,,2)`); behaves like a blank.
    Missing,
}

impl EvalValue {
    /// The error, if this is an error value.
    pub fn as_error(&self) -> Option<ErrorValue> {
        match self {
            EvalValue::Value(Value::Error(e)) => Some(*e),
            _ => None,
        }
    }
}

impl From<Value> for EvalValue {
    fn from(v: Value) -> EvalValue {
        EvalValue::Value(v)
    }
}

impl From<ErrorValue> for EvalValue {
    fn from(e: ErrorValue) -> EvalValue {
        EvalValue::Value(Value::Error(e))
    }
}

impl From<f64> for EvalValue {
    fn from(n: f64) -> EvalValue {
        EvalValue::Value(Value::number(n))
    }
}

impl From<bool> for EvalValue {
    fn from(b: bool) -> EvalValue {
        EvalValue::Value(Value::Bool(b))
    }
}

impl From<Reference> for EvalValue {
    fn from(r: Reference) -> EvalValue {
        EvalValue::Ref(r)
    }
}

/// Lexical bindings created by `LET` and lambda invocation: an immutable chain of frames.
/// Names compare case-insensitively.
#[derive(Debug)]
pub struct Scope {
    name: Arc<str>,
    value: EvalValue,
    parent: Option<Arc<Scope>>,
}

impl Scope {
    /// A new frame binding `name` on top of `parent`.
    pub fn bind(parent: Option<Arc<Scope>>, name: &str, value: EvalValue) -> Arc<Scope> {
        Arc::new(Scope {
            name: Arc::from(name.to_uppercase()),
            value,
            parent,
        })
    }

    /// The innermost binding of `name`.
    pub fn lookup(&self, name: &str) -> Option<&EvalValue> {
        let mut s = Some(self);
        while let Some(frame) = s {
            if frame
                .name
                .chars()
                .eq(name.chars().flat_map(char::to_uppercase))
            {
                return Some(&frame.value);
            }
            s = frame.parent.as_deref();
        }
        None
    }
}

/// A lambda function value: parameters, body, the scope it closed over and the anchor its
/// relative references resolve against.
#[derive(Debug)]
pub struct Lambda {
    params: Vec<Arc<str>>,
    body: Arc<Expr>,
    closure: Option<Arc<Scope>>,
    anchor: (u32, u32),
}

impl Lambda {
    pub(crate) fn new(
        params: Vec<Arc<str>>,
        body: Arc<Expr>,
        closure: Option<Arc<Scope>>,
        anchor: (u32, u32),
    ) -> Lambda {
        Lambda {
            params,
            body,
            closure,
            anchor,
        }
    }

    /// Parameter names as written.
    pub fn params(&self) -> Vec<&str> {
        self.params.iter().map(|p| &**p).collect()
    }

    /// Number of parameters.
    pub fn arity(&self) -> usize {
        self.params.len()
    }

    pub(crate) fn body(&self) -> &Arc<Expr> {
        &self.body
    }

    pub(crate) fn closure(&self) -> Option<&Arc<Scope>> {
        self.closure.as_ref()
    }

    pub(crate) fn param_names(&self) -> &[Arc<str>] {
        &self.params
    }

    pub(crate) fn anchor(&self) -> (u32, u32) {
        self.anchor
    }
}

/// A function argument: evaluated, or (for lazy positions) the unevaluated expression with the
/// scope at the call site. Use [`CallContext::force`](super::CallContext::force) to evaluate.
#[derive(Debug, Clone)]
pub enum Arg<'e> {
    /// An evaluated argument.
    Value(EvalValue),
    /// An unevaluated argument.
    Lazy(LazyArg<'e>),
}

/// An argument passed unevaluated.
#[derive(Debug, Clone)]
pub struct LazyArg<'e> {
    pub(crate) expr: &'e Arc<Expr>,
    pub(crate) scope: Option<Arc<Scope>>,
    pub(crate) cached: Option<EvalValue>,
}

impl<'e> LazyArg<'e> {
    /// The expression.
    pub fn expr(&self) -> &'e Arc<Expr> {
        self.expr
    }

    /// The lexical scope at the call site.
    pub fn scope(&self) -> Option<&Arc<Scope>> {
        self.scope.as_ref()
    }

    /// True when the argument was left empty (`IF(A1,,2)`).
    pub fn is_empty(&self) -> bool {
        matches!(**self.expr, Expr::Empty)
    }
}

impl Arg<'_> {
    /// True when the argument was omitted or left empty.
    pub fn is_omitted(&self) -> bool {
        match self {
            Arg::Value(EvalValue::Missing) => true,
            Arg::Value(_) => false,
            Arg::Lazy(l) => l.is_empty(),
        }
    }
}

/// Visibility of a row for `SUBTOTAL` and `AGGREGATE`.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum RowVisibility {
    /// Shown.
    #[default]
    Visible,
    /// Hidden manually or collapsed by an outline.
    Hidden,
    /// Hidden by an AutoFilter or advanced filter.
    Filtered,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn keys_pack_and_unpack() {
        let k = cell_key(SheetId(7), 1_048_575, 16_383);
        assert_eq!(unpack_key(k), (SheetId(7), 1_048_575, 16_383));
        assert!(cell_key(SheetId(1), 0, 5) < cell_key(SheetId(1), 1, 0));
    }

    #[test]
    fn scopes_shadow_case_insensitively() {
        let a = Scope::bind(None, "x", EvalValue::from(1.0));
        let b = Scope::bind(Some(a.clone()), "X", EvalValue::from(2.0));
        assert_eq!(b.lookup("x"), Some(&EvalValue::from(2.0)));
        assert_eq!(a.lookup("X"), Some(&EvalValue::from(1.0)));
        assert_eq!(b.lookup("y"), None);
    }
}
