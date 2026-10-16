// SPDX-License-Identifier: AGPL-3.0-or-later
//! The compiled form of a formula.
//!
//! An [`Expr`] is the AST with spans and parentheses removed, sheet names resolved to ids, function
//! names resolved to registry ids and every relative row or column stored as an offset from the
//! formula's cell. It is therefore independent of where the formula lives: a formula filled down a
//! million rows compiles to one shared `Expr` (like Excel's shared formulas), which keeps memory
//! and compile time small.

use std::sync::Arc;

use super::types::SheetId;
use crate::parser::{BinaryOp, Node, NodeKind, PostfixOp, RefAddress, StructuredRef, UnaryOp};
use crate::refs::{Bounds, MAX_COLS, MAX_ROWS};
use crate::registry::{FunctionId, normalize_function_name};
use crate::values::{Array2D, ErrorValue, Value};

/// One row or column of a compiled reference: an absolute index, or an offset from the anchor.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct Coord {
    /// Index (absolute) or offset (relative).
    pub value: i32,
    /// `$`.
    pub abs: bool,
}

impl Coord {
    fn of(index: u32, abs: bool, base: u32) -> Coord {
        Coord {
            value: if abs {
                index as i32
            } else {
                index as i32 - base as i32
            },
            abs,
        }
    }

    /// The index at `base`, if inside `0..max`.
    pub fn resolve(self, base: u32, max: u32) -> Option<u32> {
        let v = if self.abs {
            i64::from(self.value)
        } else {
            i64::from(base) + i64::from(self.value)
        };
        (0..i64::from(max)).contains(&v).then_some(v as u32)
    }
}

/// A compiled reference address.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum RelAddress {
    /// One cell.
    Cell {
        /// Row.
        row: Coord,
        /// Column.
        col: Coord,
    },
    /// A rectangle.
    Area {
        /// First row.
        r0: Coord,
        /// First column.
        c0: Coord,
        /// Last row.
        r1: Coord,
        /// Last column.
        c1: Coord,
    },
    /// Whole rows.
    Rows {
        /// First row.
        r0: Coord,
        /// Last row.
        r1: Coord,
    },
    /// Whole columns.
    Cols {
        /// First column.
        c0: Coord,
        /// Last column.
        c1: Coord,
    },
}

impl RelAddress {
    /// Compiles an address of a formula anchored at `(row, col)`.
    pub fn from_address(a: &RefAddress, anchor: (u32, u32)) -> RelAddress {
        let (ar, ac) = anchor;
        match *a {
            RefAddress::Cell {
                row,
                col,
                row_abs,
                col_abs,
            } => RelAddress::Cell {
                row: Coord::of(row, row_abs, ar),
                col: Coord::of(col, col_abs, ac),
            },
            RefAddress::Area {
                r0,
                c0,
                r1,
                c1,
                r0_abs,
                c0_abs,
                r1_abs,
                c1_abs,
            } => RelAddress::Area {
                r0: Coord::of(r0, r0_abs, ar),
                c0: Coord::of(c0, c0_abs, ac),
                r1: Coord::of(r1, r1_abs, ar),
                c1: Coord::of(c1, c1_abs, ac),
            },
            RefAddress::Rows {
                r0,
                r1,
                r0_abs,
                r1_abs,
            } => RelAddress::Rows {
                r0: Coord::of(r0, r0_abs, ar),
                r1: Coord::of(r1, r1_abs, ar),
            },
            RefAddress::Cols {
                c0,
                c1,
                c0_abs,
                c1_abs,
            } => RelAddress::Cols {
                c0: Coord::of(c0, c0_abs, ac),
                c1: Coord::of(c1, c1_abs, ac),
            },
        }
    }

    /// The rectangle at `anchor`, or `None` when it falls off the sheet.
    pub fn resolve(&self, anchor: (u32, u32)) -> Option<Bounds> {
        let (ar, ac) = anchor;
        let (r0, c0, r1, c1) = match *self {
            RelAddress::Cell { row, col } => {
                let r = row.resolve(ar, MAX_ROWS)?;
                let c = col.resolve(ac, MAX_COLS)?;
                (r, c, r, c)
            }
            RelAddress::Area { r0, c0, r1, c1 } => (
                r0.resolve(ar, MAX_ROWS)?,
                c0.resolve(ac, MAX_COLS)?,
                r1.resolve(ar, MAX_ROWS)?,
                c1.resolve(ac, MAX_COLS)?,
            ),
            RelAddress::Rows { r0, r1 } => (
                r0.resolve(ar, MAX_ROWS)?,
                0,
                r1.resolve(ar, MAX_ROWS)?,
                MAX_COLS - 1,
            ),
            RelAddress::Cols { c0, c1 } => (
                0,
                c0.resolve(ac, MAX_COLS)?,
                MAX_ROWS - 1,
                c1.resolve(ac, MAX_COLS)?,
            ),
        };
        Some(Bounds {
            r0: r0.min(r1),
            c0: c0.min(c1),
            r1: r0.max(r1),
            c1: c0.max(c1),
        })
    }

    /// True for a single-cell address.
    pub fn is_cell(&self) -> bool {
        matches!(self, RelAddress::Cell { .. })
    }
}

/// Which sheet(s) a compiled reference points to.
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub enum SheetSel {
    /// The sheet of the cell being calculated.
    Current,
    /// One sheet.
    Sheet(SheetId),
    /// A 3-D span, first to last in workbook order.
    Span(SheetId, SheetId),
    /// A sheet that does not exist (yet); `#REF!` until a sheet of that name is added.
    Unresolved,
    /// Another workbook; external links are not supported, so `#REF!`.
    External,
}

/// Where a defined name is looked up.
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub enum NameScope {
    /// The calling cell's sheet, then the workbook (or a LET/LAMBDA variable).
    Default,
    /// An explicit sheet (`Sheet1!Total`).
    Sheet(SheetId),
    /// A sheet that does not exist.
    Unresolved,
}

/// A resolved or unknown function.
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub enum FuncRef {
    /// A registered function.
    Known(FunctionId),
    /// Not a registered function: a named LAMBDA or `#NAME?` (normalised name).
    Unknown(Arc<str>),
}

/// A compiled expression.
#[derive(Debug, Clone, PartialEq)]
pub enum Expr {
    /// A number.
    Number(f64),
    /// Text.
    Str(Arc<str>),
    /// A logical.
    Bool(bool),
    /// An error literal.
    Error(ErrorValue),
    /// A reference.
    Ref(SheetSel, RelAddress),
    /// A defined name, table name or variable.
    Name {
        /// The name as written.
        name: Arc<str>,
        /// Where to look it up.
        scope: NameScope,
    },
    /// A structured reference.
    Structured(Arc<StructuredRef>),
    /// An array constant.
    Array(Arc<Array2D>),
    /// A prefix operator.
    Unary(UnaryOp, Box<Expr>),
    /// A postfix operator.
    Postfix(PostfixOp, Box<Expr>),
    /// A binary operator.
    Binary(BinaryOp, Box<Expr>, Box<Expr>),
    /// A function call.
    Call(FuncRef, Box<[Arc<Expr>]>),
    /// Calling the result of an expression.
    Invoke(Box<Expr>, Box<[Arc<Expr>]>),
    /// An omitted argument.
    Empty,
}

impl Expr {
    /// The name, when this is an unqualified name (a candidate LET/LAMBDA variable).
    pub fn as_local_name(&self) -> Option<&str> {
        match self {
            Expr::Name {
                name,
                scope: NameScope::Default,
            } => Some(name),
            _ => None,
        }
    }

    /// True for expressions that can only produce references (what `INDIRECT` accepts).
    pub fn is_reference_expression(&self) -> bool {
        match self {
            Expr::Ref(..) | Expr::Name { .. } | Expr::Structured(_) => true,
            Expr::Binary(BinaryOp::Range | BinaryOp::Intersect | BinaryOp::Union, l, r) => {
                l.is_reference_expression() && r.is_reference_expression()
            }
            Expr::Postfix(PostfixOp::Spill, e) => e.is_reference_expression(),
            _ => false,
        }
    }

    /// Visits this expression and every sub-expression (pre-order).
    pub fn visit(&self, f: &mut impl FnMut(&Expr)) {
        f(self);
        match self {
            Expr::Unary(_, e) | Expr::Postfix(_, e) => e.visit(f),
            Expr::Binary(_, l, r) => {
                l.visit(f);
                r.visit(f);
            }
            Expr::Call(_, args) => args.iter().for_each(|a| a.visit(f)),
            Expr::Invoke(callee, args) => {
                callee.visit(f);
                args.iter().for_each(|a| a.visit(f));
            }
            _ => {}
        }
    }
}

/// What compilation needs to know about the workbook.
pub trait CompileContext {
    /// The id of a sheet name (case-insensitive).
    fn sheet_id(&self, name: &str) -> Option<SheetId>;
    /// The id of a function name (already normalised).
    fn function(&self, normalized_name: &str) -> Option<FunctionId>;
}

fn sheet_sel(prefix: Option<&crate::parser::SheetPrefix>, ctx: &dyn CompileContext) -> SheetSel {
    let Some(p) = prefix else {
        return SheetSel::Current;
    };
    if p.workbook.is_some() {
        return SheetSel::External;
    }
    let Some(a) = ctx.sheet_id(&p.sheet) else {
        return SheetSel::Unresolved;
    };
    match &p.sheet2 {
        None => SheetSel::Sheet(a),
        Some(s2) => match ctx.sheet_id(s2) {
            Some(b) => SheetSel::Span(a, b),
            None => SheetSel::Unresolved,
        },
    }
}

fn literal(node: &Node) -> Value {
    match &node.kind {
        NodeKind::Number(n) => Value::number(*n),
        NodeKind::Str(s) => Value::from(s.as_str()),
        NodeKind::Bool(b) => Value::Bool(*b),
        NodeKind::Error { code, .. } => Value::Error(*code),
        _ => Value::Error(ErrorValue::Value),
    }
}

/// Compiles an AST of a formula anchored at `(row, col)`.
pub fn compile(node: &Node, anchor: (u32, u32), ctx: &dyn CompileContext) -> Expr {
    match &node.kind {
        NodeKind::Number(n) => Expr::Number(*n),
        NodeKind::Str(s) => Expr::Str(Arc::from(s.as_str())),
        NodeKind::Bool(b) => Expr::Bool(*b),
        NodeKind::Error { code, .. } => Expr::Error(*code),
        NodeKind::Ref { prefix, address } => Expr::Ref(
            sheet_sel(prefix.as_ref(), ctx),
            RelAddress::from_address(address, anchor),
        ),
        NodeKind::Name { name, prefix } => Expr::Name {
            name: Arc::from(name.as_str()),
            scope: match prefix {
                None => NameScope::Default,
                Some(p) => match ctx.sheet_id(&p.sheet) {
                    Some(id) if p.workbook.is_none() => NameScope::Sheet(id),
                    _ => NameScope::Unresolved,
                },
            },
        },
        NodeKind::Structured(s) => Expr::Structured(Arc::new((**s).clone())),
        NodeKind::Array(rows) => {
            let data: Vec<Vec<Value>> = rows
                .iter()
                .map(|r| r.iter().map(literal).collect())
                .collect();
            match Array2D::from_rows(data) {
                Some(a) => Expr::Array(Arc::new(a)),
                None => Expr::Error(ErrorValue::Value),
            }
        }
        NodeKind::Unary { op, operand } => {
            Expr::Unary(*op, Box::new(compile(operand, anchor, ctx)))
        }
        NodeKind::Postfix { op, operand } => {
            Expr::Postfix(*op, Box::new(compile(operand, anchor, ctx)))
        }
        NodeKind::Binary { op, left, right } => Expr::Binary(
            *op,
            Box::new(compile(left, anchor, ctx)),
            Box::new(compile(right, anchor, ctx)),
        ),
        NodeKind::Call { name, args } => {
            let normalized = normalize_function_name(name);
            let func = match ctx.function(&normalized) {
                Some(id) => FuncRef::Known(id),
                None => FuncRef::Unknown(Arc::from(normalized)),
            };
            Expr::Call(
                func,
                args.iter()
                    .map(|a| Arc::new(compile(a, anchor, ctx)))
                    .collect(),
            )
        }
        NodeKind::Invoke { callee, args } => Expr::Invoke(
            Box::new(compile(callee, anchor, ctx)),
            args.iter()
                .map(|a| Arc::new(compile(a, anchor, ctx)))
                .collect(),
        ),
        NodeKind::Paren(e) => compile(e, anchor, ctx),
        NodeKind::Empty => Expr::Empty,
    }
}
